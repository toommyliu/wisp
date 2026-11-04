use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use parking_lot::Mutex;
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, AsHeaderName, HeaderMap, HeaderValue, IF_NONE_MATCH, LOCATION, RETRY_AFTER, USER_AGENT,
};
use url::Url;

const API_VERSION: &str = "2022-11-28";
const RETRY_BASE: Duration = Duration::from_millis(200);
const FIRST_COOLDOWN: Duration = Duration::from_secs(60);
const MAX_COOLDOWN: Duration = Duration::from_secs(15 * 60);

pub(crate) type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub(crate) type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

pub(crate) struct Get {
    pub(crate) url: Url,
    pub(crate) etag: Option<String>,
    pub(crate) attempts: u32,
    pub(crate) max_bytes: usize,
    pub(crate) timeout: Duration,
}

pub(crate) struct Response {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
}

impl Response {
    pub(crate) fn header(&self, name: impl AsHeaderName) -> Option<&str> {
        header(&self.headers, name)
    }
}

fn header(headers: &HeaderMap, name: impl AsHeaderName) -> Option<&str> {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::trim)
        .find(|value| !value.is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum HttpError {
    #[error("HTTP response exceeds the {0} byte limit.")]
    TooLarge(usize),
    #[error("HTTP response timed out.")]
    TimedOut,
    #[error("HTTP request exceeded its redirect limit.")]
    Redirect,
    #[error("HTTP request failed.")]
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum GitHubError {
    #[error(
        "GitHub's request limit has been reached. Try again after {}.",
        .0.to_rfc3339_opts(SecondsFormat::Millis, true)
    )]
    RateLimited(DateTime<Utc>),
    #[error("The GitHub credential is invalid, expired, or revoked.")]
    Authentication,
    #[error("GitHub denied access to the requested resource.")]
    Permission,
    #[error("GitHub couldn't find the requested resource.")]
    NotFound,
    #[error("GitHub rejected the request.")]
    InvalidRequest,
    #[error("GitHub is unavailable (HTTP {}).", .0.as_u16())]
    Unavailable(StatusCode),
    #[error(
        "GitHub returned HTTP {} {}.",
        .0.as_u16(),
        .0.canonical_reason().unwrap_or_default()
    )]
    Unexpected(StatusCode),
    #[error(transparent)]
    Http(HttpError),
}

impl GitHubError {
    fn retryable(&self) -> bool {
        matches!(
            self,
            GitHubError::Unavailable(_)
                | GitHubError::Http(HttpError::Failed | HttpError::TimedOut)
        )
    }
}

pub(crate) trait Transport: Send + Sync + 'static {
    fn get(
        &self,
        url: Url,
        headers: HeaderMap,
        max_bytes: usize,
        timeout: Duration,
    ) -> BoxFuture<Result<Response, HttpError>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cooldown {
    strikes: u32,
    until: DateTime<Utc>,
}

pub(crate) struct GitHub {
    transport: Arc<dyn Transport>,
    user_agent: HeaderValue,
    clock: Clock,
    jitter: fn() -> f64,
    cooldown: Mutex<Option<Cooldown>>,
}

impl GitHub {
    pub(crate) fn new(
        transport: Arc<dyn Transport>,
        user_agent: &str,
        clock: Clock,
        jitter: fn() -> f64,
    ) -> Self {
        Self {
            transport,
            user_agent: HeaderValue::from_str(user_agent)
                .expect("the user agent is a header value"),
            clock,
            jitter,
            cooldown: Mutex::new(None),
        }
    }

    pub(crate) async fn get(&self, request: &Get) -> Result<Response, GitHubError> {
        let mut attempt = 1;
        loop {
            match self.execute(request).await {
                Err(error) if error.retryable() && attempt < request.attempts => {
                    let delay = RETRY_BASE * 2u32.pow(attempt - 1);
                    tokio::time::sleep(delay.mul_f64(0.8 + 0.4 * (self.jitter)())).await;
                    attempt += 1;
                }
                outcome => return outcome,
            }
        }
    }

    async fn execute(&self, request: &Get) -> Result<Response, GitHubError> {
        let observed = *self.cooldown.lock();
        if let Some(cooldown) = observed
            && cooldown.until > self.now()
        {
            return Err(GitHubError::RateLimited(cooldown.until));
        }
        let response = self
            .transport
            .get(
                request.url.clone(),
                self.headers(request),
                request.max_bytes,
                request.timeout,
            )
            .await
            .map_err(GitHubError::Http)?;
        let recorded = self.record(observed, &response, self.now());
        let status = response.status;
        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            return Ok(response);
        }
        Err(match (recorded, status.as_u16()) {
            (Some(cooldown), _) => GitHubError::RateLimited(cooldown.until),
            (None, 401) => GitHubError::Authentication,
            (None, 403) => GitHubError::Permission,
            (None, 404) => GitHubError::NotFound,
            (None, 422) => GitHubError::InvalidRequest,
            (None, 500..) => GitHubError::Unavailable(status),
            (None, _) => GitHubError::Unexpected(status),
        })
    }

    fn headers(&self, request: &Get) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "x-github-api-version",
            HeaderValue::from_static(API_VERSION),
        );
        headers.insert(USER_AGENT, self.user_agent.clone());
        if let Some(etag) = request
            .etag
            .as_deref()
            .and_then(|etag| HeaderValue::from_str(etag).ok())
        {
            headers.insert(IF_NONE_MATCH, etag);
        }
        headers
    }

    fn record(
        &self,
        observed: Option<Cooldown>,
        response: &Response,
        now: DateTime<Utc>,
    ) -> Option<Cooldown> {
        let mut current = self.cooldown.lock();
        if !is_rate_limit(response, now) {
            if *current == observed {
                *current = None;
            }
            return None;
        }
        let superseded = current.filter(|_| *current != observed);
        let strikes = match superseded {
            Some(cooldown) => cooldown.strikes,
            None => current.map_or(0, |cooldown| cooldown.strikes) + 1,
        };
        let until = retry_at(&response.headers, now).unwrap_or_else(|| {
            let cooldown = FIRST_COOLDOWN
                .saturating_mul(2u32.saturating_pow(strikes - 1))
                .min(MAX_COOLDOWN);
            now + TimeDelta::from_std(cooldown).expect("the cooldown fits a time delta")
        });
        if let Some(cooldown) = superseded
            && until <= cooldown.until
        {
            return Some(cooldown);
        }
        let cooldown = Cooldown { strikes, until };
        *current = Some(cooldown);
        Some(cooldown)
    }

    fn now(&self) -> DateTime<Utc> {
        (self.clock)().into()
    }
}

fn retry_at(headers: &HeaderMap, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if let Some(value) = header(headers, RETRY_AFTER) {
        if let Some(seconds) = number(value) {
            return later(now, seconds.max(0.0));
        }
        if let Ok(date) = DateTime::parse_from_rfc2822(value) {
            return Some(now.max(date.to_utc()));
        }
    }
    if header(headers, "x-ratelimit-remaining") == Some("0")
        && let Some(reset) = header(headers, "x-ratelimit-reset").and_then(number)
    {
        let reset = DateTime::from_timestamp_millis((reset * 1000.0) as i64)?;
        return Some(now.max(reset));
    }
    None
}

fn number(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn later(now: DateTime<Utc>, seconds: f64) -> Option<DateTime<Utc>> {
    now.checked_add_signed(TimeDelta::try_milliseconds((seconds * 1000.0) as i64)?)
}

fn is_rate_limit(response: &Response, now: DateTime<Utc>) -> bool {
    match response.status.as_u16() {
        429 => true,
        403 => {
            let body = response.body.to_ascii_lowercase();
            let says = |needle: &[u8]| body.windows(needle.len()).any(|window| window == needle);
            retry_at(&response.headers, now).is_some()
                || says(b"secondary limit")
                || says(b"rate limit")
        }
        _ => false,
    }
}

#[derive(Default)]
pub(crate) struct Live {
    client: OnceLock<reqwest::Client>,
}

impl Live {
    fn client(&self) -> Result<reqwest::Client, HttpError> {
        if let Some(client) = self.client.get() {
            return Ok(client.clone());
        }
        let client = client(reqwest::Client::builder()).map_err(|_| HttpError::Failed)?;
        Ok(self.client.get_or_init(|| client).clone())
    }
}

impl Transport for Live {
    fn get(
        &self,
        url: Url,
        headers: HeaderMap,
        max_bytes: usize,
        timeout: Duration,
    ) -> BoxFuture<Result<Response, HttpError>> {
        let client = self.client();
        Box::pin(async move { request(&client?, url, headers, max_bytes, timeout).await })
    }
}

fn client(builder: reqwest::ClientBuilder) -> reqwest::Result<reqwest::Client> {
    builder
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .no_proxy()
        .build()
}

async fn request(
    client: &reqwest::Client,
    url: Url,
    headers: HeaderMap,
    max_bytes: usize,
    timeout: Duration,
) -> Result<Response, HttpError> {
    let sent = client.get(url).headers(headers).send();
    let mut response = tokio::time::timeout(timeout, sent)
        .await
        .map_err(|_| HttpError::TimedOut)?
        .map_err(|_| HttpError::Failed)?;
    let status = response.status();
    if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
        && response.headers().contains_key(LOCATION)
    {
        return Err(HttpError::Redirect);
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(HttpError::TooLarge(max_bytes));
    }
    let mut body = Vec::new();
    while let Some(chunk) = tokio::time::timeout(timeout, response.chunk())
        .await
        .map_err(|_| HttpError::TimedOut)?
        .map_err(|_| HttpError::Failed)?
    {
        if body.len() + chunk.len() > max_bytes {
            return Err(HttpError::TooLarge(max_bytes));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Response {
        status,
        headers: std::mem::take(response.headers_mut()),
        body,
    })
}

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod tests;
