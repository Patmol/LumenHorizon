use std::{future::Future, pin::Pin, sync::Arc};

use serde::Serialize;
use sqlx::PgPool;

use crate::{
    config::AppConfig,
    storage::{BlobStorageClient, QueueClient},
};

type ReadinessFuture = Pin<Box<dyn Future<Output = ReadinessReport> + Send>>;

#[derive(Clone)]
pub struct ReadinessProbe {
    check: Arc<dyn Fn() -> ReadinessFuture + Send + Sync>,
}

impl ReadinessProbe {
    pub fn new<F, Fut>(check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadinessReport> + Send + 'static,
    {
        Self {
            check: Arc::new(move || Box::pin(check())),
        }
    }

    pub fn from_dependencies(config: AppConfig, pool: PgPool) -> Self {
        Self::new(move || {
            let config = config.clone();
            let pool = pool.clone();

            async move { check_dependencies(&config, &pool).await }
        })
    }

    pub async fn check(&self) -> ReadinessReport {
        (self.check)().await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadinessReport {
    pub status: &'static str,
    pub checks: Vec<ReadinessCheck>,
}

impl ReadinessReport {
    pub fn new(checks: Vec<ReadinessCheck>) -> Self {
        let status = if checks.iter().all(ReadinessCheck::is_ready) {
            "ready"
        } else {
            "not_ready"
        };

        Self { status, checks }
    }

    pub fn is_ready(&self) -> bool {
        self.status == "ready"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadinessCheck {
    pub name: &'static str,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ReadinessCheck {
    pub fn ready(name: &'static str) -> Self {
        Self {
            name,
            status: "ready",
            message: None,
        }
    }

    pub(crate) fn unavailable(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: "unavailable",
            message: Some(message.into()),
        }
    }

    fn from_result<E>(name: &'static str, result: Result<(), E>) -> Self
    where
        E: std::fmt::Display,
    {
        match result {
            Ok(()) => Self::ready(name),
            Err(error) => Self::unavailable(name, error.to_string()),
        }
    }

    fn is_ready(&self) -> bool {
        self.status == "ready"
    }
}

async fn check_dependencies(config: &AppConfig, pool: &PgPool) -> ReadinessReport {
    let checks = vec![
        ReadinessCheck::from_result("postgres", check_postgres(pool).await),
        ReadinessCheck::from_result("raw_blob_container", check_blob_storage(config).await),
        ReadinessCheck::from_result("processing_queue", check_queue(config).await),
    ];

    ReadinessReport::new(checks)
}

async fn check_postgres(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .map(|_| ())
}

async fn check_blob_storage(config: &AppConfig) -> Result<(), crate::storage::StorageError> {
    BlobStorageClient::new(config)?
        .check_raw_container_access()
        .await
}

async fn check_queue(config: &AppConfig) -> Result<(), crate::storage::StorageError> {
    QueueClient::new(config)?
        .check_queue_access(&config.azure_queue_name)
        .await
}

#[cfg(test)]
mod tests {
    use super::{ReadinessCheck, ReadinessReport};

    #[test]
    fn report_is_ready_when_all_checks_are_ready() {
        let report = ReadinessReport::new(vec![
            ReadinessCheck::ready("postgres"),
            ReadinessCheck::ready("raw_blob_container"),
        ]);

        assert!(report.is_ready());
        assert_eq!(report.status, "ready");
    }

    #[test]
    fn report_is_not_ready_when_any_check_is_unavailable() {
        let report = ReadinessReport::new(vec![
            ReadinessCheck::ready("postgres"),
            ReadinessCheck::unavailable("processing_queue", "queue unavailable"),
        ]);

        assert!(!report.is_ready());
        assert_eq!(report.status, "not_ready");
    }
}
