mod audit;
mod middleware;
mod routes;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::{
    http::HeaderName,
    middleware as axum_middleware,
    routing::{get, post},
    Router,
};

use crate::state::AppState;

use self::{
    middleware::{request_context, require_admin},
    routes::{
        deep_health, health, latest_tile_manifest, list_ingest_runs, list_processing_runs,
        list_tile_sets, not_found, ready, requeue_processing_item, search_sites, site_detail,
        site_score, tile_classes, tile_manifest_by_id, tile_redirect, trigger_ingest,
    },
};

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AdminRoleRequirement {
    ConfiguredAdmin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AdminRoutePolicy {
    pub method: &'static str,
    pub path: &'static str,
    pub action: &'static str,
    pub role: AdminRoleRequirement,
}

pub(super) const ADMIN_ROUTE_POLICIES: &[AdminRoutePolicy] = &[
    AdminRoutePolicy {
        method: "post",
        path: "/api/v1/admin/ingest/trigger",
        action: "ingest.trigger",
        role: AdminRoleRequirement::ConfiguredAdmin,
    },
    AdminRoutePolicy {
        method: "get",
        path: "/api/v1/admin/ingest/runs",
        action: "ingest.runs.list",
        role: AdminRoleRequirement::ConfiguredAdmin,
    },
    AdminRoutePolicy {
        method: "get",
        path: "/api/v1/admin/processing/runs",
        action: "processing.runs.list",
        role: AdminRoleRequirement::ConfiguredAdmin,
    },
    AdminRoutePolicy {
        method: "post",
        path: "/api/v1/admin/processing/requeue",
        action: "processing.requeue",
        role: AdminRoleRequirement::ConfiguredAdmin,
    },
    AdminRoutePolicy {
        method: "get",
        path: "/api/v1/admin/health/deep",
        action: "health.deep",
        role: AdminRoleRequirement::ConfiguredAdmin,
    },
];

pub(super) fn admin_route_policy_for_path(path: &str) -> Option<&'static AdminRoutePolicy> {
    ADMIN_ROUTE_POLICIES
        .iter()
        .find(|policy| policy.path == path)
}

pub async fn serve(state: AppState) -> Result<(), ServerError> {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), state.config.port);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|source| ServerError::Bind { address, source })?;

    tracing::info!(address = %address, "api-gateway HTTP server listening");

    axum::serve(listener, router(state))
        .await
        .map_err(ServerError::Serve)
}

pub fn router(state: AppState) -> Router {
    let admin_routes = Router::new()
        .route("/ingest/trigger", post(trigger_ingest))
        .route("/ingest/runs", get(list_ingest_runs))
        .route("/processing/runs", get(list_processing_runs))
        .route("/processing/requeue", post(requeue_processing_item))
        .route("/health/deep", get(deep_health))
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            require_admin,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/tiles/manifest", get(latest_tile_manifest))
        .route(
            "/api/v1/tiles/manifest/{tile_set_id}",
            get(tile_manifest_by_id),
        )
        .route("/api/v1/tiles/sets", get(list_tile_sets))
        .route("/api/v1/tiles/classes", get(tile_classes))
        .route(
            "/api/v1/tiles/{tile_set_id}/{z}/{x}/{*tile_file}",
            get(tile_redirect),
        )
        .route("/api/v1/sites", get(search_sites))
        .route("/api/v1/sites/{site_id}", get(site_detail))
        .route("/api/v1/sites/{site_id}/score", get(site_score))
        .nest("/api/v1/admin", admin_routes)
        .fallback(not_found)
        .with_state(state.clone())
        .layer(axum_middleware::from_fn_with_state(state, request_context))
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
    use std::{
        collections::{BTreeSet, HashMap},
        sync::Arc,
        time::Duration,
    };

    use axum::{
        body::{to_bytes, Body},
        http::{header, Method, Request, StatusCode},
        response::Response,
    };
    use chrono::{NaiveDate, TimeZone, Utc};
    use serde_json::Value;
    use serde_yaml::{Mapping, Value as YamlValue};
    use tower::ServiceExt;

    use super::{router, AdminRoleRequirement, ADMIN_ROUTE_POLICIES, REQUEST_ID_HEADER};
    use crate::{
        config::AppConfig,
        db::{
            DbError, DbFuture, GatewayDatabaseClient, IngestRunSummary, ProcessingRequeueRecord,
            ProcessingRunSummary, TileSetSummary,
        },
        state::AppState,
        storage::{StorageError, StorageFuture, TileManifestStorage},
    };

    fn test_state() -> AppState {
        test_state_with_config(&[])
    }

    fn test_state_with_config(overrides: &[(&str, &str)]) -> AppState {
        let config = AppConfig::from_lookup(|name| match name {
            "JWT_ISSUER" => Some("https://login.microsoftonline.com/test/v2.0".to_owned()),
            "JWT_AUDIENCE" => Some("api://lumenhorizon-admin".to_owned()),
            "JWKS_URL" => {
                Some("https://login.microsoftonline.com/test/discovery/v2.0/keys".to_owned())
            }
            _ => overrides
                .iter()
                .find_map(|(key, value)| (*key == name).then(|| (*value).to_owned())),
        })
        .unwrap();

        AppState::new(config)
    }

    fn test_state_with_storage(storage: FakeTileManifestStorage) -> AppState {
        let mut state = test_state();
        state.tile_manifest_storage = Some(Arc::new(storage));
        state
    }

    fn test_state_with_database(database: FakeGatewayDatabase) -> AppState {
        let mut state = test_state();
        state.database = Some(Arc::new(database));
        state
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum AuthRequirement {
        Anonymous,
        AdminJwt,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct RouteContract {
        method: &'static str,
        path: &'static str,
        auth: AuthRequirement,
    }

    const API_ROUTE_CONTRACT: &[RouteContract] = &[
        RouteContract {
            method: "get",
            path: "/api/v1/tiles/manifest",
            auth: AuthRequirement::Anonymous,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/tiles/manifest/{tile_set_id}",
            auth: AuthRequirement::Anonymous,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/tiles/sets",
            auth: AuthRequirement::Anonymous,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/tiles/classes",
            auth: AuthRequirement::Anonymous,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/tiles/{tile_set_id}/{z}/{x}/{y}.png",
            auth: AuthRequirement::Anonymous,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/sites",
            auth: AuthRequirement::Anonymous,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/sites/{site_id}",
            auth: AuthRequirement::Anonymous,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/sites/{site_id}/score",
            auth: AuthRequirement::Anonymous,
        },
        RouteContract {
            method: "post",
            path: "/api/v1/admin/ingest/trigger",
            auth: AuthRequirement::AdminJwt,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/admin/ingest/runs",
            auth: AuthRequirement::AdminJwt,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/admin/processing/runs",
            auth: AuthRequirement::AdminJwt,
        },
        RouteContract {
            method: "post",
            path: "/api/v1/admin/processing/requeue",
            auth: AuthRequirement::AdminJwt,
        },
        RouteContract {
            method: "get",
            path: "/api/v1/admin/health/deep",
            auth: AuthRequirement::AdminJwt,
        },
    ];

    #[test]
    fn openapi_contract_covers_gateway_api_routes() {
        let document: YamlValue = serde_yaml::from_str(include_str!("../openapi/openapi.yaml"))
            .expect("OpenAPI document should parse as YAML");
        let root = document
            .as_mapping()
            .expect("OpenAPI document should be a mapping");

        assert_eq!(string_field(root, "openapi"), Some("3.1.0"));
        assert_admin_jwt_scheme(root);

        let paths = mapping_field(root, "paths").expect("OpenAPI document should define paths");
        let documented_paths = paths
            .keys()
            .map(|key| key.as_str().expect("OpenAPI path keys should be strings"))
            .collect::<BTreeSet<_>>();
        let expected_paths = API_ROUTE_CONTRACT
            .iter()
            .map(|route| route.path)
            .collect::<BTreeSet<_>>();

        assert_eq!(documented_paths, expected_paths);

        for route in API_ROUTE_CONTRACT {
            let path_item = paths
                .get(YamlValue::String(route.path.to_owned()))
                .and_then(YamlValue::as_mapping)
                .unwrap_or_else(|| panic!("OpenAPI path missing: {}", route.path));
            let operation = path_item
                .get(YamlValue::String(route.method.to_owned()))
                .and_then(YamlValue::as_mapping)
                .unwrap_or_else(|| {
                    panic!("OpenAPI operation missing: {} {}", route.method, route.path)
                });

            assert_route_security(route, operation);
            assert_rate_limit_response(route, operation);
            if route.auth == AuthRequirement::AdminJwt {
                assert_admin_error_responses(route, operation);
            }
        }
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
        assert_security_headers(response.headers());
        assert!(response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value, serde_json::json!({ "status": "healthy" }));
    }

    #[tokio::test]
    async fn manifest_route_reports_unconfigured_storage_with_request_id() {
        let request_id = uuid::Uuid::new_v4();
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/tiles/manifest")
                    .header(REQUEST_ID_HEADER, request_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers().get(REQUEST_ID_HEADER).unwrap(),
            request_id.to_string().as_str()
        );
        assert_security_headers(response.headers());

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert!(value["data"].is_null());
        assert_eq!(value["meta"]["request_id"], request_id.to_string());
        assert_eq!(value["error"]["code"], "service_unavailable");
        assert_eq!(
            value["error"]["message"],
            "tile manifest storage is not configured"
        );
    }

    #[tokio::test]
    async fn latest_manifest_returns_manifest_with_short_cache_header() {
        let response = router(test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("tile-set-1")),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/manifest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=300, must-revalidate"
        );
        assert_security_headers(response.headers());

        let value = json_body(response).await;
        assert_eq!(value["data"]["tile_set_id"], "tile-set-1");
        assert!(value["meta"]["request_id"].as_str().is_some());
        assert!(value["meta"]["timestamp"].as_str().is_some());
        assert!(value["error"].is_null());
    }

    #[tokio::test]
    async fn latest_manifest_can_select_product_scoped_pointer() {
        let response = router(test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("public-latest"))
                .with_product_manifest("VNP46A2", sample_manifest("daily-latest")),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/manifest?product=VNP46A2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let value = json_body(response).await;
        assert_eq!(value["data"]["tile_set_id"], "daily-latest");
        assert!(value["error"].is_null());
    }

    #[tokio::test]
    async fn latest_manifest_rejects_invalid_product_query() {
        let response = router(test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("public-latest")),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/manifest?product=UNKNOWN")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let value = json_body(response).await;
        assert_eq!(value["error"]["code"], "invalid_request");
        assert_eq!(value["error"]["message"], "invalid product");
    }

    #[tokio::test]
    async fn immutable_manifest_returns_manifest_with_immutable_cache_header() {
        let response = router(test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("tile-set-1")),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/manifest/tile-set-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );

        let value = json_body(response).await;
        assert_eq!(value["data"]["tile_set_id"], "tile-set-1");
        assert!(value["error"].is_null());
    }

    #[tokio::test]
    async fn tile_redirect_substitutes_manifest_template_for_mapkit_coordinates() {
        let response = router(test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("tile-set-1")),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/tile-set-1/3/1/2.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "https://tiles.lumenhorizon.com/tiles/tile-set-1/3/1/2.png"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
    }

    #[tokio::test]
    async fn tile_redirect_rejects_display_overzoom_requests() {
        let response = router(test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("tile-set-1")),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/tile-set-1/5/8/12.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let value = json_body(response).await;
        assert_eq!(value["error"]["code"], "invalid_request");
        assert_eq!(
            value["error"]["message"],
            "tile zoom is outside tile set range"
        );
    }

    #[tokio::test]
    async fn tile_redirect_returns_not_found_for_coordinates_outside_manifest_bounds() {
        let response = router(test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("tile-set-1")),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/tile-set-1/3/7/7.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let value = json_body(response).await;
        assert_eq!(value["error"]["code"], "tile_not_found");
        assert_eq!(value["error"]["message"], "tile is outside tile set bounds");
    }

    #[tokio::test]
    async fn tile_redirect_rejects_impossible_coordinates() {
        let response = router(test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("tile-set-1")),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/tile-set-1/3/8/7.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let value = json_body(response).await;
        assert_eq!(value["error"]["code"], "invalid_request");
        assert_eq!(
            value["error"]["message"],
            "tile coordinate is outside zoom range"
        );
    }

    #[tokio::test]
    async fn tile_sets_are_paginated_with_opaque_cursor() {
        let response = router(test_state_with_database(FakeGatewayDatabase {
            tile_sets: vec![
                sample_tile_set("tile-set-latest", true),
                sample_tile_set("tile-set-older", false),
            ],
        }))
        .oneshot(
            Request::builder()
                .uri("/api/v1/tiles/sets?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let value = json_body(response).await;
        assert_eq!(value["data"].as_array().unwrap().len(), 1);
        assert_eq!(value["data"][0]["tile_set_id"], "tile-set-latest");
        assert_eq!(value["meta"]["next_cursor"], "1");
        assert!(value["error"].is_null());
    }

    #[tokio::test]
    async fn tile_sets_reject_invalid_pagination_parameters() {
        for uri in [
            "/api/v1/tiles/sets?limit=0",
            "/api/v1/tiles/sets?limit=101",
            "/api/v1/tiles/sets?cursor=-1",
            "/api/v1/tiles/sets?cursor=not-a-cursor",
            "/api/v1/tiles/sets?limit=1&limit=2",
            "/api/v1/tiles/sets?unknown=true",
        ] {
            let response = router(test_state_with_database(FakeGatewayDatabase::default()))
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");

            let value = json_body(response).await;
            assert_eq!(value["error"]["code"], "invalid_request", "{uri}");
        }
    }

    #[tokio::test]
    async fn invalid_site_parameters_are_rejected_before_deferred_response() {
        let cases = [
            "/api/v1/sites/not-a-uuid",
            "/api/v1/sites/not-a-uuid/score",
            "/api/v1/sites?lat=91&lon=10",
            "/api/v1/sites?lat=10",
            "/api/v1/sites?west=-120&south=30&east=-130&north=40",
            "/api/v1/sites?unknown=true",
        ];

        for uri in cases {
            let response = router(test_state())
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
            let value = json_body(response).await;
            assert_eq!(value["error"]["code"], "invalid_request", "{uri}");
            assert!(value["error"].get("details").is_none(), "{uri}");
        }
    }

    #[tokio::test]
    async fn public_api_routes_are_anonymous() {
        let cases = [
            (
                "/api/v1/tiles/manifest",
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
            ),
            (
                "/api/v1/tiles/manifest/tile-set-1",
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
            ),
            (
                "/api/v1/tiles/sets",
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
            ),
            ("/api/v1/tiles/classes", StatusCode::OK, ""),
            (
                "/api/v1/tiles/tile-set-1/3/1/2.png",
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
            ),
            (
                "/api/v1/sites",
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
            ),
            (
                "/api/v1/sites/00000000-0000-0000-0000-000000000001",
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
            ),
            (
                "/api/v1/sites/00000000-0000-0000-0000-000000000001/score",
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
            ),
        ];

        for (uri, expected_status, expected_error_code) in cases {
            let response = router(test_state())
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(
                response.status(),
                expected_status,
                "unexpected status for {uri}"
            );
            assert_ne!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
            assert_ne!(response.status(), StatusCode::FORBIDDEN, "{uri}");
            assert_security_headers(response.headers());

            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            if expected_status.is_success() {
                assert!(value["error"].is_null(), "unexpected error for {uri}");
            } else {
                assert_eq!(value["error"]["code"], expected_error_code, "{uri}");
                assert!(value["error"].get("details").is_none(), "{uri}");
            }
        }
    }

    #[tokio::test]
    async fn public_tile_metadata_rate_limit_returns_retry_after() {
        let state = test_state();

        for _ in 0..180 {
            let response = router(state.clone())
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/tiles/classes")
                        .header("x-forwarded-for", "203.0.113.10")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/tiles/classes")
                    .header("x-forwarded-for", "203.0.113.10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key(header::RETRY_AFTER));

        let value = json_body(response).await;
        assert_eq!(value["error"]["code"], "rate_limited");
        assert_eq!(value["error"]["message"], "rate limit exceeded");
    }

    #[tokio::test]
    async fn tile_redirect_rate_limit_uses_separate_route_class() {
        let state = test_state_with_storage(FakeTileManifestStorage::with_manifest(
            sample_manifest("tile-set-1"),
        ));

        for _ in 0..720 {
            let response = router(state.clone())
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/tiles/tile-set-1/3/1/2.png")
                        .header("x-forwarded-for", "203.0.113.11")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FOUND);
        }

        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/tiles/tile-set-1/3/1/2.png")
                    .header("x-forwarded-for", "203.0.113.11")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key(header::RETRY_AFTER));
        assert_eq!(json_body(response).await["error"]["code"], "rate_limited");
    }

    #[tokio::test]
    async fn authentication_failures_are_rate_limited_by_source() {
        let state = test_state();

        for _ in 0..15 {
            let response = router(state.clone())
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/v1/admin/ingest/trigger")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("x-forwarded-for", "203.0.113.12")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }

        let response = router(state)
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/ingest/trigger")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-forwarded-for", "203.0.113.12")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key(header::RETRY_AFTER));
        assert_eq!(json_body(response).await["error"]["code"], "rate_limited");
    }

    #[tokio::test]
    async fn admin_routes_reject_anonymous_callers() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/ingest/trigger")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value["error"]["code"], "unauthenticated");
        assert_eq!(value["error"]["message"], "authentication required");
    }

    #[tokio::test]
    async fn all_admin_routes_reject_anonymous_callers() {
        let cases = [
            (Method::POST, "/api/v1/admin/ingest/trigger"),
            (Method::GET, "/api/v1/admin/ingest/runs"),
            (Method::GET, "/api/v1/admin/processing/runs"),
            (Method::POST, "/api/v1/admin/processing/requeue"),
            (Method::GET, "/api/v1/admin/health/deep"),
        ];

        for (method, uri) in cases {
            let builder = Request::builder().method(method.clone()).uri(uri);
            let request = if method == Method::POST {
                builder
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap()
            } else {
                builder.body(Body::empty()).unwrap()
            };

            let response = router(test_state()).oneshot(request).await.unwrap();

            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {uri}"
            );
            assert_eq!(
                response.headers().get(header::CACHE_CONTROL).unwrap(),
                "no-store"
            );
            assert_security_headers(response.headers());

            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["error"]["code"], "unauthenticated", "{method} {uri}");
            assert!(value["error"].get("details").is_none(), "{method} {uri}");
        }
    }

    #[tokio::test]
    async fn immutable_manifest_route_uses_immutable_cache_header() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/tiles/manifest/tile-set-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
    }

    #[tokio::test]
    async fn admin_writes_require_json_content_type_for_bodies() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/ingest/trigger")
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value["error"]["code"], "invalid_request");
        assert_eq!(
            value["error"]["message"],
            "admin writes require application/json"
        );
    }

    #[tokio::test]
    async fn unsupported_methods_return_sanitized_envelope() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/tiles/classes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_security_headers(response.headers());

        let value = json_body(response).await;
        assert_eq!(value["error"]["code"], "invalid_request");
        assert_eq!(
            value["error"]["message"],
            "method is not supported for this route"
        );
    }

    #[tokio::test]
    async fn route_timeout_returns_sanitized_envelope() {
        let mut state = test_state_with_storage(
            FakeTileManifestStorage::with_manifest(sample_manifest("tile-set-1"))
                .with_delay(Duration::from_millis(25)),
        );
        state.config.public_timeout = Duration::from_millis(1);

        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/tiles/manifest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_security_headers(response.headers());

        let value = json_body(response).await;
        assert_eq!(value["error"]["code"], "service_unavailable");
        assert_eq!(value["error"]["message"], "request timed out");
        assert!(value["error"].get("details").is_none());
    }

    #[tokio::test]
    async fn get_request_bodies_are_rejected() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/tiles/classes")
                    .header(header::CONTENT_LENGTH, "2")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value["error"]["code"], "invalid_request");
        assert_eq!(
            value["error"]["message"],
            "GET request bodies are not accepted"
        );
    }

    fn assert_security_headers(headers: &axum::http::HeaderMap) {
        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
        assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
    }

    #[test]
    fn admin_route_policies_keep_single_launch_role() {
        let expected_admin_routes = API_ROUTE_CONTRACT
            .iter()
            .filter(|route| route.auth == AuthRequirement::AdminJwt)
            .map(|route| (route.method, route.path))
            .collect::<BTreeSet<_>>();
        let policy_routes = ADMIN_ROUTE_POLICIES
            .iter()
            .map(|policy| {
                assert_eq!(policy.role, AdminRoleRequirement::ConfiguredAdmin);
                (policy.method, policy.path)
            })
            .collect::<BTreeSet<_>>();

        assert_eq!(policy_routes, expected_admin_routes);
    }

    async fn json_body(response: Response) -> Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn sample_manifest(tile_set_id: &str) -> Value {
        serde_json::json!({
            "tile_set_id": tile_set_id,
            "dataset_date": "2026-05-21",
            "generated_at": "2026-05-21T09:15:00Z",
            "classification_version": "radiance-dark-sky-v1",
            "render_version": "tiles-v1",
            "processor_version": "processing-svc:test",
            "format": "png",
            "tile_size": 256,
            "min_zoom": 3,
            "max_native_zoom": 4,
            "max_display_zoom": 6,
            "bounds": {
                "west": -125.0,
                "south": 24.0,
                "east": -66.0,
                "north": 50.0
            },
            "tile_url_template": format!("https://tiles.lumenhorizon.com/tiles/{tile_set_id}/{{z}}/{{x}}/{{y}}.png"),
            "tile_count": 42,
            "source_granules": [],
            "checksums": {
                "manifest_sha256": "test-manifest-sha256"
            }
        })
    }

    fn sample_tile_set(tile_set_id: &str, latest: bool) -> TileSetSummary {
        TileSetSummary {
            tile_set_id: tile_set_id.to_owned(),
            dataset_date: NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
            classification_version: "radiance-dark-sky-v1".to_owned(),
            render_version: "tiles-v1".to_owned(),
            format: "png".to_owned(),
            min_zoom: 3,
            max_native_zoom: 4,
            max_display_zoom: 6,
            bounds: serde_json::json!({
                "west": -125.0,
                "south": 24.0,
                "east": -66.0,
                "north": 50.0
            }),
            tile_count: 42,
            manifest_blob_path: format!("manifests/{tile_set_id}.json"),
            latest,
            product: Some("VNP46A2".to_owned()),
            cadence: Some("daily".to_owned()),
            tile_set_kind: "mosaic".to_owned(),
            product_latest: latest,
            created_at: Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
        }
    }

    #[derive(Clone)]
    struct FakeTileManifestStorage {
        latest_manifest: Value,
        product_latest_manifests: HashMap<String, Value>,
        manifests: HashMap<String, Value>,
        delay: Option<Duration>,
    }

    impl FakeTileManifestStorage {
        fn with_manifest(manifest: Value) -> Self {
            let tile_set_id = manifest["tile_set_id"].as_str().unwrap().to_owned();
            Self {
                latest_manifest: manifest.clone(),
                product_latest_manifests: HashMap::new(),
                manifests: HashMap::from([(tile_set_id, manifest)]),
                delay: None,
            }
        }

        fn with_product_manifest(mut self, product: &str, manifest: Value) -> Self {
            let tile_set_id = manifest["tile_set_id"].as_str().unwrap().to_owned();
            self.product_latest_manifests
                .insert(product.to_owned(), manifest.clone());
            self.manifests.insert(tile_set_id, manifest);
            self
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = Some(delay);
            self
        }
    }

    impl TileManifestStorage for FakeTileManifestStorage {
        fn latest_manifest(&self) -> StorageFuture<'_, Value> {
            let manifest = self.latest_manifest.clone();
            let delay = self.delay;
            Box::pin(async move {
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
                Ok(manifest)
            })
        }

        fn latest_manifest_for_product<'a>(&'a self, product: &'a str) -> StorageFuture<'a, Value> {
            let delay = self.delay;
            let result = self
                .product_latest_manifests
                .get(product)
                .cloned()
                .ok_or_else(|| StorageError::BlobStatus {
                    blob_path: format!("manifests/latest/{product}.json"),
                    status: reqwest::StatusCode::NOT_FOUND,
                    body: "not found".to_owned(),
                });
            Box::pin(async move {
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
                result
            })
        }

        fn manifest_by_id<'a>(&'a self, tile_set_id: &'a str) -> StorageFuture<'a, Value> {
            let delay = self.delay;
            let result =
                self.manifests
                    .get(tile_set_id)
                    .cloned()
                    .ok_or_else(|| StorageError::BlobStatus {
                        blob_path: format!("manifests/{tile_set_id}.json"),
                        status: reqwest::StatusCode::NOT_FOUND,
                        body: "not found".to_owned(),
                    });
            Box::pin(async move {
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
                result
            })
        }
    }

    #[derive(Default)]
    struct FakeGatewayDatabase {
        tile_sets: Vec<TileSetSummary>,
    }

    impl GatewayDatabaseClient for FakeGatewayDatabase {
        fn check(&self) -> DbFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }

        fn list_tile_sets(&self, limit: i64, offset: i64) -> DbFuture<'_, Vec<TileSetSummary>> {
            let rows = self
                .tile_sets
                .iter()
                .skip(offset as usize)
                .take(limit as usize)
                .cloned()
                .collect::<Vec<_>>();
            Box::pin(async move { Ok(rows) })
        }

        fn list_ingest_runs(
            &self,
            _limit: i64,
            _offset: i64,
        ) -> DbFuture<'_, Vec<IngestRunSummary>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn list_processing_runs(
            &self,
            _limit: i64,
            _offset: i64,
        ) -> DbFuture<'_, Vec<ProcessingRunSummary>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn processing_message_for_requeue(
            &self,
            _ingest_id: uuid::Uuid,
        ) -> DbFuture<'_, ProcessingRequeueRecord> {
            Box::pin(async { Err(DbError::NotFound) })
        }
    }

    fn mapping_field<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Mapping> {
        mapping
            .get(YamlValue::String(key.to_owned()))
            .and_then(YamlValue::as_mapping)
    }

    fn sequence_field<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Vec<YamlValue>> {
        mapping
            .get(YamlValue::String(key.to_owned()))
            .and_then(YamlValue::as_sequence)
    }

    fn string_field<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a str> {
        mapping
            .get(YamlValue::String(key.to_owned()))
            .and_then(YamlValue::as_str)
    }

    fn assert_admin_jwt_scheme(root: &Mapping) {
        let components = mapping_field(root, "components").expect("components should exist");
        let schemes =
            mapping_field(components, "securitySchemes").expect("security schemes should exist");
        let admin_jwt = mapping_field(schemes, "AdminJwt").expect("AdminJwt scheme should exist");

        assert_eq!(string_field(admin_jwt, "type"), Some("http"));
        assert_eq!(string_field(admin_jwt, "scheme"), Some("bearer"));
        assert_eq!(string_field(admin_jwt, "bearerFormat"), Some("JWT"));
    }

    fn assert_route_security(route: &RouteContract, operation: &Mapping) {
        let security = sequence_field(operation, "security")
            .unwrap_or_else(|| panic!("security missing for {} {}", route.method, route.path));

        match route.auth {
            AuthRequirement::Anonymous => {
                assert!(
                    security.is_empty(),
                    "anonymous route should declare empty security: {} {}",
                    route.method,
                    route.path
                );
            }
            AuthRequirement::AdminJwt => {
                assert_eq!(
                    security.len(),
                    1,
                    "admin route should declare AdminJwt only"
                );
                let scheme = security[0]
                    .as_mapping()
                    .and_then(|entry| entry.get(YamlValue::String("AdminJwt".to_owned())))
                    .unwrap_or_else(|| {
                        panic!("admin route should require AdminJwt: {}", route.path)
                    });
                assert!(
                    scheme.as_sequence().is_some(),
                    "AdminJwt value should be a scope list"
                );
            }
        }
    }

    fn assert_rate_limit_response(route: &RouteContract, operation: &Mapping) {
        let responses = mapping_field(operation, "responses")
            .unwrap_or_else(|| panic!("responses missing for {} {}", route.method, route.path));

        assert!(
            responses.contains_key(YamlValue::String("429".to_owned())),
            "route should document rate-limit response: {} {}",
            route.method,
            route.path
        );
    }

    fn assert_admin_error_responses(route: &RouteContract, operation: &Mapping) {
        let responses = mapping_field(operation, "responses")
            .unwrap_or_else(|| panic!("responses missing for {} {}", route.method, route.path));

        for status in ["401", "403"] {
            assert!(
                responses.contains_key(YamlValue::String(status.to_owned())),
                "admin route should document {status}: {} {}",
                route.method,
                route.path
            );
        }
    }
}
