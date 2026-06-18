use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Instant,
};

use axum::{
    body::Body,
    extract::{Extension, Request, State},
    http::{header::HeaderName, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    error::{ApiEnvelope, ApiErrorBody, ApiMeta},
    observability,
    readiness::ReadinessReport,
    state::AppState,
};

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Clone, Copy, Debug)]
struct RequestId(Uuid);

pub async fn serve(state: AppState) -> Result<(), ServerError> {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), state.config.port);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|source| ServerError::Bind { address, source })?;

    tracing::info!(address = %address, "ingest-svc HTTP server listening");

    axum::serve(listener, router(state))
        .await
        .map_err(ServerError::Serve)
}

pub fn router(state: AppState) -> Router {
    let admin_routes = Router::new()
        .route("/ingest/trigger", post(trigger_ingest))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_internal_admin,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .nest("/admin", admin_routes)
        .with_state(state)
        .layer(middleware::from_fn(assign_request_id))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "healthy" })
}

async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadinessReport>) {
    let report = state.readiness.check().await;
    let status = if report.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(report))
}

async fn trigger_ingest(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
) -> Json<ApiEnvelope<TriggerIngestResponse>> {
    let ingest_id = Uuid::new_v4();

    tracing::info!(
        request_id = %request_id.0,
        ingest_id = %ingest_id,
        azure_queue_name = state.config.azure_queue_name,
        database_pool_size = state.pool.size(),
        "placeholder ingest trigger accepted"
    );

    Json(ApiEnvelope::success(
        TriggerIngestResponse { ingest_id },
        request_id.0,
    ))
}

async fn assign_request_id(mut request: Request<Body>, next: Next) -> Response {
    let request_id = request_id_from_headers(request.headers());
    let method = request.method().clone();
    let uri = request.uri().clone();
    let started_at = Instant::now();
    request.extensions_mut().insert(RequestId(request_id));

    let span = tracing::info_span!(
        "http_request",
        service = observability::SERVICE_NAME,
        service_version = observability::SERVICE_VERSION,
        request_id = %request_id,
        method = %method,
        uri = %uri,
        status = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    );
    let mut response = next.run(request).instrument(span.clone()).await;
    let duration = started_at.elapsed();
    let status = response.status().as_u16();

    span.record("status", status);
    span.record("duration_ms", duration.as_millis() as u64);
    tracing::info!(
        parent: &span,
        status,
        duration_ms = duration.as_millis() as u64,
        "HTTP request completed"
    );

    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id.to_string()).expect("UUID is a valid header value"),
    );

    response
}

async fn require_internal_admin(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(auth) = state.config.internal_admin_auth.as_ref() else {
        return next.run(request).await;
    };
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .copied()
        .unwrap_or_else(|| RequestId(Uuid::new_v4()));

    match internal_admin_token(request.headers(), &auth.header_name) {
        Some(token) if constant_time_eq(token.as_bytes(), auth.token.as_bytes()) => {
            next.run(request).await
        }
        Some(_) => internal_admin_auth_error(
            request_id.0,
            StatusCode::FORBIDDEN,
            "forbidden",
            "internal service authorization is invalid",
        ),
        None => internal_admin_auth_error(
            request_id.0,
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "internal service authorization is required",
        ),
    }
}

fn request_id_from_headers(headers: &axum::http::HeaderMap) -> Uuid {
    headers
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

fn internal_admin_token<'a>(headers: &'a HeaderMap, header_name: &str) -> Option<&'a str> {
    headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
}

fn internal_admin_auth_error(
    request_id: Uuid,
    status: StatusCode,
    code: &'static str,
    message: &'static str,
) -> Response {
    (
        status,
        Json(ApiEnvelope::<serde_json::Value> {
            data: None,
            meta: ApiMeta {
                request_id,
                timestamp: chrono::Utc::now(),
            },
            error: Some(ApiErrorBody { code, message }),
        }),
    )
        .into_response()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right.iter())
        .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct TriggerIngestResponse {
    ingest_id: Uuid,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("server error: failed to bind HTTP listener on {address}: {source}")]
    Bind {
        address: SocketAddr,
        source: std::io::Error,
    },
    #[error("server error: HTTP server failed: {0}")]
    Serve(std::io::Error),
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request, StatusCode},
    };
    use serde_json::Value;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::{router, REQUEST_ID_HEADER};
    use crate::{
        config::{AppConfig, InternalAdminAuthConfig},
        readiness::{ReadinessCheck, ReadinessProbe, ReadinessReport},
        state::AppState,
    };

    const TEST_STORAGE_ACCESS_KEY: &str = "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5";

    fn test_state() -> AppState {
        test_state_with_readiness(ReadinessReport::new(vec![
            ReadinessCheck::ready("postgres"),
            ReadinessCheck::ready("raw_blob_container"),
            ReadinessCheck::ready("processing_queue"),
        ]))
    }

    fn test_state_with_readiness(report: ReadinessReport) -> AppState {
        let config = AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some(TEST_STORAGE_ACCESS_KEY.to_owned()),
            _ => None,
        })
        .unwrap();
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://localhost/lumenhorizon")
            .unwrap();
        let readiness = ReadinessProbe::new(move || {
            let report = report.clone();

            async move { report }
        });

        AppState::with_readiness(config, pool, readiness)
    }

    fn test_state_with_internal_auth() -> AppState {
        let mut state = test_state();
        state.config.internal_admin_auth = Some(InternalAdminAuthConfig {
            header_name: "x-lumenhorizon-internal-token".to_owned(),
            token: "0123456789abcdef0123456789abcdef".to_owned(),
        });
        state
    }

    #[tokio::test]
    async fn health_returns_dependency_free_liveness_response() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key(REQUEST_ID_HEADER));

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value, serde_json::json!({ "status": "healthy" }));
    }

    #[tokio::test]
    async fn ready_returns_success_when_dependencies_are_available() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key(REQUEST_ID_HEADER));

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value["status"], "ready");
        assert_eq!(value["checks"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn ready_returns_failure_when_dependency_is_unavailable() {
        let response = router(test_state_with_readiness(ReadinessReport::new(vec![
            ReadinessCheck::ready("postgres"),
            ReadinessCheck::unavailable("processing_queue", "queue unavailable"),
        ])))
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(response.headers().contains_key(REQUEST_ID_HEADER));

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value["status"], "not_ready");
        assert_eq!(value["checks"][1]["name"], "processing_queue");
        assert_eq!(value["checks"][1]["status"], "unavailable");
        assert_eq!(value["checks"][1]["message"], "queue unavailable");
    }

    #[tokio::test]
    async fn trigger_ingest_returns_placeholder_envelope() {
        let request_id = uuid::Uuid::new_v4();
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/ingest/trigger")
                    .header(REQUEST_ID_HEADER, request_id.to_string())
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(REQUEST_ID_HEADER).unwrap(),
            request_id.to_string().as_str()
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert!(value["data"]["ingest_id"].as_str().is_some());
        assert_eq!(value["meta"]["request_id"], request_id.to_string());
        assert!(value["meta"]["timestamp"].as_str().is_some());
        assert!(value["error"].is_null());
    }

    #[tokio::test]
    async fn trigger_ingest_requires_internal_auth_when_configured() {
        let response = router(test_state_with_internal_auth())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/ingest/trigger")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "unauthenticated");
        assert_eq!(
            value["error"]["message"],
            "internal service authorization is required"
        );
    }

    #[tokio::test]
    async fn trigger_ingest_rejects_invalid_internal_auth() {
        let response = router(test_state_with_internal_auth())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/ingest/trigger")
                    .header("x-lumenhorizon-internal-token", "wrong-token")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "forbidden");
        assert_eq!(
            value["error"]["message"],
            "internal service authorization is invalid"
        );
    }

    #[tokio::test]
    async fn trigger_ingest_accepts_valid_internal_auth() {
        let response = router(test_state_with_internal_auth())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/ingest/trigger")
                    .header(
                        "x-lumenhorizon-internal-token",
                        "0123456789abcdef0123456789abcdef",
                    )
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert!(value["data"]["ingest_id"].as_str().is_some());
        assert!(value["error"].is_null());
    }
}
