use std::sync::Arc;

use crate::{
    auth::AuthService,
    config::AppConfig,
    db::{GatewayDatabase, GatewayDatabaseClient},
    rate_limit::RateLimiter,
    readiness::ReadinessProbe,
    storage::{TileManifestStorage, TileManifestStorageClient},
    upstream::{IngestAdminClient, ProcessingQueueClient},
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub readiness: ReadinessProbe,
    pub auth: Arc<AuthService>,
    pub rate_limiter: Arc<RateLimiter>,
    pub database: Option<Arc<dyn GatewayDatabaseClient>>,
    pub tile_manifest_storage: Option<Arc<dyn TileManifestStorage>>,
    pub ingest_admin: Option<Arc<IngestAdminClient>>,
    pub processing_queue: Option<Arc<ProcessingQueueClient>>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let auth = Arc::new(AuthService::new(config.auth.clone()));
        let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit.clone()));
        let readiness = ReadinessProbe::from_config(config.clone(), rate_limiter.clone());
        let database: Option<Arc<dyn GatewayDatabaseClient>> =
            config.database.as_ref().and_then(|database_config| {
                GatewayDatabase::new(database_config)
                .map(|database| Arc::new(database) as Arc<dyn GatewayDatabaseClient>)
                .map_err(|error| {
                    tracing::error!(error = %error, "failed to initialize gateway database client");
                    error
                })
                .ok()
            });
        let tile_manifest_storage: Option<Arc<dyn TileManifestStorage>> = config
            .tile_manifest_storage
            .as_ref()
            .and_then(|storage_config| {
                TileManifestStorageClient::new(
                    storage_config,
                    config.public_timeout,
                    config.http_retry,
                )
                .map(|storage| Arc::new(storage) as Arc<dyn TileManifestStorage>)
                .map_err(|error| {
                    tracing::error!(error = %error, "failed to initialize tile manifest storage client");
                    error
                })
                .ok()
            });
        let ingest_admin = config.ingest_admin.as_ref().and_then(|ingest_config| {
            IngestAdminClient::new(
                ingest_config,
                config.internal_service_auth.as_ref(),
                config.admin_timeout,
                config.http_retry,
            )
            .map(Arc::new)
            .map_err(|error| {
                tracing::error!(error = %error, "failed to initialize ingest admin client");
                error
            })
            .ok()
        });
        let processing_queue = config.processing_queue.as_ref().and_then(|queue_config| {
            ProcessingQueueClient::new(queue_config, config.admin_timeout, config.http_retry)
                .map(Arc::new)
                .map_err(|error| {
                    tracing::error!(error = %error, "failed to initialize processing queue client");
                    error
                })
                .ok()
        });

        Self {
            config,
            readiness,
            auth,
            rate_limiter,
            database,
            tile_manifest_storage,
            ingest_admin,
            processing_queue,
        }
    }
}
