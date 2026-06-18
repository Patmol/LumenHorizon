use chrono::{DateTime, Utc};
use serde::Serialize;
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
            },
            error: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiMeta {
    pub request_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: &'static str,
    pub message: &'static str,
}
