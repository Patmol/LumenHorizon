use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use uuid::Uuid;

use crate::{error::GatewayError, observability, rate_limit::RouteClass, state::AppState};

use super::{
    admin_route_policy_for_path,
    audit::{action_for_path, emit_admin_audit},
    AdminRoleRequirement, REQUEST_ID_HEADER,
};

const CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");
const FRAME_OPTIONS: HeaderName = HeaderName::from_static("x-frame-options");
const REFERRER_POLICY: HeaderName = HeaderName::from_static("referrer-policy");
const PERMISSIONS_POLICY: HeaderName = HeaderName::from_static("permissions-policy");

#[derive(Clone, Copy, Debug)]
pub(super) struct RequestId(pub Uuid);

#[derive(Clone, Debug)]
struct ClientKey(String);

pub(super) async fn request_context(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = request_id_from_headers(request.headers());
    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_owned();
    let started_at = Instant::now();
    let client_key = client_key_from_headers(request.headers());
    request.extensions_mut().insert(RequestId(request_id));
    request
        .extensions_mut()
        .insert(ClientKey(client_key.clone()));

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

    let hardening_result = validate_request_hardening(&state, &method, request.headers(), &path);
    let rate_limit_result = if hardening_result.is_ok() && !path.starts_with("/api/v1/admin/") {
        match route_class_for_public(&method, &path) {
            Some(route_class) => state.rate_limiter.check(client_key, route_class).await,
            None => Ok(()),
        }
    } else {
        Ok(())
    };

    let route_timeout = route_timeout(&state, &path);
    let mut response = match hardening_result.and(rate_limit_result) {
        Ok(()) => {
            match tokio::time::timeout(route_timeout, next.run(request).instrument(span.clone()))
                .await
            {
                Ok(response) => response,
                Err(_) => GatewayError::service_unavailable("request timed out")
                    .into_response_with_request_id(request_id),
            }
        }
        Err(error) => error.into_response_with_request_id(request_id),
    };

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

    apply_standard_headers(response.headers_mut(), request_id, &path);

    response
}

pub(super) async fn require_admin(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .copied()
        .unwrap_or_else(|| RequestId(Uuid::new_v4()));
    let path = request.uri().path().to_owned();
    let route_class = if request.method() == Method::POST {
        RouteClass::AdminWrite
    } else {
        RouteClass::AdminRead
    };

    let required_role = match admin_route_policy_for_path(&path)
        .map(|policy| policy.role)
        .unwrap_or(AdminRoleRequirement::ConfiguredAdmin)
    {
        AdminRoleRequirement::ConfiguredAdmin => state.config.auth.admin_required_role.as_str(),
    };

    match state
        .auth
        .authenticate_with_role(request.headers(), required_role)
        .await
    {
        Ok(admin) => {
            let key = format!("{}:{path}", admin.subject);
            if let Err(error) = state.rate_limiter.check(key, route_class).await {
                emit_admin_audit(
                    &request_id,
                    &admin,
                    action_for_path(&path),
                    None,
                    if error.status == StatusCode::TOO_MANY_REQUESTS {
                        "rate_limited"
                    } else {
                        "error"
                    },
                    error.status,
                );
                return error.into_response_with_request_id(request_id.0);
            }

            request.extensions_mut().insert(admin);
            next.run(request).await
        }
        Err(error) => {
            let client_key = request
                .extensions()
                .get::<ClientKey>()
                .map(|key| key.0.clone())
                .unwrap_or_else(|| "unknown".to_owned());
            if let Err(rate_limit_error) = state
                .rate_limiter
                .check(client_key, RouteClass::AuthFailure)
                .await
            {
                return rate_limit_error.into_response_with_request_id(request_id.0);
            }

            error.into_response_with_request_id(request_id.0)
        }
    }
}

fn validate_request_hardening(
    state: &AppState,
    method: &Method,
    headers: &HeaderMap,
    path: &str,
) -> Result<(), GatewayError> {
    if path.len() > state.config.max_url_length_bytes {
        return Err(GatewayError::invalid_request("request URI is too long"));
    }

    if let Some(expected_method) = expected_method_for_api_path(path) {
        if !method.as_str().eq_ignore_ascii_case(expected_method) {
            return Err(GatewayError::invalid_request(
                "method is not supported for this route",
            ));
        }
    }

    if method == Method::GET && content_length(headers).unwrap_or(0) > 0 {
        return Err(GatewayError::invalid_request(
            "GET request bodies are not accepted",
        ));
    }

    if path.starts_with("/api/v1/admin/") && method == Method::POST {
        if content_length(headers).unwrap_or(0) > state.config.admin_max_body_bytes {
            return Err(GatewayError::invalid_request("request body is too large"));
        }

        if !is_json_content_type(headers) {
            return Err(GatewayError::invalid_request(
                "admin writes require application/json",
            ));
        }
    }

    Ok(())
}

fn route_timeout(state: &AppState, path: &str) -> Duration {
    if path == "/health" || path == "/ready" {
        state.config.health_timeout
    } else if path.starts_with("/api/v1/admin/") {
        state.config.admin_timeout
    } else {
        state.config.public_timeout
    }
}

fn expected_method_for_api_path(path: &str) -> Option<&'static str> {
    if let Some(policy) = admin_route_policy_for_path(path) {
        return Some(policy.method);
    }

    if matches!(path, "/health" | "/ready")
        || matches!(
            path,
            "/api/v1/tiles/manifest" | "/api/v1/tiles/sets" | "/api/v1/tiles/classes"
        )
        || path.starts_with("/api/v1/tiles/manifest/")
        || path.starts_with("/api/v1/tiles/")
        || path == "/api/v1/sites"
        || path.starts_with("/api/v1/sites/")
    {
        return Some("get");
    }

    None
}

fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.starts_with("application/json"))
        .unwrap_or(false)
}

fn route_class_for_public(method: &Method, path: &str) -> Option<RouteClass> {
    if method != Method::GET {
        return None;
    }

    if path.ends_with(".png") && path.starts_with("/api/v1/tiles/") {
        return Some(RouteClass::TileRedirect);
    }

    if path.starts_with("/api/v1/tiles/") {
        return Some(RouteClass::PublicTileMetadata);
    }

    if path.starts_with("/api/v1/sites") {
        return Some(RouteClass::PublicSiteRead);
    }

    None
}

fn apply_standard_headers(headers: &mut HeaderMap, request_id: Uuid, path: &str) {
    headers.insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id.to_string()).expect("UUID is a valid header value"),
    );
    headers.insert(CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(
        PERMISSIONS_POLICY,
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );

    if path.starts_with("/api/v1/admin/") {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    } else if path.contains("/manifest/") {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
}

fn request_id_from_headers(headers: &HeaderMap) -> Uuid {
    headers
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

fn client_key_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_owned()
}
