//! Processing-message orchestration.
//!
//! This module turns a parsed processing queue message into DB state, local HDF
//! input, quality metadata, generated tiles, and published tile manifests.

use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    db, generate, hdf_cli,
    manifest::SourceGranule,
    models, publish, science, storage,
    tiles::{clip_bounds, viirs_tile_bounds, GeographicBounds},
    ui, ServiceError,
};

use super::{
    metadata::{build_quality_metadata, cloud_rejection_reason},
    paths::{local_granule_workspace, LocalGranuleWorkspace},
};

/// Parses and processes one raw processing queue payload.
pub(super) async fn process_message_payload(
    config: &AppConfig,
    message: &str,
    correlation_id: Uuid,
) -> Result<(), ServiceError> {
    let processing_message = models::ProcessingMessage::parse_json(message)?;

    process_parsed_message(config, &processing_message, correlation_id).await
}

/// Processes one parsed queue message through validation, tile generation, and publication.
///
/// The function records the processing attempt, downloads the raw granule,
/// validates product-specific datasets, samples quality metadata, rejects overly
/// cloudy granules, and publishes a tile set for accepted granules.
pub(super) async fn process_parsed_message(
    config: &AppConfig,
    processing_message: &models::ProcessingMessage,
    correlation_id: Uuid,
) -> Result<(), ServiceError> {
    let workspace = local_granule_workspace(processing_message, correlation_id);
    let result = process_parsed_message_with_workspace(
        config,
        processing_message,
        correlation_id,
        workspace.granule_path(),
    )
    .await;

    cleanup_local_workspace(&workspace, processing_message, correlation_id).await;

    result
}

async fn cleanup_local_workspace(
    workspace: &LocalGranuleWorkspace,
    processing_message: &models::ProcessingMessage,
    correlation_id: Uuid,
) {
    match tokio::fs::remove_dir_all(workspace.root()).await {
        Ok(()) => {
            tracing::debug!(
                command_correlation_id = %correlation_id,
                ingest_id = %processing_message.ingest_id,
                local_workspace = %workspace.root().display(),
                "removed local processing workspace"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                command_correlation_id = %correlation_id,
                ingest_id = %processing_message.ingest_id,
                local_workspace = %workspace.root().display(),
                error = %error,
                "failed to remove local processing workspace"
            );
        }
    }
}

async fn process_parsed_message_with_workspace(
    config: &AppConfig,
    processing_message: &models::ProcessingMessage,
    correlation_id: Uuid,
    local_granule_path: &Path,
) -> Result<(), ServiceError> {
    ui::status(format_args!(
        "processing ingest {} ({}, h{:02}v{:02})",
        processing_message.ingest_id,
        processing_message.product,
        processing_message.tile_h,
        processing_message.tile_v
    ));
    tracing::info!(
        command_correlation_id = %correlation_id,
        ingest_id = %processing_message.ingest_id,
        blob_path = processing_message.blob_path,
        product = processing_message.product,
        tile_h = processing_message.tile_h,
        tile_v = processing_message.tile_v,
        "processing message accepted"
    );

    ui::status(format_args!("recording processing start in PostgreSQL"));
    let pool = db::connect(&config.database_url).await?;
    let processing_log = db::upsert_processing_started(&pool, processing_message).await?;
    ui::success(format_args!(
        "recorded processing attempt {} ({})",
        processing_log.attempts, processing_log.id
    ));

    let processing_bounds = match processing_bounds_for_message(config, processing_message) {
        Ok(processing_bounds) => processing_bounds,
        Err(generate::GenerateError::ConfiguredBoundsOutsideSource {
            source_bounds,
            configured_bounds,
        }) => {
            let rejection_reason = generate::GenerateError::ConfiguredBoundsOutsideSource {
                source_bounds,
                configured_bounds,
            }
            .to_string();

            ui::warn(format_args!(
                "rejecting ingest {}: {}",
                processing_message.ingest_id, rejection_reason
            ));
            db::mark_processing_rejected(&pool, processing_message.ingest_id, &rejection_reason)
                .await?;

            tracing::info!(
                command_correlation_id = %correlation_id,
                ingest_id = %processing_message.ingest_id,
                processing_log_id = %processing_log.id,
                source_bounds_west = source_bounds.west,
                source_bounds_south = source_bounds.south,
                source_bounds_east = source_bounds.east,
                source_bounds_north = source_bounds.north,
                configured_bounds_west = configured_bounds.west,
                configured_bounds_south = configured_bounds.south,
                configured_bounds_east = configured_bounds.east,
                configured_bounds_north = configured_bounds.north,
                terminal_status = "rejected",
                "rejected processing message because source bounds do not overlap configured tile bounds"
            );

            return Ok(());
        }
        Err(error) => return Err(ServiceError::from(error)),
    };

    let product = processing_message.product_kind()?;
    let dataset_mapping = science::dataset_mapping_for_product(product);

    // Fail early before downloading data if the runtime image cannot inspect HDF files.
    ui::status(format_args!("checking GDAL runtime"));
    hdf_cli::verify_gdalinfo_available()?;

    if let Some(parent) = local_granule_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|source| {
            storage::StorageError::WriteBlobFile {
                blob_path: processing_message.blob_path.clone(),
                path: parent.to_path_buf(),
                source,
            }
        })?;
    }

    let blob_client = storage::BlobStorageClient::new(config)?;
    ui::status(format_args!(
        "downloading raw blob '{}' from '{}'",
        processing_message.blob_path, config.raw_viirs_container
    ));
    blob_client
        .download_raw_blob_to_path(
            &config.raw_viirs_container,
            &processing_message.blob_path,
            local_granule_path,
        )
        .await?;
    ui::success(format_args!(
        "downloaded raw blob to {}",
        local_granule_path.display()
    ));

    ui::status(format_args!("inspecting HDF datasets"));
    let radiance_shape = hdf_cli::radiance_shape(local_granule_path, dataset_mapping)?;

    let quality_shape =
        hdf_cli::dataset_shape(local_granule_path, dataset_mapping.quality_dataset)?;

    // Daily products carry cloud-mask datasets; monthly products carry observation counts.
    let cloud_shape = match dataset_mapping.cloud_dataset {
        Some(dataset) => Some(hdf_cli::dataset_shape(local_granule_path, dataset)?),
        None => None,
    };

    let observation_count_shape = match dataset_mapping.observation_count_dataset {
        Some(dataset) => Some(hdf_cli::dataset_shape(local_granule_path, dataset)?),
        None => None,
    };

    ui::success(format_args!(
        "HDF datasets ready: radiance {}x{}",
        radiance_shape.width, radiance_shape.height
    ));

    science::validate_matching_shape(
        dataset_mapping.radiance_dataset,
        radiance_shape,
        dataset_mapping.quality_dataset,
        quality_shape,
    )?;

    if let Some(shape) = cloud_shape {
        if let Some(dataset) = dataset_mapping.cloud_dataset {
            science::validate_matching_shape(
                dataset_mapping.radiance_dataset,
                radiance_shape,
                dataset,
                shape,
            )?;
        }
    }

    if let Some(shape) = observation_count_shape {
        if let Some(dataset) = dataset_mapping.observation_count_dataset {
            science::validate_matching_shape(
                dataset_mapping.radiance_dataset,
                radiance_shape,
                dataset,
                shape,
            )?;
        }
    }

    let sample_window = hdf_cli::RasterWindow {
        x_offset: 0,
        y_offset: 0,
        width: 2,
        height: 2,
    };

    // The small sample window is a cheap quality gate before full tile rendering.
    ui::status(format_args!("sampling quality gate"));
    let radiance_samples = hdf_cli::dataset_window_samples(
        local_granule_path,
        dataset_mapping.radiance_dataset,
        sample_window,
    )?;

    let quality_samples = hdf_cli::dataset_window_samples(
        local_granule_path,
        dataset_mapping.quality_dataset,
        sample_window,
    )?;

    let cloud_samples = match dataset_mapping.cloud_dataset {
        Some(dataset) => Some(hdf_cli::dataset_window_samples(
            local_granule_path,
            dataset,
            sample_window,
        )?),
        None => None,
    };

    let observation_count_samples = match dataset_mapping.observation_count_dataset {
        Some(dataset) => Some(hdf_cli::dataset_window_samples(
            local_granule_path,
            dataset,
            sample_window,
        )?),
        None => None,
    };

    science::validate_sample_count(
        dataset_mapping.radiance_dataset,
        sample_window,
        radiance_samples.len(),
    )?;

    science::validate_sample_count(
        dataset_mapping.quality_dataset,
        sample_window,
        quality_samples.len(),
    )?;

    if let Some(samples) = &cloud_samples {
        if let Some(dataset) = dataset_mapping.cloud_dataset {
            science::validate_sample_count(dataset, sample_window, samples.len())?;
        }
    }

    if let Some(samples) = &observation_count_samples {
        if let Some(dataset) = dataset_mapping.observation_count_dataset {
            science::validate_sample_count(dataset, sample_window, samples.len())?;
        }
    }

    let mut sample_classifications = Vec::with_capacity(radiance_samples.len());

    for (index, radiance_sample) in radiance_samples.iter().enumerate() {
        let classification = science::classify_pixel_sample(
            dataset_mapping,
            radiance_sample.value,
            quality_samples[index].value,
            cloud_samples
                .as_ref()
                .and_then(|samples| samples.get(index))
                .map(|sample| sample.value),
            observation_count_samples
                .as_ref()
                .and_then(|samples| samples.get(index))
                .map(|sample| sample.value),
        )?;

        sample_classifications.push(classification);
    }

    let quality_summary = science::summarize_quality(&sample_classifications);
    let exceeds_max_cloud_fraction =
        science::exceeds_max_cloud_fraction(&quality_summary, config.max_cloud_fraction);

    let first_sample_classification = sample_classifications.first();

    let first_dark_sky_class = radiance_samples
        .first()
        .and_then(|sample| science::classify_dark_sky(sample.value));

    let quality_metadata = build_quality_metadata(
        sample_window,
        &quality_summary,
        config.max_cloud_fraction,
        exceeds_max_cloud_fraction,
        first_dark_sky_class,
    );

    db::update_processing_metadata(
        &pool,
        processing_log.id,
        quality_metadata,
        f64::from(quality_summary.cloud_fraction),
        quality_summary.valid_pixel_count as i64,
        quality_summary.rejected_pixel_count as i64,
    )
    .await?;

    ui::success(format_args!(
        "quality gate sampled {} pixel(s): valid={}, rejected={}, cloud_fraction={:.3}",
        quality_summary.total_pixel_count,
        quality_summary.valid_pixel_count,
        quality_summary.rejected_pixel_count,
        quality_summary.cloud_fraction
    ));

    if let Some(rejection_reason) =
        cloud_rejection_reason(&quality_summary, config.max_cloud_fraction)
    {
        // Rejected granules keep quality metadata but do not publish tile artifacts.
        ui::warn(format_args!(
            "rejecting ingest {}: {}",
            processing_message.ingest_id, rejection_reason
        ));
        db::mark_processing_rejected(&pool, processing_message.ingest_id, rejection_reason).await?;

        tracing::info!(
            command_correlation_id = %correlation_id,
            ingest_id = %processing_message.ingest_id,
            processing_log_id = %processing_log.id,
            quality_cloud_fraction = quality_summary.cloud_fraction,
            valid_pixel_count = quality_summary.valid_pixel_count,
            rejected_pixel_count = quality_summary.rejected_pixel_count,
            max_cloud_fraction = config.max_cloud_fraction,
            terminal_status = "rejected",
            "rejected processing message because cloud fraction exceeds configured maximum"
        );

        return Ok(());
    }

    tracing::info!(
        command_correlation_id = %correlation_id,
        ingest_id = %processing_message.ingest_id,
        processing_log_id = %processing_log.id,
        processing_attempts = processing_log.attempts,
        blob_path = processing_message.blob_path,
        product = processing_message.product,
        tile_h = processing_message.tile_h,
        tile_v = processing_message.tile_v,
        science_cadence = ?dataset_mapping.cadence,
        radiance_dataset = dataset_mapping.radiance_dataset,
        quality_dataset = dataset_mapping.quality_dataset,
        cloud_dataset = dataset_mapping.cloud_dataset.unwrap_or("none"),
        observation_count_dataset = dataset_mapping
            .observation_count_dataset
            .unwrap_or("none"),
        local_granule_path = %local_granule_path.display(),
        radiance_width = radiance_shape.width,
        radiance_height = radiance_shape.height,
        quality_width = quality_shape.width,
        quality_height = quality_shape.height,
        cloud_width = cloud_shape.map(|shape| shape.width),
        cloud_height = cloud_shape.map(|shape| shape.height),
        observation_count_width = observation_count_shape.map(|shape| shape.width),
        observation_count_height = observation_count_shape.map(|shape| shape.height),
        sample_window_x = sample_window.x_offset,
        sample_window_y = sample_window.y_offset,
        sample_window_width = sample_window.width,
        sample_window_height = sample_window.height,
        radiance_sample_count = radiance_samples.len(),
        quality_sample_count = quality_samples.len(),
        cloud_sample_count = cloud_samples.as_ref().map(Vec::len),
        observation_count_sample_count = observation_count_samples.as_ref().map(Vec::len),
        radiance_first_sample = ?radiance_samples.first(),
        quality_first_sample = ?quality_samples.first(),
        cloud_first_sample = ?cloud_samples.as_ref().and_then(|samples| samples.first()),
        radiance_sample_quality = ?first_sample_classification.map(|classification| classification.radiance_quality),
        quality_sample_value = first_sample_classification.map(|classification| classification.quality_sample),
        quality_sample_quality = ?first_sample_classification.map(|classification| classification.quality_mask_quality),
        cloud_contaminated_sample = first_sample_classification.and_then(|classification| classification.cloud_contaminated),
        observation_count_sample = first_sample_classification.and_then(|classification| classification.observation_count_sample),
        classified_sample_count = sample_classifications.len(),
        observation_count_first_sample = ?observation_count_samples
            .as_ref()
            .and_then(|samples| samples.first()),
        quality_rule_version = science::QUALITY_RULE_VERSION,
        quality_total_pixel_count = quality_summary.total_pixel_count,
        quality_valid_pixel_count = quality_summary.valid_pixel_count,
        quality_rejected_pixel_count = quality_summary.rejected_pixel_count,
        quality_cloud_contaminated_valid_pixel_count =
            quality_summary.cloud_contaminated_valid_pixel_count,
        quality_cloud_fraction = quality_summary.cloud_fraction,
        max_cloud_fraction = config.max_cloud_fraction,
        exceeds_max_cloud_fraction,
        dark_sky_classification_version = science::DARK_SKY_CLASSIFICATION_VERSION,
        dark_sky_first_sample_class = first_dark_sky_class.map(|classification| classification.class),
        dark_sky_first_sample_color = first_dark_sky_class.map(|classification| classification.color_hex),
        dark_sky_first_sample_label = first_dark_sky_class.map(|classification| classification.label),
        "validated processing message and recorded processing start"
    );

    let tile_set_id = tile_set_id_for_message(config, processing_message, processing_log.attempts);
    let source_granules = vec![source_granule_for_message(processing_message)];
    let processor_version = processor_version();

    ui::status(format_args!(
        "preparing tile generation for tile set {}",
        tile_set_id
    ));
    tracing::info!(
        command_correlation_id = %correlation_id,
        ingest_id = %processing_message.ingest_id,
        processing_log_id = %processing_log.id,
        tile_set_id = %tile_set_id,
        source_bounds_west = processing_bounds.source_bounds.west,
        source_bounds_south = processing_bounds.source_bounds.south,
        source_bounds_east = processing_bounds.source_bounds.east,
        source_bounds_north = processing_bounds.source_bounds.north,
        configured_bounds_west = processing_bounds.configured_bounds.west,
        configured_bounds_south = processing_bounds.configured_bounds.south,
        configured_bounds_east = processing_bounds.configured_bounds.east,
        configured_bounds_north = processing_bounds.configured_bounds.north,
        generation_bounds_west = processing_bounds.generation_bounds.west,
        generation_bounds_south = processing_bounds.generation_bounds.south,
        generation_bounds_east = processing_bounds.generation_bounds.east,
        generation_bounds_north = processing_bounds.generation_bounds.north,
        "preparing tile generation for processing message"
    );

    let tile_set =
        generate::generate_tile_set_for_granule_with_manifest(generate::GranuleTileSetRequest {
            config,
            granule_path: local_granule_path,
            mapping: dataset_mapping,
            tile_set_id,
            dataset_date: processing_message.granule_date.date_naive(),
            generated_at: Utc::now(),
            processor_version,
            source_bounds: processing_bounds.source_bounds,
            raster_shape: radiance_shape,
            source_granules,
        })
        .await?;
    ui::success(format_args!(
        "generated {} tile(s) for {}",
        tile_set.tiles.len(),
        tile_set.manifest.tile_set_id
    ));

    ui::status(format_args!(
        "publishing intermediate tile set {} to '{}'",
        tile_set.manifest.tile_set_id, config.processed_tiles_container
    ));
    let publication = publish::publish_generated_tile_set(
        config,
        &pool,
        &blob_client,
        &tile_set,
        publish::PublicationMode::IntermediateGranule {
            product: processing_message.product.as_str(),
        },
        Utc::now(),
    )
    .await?;
    ui::success(format_args!(
        "published intermediate tile set {}; latest pointer unchanged",
        tile_set.manifest.tile_set_id
    ));

    ui::status(format_args!("marking processing row as processed"));
    db::mark_processing_processed_with_tile_set(
        &pool,
        processing_message.ingest_id,
        &tile_set.manifest.tile_set_id,
    )
    .await?;
    ui::success(format_args!(
        "processing complete for ingest {}",
        processing_message.ingest_id
    ));

    tracing::info!(
        command_correlation_id = %correlation_id,
        ingest_id = %processing_message.ingest_id,
        processing_log_id = %processing_log.id,
        tile_set_id = tile_set.manifest.tile_set_id,
        tile_count = tile_set.tiles.len(),
        cloud_fraction = quality_summary.cloud_fraction,
        valid_pixel_count = quality_summary.valid_pixel_count,
        rejected_pixel_count = quality_summary.rejected_pixel_count,
        manifest_sha256 = tile_set.manifest.checksums.manifest_sha256,
        public_latest_promoted = publication.public_latest_pointer.is_some(),
        product_latest_promoted = publication.product_latest_pointer.is_some(),
        terminal_status = "processed",
        "generated and published intermediate tile set for processing message"
    );

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ProcessingBounds {
    source_bounds: GeographicBounds,
    configured_bounds: GeographicBounds,
    generation_bounds: GeographicBounds,
}

fn processing_bounds_for_message(
    config: &AppConfig,
    processing_message: &models::ProcessingMessage,
) -> Result<ProcessingBounds, generate::GenerateError> {
    let source_bounds = viirs_tile_bounds(processing_message.tile_h, processing_message.tile_v)
        .map_err(generate::GenerateError::from)?;
    let configured_bounds = GeographicBounds::from(config.tile_bounds);
    let generation_bounds = clip_bounds(source_bounds, configured_bounds).ok_or(
        generate::GenerateError::ConfiguredBoundsOutsideSource {
            source_bounds,
            configured_bounds,
        },
    )?;

    Ok(ProcessingBounds {
        source_bounds,
        configured_bounds,
        generation_bounds,
    })
}

/// Builds a stable tile-set identifier for one processing attempt.
///
/// The identifier includes the granule date, classification version, an ingest
/// prefix, and the processing attempt so retries produce distinct artifacts.
fn tile_set_id_for_message(
    config: &AppConfig,
    processing_message: &models::ProcessingMessage,
    attempt: i32,
) -> String {
    let ingest_id = processing_message.ingest_id.simple().to_string();
    let ingest_prefix = &ingest_id[..8];

    format!(
        "{}-{}-{}-a{}",
        processing_message.granule_date.date_naive(),
        config.tile_classification_version,
        ingest_prefix,
        attempt.max(1)
    )
}

/// Converts the processing queue contract into the manifest source-granule shape.
fn source_granule_for_message(processing_message: &models::ProcessingMessage) -> SourceGranule {
    SourceGranule {
        ingest_id: processing_message.ingest_id,
        product: processing_message.product.clone(),
        blob_path: processing_message.blob_path.clone(),
    }
}

/// Returns the processor identifier recorded in tile manifests.
fn processor_version() -> String {
    format!("processing-svc:{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use super::*;
    use crate::config::TileBounds;

    fn test_config() -> AppConfig {
        AppConfig {
            rust_log: "processing_svc=debug".to_owned(),
            database_url: "postgres://localhost/lumenhorizon".to_owned(),
            azure_storage_account: "devstoreaccount1".to_owned(),
            azure_storage_access_key: "test-key".to_owned(),
            azure_storage_emulator_host: Some("127.0.0.1".to_owned()),
            azure_queue_name: "viirs-processing".to_owned(),
            azure_deadletter_queue_name: "viirs-processing-deadletter".to_owned(),
            raw_viirs_container: "raw-viirs".to_owned(),
            processed_tiles_container: "processed-tiles".to_owned(),
            max_cloud_fraction: 0.4,
            processing_visibility_timeout_seconds: 900,
            processing_max_dequeue_count: 5,
            processing_max_parallelism: 1,
            http_request_timeout: std::time::Duration::from_secs(30),
            http_retry: shared::http_retry::RetryConfig {
                max_attempts: 3,
                base_delay: std::time::Duration::from_millis(250),
                max_delay: std::time::Duration::from_millis(5_000),
            },
            tile_min_zoom: 3,
            tile_max_native_zoom: 10,
            tile_max_display_zoom: 12,
            tile_size: 256,
            tile_format: "png".to_owned(),
            tile_classification_version: "radiance-dark-sky-v1".to_owned(),
            tile_render_version: "tiles-v1".to_owned(),
            tile_cdn_base_url: "https://tiles.lumenhorizon.com".to_owned(),
            tile_bounds: TileBounds {
                west: -125.0,
                south: 24.0,
                east: -66.0,
                north: 50.0,
            },
            tile_immutable_cache_control: "public, max-age=31536000, immutable".to_owned(),
            tile_latest_cache_control: "public, max-age=300, must-revalidate".to_owned(),
            raw_granule_retention_days: 90,
            processed_tile_set_retention_days: 180,
            retention_protected_prior_tile_sets: 2,
            retention_batch_limit: 500,
            retention_tile_blob_limit: 5_000,
        }
    }

    fn processing_message() -> models::ProcessingMessage {
        models::ProcessingMessage::new(
            Uuid::parse_str("00000000-0000-0000-0000-00000000abcd").unwrap(),
            "VNP46A2/2026-05-21/h11v06.h5",
            "VNP46A2",
            Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
            11,
            6,
        )
        .unwrap()
    }

    fn processing_message_for_tile(tile_h: i16, tile_v: i16) -> models::ProcessingMessage {
        models::ProcessingMessage::new(
            Uuid::parse_str("00000000-0000-0000-0000-00000000abcd").unwrap(),
            format!("VNP46A2/2026-05-21/h{tile_h:02}v{tile_v:02}.h5"),
            "VNP46A2",
            Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
            tile_h,
            tile_v,
        )
        .unwrap()
    }

    #[test]
    fn tile_set_id_is_deterministic_and_attempt_scoped() {
        let id = tile_set_id_for_message(&test_config(), &processing_message(), 2);

        assert_eq!(id, "2026-05-21-radiance-dark-sky-v1-00000000-a2");
    }

    #[test]
    fn processing_bounds_rejects_boundary_only_source_bounds() {
        let config = test_config();
        let message = processing_message_for_tile(6, 3);
        let error = processing_bounds_for_message(&config, &message).unwrap_err();

        assert!(matches!(
            error,
            generate::GenerateError::ConfiguredBoundsOutsideSource {
                source_bounds,
                configured_bounds,
            } if source_bounds == GeographicBounds {
                west: -120.0,
                south: 50.0,
                east: -110.0,
                north: 60.0,
            } && configured_bounds == GeographicBounds::from(config.tile_bounds)
        ));
    }

    #[test]
    fn processing_bounds_returns_clipped_generation_bounds_for_overlap() {
        let bounds = processing_bounds_for_message(&test_config(), &processing_message()).unwrap();

        assert_eq!(
            bounds,
            ProcessingBounds {
                source_bounds: GeographicBounds {
                    west: -70.0,
                    south: 20.0,
                    east: -60.0,
                    north: 30.0,
                },
                configured_bounds: GeographicBounds {
                    west: -125.0,
                    south: 24.0,
                    east: -66.0,
                    north: 50.0,
                },
                generation_bounds: GeographicBounds {
                    west: -70.0,
                    south: 24.0,
                    east: -66.0,
                    north: 30.0,
                },
            }
        );
    }

    #[test]
    fn processing_bounds_rejects_invalid_viirs_tile_coordinates() {
        let message = processing_message_for_tile(36, 3);
        let error = processing_bounds_for_message(&test_config(), &message).unwrap_err();

        assert!(matches!(
            error,
            generate::GenerateError::TileMath(crate::tiles::TileMathError::InvalidViirsTile {
                tile_h: 36,
                tile_v: 3,
            })
        ));
    }

    #[test]
    fn source_granule_uses_processing_message_contract() {
        let message = processing_message();
        let source_granule = source_granule_for_message(&message);

        assert_eq!(source_granule.ingest_id, message.ingest_id);
        assert_eq!(source_granule.product, "VNP46A2");
        assert_eq!(source_granule.blob_path, "VNP46A2/2026-05-21/h11v06.h5");
    }

    #[test]
    fn processor_version_uses_package_version() {
        assert_eq!(
            processor_version(),
            format!("processing-svc:{}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[tokio::test]
    async fn cleanup_local_workspace_removes_directory() {
        let workspace = local_granule_workspace(&processing_message(), test_correlation_id());
        tokio::fs::create_dir_all(workspace.root()).await.unwrap();
        tokio::fs::write(workspace.granule_path(), b"test")
            .await
            .unwrap();

        cleanup_local_workspace(&workspace, &processing_message(), test_correlation_id()).await;

        assert!(!workspace.root().exists());
    }

    #[tokio::test]
    async fn cleanup_local_workspace_ignores_missing_directory() {
        let workspace = local_granule_workspace(&processing_message(), test_correlation_id());

        cleanup_local_workspace(&workspace, &processing_message(), test_correlation_id()).await;
    }

    fn test_correlation_id() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-00000000c0de").unwrap()
    }
}
