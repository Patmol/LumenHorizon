use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ApiEnvelope<T>
where
    T: Serialize,
{
    pub data: Option<T>,
    pub meta: ApiMeta,
    pub error: Option<ApiErrorBody>,
}

impl<T> ApiEnvelope<T>
where
    T: Serialize,
{
    pub fn success(data: T, request_id: Uuid) -> Self {
        Self {
            data: Some(data),
            meta: ApiMeta {
                request_id,
                timestamp: Utc::now(),
                next_cursor: None,
            },
            error: None,
        }
    }

    pub fn success_with_cursor(data: T, request_id: Uuid, next_cursor: Option<String>) -> Self {
        Self {
            data: Some(data),
            meta: ApiMeta {
                request_id,
                timestamp: Utc::now(),
                next_cursor,
            },
            error: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiMeta {
    pub request_id: Uuid,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: &'static str,
    pub message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct GatewayError {
    pub status: StatusCode,
    pub code: ErrorCode,
    pub message: &'static str,
    pub retry_after_seconds: Option<u64>,
    pub request_id: Option<Uuid>,
}

impl GatewayError {
    pub fn new(status: StatusCode, code: ErrorCode, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            retry_after_seconds: None,
            request_id: None,
        }
    }

    pub fn invalid_request(message: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, message)
    }

    pub fn unauthenticated(message: &'static str) -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Unauthenticated,
            message,
        )
    }

    pub fn forbidden(message: &'static str) -> Self {
        Self::new(StatusCode::FORBIDDEN, ErrorCode::Forbidden, message)
    }

    pub fn not_found(message: &'static str) -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, message)
    }

    #[allow(dead_code)]
    pub fn tile_not_found(message: &'static str) -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorCode::TileNotFound, message)
    }

    #[allow(dead_code)]
    pub fn conflict(message: &'static str) -> Self {
        Self::new(StatusCode::CONFLICT, ErrorCode::Conflict, message)
    }

    #[allow(dead_code)]
    pub fn unprocessable_entity(message: &'static str) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::UnprocessableEntity,
            message,
        )
    }

    pub fn rate_limited(retry_after_seconds: u64) -> Self {
        Self {
            retry_after_seconds: Some(retry_after_seconds),
            ..Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                ErrorCode::RateLimited,
                "rate limit exceeded",
            )
        }
    }

    #[allow(dead_code)]
    pub fn internal_error() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::InternalError,
            "internal server error",
        )
    }

    #[allow(dead_code)]
    pub fn upstream_error(message: &'static str) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, ErrorCode::UpstreamError, message)
    }

    pub fn service_unavailable(message: &'static str) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::ServiceUnavailable,
            message,
        )
    }

    pub fn tile_unavailable(message: &'static str) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::TileUnavailable,
            message,
        )
    }

    pub fn with_request_id(mut self, request_id: Uuid) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn into_response_with_request_id(self, request_id: Uuid) -> Response {
        self.with_request_id(request_id).into_response()
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let request_id = self.request_id.unwrap_or_else(Uuid::new_v4);
        let mut headers = HeaderMap::new();
        headers.insert(
            super::server::REQUEST_ID_HEADER,
            HeaderValue::from_str(&request_id.to_string()).expect("UUID is a valid header value"),
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        if let Some(retry_after_seconds) = self.retry_after_seconds {
            headers.insert(
                header::RETRY_AFTER,
                HeaderValue::from_str(&retry_after_seconds.to_string())
                    .expect("integer is a valid header value"),
            );
        }

        let envelope = ErrorEnvelope {
            data: None,
            meta: ApiMeta {
                request_id,
                timestamp: Utc::now(),
                next_cursor: None,
            },
            error: ApiErrorBody {
                code: self.code.as_str(),
                message: self.message,
                details: None,
            },
        };

        (self.status, headers, Json(envelope)).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    data: Option<Value>,
    meta: ApiMeta,
    error: ApiErrorBody,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidRequest,
    Unauthenticated,
    Forbidden,
    NotFound,
    #[allow(dead_code)]
    TileNotFound,
    #[allow(dead_code)]
    Conflict,
    #[allow(dead_code)]
    UnprocessableEntity,
    RateLimited,
    #[allow(dead_code)]
    InternalError,
    #[allow(dead_code)]
    UpstreamError,
    ServiceUnavailable,
    TileUnavailable,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::TileNotFound => "tile_not_found",
            Self::Conflict => "conflict",
            Self::UnprocessableEntity => "unprocessable_entity",
            Self::RateLimited => "rate_limited",
            Self::InternalError => "internal_error",
            Self::UpstreamError => "upstream_error",
            Self::ServiceUnavailable => "service_unavailable",
            Self::TileUnavailable => "tile_unavailable",
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode};
    use serde_json::Value;
    use uuid::Uuid;

    use super::{ErrorCode, GatewayError};

    #[tokio::test]
    async fn error_response_uses_sanitized_envelope() {
        let request_id = Uuid::new_v4();
        let response = GatewayError::unauthenticated("authentication required")
            .into_response_with_request_id(request_id);

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert!(value["data"].is_null());
        assert_eq!(value["meta"]["request_id"], request_id.to_string());
        assert_eq!(value["error"]["code"], "unauthenticated");
        assert_eq!(value["error"]["message"], "authentication required");
        assert!(value["error"].get("details").is_none());
    }

    #[test]
    fn error_code_strings_match_openapi_contract() {
        assert_eq!(ErrorCode::InvalidRequest.as_str(), "invalid_request");
        assert_eq!(ErrorCode::Unauthenticated.as_str(), "unauthenticated");
        assert_eq!(ErrorCode::Forbidden.as_str(), "forbidden");
        assert_eq!(ErrorCode::NotFound.as_str(), "not_found");
        assert_eq!(ErrorCode::TileNotFound.as_str(), "tile_not_found");
        assert_eq!(ErrorCode::Conflict.as_str(), "conflict");
        assert_eq!(
            ErrorCode::UnprocessableEntity.as_str(),
            "unprocessable_entity"
        );
        assert_eq!(ErrorCode::RateLimited.as_str(), "rate_limited");
        assert_eq!(ErrorCode::InternalError.as_str(), "internal_error");
        assert_eq!(ErrorCode::UpstreamError.as_str(), "upstream_error");
        assert_eq!(
            ErrorCode::ServiceUnavailable.as_str(),
            "service_unavailable"
        );
        assert_eq!(ErrorCode::TileUnavailable.as_str(), "tile_unavailable");
    }
}
