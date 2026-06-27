mod failures;
mod ingest_log;
mod pipeline;
mod recovery;
mod summary;
mod validation;

use std::collections::BTreeMap;

use chrono::NaiveDate;
use shared::slippy_tiles::{
    summarize_viirs_coverage, viirs_tile_overlaps_bounds, viirs_tiles_for_bounds, GeographicBounds,
    TileMathError, ViirsTileCoord,
};
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;

use ingest_log::get_discovery_resume_points_for_products;
use pipeline::process_granule;
pub use recovery::{replay_rejected_granule, run_recovery};
pub use summary::IngestSummary;

use crate::{
    cmr::{CmrClient, CmrError},
    config::AppConfig,
    earthdata::{EarthdataClient, EarthdataError},
    models::GranuleCandidate,
    storage::{BlobStorageClient, QueueClient, StorageError},
};

pub async fn run_ingest(
    config: &AppConfig,
    pool: &PgPool,
    correlation_id: Uuid,
) -> Result<IngestSummary, IngestError> {
    let span = tracing::info_span!(
        "ingest_run",
        service = crate::observability::SERVICE_NAME,
        service_version = crate::observability::SERVICE_VERSION,
        command = "ingest",
        correlation_id = %correlation_id,
        bounding_box = %config.bounding_box,
        ingest_cadence = config.ingest_cadence.as_str(),
        ingest_products = config.ingest_products.join(","),
        discovered = tracing::field::Empty,
        eligible = tracing::field::Empty,
        filtered_out_of_bounds = tracing::field::Empty,
        attempted = tracing::field::Empty,
        enqueued = tracing::field::Empty,
        failed = tracing::field::Empty,
        rejected = tracing::field::Empty,
        skipped = tracing::field::Empty,
    );

    run_ingest_inner(config, pool, correlation_id)
        .instrument(span)
        .await
}

async fn run_ingest_inner(
    config: &AppConfig,
    pool: &PgPool,
    correlation_id: Uuid,
) -> Result<IngestSummary, IngestError> {
    tracing::info!(
        bounding_box = %config.bounding_box,
        ingest_cadence = config.ingest_cadence.as_str(),
        ingest_products = config.ingest_products.join(","),
        "starting raw blob ingest"
    );

    let resume_points =
        get_discovery_resume_points_for_products(pool, &config.ingest_products).await?;
    if resume_points.is_empty() {
        tracing::info!(
            ingest_products = config.ingest_products.join(","),
            "no previous non-failed ingest records found for selected products; using default CMR temporal start"
        );
    } else {
        for product in &config.ingest_products {
            if let Some(resume_from) = resume_points.get(product) {
                tracing::info!(
                    product,
                    resume_granule_date = %resume_from,
                    "using product-specific ingest_log resume point for CMR discovery"
                );
            }
        }
    }

    let cmr = CmrClient::new(config)?;
    let discovery = cmr.discover(config, &resume_points).await?;
    let discovered = discovery.total_granules();

    let earthdata = EarthdataClient::new(config)?;
    let storage = BlobStorageClient::new(config)?;
    let queue = QueueClient::new(config)?;
    let mut summary = IngestSummary::new(discovered);
    let configured_bounds = GeographicBounds::from(config.bounding_box);
    let expected_viirs_tiles = viirs_tiles_for_bounds(configured_bounds)?;
    let mut in_bounds_discovery = BTreeMap::new();
    let mut eligible_granules = Vec::new();

    for granule in discovery
        .products
        .iter()
        .flat_map(|product| product.granules.iter())
    {
        let coord = viirs_coord_for_granule(granule);
        match viirs_tile_overlaps_bounds(coord.tile_h, coord.tile_v, configured_bounds) {
            Ok(true) => {
                in_bounds_discovery
                    .entry((granule.product.clone(), granule.granule_date.date_naive()))
                    .or_insert_with(Vec::new)
                    .push(coord);
                eligible_granules.push(granule);
            }
            Ok(false) => {
                summary.record_filtered_out_of_bounds();
                tracing::info!(
                    product = granule.product,
                    granule_date = %granule.granule_date.date_naive(),
                    tile_h = coord.tile_h,
                    tile_v = coord.tile_v,
                    bounding_box = %config.bounding_box,
                    "filtered CMR candidate outside configured ingest bounds"
                );
            }
            Err(TileMathError::InvalidViirsTile { .. }) => {
                summary.record_filtered_out_of_bounds();
                tracing::warn!(
                    product = granule.product,
                    granule_date = %granule.granule_date.date_naive(),
                    tile_h = coord.tile_h,
                    tile_v = coord.tile_v,
                    "filtered CMR candidate with invalid VIIRS tile coordinates"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }

    log_discovery_coverage(&expected_viirs_tiles, &in_bounds_discovery);

    for granule in eligible_granules {
        let outcome = process_granule(
            config,
            pool,
            &earthdata,
            &storage,
            &queue,
            correlation_id,
            granule,
        )
        .await;
        summary.record_attempt(outcome);
    }

    let span = tracing::Span::current();
    span.record("discovered", summary.discovered);
    span.record("eligible", summary.attempted);
    span.record("filtered_out_of_bounds", summary.filtered_out_of_bounds);
    span.record("attempted", summary.attempted);
    span.record("enqueued", summary.enqueued);
    span.record("failed", summary.failed);
    span.record("rejected", summary.rejected);
    span.record("skipped", summary.skipped);

    tracing::info!(
        discovered = summary.discovered,
        eligible = summary.attempted,
        filtered_out_of_bounds = summary.filtered_out_of_bounds,
        downloaded = summary.downloaded,
        attempted = summary.attempted,
        validated = summary.validated,
        enqueued = summary.enqueued,
        rejected = summary.rejected,
        failed = summary.failed,
        skipped = summary.skipped,
        "raw blob ingest completed"
    );

    Ok(summary)
}

fn viirs_coord_for_granule(granule: &GranuleCandidate) -> ViirsTileCoord {
    ViirsTileCoord {
        tile_h: i16::from(granule.tile.h),
        tile_v: i16::from(granule.tile.v),
    }
}

fn log_discovery_coverage(
    expected_tiles: &[ViirsTileCoord],
    in_bounds_discovery: &BTreeMap<(String, NaiveDate), Vec<ViirsTileCoord>>,
) {
    for ((product, dataset_date), present_tiles) in in_bounds_discovery {
        let coverage = summarize_viirs_coverage(
            expected_tiles.iter().copied(),
            present_tiles.iter().copied(),
        );

        if coverage.complete {
            tracing::info!(
                product,
                dataset_date = %dataset_date,
                expected_tile_count = coverage.expected_tile_count,
                present_tile_count = coverage.present_tile_count,
                coverage_fraction = coverage.coverage_fraction,
                "CMR discovery covered all expected in-bounds VIIRS tiles"
            );
        } else {
            tracing::warn!(
                product,
                dataset_date = %dataset_date,
                expected_tile_count = coverage.expected_tile_count,
                present_tile_count = coverage.present_tile_count,
                coverage_fraction = coverage.coverage_fraction,
                missing_tiles = %format_viirs_tiles(&coverage.missing_tiles),
                missing_columns = ?coverage.missing_columns,
                missing_rows = ?coverage.missing_rows,
                "CMR discovery did not cover all expected in-bounds VIIRS tiles"
            );
        }
    }
}

fn format_viirs_tiles(tiles: &[ViirsTileCoord]) -> String {
    tiles
        .iter()
        .map(|coord| format!("h{:02}v{:02}", coord.tile_h, coord.tile_v))
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("{0}")]
    Cmr(#[from] CmrError),
    #[error("{0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Earthdata(#[from] EarthdataError),
    #[error("{0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    TileMath(#[from] TileMathError),
    #[error("ingest replay error: ingest row {ingest_id} was not found or is not rejected")]
    ReplayNotRejected { ingest_id: Uuid },
}

#[cfg(test)]
mod tests;
