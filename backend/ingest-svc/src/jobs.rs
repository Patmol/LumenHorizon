mod failures;
mod ingest_log;
mod pipeline;
mod recovery;
mod summary;
mod validation;

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
        ingest_max_granules = config.ingest_max_granules,
        discovered = tracing::field::Empty,
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
        ingest_max_granules = config.ingest_max_granules,
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

    let granules = discovery
        .products
        .iter()
        .flat_map(|product| product.granules.iter())
        .take(config.ingest_max_granules.unwrap_or(usize::MAX));

    let mut summary = IngestSummary::new(discovered);

    for granule in granules {
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
    span.record("attempted", summary.attempted);
    span.record("enqueued", summary.enqueued);
    span.record("failed", summary.failed);
    span.record("rejected", summary.rejected);
    span.record("skipped", summary.skipped);

    tracing::info!(
        discovered = summary.discovered,
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
    #[error("ingest replay error: ingest row {ingest_id} was not found or is not rejected")]
    ReplayNotRejected { ingest_id: Uuid },
}

#[cfg(test)]
mod tests;
