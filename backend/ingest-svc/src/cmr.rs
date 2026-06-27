use std::collections::HashMap;

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::Deserialize;
use url::Url;

use crate::{
    clients::{
        build_http_client, is_retryable_status, retry_async, RetryContext, RetryIdempotency,
    },
    config::AppConfig,
    models::{GranuleCandidate, TileCoordinate},
};

const CMR_GRANULES_URL: &str = "https://cmr.earthdata.nasa.gov/search/granules.json";
const CMR_DATA_LINK_REL: &str = concat!("http", "://esipfed.org/ns/fedsearch/1.1/data#");
const DEFAULT_TEMPORAL_START: &str = "2024-01-01T00:00:00Z";
const PAGE_SIZE: usize = 200;
const PAGE_SIZE_STR: &str = "200";

#[derive(Debug, Clone)]
pub struct CmrClient {
    http: reqwest::Client,
}

impl CmrClient {
    pub fn new(config: &AppConfig) -> Result<Self, CmrError> {
        let http = build_http_client(config.http_request_timeout).map_err(CmrError::BuildClient)?;

        Ok(Self { http })
    }

    pub async fn discover(
        &self,
        config: &AppConfig,
        resume_points: &HashMap<String, DateTime<Utc>>,
    ) -> Result<DiscoverySummary, CmrError> {
        let mut products = Vec::with_capacity(config.ingest_products.len());

        for product in &config.ingest_products {
            let resume_from = resume_points.get(product).copied();
            let discovery = self.discover_product(config, product, resume_from).await?;

            tracing::info!(
                product = discovery.product,
                discovered_granules = discovery.granules.len(),
                skipped_entries = discovery.skipped_entries,
                pages_fetched = discovery.pages_fetched,
                "CMR discovery completed for product"
            );

            for granule in discovery.granules.iter().take(5) {
                tracing::info!(
                    product = granule.product,
                    granule_title = granule.title,
                    producer_granule_id = granule.producer_granule_id,
                    granule_date = %granule.granule_date,
                    tile_h = granule.tile.h,
                    tile_v = granule.tile.v,
                    "CMR candidate granule discovered"
                );
            }

            let omitted_granules = discovery.granules.len().saturating_sub(5);
            if omitted_granules > 0 {
                tracing::info!(
                    product = discovery.product,
                    omitted_granules,
                    "additional CMR candidate granules omitted from dry-run log sample"
                );
            }

            products.push(discovery);
        }

        Ok(DiscoverySummary { products })
    }

    async fn discover_product(
        &self,
        config: &AppConfig,
        product: &str,
        resume_from: Option<DateTime<Utc>>,
    ) -> Result<ProductDiscovery, CmrError> {
        let mut discovery = empty_product_discovery(product);
        let mut page_num = 1usize;

        loop {
            let url = discovery_url(config, product, page_num, resume_from)?;
            tracing::info!(product, page_num, url = %url, "requesting CMR granules page");

            let response = self.request_product_page(config, product, url).await?;
            let page_entries = append_page_discovery(&mut discovery, response, page_num);

            tracing::info!(
                product,
                page_num,
                page_entries,
                discovered_granules = discovery.granules.len(),
                skipped_entries = discovery.skipped_entries,
                "CMR discovery page processed"
            );

            if !should_fetch_next_page(page_entries) {
                break;
            }

            page_num += 1;
        }

        Ok(discovery)
    }

    async fn request_product_page(
        &self,
        config: &AppConfig,
        product: &str,
        url: Url,
    ) -> Result<CmrGranulesResponse, CmrError> {
        retry_async(
            config.http_retry,
            RetryContext {
                dependency: "cmr",
                operation: "granule_page_fetch",
                idempotency: RetryIdempotency::Idempotent,
            },
            || {
                let url = url.clone();

                async move { self.request_product_page_once(config, product, url).await }
            },
            CmrError::is_retryable,
        )
        .await
    }

    async fn request_product_page_once(
        &self,
        config: &AppConfig,
        product: &str,
        url: Url,
    ) -> Result<CmrGranulesResponse, CmrError> {
        let mut request = self.http.get(url);
        if let Some(token) = config
            .earthdata_bearer_token
            .as_deref()
            .filter(|token| !token.eq_ignore_ascii_case("replace-me"))
        {
            request = request.bearer_auth(token);
        }

        let response = request.send().await.map_err(CmrError::Request)?;
        let status = response.status();

        if !status.is_success() {
            return Err(CmrError::Status {
                product: product.to_owned(),
                status,
            });
        }

        let response = response
            .json::<CmrGranulesResponse>()
            .await
            .map_err(CmrError::Decode)?;

        Ok(response)
    }
}

fn parse_product_response(product: &str, response: CmrGranulesResponse) -> ProductDiscovery {
    let mut granules = Vec::new();
    let mut skipped_entries = 0usize;

    for entry in response.feed.entry {
        match parse_entry(product, entry) {
            Some(granule) => granules.push(granule),
            None => skipped_entries += 1,
        }
    }

    ProductDiscovery {
        product: product.to_owned(),
        granules,
        skipped_entries,
        pages_fetched: 1,
    }
}

fn empty_product_discovery(product: &str) -> ProductDiscovery {
    ProductDiscovery {
        product: product.to_owned(),
        granules: Vec::new(),
        skipped_entries: 0,
        pages_fetched: 0,
    }
}

fn append_page_discovery(
    aggregate: &mut ProductDiscovery,
    response: CmrGranulesResponse,
    page_num: usize,
) -> usize {
    let page_entries = response.entry_count();
    let page_discovery = parse_product_response(&aggregate.product, response);

    aggregate.granules.extend(page_discovery.granules);
    aggregate.skipped_entries += page_discovery.skipped_entries;
    aggregate.pages_fetched = page_num;

    page_entries
}

fn should_fetch_next_page(page_entries: usize) -> bool {
    page_entries >= PAGE_SIZE
}

fn parse_entry(product: &str, entry: CmrEntry) -> Option<GranuleCandidate> {
    let data_href = entry.data_href()?;
    let producer_granule_id = entry
        .producer_granule_id
        .clone()
        .unwrap_or_else(|| entry.title.clone());
    let tile = TileCoordinate::parse_from(&producer_granule_id)
        .or_else(|| TileCoordinate::parse_from(&entry.title))
        .or_else(|| TileCoordinate::parse_from(&data_href))?;
    let granule_date = DateTime::parse_from_rfc3339(&entry.time_start)
        .ok()?
        .with_timezone(&Utc);

    Some(GranuleCandidate {
        product: product.to_owned(),
        title: entry.title,
        producer_granule_id,
        data_href,
        granule_date,
        tile,
    })
}

fn discovery_url(
    config: &AppConfig,
    product: &str,
    page_num: usize,
    resume_from: Option<DateTime<Utc>>,
) -> Result<Url, CmrError> {
    let mut url = Url::parse(CMR_GRANULES_URL).map_err(CmrError::Url)?;
    let temporal_start = resume_from
        .map(|timestamp| timestamp.to_rfc3339())
        .unwrap_or_else(|| DEFAULT_TEMPORAL_START.to_owned());

    url.query_pairs_mut()
        .append_pair("short_name", product)
        .append_pair("temporal", &format!("{temporal_start},"))
        .append_pair("bounding_box", &config.bounding_box.to_string())
        .append_pair("page_size", PAGE_SIZE_STR)
        .append_pair("page_num", &page_num.to_string())
        .append_pair("sort_key", "-start_date");

    Ok(url)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverySummary {
    pub products: Vec<ProductDiscovery>,
}

impl DiscoverySummary {
    pub fn total_granules(&self) -> usize {
        self.products
            .iter()
            .map(|product| product.granules.len())
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDiscovery {
    pub product: String,
    pub granules: Vec<GranuleCandidate>,
    pub skipped_entries: usize,
    pub pages_fetched: usize,
}

#[derive(Debug, Deserialize)]
pub struct CmrGranulesResponse {
    feed: CmrFeed,
}

impl CmrGranulesResponse {
    fn entry_count(&self) -> usize {
        self.feed.entry.len()
    }
}

#[derive(Debug, Deserialize)]
struct CmrFeed {
    #[serde(default)]
    entry: Vec<CmrEntry>,
}

#[derive(Debug, Deserialize)]
pub struct CmrEntry {
    title: String,
    producer_granule_id: Option<String>,
    time_start: String,
    #[serde(default)]
    links: Vec<CmrLink>,
}

impl CmrEntry {
    fn data_href(&self) -> Option<String> {
        self.links
            .iter()
            .find(|link| {
                link.rel.as_deref() == Some(CMR_DATA_LINK_REL)
                    && link.href.to_ascii_lowercase().ends_with(".h5")
            })
            .map(|link| link.href.clone())
    }
}

#[derive(Debug, Deserialize)]
struct CmrLink {
    href: String,
    rel: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CmrError {
    #[error("CMR discovery error: failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("CMR discovery error: failed to decode response: {0}")]
    Decode(reqwest::Error),
    #[error("CMR discovery error: request failed: {0}")]
    Request(reqwest::Error),
    #[error("CMR discovery error: CMR returned {status} for product {product}")]
    Status { product: String, status: StatusCode },
    #[error("CMR discovery error: failed to build request URL: {0}")]
    Url(url::ParseError),
}

impl CmrError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Decode(error) => error.is_timeout() || error.is_body() || error.is_decode(),
            Self::Request(error) => error.is_timeout() || error.is_connect(),
            Self::Status { status, .. } => is_retryable_status(*status),
            Self::BuildClient(_) | Self::Url(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use axum::{
        http::{header::CONTENT_TYPE, StatusCode},
        response::IntoResponse,
        routing::get,
        Router,
    };
    use chrono::{TimeZone, Utc};

    use crate::config::AppConfig;

    use super::{
        append_page_discovery, discovery_url, empty_product_discovery, parse_product_response,
        should_fetch_next_page, CmrClient, CmrGranulesResponse, CMR_DATA_LINK_REL,
        DEFAULT_TEMPORAL_START,
    };

    const TEST_STORAGE_ACCESS_KEY: &str = "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5";

    fn test_config() -> AppConfig {
        AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some(TEST_STORAGE_ACCESS_KEY.to_owned()),
            _ => None,
        })
        .expect("test configuration should be valid")
    }

    fn fast_retry_config() -> AppConfig {
        AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some(TEST_STORAGE_ACCESS_KEY.to_owned()),
            "HTTP_RETRY_MAX_ATTEMPTS" => Some("2".to_owned()),
            "HTTP_RETRY_BASE_DELAY_MS" => Some("1".to_owned()),
            "HTTP_RETRY_MAX_DELAY_MS" => Some("1".to_owned()),
            _ => None,
        })
        .expect("test configuration should be valid")
    }

    fn query_param(url: &url::Url, name: &str) -> Option<String> {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    }

    #[tokio::test]
    async fn retries_cmr_page_decode_failure() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let app_attempts = Arc::clone(&attempts);
        let valid_body = Arc::new(
            serde_json::json!({
                "feed": {
                    "entry": [{
                        "title": "VNP46A2.A2024142.h11v06.002.2024143000000.h5",
                        "producer_granule_id": "VNP46A2.A2024142.h11v06.002.2024143000000.h5",
                        "time_start": "2024-05-21T00:00:00Z",
                        "links": [{
                            "rel": CMR_DATA_LINK_REL,
                            "href": "https://archive.example.test/VNP46A2.A2024142.h11v06.002.2024143000000.h5"
                        }]
                    }]
                }
            })
            .to_string(),
        );
        let app_valid_body = Arc::clone(&valid_body);
        let app = Router::new().route(
            "/granules.json",
            get(move || {
                let app_attempts = Arc::clone(&app_attempts);
                let app_valid_body = Arc::clone(&app_valid_body);

                async move {
                    let attempt = app_attempts.fetch_add(1, Ordering::SeqCst);
                    let body = if attempt == 0 {
                        "{".to_owned()
                    } else {
                        (*app_valid_body).clone()
                    };

                    (StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let config = fast_retry_config();
        let client = CmrClient::new(&config).unwrap();

        let response = client
            .request_product_page(
                &config,
                "VNP46A2",
                url::Url::parse(&format!("{}://{addr}/granules.json", "http")).unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.entry_count(), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        server.abort();
    }

    #[test]
    fn parses_cmr_entry_into_granule_candidate() {
        let response: CmrGranulesResponse = serde_json::from_value(serde_json::json!({
            "feed": {
                "entry": [{
                    "title": "VNP46A2.A2024142.h11v06.002.2024143000000.h5",
                    "producer_granule_id": "VNP46A2.A2024142.h11v06.002.2024143000000.h5",
                    "time_start": "2024-05-21T00:00:00.000Z",
                    "links": [{
                        "rel": CMR_DATA_LINK_REL,
                        "href": "https://archive.example.test/VNP46A2.A2024142.h11v06.002.2024143000000.h5"
                    }]
                }]
            }
        }))
        .unwrap();

        let discovery = parse_product_response("VNP46A2", response);

        assert_eq!(discovery.product, "VNP46A2");
        assert_eq!(discovery.skipped_entries, 0);
        assert_eq!(discovery.pages_fetched, 1);
        assert_eq!(discovery.granules.len(), 1);
        assert_eq!(discovery.granules[0].tile.h, 11);
        assert_eq!(discovery.granules[0].tile.v, 6);
        assert_eq!(discovery.granules[0].product, "VNP46A2");
    }

    #[test]
    fn falls_back_to_title_and_href_for_tile_coordinate() {
        let response: CmrGranulesResponse = serde_json::from_value(serde_json::json!({
            "feed": {
                "entry": [
                    {
                        "title": "VNP46A2.A2024142.h09v04.002.2024143000000.h5",
                        "time_start": "2024-05-21T00:00:00Z",
                        "links": [{
                            "rel": CMR_DATA_LINK_REL,
                            "href": "https://archive.example.test/no-tile-name.h5"
                        }]
                    },
                    {
                        "title": "no-tile-title",
                        "producer_granule_id": "no-tile-producer-id",
                        "time_start": "2024-05-21T00:00:00Z",
                        "links": [{
                            "rel": CMR_DATA_LINK_REL,
                            "href": "https://archive.example.test/VNP46A2.A2024142.h10v05.002.2024143000000.h5"
                        }]
                    }
                ]
            }
        }))
        .unwrap();

        let discovery = parse_product_response("VNP46A2", response);

        assert_eq!(discovery.granules.len(), 2);
        assert_eq!(discovery.granules[0].tile.h, 9);
        assert_eq!(discovery.granules[0].tile.v, 4);
        assert_eq!(discovery.granules[1].tile.h, 10);
        assert_eq!(discovery.granules[1].tile.v, 5);
    }

    #[test]
    fn skips_entries_without_data_h5_link_or_tile() {
        let response: CmrGranulesResponse = serde_json::from_value(serde_json::json!({
            "feed": {
                "entry": [
                    {
                        "title": "no-tile",
                        "producer_granule_id": "no-tile",
                        "time_start": "2024-05-21T00:00:00Z",
                        "links": [{
                            "rel": CMR_DATA_LINK_REL,
                            "href": "https://archive.example.test/no-tile.h5"
                        }]
                    },
                    {
                        "title": "VNP46A2.A2024142.h11v06.002.2024143000000.h5",
                        "producer_granule_id": "VNP46A2.A2024142.h11v06.002.2024143000000.h5",
                        "time_start": "2024-05-21T00:00:00Z",
                        "links": [{
                            "rel": "urn:not-data",
                            "href": "https://archive.example.test/VNP46A2.A2024142.h11v06.002.2024143000000.h5"
                        }]
                    }
                ]
            }
        }))
        .unwrap();

        let discovery = parse_product_response("VNP46A2", response);

        assert!(discovery.granules.is_empty());
        assert_eq!(discovery.skipped_entries, 2);
        assert_eq!(discovery.pages_fetched, 1);
    }

    #[test]
    fn discovery_url_includes_page_number_and_defaults_temporal_start() {
        let config = test_config();
        let expected_temporal = format!("{DEFAULT_TEMPORAL_START},");

        let url = discovery_url(&config, "VNP46A2", 3, None).unwrap();

        assert_eq!(query_param(&url, "short_name").as_deref(), Some("VNP46A2"));
        assert_eq!(query_param(&url, "page_size").as_deref(), Some("200"));
        assert_eq!(query_param(&url, "page_num").as_deref(), Some("3"));
        assert_eq!(
            query_param(&url, "temporal").as_deref(),
            Some(expected_temporal.as_str())
        );
    }

    #[test]
    fn discovery_url_uses_resume_timestamp_for_temporal_start() {
        let config = test_config();
        let resume_from = Utc.with_ymd_and_hms(2024, 5, 20, 6, 30, 0).unwrap();
        let expected_temporal = format!("{},", resume_from.to_rfc3339());

        let url = discovery_url(&config, "VJ146A2", 1, Some(resume_from)).unwrap();

        assert_eq!(query_param(&url, "page_num").as_deref(), Some("1"));
        assert_eq!(
            query_param(&url, "temporal").as_deref(),
            Some(expected_temporal.as_str())
        );
    }

    #[test]
    fn append_page_discovery_accumulates_multi_page_results() {
        let mut aggregate = empty_product_discovery("VNP46A2");

        let page_one: CmrGranulesResponse = serde_json::from_value(serde_json::json!({
            "feed": {
                "entry": [
                    {
                        "title": "VNP46A2.A2024142.h11v06.002.2024143000000.h5",
                        "producer_granule_id": "VNP46A2.A2024142.h11v06.002.2024143000000.h5",
                        "time_start": "2024-05-21T00:00:00Z",
                        "links": [{
                            "rel": CMR_DATA_LINK_REL,
                            "href": "https://archive.example.test/VNP46A2.A2024142.h11v06.002.2024143000000.h5"
                        }]
                    },
                    {
                        "title": "invalid-no-tile",
                        "producer_granule_id": "invalid-no-tile",
                        "time_start": "2024-05-21T00:00:00Z",
                        "links": [{
                            "rel": CMR_DATA_LINK_REL,
                            "href": "https://archive.example.test/invalid-no-tile.h5"
                        }]
                    }
                ]
            }
        }))
        .unwrap();

        let page_two: CmrGranulesResponse = serde_json::from_value(serde_json::json!({
            "feed": {
                "entry": [{
                    "title": "VNP46A2.A2024143.h12v06.002.2024144000000.h5",
                    "producer_granule_id": "VNP46A2.A2024143.h12v06.002.2024144000000.h5",
                    "time_start": "2024-05-22T00:00:00Z",
                    "links": [{
                        "rel": CMR_DATA_LINK_REL,
                        "href": "https://archive.example.test/VNP46A2.A2024143.h12v06.002.2024144000000.h5"
                    }]
                }]
            }
        }))
        .unwrap();

        let page_one_entries = append_page_discovery(&mut aggregate, page_one, 1);
        let page_two_entries = append_page_discovery(&mut aggregate, page_two, 2);

        assert_eq!(page_one_entries, 2);
        assert_eq!(page_two_entries, 1);
        assert_eq!(aggregate.granules.len(), 2);
        assert_eq!(aggregate.skipped_entries, 1);
        assert_eq!(aggregate.pages_fetched, 2);
    }

    #[test]
    fn pagination_continues_only_for_full_pages() {
        assert!(should_fetch_next_page(200));
        assert!(should_fetch_next_page(201));
        assert!(!should_fetch_next_page(199));
        assert!(!should_fetch_next_page(0));
    }
}
