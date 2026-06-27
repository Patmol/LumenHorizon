use axum::{
    body::Bytes,
    extract::{Extension, Path, RawQuery, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use shared::{
    dark_sky::{DARK_SKY_CLASSES, DARK_SKY_CLASSIFICATION_VERSION},
    processing_message::{ProcessingMessage, ProcessingProduct},
    slippy_tiles::{
        clip_bounds, tile_bounds, validate_tile_coord, GeographicBounds, TileCoord, TileMathError,
    },
};
use uuid::Uuid;

use crate::{
    auth::AdminContext,
    db::DbError,
    error::{ApiEnvelope, GatewayError},
    readiness::ReadinessReport,
    state::AppState,
    storage::{validate_tile_set_id, StorageError},
};

use super::{audit::emit_admin_audit, middleware::RequestId};

pub(super) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "healthy" })
}

pub(super) async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadinessReport>) {
    let report = state.readiness.check().await;
    let status = if report.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(report))
}

pub(super) async fn latest_tile_manifest(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let query = match latest_manifest_query_from_raw_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return error.into_response_with_request_id(request_id.0),
    };

    let Some(storage) = state.tile_manifest_storage.as_ref() else {
        return GatewayError::service_unavailable("tile manifest storage is not configured")
            .into_response_with_request_id(request_id.0);
    };

    let manifest_result = match query.product.as_deref() {
        Some(product) => storage.latest_manifest_for_product(product).await,
        None => storage.latest_manifest().await,
    };

    match manifest_result {
        Ok(manifest) => {
            let mut headers = HeaderMap::new();
            let cache_control = HeaderValue::from_str(&state.config.tile_latest_cache_control)
                .unwrap_or_else(|_| {
                    HeaderValue::from_static("public, max-age=300, must-revalidate")
                });
            headers.insert(header::CACHE_CONTROL, cache_control);

            (
                StatusCode::OK,
                headers,
                Json(ApiEnvelope::success(manifest, request_id.0)),
            )
                .into_response()
        }
        Err(error) => manifest_storage_error_response(error, request_id, true),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LatestManifestQuery {
    product: Option<String>,
}

fn latest_manifest_query_from_raw_query(
    raw_query: Option<&str>,
) -> Result<LatestManifestQuery, GatewayError> {
    let mut product = None;

    for (key, value) in url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "product" if product.is_none() => {
                let value = value.into_owned();
                ProcessingProduct::parse(&value)
                    .map_err(|_| GatewayError::invalid_request("invalid product"))?;
                product = Some(value);
            }
            "product" => return Err(GatewayError::invalid_request("duplicate product parameter")),
            _ => return Err(GatewayError::invalid_request("unsupported query parameter")),
        }
    }

    Ok(LatestManifestQuery { product })
}

pub(super) async fn tile_manifest_by_id(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    Path(tile_set_id): Path<String>,
) -> Response {
    if validate_tile_set_id(&tile_set_id).is_err() {
        return GatewayError::invalid_request("invalid tile set id")
            .into_response_with_request_id(request_id.0);
    }

    let Some(storage) = state.tile_manifest_storage.as_ref() else {
        return GatewayError::service_unavailable("tile manifest storage is not configured")
            .into_response_with_request_id(request_id.0);
    };

    match storage.manifest_by_id(&tile_set_id).await {
        Ok(manifest) => (
            StatusCode::OK,
            Json(ApiEnvelope::success(manifest, request_id.0)),
        )
            .into_response(),
        Err(error) => manifest_storage_error_response(error, request_id, false),
    }
}

pub(super) async fn list_tile_sets(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let pagination = match pagination_from_raw_query(raw_query.as_deref()) {
        Ok(pagination) => pagination,
        Err(error) => return error.into_response_with_request_id(request_id.0),
    };
    let Some(database) = state.database.as_ref() else {
        return GatewayError::service_unavailable("database is not configured")
            .into_response_with_request_id(request_id.0);
    };

    match database
        .list_tile_sets(pagination.query_limit(), pagination.offset)
        .await
    {
        Ok(rows) => paged_response(rows, pagination, request_id),
        Err(error) => database_error_response(error, request_id, "list tile sets failed"),
    }
}

pub(super) async fn tile_classes(Extension(request_id): Extension<RequestId>) -> Response {
    let classes = json!({
        "classification_version": DARK_SKY_CLASSIFICATION_VERSION,
        "radiance_units": "nW/cm^2/sr",
        "classes": DARK_SKY_CLASSES
    });

    (
        StatusCode::OK,
        Json(ApiEnvelope::success(classes, request_id.0)),
    )
        .into_response()
}

pub(super) async fn tile_redirect(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    Path((tile_set_id, z, x, tile_file)): Path<(String, String, String, String)>,
) -> Response {
    if validate_tile_set_id(&tile_set_id).is_err() {
        return GatewayError::invalid_request("invalid tile set id")
            .into_response_with_request_id(request_id.0);
    }

    let coord = match parse_tile_coord(&z, &x, &tile_file) {
        Ok(coord) => coord,
        Err(error) => return error.into_response_with_request_id(request_id.0),
    };

    let Some(storage) = state.tile_manifest_storage.as_ref() else {
        return GatewayError::service_unavailable("tile manifest storage is not configured")
            .into_response_with_request_id(request_id.0);
    };

    let manifest = match storage.manifest_by_id(&tile_set_id).await {
        Ok(manifest) => manifest,
        Err(error) => return manifest_storage_error_response(error, request_id, false),
    };
    let manifest = match serde_json::from_value::<TileRedirectManifest>(manifest) {
        Ok(manifest) => manifest,
        Err(error) => {
            tracing::warn!(error = %error, tile_set_id, "tile manifest could not be used for redirect");
            return GatewayError::tile_unavailable("tile manifest is not redirectable")
                .into_response_with_request_id(request_id.0);
        }
    };

    if coord.z < manifest.min_zoom || coord.z > manifest.max_native_zoom {
        return GatewayError::invalid_request("tile zoom is outside tile set range")
            .into_response_with_request_id(request_id.0);
    }

    match tile_bounds(coord) {
        Ok(bounds) if clip_bounds(bounds, manifest.bounds).is_some() => {}
        Ok(_) => {
            return GatewayError::tile_not_found("tile is outside tile set bounds")
                .into_response_with_request_id(request_id.0);
        }
        Err(error) => {
            return tile_math_error_response(error).into_response_with_request_id(request_id.0);
        }
    }

    let redirect_url = manifest
        .tile_url_template
        .replace("{z}", &coord.z.to_string())
        .replace("{x}", &coord.x.to_string())
        .replace("{y}", &coord.y.to_string());

    let Ok(location) = HeaderValue::from_str(&redirect_url) else {
        tracing::warn!(
            tile_set_id = manifest.tile_set_id,
            "tile manifest produced invalid redirect URL"
        );
        return GatewayError::tile_unavailable("tile redirect URL is invalid")
            .into_response_with_request_id(request_id.0);
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::LOCATION, location);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    (StatusCode::FOUND, headers).into_response()
}

pub(super) async fn search_sites(
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(error) = validate_site_search_query(raw_query.as_deref()) {
        return error.into_response_with_request_id(request_id.0);
    }

    GatewayError::service_unavailable("site search is deferred")
        .into_response_with_request_id(request_id.0)
}

pub(super) async fn site_detail(
    Extension(request_id): Extension<RequestId>,
    Path(site_id): Path<String>,
) -> Response {
    if Uuid::parse_str(&site_id).is_err() {
        return GatewayError::invalid_request("invalid site id")
            .into_response_with_request_id(request_id.0);
    }

    GatewayError::service_unavailable("site detail is deferred")
        .into_response_with_request_id(request_id.0)
}

pub(super) async fn site_score(
    Extension(request_id): Extension<RequestId>,
    Path(site_id): Path<String>,
) -> Response {
    if Uuid::parse_str(&site_id).is_err() {
        return GatewayError::invalid_request("invalid site id")
            .into_response_with_request_id(request_id.0);
    }

    GatewayError::service_unavailable("site scoring is deferred")
        .into_response_with_request_id(request_id.0)
}

pub(super) async fn trigger_ingest(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    Extension(admin): Extension<AdminContext>,
    body: Bytes,
) -> Response {
    if let Err(error) = parse_empty_admin_body(&body) {
        emit_admin_audit(
            &request_id,
            &admin,
            "ingest.trigger",
            None,
            "validation_failed",
            error.status,
        );
        return error.into_response_with_request_id(request_id.0);
    }

    let Some(ingest_admin) = state.ingest_admin.as_ref() else {
        emit_admin_audit(
            &request_id,
            &admin,
            "ingest.trigger",
            None,
            "error",
            StatusCode::SERVICE_UNAVAILABLE,
        );
        return GatewayError::service_unavailable("ingest service is not configured")
            .into_response_with_request_id(request_id.0);
    };

    let upstream_response = match ingest_admin.trigger_ingest(request_id.0).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = %error, "ingest trigger upstream call failed");
            emit_admin_audit(
                &request_id,
                &admin,
                "ingest.trigger",
                None,
                "error",
                StatusCode::BAD_GATEWAY,
            );
            return GatewayError::upstream_error("ingest service trigger failed")
                .into_response_with_request_id(request_id.0);
        }
    };

    emit_admin_audit(
        &request_id,
        &admin,
        "ingest.trigger",
        None,
        "success",
        StatusCode::ACCEPTED,
    );

    let response = json!({
        "accepted": true,
        "mode": "ingest-service",
        "upstream": upstream_response
    });

    (
        StatusCode::ACCEPTED,
        Json(ApiEnvelope::success(response, request_id.0)),
    )
        .into_response()
}

pub(super) async fn list_ingest_runs(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let pagination = match pagination_from_raw_query(raw_query.as_deref()) {
        Ok(pagination) => pagination,
        Err(error) => return error.into_response_with_request_id(request_id.0),
    };
    let Some(database) = state.database.as_ref() else {
        return GatewayError::service_unavailable("database is not configured")
            .into_response_with_request_id(request_id.0);
    };

    match database
        .list_ingest_runs(pagination.query_limit(), pagination.offset)
        .await
    {
        Ok(rows) => paged_response(rows, pagination, request_id),
        Err(error) => database_error_response(error, request_id, "list ingest runs failed"),
    }
}

pub(super) async fn list_processing_runs(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let pagination = match pagination_from_raw_query(raw_query.as_deref()) {
        Ok(pagination) => pagination,
        Err(error) => return error.into_response_with_request_id(request_id.0),
    };
    let Some(database) = state.database.as_ref() else {
        return GatewayError::service_unavailable("database is not configured")
            .into_response_with_request_id(request_id.0);
    };

    match database
        .list_processing_runs(pagination.query_limit(), pagination.offset)
        .await
    {
        Ok(rows) => paged_response(rows, pagination, request_id),
        Err(error) => database_error_response(error, request_id, "list processing runs failed"),
    }
}

pub(super) async fn requeue_processing_item(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    Extension(admin): Extension<AdminContext>,
    body: Bytes,
) -> Response {
    let request = match parse_requeue_request(&body) {
        Ok(request) => request,
        Err(error) => {
            emit_admin_audit(
                &request_id,
                &admin,
                "processing.requeue",
                None,
                "validation_failed",
                error.status,
            );
            return error.into_response_with_request_id(request_id.0);
        }
    };

    let Some(database) = state.database.as_ref() else {
        emit_admin_audit(
            &request_id,
            &admin,
            "processing.requeue",
            Some(&request.ingest_id.to_string()),
            "error",
            StatusCode::SERVICE_UNAVAILABLE,
        );
        return GatewayError::service_unavailable("database is not configured")
            .into_response_with_request_id(request_id.0);
    };
    let Some(queue) = state.processing_queue.as_ref() else {
        emit_admin_audit(
            &request_id,
            &admin,
            "processing.requeue",
            Some(&request.ingest_id.to_string()),
            "error",
            StatusCode::SERVICE_UNAVAILABLE,
        );
        return GatewayError::service_unavailable("processing queue is not configured")
            .into_response_with_request_id(request_id.0);
    };

    let record = match database
        .processing_message_for_requeue(request.ingest_id)
        .await
    {
        Ok(record) => record,
        Err(DbError::NotFound) => {
            emit_admin_audit(
                &request_id,
                &admin,
                "processing.requeue",
                Some(&request.ingest_id.to_string()),
                "not_found",
                StatusCode::NOT_FOUND,
            );
            return GatewayError::not_found("processing item not found")
                .into_response_with_request_id(request_id.0);
        }
        Err(error) => {
            emit_admin_audit(
                &request_id,
                &admin,
                "processing.requeue",
                Some(&request.ingest_id.to_string()),
                "error",
                StatusCode::SERVICE_UNAVAILABLE,
            );
            return database_error_response(error, request_id, "processing requeue lookup failed");
        }
    };

    match record.processing_status.as_deref() {
        Some("failed" | "rejected" | "deadlettered") => {}
        Some("processing" | "processed") => {
            emit_admin_audit(
                &request_id,
                &admin,
                "processing.requeue",
                Some(&request.ingest_id.to_string()),
                "conflict",
                StatusCode::CONFLICT,
            );
            return GatewayError::conflict("processing item is not requeueable")
                .into_response_with_request_id(request_id.0);
        }
        _ => {
            emit_admin_audit(
                &request_id,
                &admin,
                "processing.requeue",
                Some(&request.ingest_id.to_string()),
                "validation_failed",
                StatusCode::UNPROCESSABLE_ENTITY,
            );
            return GatewayError::unprocessable_entity(
                "processing item has no failed processing run",
            )
            .into_response_with_request_id(request_id.0);
        }
    }

    let message = match ProcessingMessage::new(
        record.ingest_id,
        record.blob_path,
        record.product,
        record.granule_date,
        record.tile_h,
        record.tile_v,
    ) {
        Ok(message) => message,
        Err(error) => {
            tracing::warn!(error = %error, ingest_id = %request.ingest_id, "processing item cannot be requeued");
            emit_admin_audit(
                &request_id,
                &admin,
                "processing.requeue",
                Some(&request.ingest_id.to_string()),
                "validation_failed",
                StatusCode::UNPROCESSABLE_ENTITY,
            );
            return GatewayError::unprocessable_entity(
                "processing item cannot produce a valid queue message",
            )
            .into_response_with_request_id(request_id.0);
        }
    };
    let message_json = match serde_json::to_string(&message) {
        Ok(message_json) => message_json,
        Err(error) => {
            tracing::error!(error = %error, ingest_id = %request.ingest_id, "failed to serialize processing requeue message");
            emit_admin_audit(
                &request_id,
                &admin,
                "processing.requeue",
                Some(&request.ingest_id.to_string()),
                "error",
                StatusCode::INTERNAL_SERVER_ERROR,
            );
            return GatewayError::internal_error().into_response_with_request_id(request_id.0);
        }
    };

    if let Err(error) = queue.enqueue_processing_message(&message_json).await {
        tracing::warn!(error = %error, ingest_id = %request.ingest_id, "processing requeue failed");
        emit_admin_audit(
            &request_id,
            &admin,
            "processing.requeue",
            Some(&request.ingest_id.to_string()),
            "error",
            StatusCode::BAD_GATEWAY,
        );
        return GatewayError::upstream_error("processing queue enqueue failed")
            .into_response_with_request_id(request_id.0);
    }

    emit_admin_audit(
        &request_id,
        &admin,
        "processing.requeue",
        Some(&request.ingest_id.to_string()),
        "success",
        StatusCode::ACCEPTED,
    );

    let response = json!({
        "accepted": true,
        "mode": "processing-queue",
        "ingest_id": request.ingest_id
    });

    (
        StatusCode::ACCEPTED,
        Json(ApiEnvelope::success(response, request_id.0)),
    )
        .into_response()
}

pub(super) async fn deep_health(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    let mut checks = Vec::new();

    checks.push(match state.database.as_ref() {
        Some(database) => dependency_check("database", database.check().await),
        None => DependencyCheck::unavailable("database", "database is not configured"),
    });

    checks.push(match state.tile_manifest_storage.as_ref() {
        Some(storage) => {
            dependency_check("tile_manifest", storage.latest_manifest().await.map(|_| ()))
        }
        None => {
            DependencyCheck::unavailable("tile_manifest", "tile manifest storage is not configured")
        }
    });

    checks.push(match state.processing_queue.as_ref() {
        Some(queue) => dependency_check("processing_queue", queue.health_check().await),
        None => {
            DependencyCheck::unavailable("processing_queue", "processing queue is not configured")
        }
    });

    checks.push(if state.ingest_admin.is_some() {
        DependencyCheck::ready("ingest_admin")
    } else {
        DependencyCheck::unavailable("ingest_admin", "ingest service is not configured")
    });

    let status = if checks.iter().all(DependencyCheck::is_ready) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let response = DeepHealthResponse {
        status: if status == StatusCode::OK {
            "ready"
        } else {
            "not_ready"
        },
        checks,
    };

    (status, Json(ApiEnvelope::success(response, request_id.0))).into_response()
}

pub(super) async fn not_found(Extension(request_id): Extension<RequestId>) -> Response {
    GatewayError::not_found("route not found").into_response_with_request_id(request_id.0)
}

fn paged_response<T>(mut rows: Vec<T>, pagination: Pagination, request_id: RequestId) -> Response
where
    T: Serialize,
{
    let next_cursor = if rows.len() > pagination.limit as usize {
        rows.truncate(pagination.limit as usize);
        Some((pagination.offset + i64::from(pagination.limit)).to_string())
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(ApiEnvelope::success_with_cursor(
            rows,
            request_id.0,
            next_cursor,
        )),
    )
        .into_response()
}

fn database_error_response(
    error: DbError,
    request_id: RequestId,
    message: &'static str,
) -> Response {
    tracing::warn!(error = %error, "database-backed route failed");
    GatewayError::service_unavailable(message).into_response_with_request_id(request_id.0)
}

fn parse_tile_coord(z: &str, x: &str, tile_file: &str) -> Result<TileCoord, GatewayError> {
    if tile_file.contains('/') || !tile_file.ends_with(".png") {
        return Err(GatewayError::invalid_request(
            "tile path must end with {y}.png",
        ));
    }

    let y = tile_file.trim_end_matches(".png");
    if y.is_empty() {
        return Err(GatewayError::invalid_request(
            "tile y coordinate is required",
        ));
    }

    let z = z
        .parse::<u8>()
        .map_err(|_| GatewayError::invalid_request("invalid tile zoom"))?;
    let x = x
        .parse::<u32>()
        .map_err(|_| GatewayError::invalid_request("invalid tile x coordinate"))?;
    let y = y
        .parse::<u32>()
        .map_err(|_| GatewayError::invalid_request("invalid tile y coordinate"))?;

    let coord = TileCoord { z, x, y };
    validate_tile_coord(coord).map_err(tile_math_error_response)?;

    Ok(coord)
}

fn tile_math_error_response(error: TileMathError) -> GatewayError {
    match error {
        TileMathError::ZoomTooLarge { .. } => {
            GatewayError::invalid_request("tile zoom exceeds supported maximum")
        }
        TileMathError::XOutOfRange { .. } | TileMathError::YOutOfRange { .. } => {
            GatewayError::invalid_request("tile coordinate is outside zoom range")
        }
        _ => GatewayError::invalid_request("invalid tile coordinate"),
    }
}

fn parse_requeue_request(body: &[u8]) -> Result<RequeueProcessingRequest, GatewayError> {
    if body.is_empty() {
        return Err(GatewayError::invalid_request(
            "processing requeue body is required",
        ));
    }

    serde_json::from_slice(body)
        .map_err(|_| GatewayError::invalid_request("invalid processing requeue body"))
}

fn parse_empty_admin_body(body: &[u8]) -> Result<(), GatewayError> {
    if body.is_empty() {
        return Ok(());
    }

    serde_json::from_slice::<EmptyAdminBody>(body)
        .map(|_| ())
        .map_err(|_| GatewayError::invalid_request("invalid admin request body"))
}

fn pagination_from_raw_query(raw_query: Option<&str>) -> Result<Pagination, GatewayError> {
    let mut limit = None;
    let mut cursor = None;

    for (key, value) in url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "limit" if limit.is_none() => limit = Some(value.into_owned()),
            "cursor" if cursor.is_none() => cursor = Some(value.into_owned()),
            "limit" | "cursor" => {
                return Err(GatewayError::invalid_request(
                    "duplicate pagination parameter",
                ));
            }
            _ => return Err(GatewayError::invalid_request("unsupported query parameter")),
        }
    }

    pagination_from_parts(limit.as_deref(), cursor.as_deref())
}

fn pagination_from_parts(
    limit: Option<&str>,
    cursor: Option<&str>,
) -> Result<Pagination, GatewayError> {
    let limit = match limit {
        Some(value) => value
            .parse::<u32>()
            .map_err(|_| GatewayError::invalid_request("invalid limit"))?,
        None => 50,
    };
    if !(1..=100).contains(&limit) {
        return Err(GatewayError::invalid_request(
            "limit must be between 1 and 100",
        ));
    }

    let offset = match cursor {
        Some(cursor) => cursor
            .parse::<i64>()
            .ok()
            .filter(|value| *value >= 0)
            .ok_or_else(|| GatewayError::invalid_request("invalid cursor"))?,
        None => 0,
    };

    Ok(Pagination { limit, offset })
}

fn validate_site_search_query(raw_query: Option<&str>) -> Result<(), GatewayError> {
    let mut lat = None;
    let mut lon = None;
    let mut radius_km = None;
    let mut west = None;
    let mut south = None;
    let mut east = None;
    let mut north = None;
    let mut limit = None;
    let mut cursor = None;

    for (key, value) in url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
        let value = value.into_owned();
        match key.as_ref() {
            "lat" if lat.is_none() => lat = Some(parse_range("lat", &value, -90.0, 90.0)?),
            "lon" if lon.is_none() => lon = Some(parse_range("lon", &value, -180.0, 180.0)?),
            "radius_km" if radius_km.is_none() => {
                radius_km = Some(parse_range("radius_km", &value, 1.0, 250.0)?)
            }
            "west" if west.is_none() => west = Some(parse_range("west", &value, -180.0, 180.0)?),
            "south" if south.is_none() => south = Some(parse_range("south", &value, -90.0, 90.0)?),
            "east" if east.is_none() => east = Some(parse_range("east", &value, -180.0, 180.0)?),
            "north" if north.is_none() => north = Some(parse_range("north", &value, -90.0, 90.0)?),
            "limit" if limit.is_none() => limit = Some(value),
            "cursor" if cursor.is_none() => cursor = Some(value),
            "lat" | "lon" | "radius_km" | "west" | "south" | "east" | "north" | "limit"
            | "cursor" => return Err(GatewayError::invalid_request("duplicate query parameter")),
            _ => return Err(GatewayError::invalid_request("unsupported query parameter")),
        }
    }

    if lat.is_some() != lon.is_some() {
        return Err(GatewayError::invalid_request(
            "lat and lon must be provided together",
        ));
    }

    if [west, south, east, north].iter().any(Option::is_some)
        && [west, south, east, north].iter().any(Option::is_none)
    {
        return Err(GatewayError::invalid_request(
            "bounds require west, south, east, and north",
        ));
    }

    if let (Some(west), Some(south), Some(east), Some(north)) = (west, south, east, north) {
        if west >= east || south >= north {
            return Err(GatewayError::invalid_request("invalid bounds"));
        }
    }

    pagination_from_parts(limit.as_deref(), cursor.as_deref()).map(|_| ())
}

fn parse_range(field: &'static str, value: &str, min: f64, max: f64) -> Result<f64, GatewayError> {
    let parsed = value.parse::<f64>().map_err(|_| {
        GatewayError::invalid_request(match field {
            "lat" => "invalid lat",
            "lon" => "invalid lon",
            "radius_km" => "invalid radius_km",
            "west" => "invalid west",
            "south" => "invalid south",
            "east" => "invalid east",
            "north" => "invalid north",
            _ => "invalid query parameter",
        })
    })?;

    if !(min..=max).contains(&parsed) {
        return Err(GatewayError::invalid_request(match field {
            "lat" => "lat must be between -90 and 90",
            "lon" => "lon must be between -180 and 180",
            "radius_km" => "radius_km must be between 1 and 250",
            "west" => "west must be between -180 and 180",
            "south" => "south must be between -90 and 90",
            "east" => "east must be between -180 and 180",
            "north" => "north must be between -90 and 90",
            _ => "query parameter is out of range",
        }));
    }

    Ok(parsed)
}

fn dependency_check<E>(name: &'static str, result: Result<(), E>) -> DependencyCheck
where
    E: std::fmt::Display,
{
    match result {
        Ok(()) => DependencyCheck::ready(name),
        Err(error) => {
            tracing::warn!(dependency = name, error = %error, "deep health dependency check failed");
            DependencyCheck::unavailable(name, "dependency check failed")
        }
    }
}

fn manifest_storage_error_response(
    error: StorageError,
    request_id: RequestId,
    latest_manifest: bool,
) -> Response {
    if matches!(error, StorageError::InvalidTileSetId) {
        return GatewayError::invalid_request("invalid tile set id")
            .into_response_with_request_id(request_id.0);
    }

    if !latest_manifest && error.is_blob_not_found() {
        return GatewayError::tile_not_found("tile manifest not found")
            .into_response_with_request_id(request_id.0);
    }

    if latest_manifest && error.is_blob_not_found() {
        tracing::warn!(error = %error, "latest tile manifest is not available");
        return GatewayError::service_unavailable("latest tile manifest is not available")
            .into_response_with_request_id(request_id.0);
    }

    tracing::warn!(error = %error, "tile manifest storage read failed");
    GatewayError::service_unavailable("tile manifest storage is unavailable")
        .into_response_with_request_id(request_id.0)
}

#[derive(Debug, Serialize)]
pub(super) struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct Pagination {
    limit: u32,
    offset: i64,
}

impl Pagination {
    fn query_limit(self) -> i64 {
        i64::from(self.limit) + 1
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyAdminBody {}

#[derive(Debug, Deserialize)]
struct TileRedirectManifest {
    tile_set_id: String,
    min_zoom: u8,
    max_native_zoom: u8,
    #[serde(rename = "max_display_zoom")]
    _max_display_zoom: u8,
    bounds: GeographicBounds,
    tile_url_template: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequeueProcessingRequest {
    ingest_id: Uuid,
}

#[derive(Debug, Serialize)]
struct DeepHealthResponse {
    status: &'static str,
    checks: Vec<DependencyCheck>,
}

#[derive(Debug, Serialize)]
struct DependencyCheck {
    name: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'static str>,
}

impl DependencyCheck {
    fn ready(name: &'static str) -> Self {
        Self {
            name,
            status: "ready",
            message: None,
        }
    }

    fn unavailable(name: &'static str, message: &'static str) -> Self {
        Self {
            name,
            status: "unavailable",
            message: Some(message),
        }
    }

    fn is_ready(&self) -> bool {
        self.status == "ready"
    }
}
