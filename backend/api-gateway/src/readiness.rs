use serde::Serialize;
use std::sync::Arc;

use crate::{
    config::{AppConfig, RateLimitBackend, RuntimeEnvironment},
    rate_limit::RateLimiter,
};

#[derive(Clone)]
pub struct ReadinessProbe {
    config: AppConfig,
    rate_limiter: Arc<RateLimiter>,
}

impl ReadinessProbe {
    pub fn from_config(config: AppConfig, rate_limiter: Arc<RateLimiter>) -> Self {
        Self {
            config,
            rate_limiter,
        }
    }

    pub async fn check(&self) -> ReadinessReport {
        ReadinessReport::new(vec![
            ReadinessCheck::ready("configuration"),
            readiness_for_rate_limit(&self.config, &self.rate_limiter).await,
            ReadinessCheck::ready("auth_configuration"),
            readiness_for_database(&self.config),
            readiness_for_tile_manifest_storage(&self.config),
            readiness_for_processing_queue(&self.config),
            readiness_for_ingest_admin(&self.config),
            readiness_for_internal_service_auth(&self.config),
        ])
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

    pub fn unavailable(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: "unavailable",
            message: Some(message.into()),
        }
    }

    fn is_ready(&self) -> bool {
        self.status == "ready"
    }
}

async fn readiness_for_rate_limit(
    config: &AppConfig,
    rate_limiter: &RateLimiter,
) -> ReadinessCheck {
    if config.rate_limit.backend == RateLimitBackend::Redis {
        if config.rate_limit.redis_url.is_none() {
            return ReadinessCheck::unavailable(
                "rate_limit_store",
                "distributed rate-limit store is required for this environment",
            );
        }

        return match rate_limiter.check_store_available().await {
            Ok(()) => ReadinessCheck::ready("rate_limit_store"),
            Err(_) => ReadinessCheck::unavailable(
                "rate_limit_store",
                "distributed rate-limit store is unavailable",
            ),
        };
    }

    if config.rate_limit.distributed_required {
        return ReadinessCheck::unavailable(
            "rate_limit_store",
            "distributed rate-limit store is required for this environment",
        );
    }

    ReadinessCheck::ready("rate_limit_store")
}

fn readiness_for_tile_manifest_storage(config: &AppConfig) -> ReadinessCheck {
    if config.tile_manifest_storage.is_some() {
        return ReadinessCheck::ready("tile_manifest_storage");
    }

    if config.runtime_environment == RuntimeEnvironment::Local {
        return ReadinessCheck::ready("tile_manifest_storage");
    }

    ReadinessCheck::unavailable(
        "tile_manifest_storage",
        "tile manifest storage is not configured; public manifest routes will return 503",
    )
}

fn readiness_for_database(config: &AppConfig) -> ReadinessCheck {
    if config.database.is_some() || config.runtime_environment == RuntimeEnvironment::Local {
        return ReadinessCheck::ready("database");
    }

    ReadinessCheck::unavailable(
        "database",
        "database is not configured; tile set and admin run routes will return 503",
    )
}

fn readiness_for_processing_queue(config: &AppConfig) -> ReadinessCheck {
    if config.processing_queue.is_some() || config.runtime_environment == RuntimeEnvironment::Local
    {
        return ReadinessCheck::ready("processing_queue");
    }

    ReadinessCheck::unavailable(
        "processing_queue",
        "processing queue is not configured; admin requeue route will return 503",
    )
}

fn readiness_for_ingest_admin(config: &AppConfig) -> ReadinessCheck {
    if config.ingest_admin.is_some() || config.runtime_environment == RuntimeEnvironment::Local {
        return ReadinessCheck::ready("ingest_admin");
    }

    ReadinessCheck::unavailable(
        "ingest_admin",
        "ingest service base URL is not configured; admin ingest trigger will return 503",
    )
}

fn readiness_for_internal_service_auth(config: &AppConfig) -> ReadinessCheck {
    if config.ingest_admin.is_none() {
        return ReadinessCheck::ready("internal_service_auth");
    }

    if config.internal_service_auth.is_some()
        || config.runtime_environment == RuntimeEnvironment::Local
    {
        return ReadinessCheck::ready("internal_service_auth");
    }

    ReadinessCheck::unavailable(
        "internal_service_auth",
        "internal service auth is not configured for admin upstream calls",
    )
}

#[cfg(test)]
mod tests {
    use super::{ReadinessCheck, ReadinessReport};

    #[test]
    fn report_is_ready_when_all_checks_are_ready() {
        let report = ReadinessReport::new(vec![
            ReadinessCheck::ready("configuration"),
            ReadinessCheck::ready("rate_limit_store"),
        ]);

        assert!(report.is_ready());
        assert_eq!(report.status, "ready");
    }

    #[test]
    fn report_is_not_ready_when_any_check_is_unavailable() {
        let report = ReadinessReport::new(vec![
            ReadinessCheck::ready("configuration"),
            ReadinessCheck::unavailable("rate_limit_store", "unavailable"),
        ]);

        assert!(!report.is_ready());
        assert_eq!(report.status, "not_ready");
    }
}
