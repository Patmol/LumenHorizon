use uuid::Uuid;

use crate::{
    config::AppConfig,
    db::{
        self, RawBlobRetentionCandidate, RetentionEvent, RetentionEventAction, RetentionEventMode,
        RetentionTargetKind, TileSetRetentionCandidate,
    },
    storage::{BlobStorageClient, DeleteBlobOutcome, StorageError},
    tiles::{latest_manifest_blob_path, manifest_blob_path, TileMathError},
    ui,
};

const RAW_REASON: &str =
    "raw granule exceeds retention policy and requires re-ingest after deletion";
const TILE_SET_REASON: &str =
    "processed tile set exceeds retention policy and is outside latest/prior protection";

pub async fn run_retention_cleanup(
    config: &AppConfig,
    execute: bool,
    cleanup_run_id: Uuid,
) -> Result<(), RetentionError> {
    let mode = if execute {
        RetentionEventMode::Execute
    } else {
        RetentionEventMode::DryRun
    };
    let mode_label = if execute { "execute" } else { "dry-run" };

    ui::status(format_args!(
        "running retention cleanup in {mode_label} mode"
    ));
    tracing::info!(
        cleanup_run_id = %cleanup_run_id,
        mode = mode_label,
        raw_granule_retention_days = config.raw_granule_retention_days,
        processed_tile_set_retention_days = config.processed_tile_set_retention_days,
        retention_protected_prior_tile_sets = config.retention_protected_prior_tile_sets,
        retention_batch_limit = config.retention_batch_limit,
        retention_tile_blob_limit = config.retention_tile_blob_limit,
        "retention cleanup started"
    );

    let pool = db::connect(&config.database_url).await?;
    let blob_client = BlobStorageClient::new(config)?;

    let raw_candidates = db::select_raw_blob_retention_candidates(
        &pool,
        config.raw_granule_retention_days,
        config.retention_batch_limit,
    )
    .await?;
    let tile_set_candidates = db::select_tile_set_retention_candidates(
        &pool,
        config.processed_tile_set_retention_days,
        config.retention_protected_prior_tile_sets,
        config.retention_batch_limit,
    )
    .await?;

    ui::status(format_args!(
        "retention selected {} raw blob(s) and {} tile set(s)",
        raw_candidates.len(),
        tile_set_candidates.len()
    ));

    let mut summary = RetentionSummary::default();

    for candidate in &raw_candidates {
        record_raw_event(
            &pool,
            cleanup_run_id,
            mode,
            candidate,
            RetentionEventAction::Selected,
            RAW_REASON,
            config,
        )
        .await?;

        if execute {
            match blob_client
                .delete_blob(&config.raw_viirs_container, &candidate.blob_path)
                .await?
            {
                DeleteBlobOutcome::Deleted => {
                    summary.raw_deleted += 1;
                    record_raw_event(
                        &pool,
                        cleanup_run_id,
                        mode,
                        candidate,
                        RetentionEventAction::Deleted,
                        RAW_REASON,
                        config,
                    )
                    .await?;
                }
                DeleteBlobOutcome::Missing => {
                    summary.raw_missing += 1;
                    record_raw_event(
                        &pool,
                        cleanup_run_id,
                        mode,
                        candidate,
                        RetentionEventAction::Missing,
                        RAW_REASON,
                        config,
                    )
                    .await?;
                }
            }
        }
    }

    for candidate in &tile_set_candidates {
        validate_manifest_path(candidate)?;
        record_tile_set_event(
            &pool,
            cleanup_run_id,
            mode,
            candidate,
            RetentionEventAction::Selected,
            TILE_SET_REASON,
        )
        .await?;

        if execute {
            match execute_tile_set_cleanup(
                config,
                &pool,
                &blob_client,
                cleanup_run_id,
                mode,
                candidate,
            )
            .await?
            {
                TileSetCleanupOutcome::Deleted => summary.tile_sets_deleted += 1,
                TileSetCleanupOutcome::Skipped => summary.tile_sets_skipped += 1,
            }
        }
    }

    ui::success(format_args!(
        "retention cleanup {mode_label} complete: raw selected={}, raw deleted={}, raw missing={}, tile sets selected={}, tile sets deleted={}, tile sets skipped={}",
        raw_candidates.len(),
        summary.raw_deleted,
        summary.raw_missing,
        tile_set_candidates.len(),
        summary.tile_sets_deleted,
        summary.tile_sets_skipped
    ));
    tracing::info!(
        cleanup_run_id = %cleanup_run_id,
        mode = mode_label,
        raw_selected = raw_candidates.len(),
        raw_deleted = summary.raw_deleted,
        raw_missing = summary.raw_missing,
        tile_sets_selected = tile_set_candidates.len(),
        tile_sets_deleted = summary.tile_sets_deleted,
        tile_sets_skipped = summary.tile_sets_skipped,
        "retention cleanup completed"
    );

    Ok(())
}

async fn execute_tile_set_cleanup(
    config: &AppConfig,
    pool: &sqlx::PgPool,
    blob_client: &BlobStorageClient,
    cleanup_run_id: Uuid,
    mode: RetentionEventMode,
    candidate: &TileSetRetentionCandidate,
) -> Result<TileSetCleanupOutcome, RetentionError> {
    let prefix = tile_set_prefix(&candidate.tile_set_id)?;
    let list_limit = config.retention_tile_blob_limit;
    let tile_blob_page = blob_client
        .list_blobs_with_prefix(&config.processed_tiles_container, &prefix, list_limit)
        .await?;
    let listed_blob_count = tile_blob_page.blob_paths.len();

    if tile_blob_page.has_more || listed_blob_count > config.retention_tile_blob_limit as usize {
        record_tile_set_event(
            pool,
            cleanup_run_id,
            mode,
            candidate,
            RetentionEventAction::Skipped,
            "tile set has more blobs than RETENTION_TILE_BLOB_LIMIT or requires paged deletion",
        )
        .await?;
        tracing::warn!(
            cleanup_run_id = %cleanup_run_id,
            tile_set_id = candidate.tile_set_id,
            listed_blob_count,
            list_has_more = tile_blob_page.has_more,
            retention_tile_blob_limit = config.retention_tile_blob_limit,
            "skipping tile set retention cleanup because prefix listing exceeded tile blob limit"
        );
        return Ok(TileSetCleanupOutcome::Skipped);
    }

    for blob_path in tile_blob_page.blob_paths {
        delete_processed_blob(
            pool,
            blob_client,
            ProcessedBlobDeletion {
                cleanup_run_id,
                mode,
                container_name: &config.processed_tiles_container,
                tile_set_id: &candidate.tile_set_id,
                blob_path: &blob_path,
                target_kind: RetentionTargetKind::ProcessedTile,
            },
        )
        .await?;
    }

    delete_processed_blob(
        pool,
        blob_client,
        ProcessedBlobDeletion {
            cleanup_run_id,
            mode,
            container_name: &config.processed_tiles_container,
            tile_set_id: &candidate.tile_set_id,
            blob_path: &candidate.manifest_blob_path,
            target_kind: RetentionTargetKind::ProcessedManifest,
        },
    )
    .await?;

    db::mark_tile_set_retention_deleted(pool, &candidate.tile_set_id, TILE_SET_REASON).await?;
    record_tile_set_event(
        pool,
        cleanup_run_id,
        mode,
        candidate,
        RetentionEventAction::Deleted,
        TILE_SET_REASON,
    )
    .await?;

    Ok(TileSetCleanupOutcome::Deleted)
}

async fn delete_processed_blob(
    pool: &sqlx::PgPool,
    blob_client: &BlobStorageClient,
    deletion: ProcessedBlobDeletion<'_>,
) -> Result<(), RetentionError> {
    let action = match blob_client
        .delete_blob(deletion.container_name, deletion.blob_path)
        .await?
    {
        DeleteBlobOutcome::Deleted => RetentionEventAction::Deleted,
        DeleteBlobOutcome::Missing => RetentionEventAction::Missing,
    };

    db::record_retention_event(
        pool,
        RetentionEvent {
            cleanup_run_id: deletion.cleanup_run_id,
            mode: deletion.mode,
            target_kind: deletion.target_kind,
            target_identifier: deletion.tile_set_id,
            blob_container: Some(deletion.container_name),
            blob_path: Some(deletion.blob_path),
            action,
            reason: TILE_SET_REASON,
        },
    )
    .await?;

    Ok(())
}

struct ProcessedBlobDeletion<'a> {
    cleanup_run_id: Uuid,
    mode: RetentionEventMode,
    container_name: &'a str,
    tile_set_id: &'a str,
    blob_path: &'a str,
    target_kind: RetentionTargetKind,
}

async fn record_raw_event(
    pool: &sqlx::PgPool,
    cleanup_run_id: Uuid,
    mode: RetentionEventMode,
    candidate: &RawBlobRetentionCandidate,
    action: RetentionEventAction,
    reason: &str,
    config: &AppConfig,
) -> Result<(), RetentionError> {
    db::record_retention_event(
        pool,
        RetentionEvent {
            cleanup_run_id,
            mode,
            target_kind: RetentionTargetKind::RawBlob,
            target_identifier: &candidate.blob_path,
            blob_container: Some(&config.raw_viirs_container),
            blob_path: Some(&candidate.blob_path),
            action,
            reason,
        },
    )
    .await?;

    Ok(())
}

async fn record_tile_set_event(
    pool: &sqlx::PgPool,
    cleanup_run_id: Uuid,
    mode: RetentionEventMode,
    candidate: &TileSetRetentionCandidate,
    action: RetentionEventAction,
    reason: &str,
) -> Result<(), RetentionError> {
    db::record_retention_event(
        pool,
        RetentionEvent {
            cleanup_run_id,
            mode,
            target_kind: RetentionTargetKind::TileSet,
            target_identifier: &candidate.tile_set_id,
            blob_container: None,
            blob_path: None,
            action,
            reason,
        },
    )
    .await?;

    Ok(())
}

fn validate_manifest_path(candidate: &TileSetRetentionCandidate) -> Result<(), RetentionError> {
    if candidate.manifest_blob_path == latest_manifest_blob_path() {
        return Err(RetentionError::LatestManifestSelected {
            tile_set_id: candidate.tile_set_id.clone(),
        });
    }

    let expected = manifest_blob_path(&candidate.tile_set_id)?;
    if candidate.manifest_blob_path != expected {
        return Err(RetentionError::ManifestPathMismatch {
            tile_set_id: candidate.tile_set_id.clone(),
            expected,
            actual: candidate.manifest_blob_path.clone(),
        });
    }

    Ok(())
}

fn tile_set_prefix(tile_set_id: &str) -> Result<String, RetentionError> {
    manifest_blob_path(tile_set_id)?;
    Ok(format!("tiles/{tile_set_id}/"))
}

#[derive(Debug, Default)]
struct RetentionSummary {
    raw_deleted: usize,
    raw_missing: usize,
    tile_sets_deleted: usize,
    tile_sets_skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TileSetCleanupOutcome {
    Deleted,
    Skipped,
}

#[derive(Debug, thiserror::Error)]
pub enum RetentionError {
    #[error(transparent)]
    Database(#[from] db::DbError),
    #[error(
        "retention cleanup tried to select latest manifest pointer for tile set '{tile_set_id}'"
    )]
    LatestManifestSelected { tile_set_id: String },
    #[error(
        "retention cleanup manifest path mismatch for tile set '{tile_set_id}': expected '{expected}', got '{actual}'"
    )]
    ManifestPathMismatch {
        tile_set_id: String,
        expected: String,
        actual: String,
    },
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    TileMath(#[from] TileMathError),
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{tile_set_prefix, validate_manifest_path, RetentionError};
    use crate::db::TileSetRetentionCandidate;

    fn candidate(tile_set_id: &str, manifest_blob_path: &str) -> TileSetRetentionCandidate {
        TileSetRetentionCandidate {
            tile_set_id: tile_set_id.to_owned(),
            classification_version: "radiance-dark-sky-v1".to_owned(),
            manifest_blob_path: manifest_blob_path.to_owned(),
            created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            tile_count: 10,
        }
    }

    #[test]
    fn builds_prefix_for_valid_tile_set() {
        assert_eq!(
            tile_set_prefix("2026-05-21-radiance-dark-sky-v1-a1b2c3d4").unwrap(),
            "tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4/"
        );
    }

    #[test]
    fn rejects_latest_manifest_pointer_as_retention_target() {
        let error = validate_manifest_path(&candidate(
            "2026-05-21-radiance-dark-sky-v1-a1b2c3d4",
            "manifests/latest.json",
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            RetentionError::LatestManifestSelected { .. }
        ));
    }

    #[test]
    fn rejects_manifest_path_mismatch() {
        let error = validate_manifest_path(&candidate(
            "2026-05-21-radiance-dark-sky-v1-a1b2c3d4",
            "manifests/other.json",
        ))
        .unwrap_err();

        assert!(matches!(error, RetentionError::ManifestPathMismatch { .. }));
    }
}
