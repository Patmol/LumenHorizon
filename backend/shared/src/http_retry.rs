use std::{future::Future, time::Duration};

use reqwest::StatusCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl RetryConfig {
    pub fn delay_for_attempt(self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(31);
        let multiplier = 1u64 << exponent;
        let base_millis = self.base_delay.as_millis().min(u128::from(u64::MAX)) as u64;
        let delay_millis = base_millis.saturating_mul(multiplier);

        Duration::from_millis(delay_millis).min(self.max_delay)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RetryContext {
    pub dependency: &'static str,
    pub operation: &'static str,
    pub idempotency: RetryIdempotency,
}

#[derive(Debug, Clone, Copy)]
pub enum RetryIdempotency {
    Idempotent,
    AtLeastOnce,
}

impl RetryIdempotency {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idempotent => "idempotent",
            Self::AtLeastOnce => "at_least_once",
        }
    }
}

pub async fn send_with_retry(
    request: reqwest::RequestBuilder,
    policy: RetryConfig,
    context: RetryContext,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut request = Some(request);
    let max_attempts = policy.max_attempts.max(1);

    for attempt in 1..=max_attempts {
        let attempt_request = if attempt == max_attempts {
            request
                .take()
                .expect("request remains available for final attempt")
        } else if let Some(cloned) = request
            .as_ref()
            .and_then(reqwest::RequestBuilder::try_clone)
        {
            cloned
        } else {
            tracing::warn!(
                dependency = context.dependency,
                operation = context.operation,
                retry_idempotency = context.idempotency.as_str(),
                attempt,
                max_attempts,
                "request body cannot be cloned; sending without additional retries"
            );
            request
                .take()
                .expect("request remains available when clone fails")
        };

        match attempt_request.send().await {
            Ok(response) => {
                let status = response.status();
                if attempt < max_attempts && is_retryable_status(status) {
                    wait_before_retry(policy, context, attempt, Some(status.as_u16()), None).await;
                    continue;
                }

                if attempt > 1 {
                    tracing::info!(
                        dependency = context.dependency,
                        operation = context.operation,
                        retry_idempotency = context.idempotency.as_str(),
                        attempt,
                        max_attempts,
                        http_status = status.as_u16(),
                        "HTTP request completed after retry"
                    );
                }

                return Ok(response);
            }
            Err(error) => {
                if attempt < max_attempts && is_retryable_error(&error) {
                    wait_before_retry(policy, context, attempt, None, Some(&error)).await;
                    continue;
                }

                return Err(error);
            }
        }
    }

    unreachable!("retry loop always returns from final attempt")
}

async fn wait_before_retry(
    policy: RetryConfig,
    context: RetryContext,
    attempt: u32,
    http_status: Option<u16>,
    error: Option<&reqwest::Error>,
) {
    let delay = policy.delay_for_attempt(attempt);

    tracing::warn!(
        dependency = context.dependency,
        operation = context.operation,
        retry_idempotency = context.idempotency.as_str(),
        attempt,
        max_attempts = policy.max_attempts,
        delay_ms = delay.as_millis() as u64,
        http_status,
        error = error.map(ToString::to_string),
        "transient HTTP request failure; retrying"
    );

    sleep(delay).await;
}

async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

fn is_retryable_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

pub fn is_retryable_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
}

pub async fn retry_async<T, E, Fut, Op, ShouldRetry>(
    policy: RetryConfig,
    context: RetryContext,
    mut operation: Op,
    should_retry: ShouldRetry,
) -> Result<T, E>
where
    Fut: Future<Output = Result<T, E>>,
    Op: FnMut() -> Fut,
    ShouldRetry: Fn(&E) -> bool,
    E: std::fmt::Display,
{
    let max_attempts = policy.max_attempts.max(1);

    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) if attempt < max_attempts && should_retry(&error) => {
                let delay = policy.delay_for_attempt(attempt);
                tracing::warn!(
                    dependency = context.dependency,
                    operation = context.operation,
                    retry_idempotency = context.idempotency.as_str(),
                    attempt,
                    max_attempts,
                    delay_ms = delay.as_millis() as u64,
                    error = %error,
                    "transient operation failure; retrying"
                );
                sleep(delay).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("retry loop always returns from final attempt")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use reqwest::StatusCode;

    use super::{is_retryable_status, RetryConfig};

    #[test]
    fn classifies_retryable_status_codes() {
        assert!(is_retryable_status(StatusCode::REQUEST_TIMEOUT));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
    }

    #[test]
    fn rejects_non_retryable_status_codes() {
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::FORBIDDEN));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
    }

    #[test]
    fn caps_exponential_backoff() {
        let policy = RetryConfig {
            max_attempts: 5,
            base_delay: Duration::from_millis(250),
            max_delay: Duration::from_millis(1_000),
        };

        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(250));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(500));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(1_000));
        assert_eq!(policy.delay_for_attempt(4), Duration::from_millis(1_000));
    }
}
