use std::time::Duration;

pub(crate) use shared::http_retry::{send_with_retry, RetryContext, RetryIdempotency};

const USER_AGENT: &str = "LumenHorizon ingest-svc/0.1";

pub(crate) fn build_http_client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()
}
