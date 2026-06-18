use chrono::{DateTime, TimeZone, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::earthdata::EarthdataError;
use crate::models::{
    GranuleCandidate, TileCoordinate, INGEST_STATUS_DOWNLOADED, INGEST_STATUS_DOWNLOADING,
    INGEST_STATUS_ENQUEUED, INGEST_STATUS_FAILED, INGEST_STATUS_REJECTED,
    INGEST_STATUS_REPLAY_PENDING, INGEST_STATUS_VALIDATED,
};

use super::failures::IngestFailureContext;
use super::ingest_log::{
    get_discovery_resume_points_for_products, insert_downloading_row, mark_downloaded,
    mark_enqueued, mark_failed, mark_outbox_completed, mark_outbox_failed, mark_rejected,
    mark_validated, pending_enqueue_outbox, replay_rejected, truncate_error_message,
    InsertDownloadingRowOutcome,
};
use super::summary::{GranuleProcessingOutcome, IngestSummary};
use super::validation::{validate_raw_granule_bytes, RawGranuleValidationError};

fn candidate_with_identity(
    product: &str,
    h: u8,
    v: u8,
    granule_date: DateTime<Utc>,
) -> GranuleCandidate {
    let title = format!(
        "{product}.A{}.h{h:02}v{v:02}.002.{}000000.h5",
        granule_date.format("%Y%j"),
        granule_date.format("%Y%j")
    );

    GranuleCandidate {
        product: product.to_owned(),
        title: title.clone(),
        producer_granule_id: title,
        data_href: "https://archive.example.test/file.h5".to_owned(),
        granule_date,
        tile: TileCoordinate { h, v },
    }
}

fn sample_candidate() -> GranuleCandidate {
    candidate_with_identity(
        "VNP46A2",
        11,
        6,
        Utc.with_ymd_and_hms(2024, 5, 21, 0, 0, 0).unwrap(),
    )
}

async fn insert_sample_row(pool: &PgPool) -> Result<Uuid, sqlx::Error> {
    let candidate = sample_candidate();
    let blob_path = candidate.raw_blob_path();

    match insert_downloading_row(pool, &candidate, &blob_path).await? {
        InsertDownloadingRowOutcome::Created(ingest_id) => Ok(ingest_id),
        InsertDownloadingRowOutcome::AlreadyExists => {
            panic!("sample row should not already exist in an isolated SQLx test database")
        }
    }
}

async fn logical_row_count(
    pool: &PgPool,
    candidate: &GranuleCandidate,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT count(*)
        FROM ingest_log
        WHERE product = $1
          AND tile_h = $2
          AND tile_v = $3
          AND granule_date = $4
        "#,
    )
    .bind(&candidate.product)
    .bind(i16::from(candidate.tile.h))
    .bind(i16::from(candidate.tile.v))
    .bind(candidate.granule_date)
    .fetch_one(pool)
    .await
}

async fn status_and_error(
    pool: &PgPool,
    ingest_id: Uuid,
) -> Result<(String, Option<String>), sqlx::Error> {
    sqlx::query_as::<_, (String, Option<String>)>(
        r#"
        SELECT status, error_message
        FROM ingest_log
        WHERE id = $1
        "#,
    )
    .bind(ingest_id)
    .fetch_one(pool)
    .await
}

async fn insert_row_with_status(
    pool: &PgPool,
    status: &str,
    granule_date: DateTime<Utc>,
    tile_h: i16,
    tile_v: i16,
) -> Result<(), sqlx::Error> {
    insert_product_row_with_status(pool, "VNP46A2", status, granule_date, tile_h, tile_v).await
}

async fn insert_product_row_with_status(
    pool: &PgPool,
    product: &str,
    status: &str,
    granule_date: DateTime<Utc>,
    tile_h: i16,
    tile_v: i16,
) -> Result<(), sqlx::Error> {
    let title = format!(
        "{product}.A{}.h{:02}v{:02}.002.{}000000.h5",
        granule_date.format("%Y%j"),
        tile_h,
        tile_v,
        granule_date.format("%Y%j")
    );
    let blob_path = format!(
        "{product}/{}/h{:02}v{:02}.h5",
        granule_date.format("%Y-%m-%d"),
        tile_h,
        tile_v
    );

    sqlx::query(
        r#"
        INSERT INTO ingest_log (
            id,
            product,
            granule_title,
            blob_path,
            tile_h,
            tile_v,
            granule_date,
            status,
            error_message
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(product)
    .bind(title)
    .bind(blob_path)
    .bind(tile_h)
    .bind(tile_v)
    .bind(granule_date)
    .bind(status)
    .execute(pool)
    .await?;

    Ok(())
}

#[test]
fn truncates_long_error_messages() {
    let message = "x".repeat(1_200);

    let truncated = truncate_error_message(&message);

    assert_eq!(truncated.len(), 1000);
}

#[test]
fn keeps_short_error_messages_unchanged() {
    let message = "HTTP 401 from Earthdata";

    assert_eq!(truncate_error_message(message), message);
}

#[test]
fn truncates_error_messages_on_character_boundaries() {
    let message = "é".repeat(1_200);

    let truncated = truncate_error_message(&message);

    assert_eq!(truncated.chars().count(), 1000);
    assert!(truncated.is_char_boundary(truncated.len()));
}

#[test]
fn records_granule_processing_outcomes_in_summary() {
    let mut summary = IngestSummary::new(5);

    summary.record_attempt(GranuleProcessingOutcome::Enqueued);
    summary.record_attempt(GranuleProcessingOutcome::RejectedAfterDownloaded);
    summary.record_attempt(GranuleProcessingOutcome::FailedAfterValidated);
    summary.record_attempt(GranuleProcessingOutcome::FailedBeforeDownloaded);
    summary.record_attempt(GranuleProcessingOutcome::Skipped);

    assert_eq!(summary.discovered, 5);
    assert_eq!(summary.attempted, 5);
    assert_eq!(summary.downloaded, 3);
    assert_eq!(summary.validated, 2);
    assert_eq!(summary.enqueued, 1);
    assert_eq!(summary.rejected, 1);
    assert_eq!(summary.failed, 2);
    assert_eq!(summary.skipped, 1);
}

#[test]
fn accepts_hdf5_magic_bytes() {
    let bytes = b"\x89HDF\r\n\x1A\nextra content";

    assert_eq!(validate_raw_granule_bytes(bytes), Ok(()));
}

#[test]
fn rejects_short_raw_granule_bytes() {
    let error = validate_raw_granule_bytes(b"short").unwrap_err();

    assert_eq!(
        error,
        RawGranuleValidationError::TooSmall {
            actual_size: 5,
            minimum_size: 8
        }
    );
}

#[test]
fn rejects_bytes_without_hdf5_magic() {
    let error = validate_raw_granule_bytes(b"not-hdf5-content").unwrap_err();

    assert_eq!(error, RawGranuleValidationError::MissingHdf5Magic);
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn duplicate_logical_granule_is_skipped(pool: PgPool) -> Result<(), sqlx::Error> {
    let candidate = sample_candidate();
    let blob_path = candidate.raw_blob_path();

    let first_insert = insert_downloading_row(&pool, &candidate, &blob_path).await?;
    let created_id = match first_insert {
        InsertDownloadingRowOutcome::Created(ingest_id) => ingest_id,
        InsertDownloadingRowOutcome::AlreadyExists => {
            panic!("first insert should create a row")
        }
    };

    let second_insert = insert_downloading_row(&pool, &candidate, &blob_path).await?;

    assert_eq!(second_insert, InsertDownloadingRowOutcome::AlreadyExists);
    assert_eq!(logical_row_count(&pool, &candidate).await?, 1);

    let row = sqlx::query(
        r#"
        SELECT id, status, blob_path
        FROM ingest_log
        WHERE product = $1
          AND tile_h = $2
          AND tile_v = $3
          AND granule_date = $4
        "#,
    )
    .bind(&candidate.product)
    .bind(i16::from(candidate.tile.h))
    .bind(i16::from(candidate.tile.v))
    .bind(candidate.granule_date)
    .fetch_one(&pool)
    .await?;

    assert_eq!(row.try_get::<Uuid, _>("id")?, created_id);
    assert_eq!(
        row.try_get::<String, _>("status")?,
        INGEST_STATUS_DOWNLOADING
    );
    assert_eq!(row.try_get::<String, _>("blob_path")?, blob_path);

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn duplicate_logical_granule_preserves_existing_record(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let candidate = sample_candidate();
    let original_blob_path = candidate.raw_blob_path();
    let original_title = candidate.title.clone();

    let first_insert = insert_downloading_row(&pool, &candidate, &original_blob_path).await?;
    assert!(matches!(
        first_insert,
        InsertDownloadingRowOutcome::Created(_)
    ));

    let mut rediscovered_candidate = candidate.clone();
    rediscovered_candidate.title = "VNP46A2.A2024142.h11v06.rediscovered.h5".to_owned();
    rediscovered_candidate.producer_granule_id = rediscovered_candidate.title.clone();
    let rediscovered_blob_path = "VNP46A2/2024-05-21/h11v06-rediscovered.h5";

    let second_insert =
        insert_downloading_row(&pool, &rediscovered_candidate, rediscovered_blob_path).await?;

    assert_eq!(second_insert, InsertDownloadingRowOutcome::AlreadyExists);
    assert_eq!(logical_row_count(&pool, &candidate).await?, 1);

    let row = sqlx::query(
        r#"
        SELECT granule_title, blob_path
        FROM ingest_log
        WHERE product = $1
          AND tile_h = $2
          AND tile_v = $3
          AND granule_date = $4
        "#,
    )
    .bind(&candidate.product)
    .bind(i16::from(candidate.tile.h))
    .bind(i16::from(candidate.tile.v))
    .bind(candidate.granule_date)
    .fetch_one(&pool)
    .await?;

    assert_eq!(row.try_get::<String, _>("granule_title")?, original_title);
    assert_eq!(row.try_get::<String, _>("blob_path")?, original_blob_path);

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn distinct_logical_granules_are_inserted(pool: PgPool) -> Result<(), sqlx::Error> {
    let first_candidate = sample_candidate();
    let second_candidate = candidate_with_identity(
        "VNP46A2",
        11,
        6,
        Utc.with_ymd_and_hms(2024, 5, 22, 0, 0, 0).unwrap(),
    );

    let first_insert =
        insert_downloading_row(&pool, &first_candidate, &first_candidate.raw_blob_path()).await?;
    let second_insert =
        insert_downloading_row(&pool, &second_candidate, &second_candidate.raw_blob_path()).await?;

    assert!(matches!(
        first_insert,
        InsertDownloadingRowOutcome::Created(_)
    ));
    assert!(matches!(
        second_insert,
        InsertDownloadingRowOutcome::Created(_)
    ));

    let row_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT count(*)
        FROM ingest_log
        WHERE product = $1
          AND tile_h = $2
          AND tile_v = $3
        "#,
    )
    .bind(&first_candidate.product)
    .bind(i16::from(first_candidate.tile.h))
    .bind(i16::from(first_candidate.tile.v))
    .fetch_one(&pool)
    .await?;

    assert_eq!(row_count, 2);

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn success_status_transitions_are_explicit(pool: PgPool) -> Result<(), sqlx::Error> {
    let ingest_id = insert_sample_row(&pool).await?;

    mark_downloaded(&pool, ingest_id).await?;
    assert_eq!(
        status_and_error(&pool, ingest_id).await?,
        (INGEST_STATUS_DOWNLOADED.to_owned(), None)
    );

    mark_validated(&pool, ingest_id).await?;
    assert_eq!(
        status_and_error(&pool, ingest_id).await?,
        (INGEST_STATUS_VALIDATED.to_owned(), None)
    );

    mark_enqueued(&pool, ingest_id).await?;
    assert_eq!(
        status_and_error(&pool, ingest_id).await?,
        (INGEST_STATUS_ENQUEUED.to_owned(), None)
    );

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn failure_statuses_record_errors_and_success_clears_them(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let ingest_id = insert_sample_row(&pool).await?;

    mark_failed(&pool, ingest_id, "Earthdata HTTP 401").await?;
    assert_eq!(
        status_and_error(&pool, ingest_id).await?,
        (
            INGEST_STATUS_FAILED.to_owned(),
            Some("Earthdata HTTP 401".to_owned())
        )
    );

    mark_downloaded(&pool, ingest_id).await?;
    assert_eq!(
        status_and_error(&pool, ingest_id).await?,
        (INGEST_STATUS_DOWNLOADED.to_owned(), None)
    );

    mark_rejected(&pool, ingest_id, "missing HDF5 magic").await?;
    assert_eq!(
        status_and_error(&pool, ingest_id).await?,
        (
            INGEST_STATUS_REJECTED.to_owned(),
            Some("missing HDF5 magic".to_owned())
        )
    );

    mark_validated(&pool, ingest_id).await?;
    assert_eq!(
        status_and_error(&pool, ingest_id).await?,
        (INGEST_STATUS_VALIDATED.to_owned(), None)
    );

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn failure_statuses_persist_contextual_error_messages(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let ingest_id = insert_sample_row(&pool).await?;
    let failure = IngestFailureContext::earthdata_download(&EarthdataError::Status {
        granule_title: "VNP46A2.A2024142.h11v06.002.h5".to_owned(),
        status: reqwest::StatusCode::FORBIDDEN,
    });

    mark_failed(&pool, ingest_id, &failure.database_message()).await?;

    assert_eq!(
        status_and_error(&pool, ingest_id).await?,
        (
            INGEST_STATUS_FAILED.to_owned(),
            Some(
                "phase=download; code=earthdata_http_status; category=auth; retry_eligible=false; http_status=403; message=Earthdata returned HTTP 403 Forbidden for granule VNP46A2.A2024142.h11v06.002.h5"
                    .to_owned()
            )
        )
    );

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn failed_status_truncates_contextual_error_messages(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let ingest_id = insert_sample_row(&pool).await?;
    let long_message = format!(
        "phase=upload; code=storage_upload_http_status; category=upstream; retry_eligible=true; http_status=503; message={}",
        "x".repeat(1_200)
    );

    mark_failed(&pool, ingest_id, &long_message).await?;

    let (_status, error_message) = status_and_error(&pool, ingest_id).await?;
    let error_message = error_message.expect("failure should persist an error message");

    assert_eq!(error_message.chars().count(), 1000);
    assert!(error_message.starts_with("phase=upload; code=storage_upload_http_status"));

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn resume_point_is_none_when_no_rows_exist(pool: PgPool) -> Result<(), sqlx::Error> {
    let products = vec!["VNP46A2".to_owned()];
    let resume_points = get_discovery_resume_points_for_products(&pool, &products).await?;

    assert!(resume_points.is_empty());

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn resume_point_uses_latest_non_failed_granule_date_by_product(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let downloaded_date = Utc.with_ymd_and_hms(2024, 5, 20, 0, 0, 0).unwrap();
    let rejected_date = Utc.with_ymd_and_hms(2024, 5, 22, 0, 0, 0).unwrap();
    let failed_date = Utc.with_ymd_and_hms(2024, 5, 30, 0, 0, 0).unwrap();

    insert_row_with_status(&pool, INGEST_STATUS_DOWNLOADED, downloaded_date, 11, 6).await?;
    insert_row_with_status(&pool, INGEST_STATUS_REJECTED, rejected_date, 12, 6).await?;
    insert_row_with_status(&pool, INGEST_STATUS_FAILED, failed_date, 13, 6).await?;

    let products = vec!["VNP46A2".to_owned()];
    let resume_points = get_discovery_resume_points_for_products(&pool, &products).await?;

    assert_eq!(resume_points.get("VNP46A2"), Some(&downloaded_date));

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn rejected_replay_marks_row_pending_and_creates_single_outbox(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let ingest_id = insert_sample_row(&pool).await?;
    mark_rejected(&pool, ingest_id, "phase=validation; code=old_rule").await?;

    assert!(replay_rejected(&pool, ingest_id, "operator_replay").await?);
    assert!(!replay_rejected(&pool, ingest_id, "operator_replay").await?);

    let (status, error_message) = status_and_error(&pool, ingest_id).await?;
    assert_eq!(status, INGEST_STATUS_REPLAY_PENDING);
    assert_eq!(error_message, None);

    let outbox = pending_enqueue_outbox(&pool).await?;
    assert_eq!(outbox.len(), 1);
    assert_eq!(outbox[0].ingest_id, ingest_id);
    assert_eq!(outbox[0].blob_path, "VNP46A2/2024-05-21/h11v06.h5");

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn completed_outbox_is_not_reselected(pool: PgPool) -> Result<(), sqlx::Error> {
    let ingest_id = insert_sample_row(&pool).await?;
    mark_validated(&pool, ingest_id).await?;
    let outbox_id =
        super::ingest_log::create_pending_enqueue_outbox(&pool, ingest_id, "test").await?;

    assert_eq!(pending_enqueue_outbox(&pool).await?.len(), 1);

    mark_enqueued(&pool, ingest_id).await?;
    mark_outbox_completed(&pool, outbox_id).await?;

    assert!(pending_enqueue_outbox(&pool).await?.is_empty());

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn outbox_failure_becomes_terminal_after_max_attempts(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let ingest_id = insert_sample_row(&pool).await?;
    mark_validated(&pool, ingest_id).await?;
    let outbox_id =
        super::ingest_log::create_pending_enqueue_outbox(&pool, ingest_id, "test").await?;

    mark_outbox_failed(&pool, outbox_id, "temporary queue outage").await?;
    mark_outbox_failed(&pool, outbox_id, "temporary queue outage").await?;

    assert_eq!(pending_enqueue_outbox(&pool).await?.len(), 1);

    mark_outbox_failed(&pool, outbox_id, "invalid blob path").await?;

    let (status, attempts, error_message, completed_at) =
        sqlx::query_as::<_, (String, i32, Option<String>, Option<DateTime<Utc>>)>(
            r#"
        SELECT status, attempts, error_message, completed_at
        FROM ingest_recovery_outbox
        WHERE id = $1
        "#,
        )
        .bind(outbox_id)
        .fetch_one(&pool)
        .await?;

    assert_eq!(status, "failed");
    assert_eq!(attempts, 3);
    assert_eq!(error_message.as_deref(), Some("invalid blob path"));
    assert!(completed_at.is_some());
    assert!(pending_enqueue_outbox(&pool).await?.is_empty());

    Ok(())
}

#[sqlx::test(migrations = "../db-migrate/migrations")]
async fn resume_points_are_independent_per_product(pool: PgPool) -> Result<(), sqlx::Error> {
    let vnp46a2_date = Utc.with_ymd_and_hms(2024, 5, 22, 0, 0, 0).unwrap();
    let vj146a2_date = Utc.with_ymd_and_hms(2024, 5, 18, 0, 0, 0).unwrap();
    let vnp46a3_date = Utc.with_ymd_and_hms(2024, 4, 1, 0, 0, 0).unwrap();

    insert_product_row_with_status(
        &pool,
        "VNP46A2",
        INGEST_STATUS_ENQUEUED,
        vnp46a2_date,
        11,
        6,
    )
    .await?;
    insert_product_row_with_status(
        &pool,
        "VJ146A2",
        INGEST_STATUS_ENQUEUED,
        vj146a2_date,
        11,
        6,
    )
    .await?;
    insert_product_row_with_status(
        &pool,
        "VNP46A3",
        INGEST_STATUS_ENQUEUED,
        vnp46a3_date,
        11,
        6,
    )
    .await?;

    let daily_products = vec!["VNP46A2".to_owned(), "VJ146A2".to_owned()];
    let daily_resume_points =
        get_discovery_resume_points_for_products(&pool, &daily_products).await?;

    assert_eq!(daily_resume_points.get("VNP46A2"), Some(&vnp46a2_date));
    assert_eq!(daily_resume_points.get("VJ146A2"), Some(&vj146a2_date));
    assert!(!daily_resume_points.contains_key("VNP46A3"));

    Ok(())
}
