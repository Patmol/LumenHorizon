use sqlx::PgPool;

use crate::{config::AppConfig, readiness::ReadinessProbe};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub pool: PgPool,
    pub readiness: ReadinessProbe,
}

impl AppState {
    pub fn new(config: AppConfig, pool: PgPool) -> Self {
        let readiness = ReadinessProbe::from_dependencies(config.clone(), pool.clone());

        Self {
            config,
            pool,
            readiness,
        }
    }

    #[cfg(test)]
    pub fn with_readiness(config: AppConfig, pool: PgPool, readiness: ReadinessProbe) -> Self {
        Self {
            config,
            pool,
            readiness,
        }
    }
}
