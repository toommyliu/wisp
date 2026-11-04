use std::sync::{Arc, Weak};
use std::task::Waker;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;
use reqwest::StatusCode;
use reqwest::header::{ETAG, LINK};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::task::AbortHandle;
use url::Url;

pub use semver::Version;

use crate::app_settings::{AppSettingsStore, SettingsError};
use crate::data::app_data::{AppData, Request};
use crate::data::documents::trim;
use crate::github::{Clock, Get, GitHub, GitHubError, Live, Transport};
use crate::reply::{Answer, Reply, Stopped, reply};
use crate::sessions::Changes;

const RELEASES_URL: &str = "https://api.github.com/repos/toommyliu/lucent/releases";
const RELEASE_PAGES: &str = "/toommyliu/lucent/releases/";
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const CHECK_ATTEMPTS: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Disabled,
    Checking {
        started_at: SystemTime,
    },
    Current {
        latest: Version,
        checked_at: SystemTime,
    },
    Available {
        release: Arc<Release>,
        checked_at: SystemTime,
    },
    Failed {
        message: String,
        checked_at: SystemTime,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    version: Version,
    page: ReleasePage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<String>,
}

impl Release {
    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn page(&self) -> &ReleasePage {
        &self.page
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn published_at(&self) -> Option<&str> {
        self.published_at.as_deref()
    }

    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ReleasePage(Url);

impl TryFrom<String> for ReleasePage {
    type Error = &'static str;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(&raw).ok_or("not a Lucent release page")
    }
}

impl From<ReleasePage> for String {
    fn from(page: ReleasePage) -> Self {
        page.0.into()
    }
}

impl ReleasePage {
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let url = Url::parse(raw).ok()?;
        let allowed = url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path().starts_with(RELEASE_PAGES);
        allowed.then_some(Self(url))
    }

    pub fn url(&self) -> &Url {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseCache {
    release: Arc<Release>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skipped_version: Option<Version>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UpdatesError {
    #[error("the update service has stopped")]
    Stopped,
}

impl Stopped for UpdatesError {
    const STOPPED: Self = UpdatesError::Stopped;
}

#[derive(Clone)]
pub struct Updates(Arc<Inner>);

struct Inner {
    runtime: Handle,
    current: Version,
    github: GitHub,
    settings: AppSettingsStore,
    app_data: Arc<AppData>,
    clock: Clock,
    book: Mutex<Book>,
    changes: Changes,
}

type Waiters = Mutex<Vec<Answer<UpdateState, UpdatesError>>>;

#[derive(Default)]
struct Book {
    state: Option<UpdateState>,
    running: Option<Running>,
    cache: Option<Option<ReleaseCache>>,
}

struct Running {
    task: AbortHandle,
    waiters: Weak<Waiters>,
}

impl Updates {
    pub(crate) fn start(runtime: &Handle, app_data: Arc<AppData>) -> Self {
        let current = env!("CARGO_PKG_VERSION");
        Self::serving(
            runtime,
            app_data,
            Arc::new(Live::default()),
            current,
            Arc::new(SystemTime::now),
            fastrand::f64,
        )
    }

    fn serving(
        runtime: &Handle,
        app_data: Arc<AppData>,
        transport: Arc<dyn Transport>,
        current: &str,
        clock: Clock,
        jitter: fn() -> f64,
    ) -> Self {
        let current = normalize(current).expect("the app version is a semantic version");
        Self(Arc::new(Inner {
            runtime: runtime.clone(),
            github: GitHub::new(
                transport,
                &format!("Lucent/{current}"),
                clock.clone(),
                jitter,
            ),
            current,
            settings: AppSettingsStore {
                app_data: app_data.clone(),
            },
            app_data,
            clock,
            book: Mutex::default(),
            changes: Changes::default(),
        }))
    }

    pub fn current_version(&self) -> &Version {
        &self.0.current
    }

    pub fn state(&self) -> Reply<UpdateState, UpdatesError> {
        self.spawn(|inner| async move { inner.state().await })
    }

    pub fn check(&self, force: bool) -> Reply<UpdateState, UpdatesError> {
        let (answer, reply) = reply();
        let mut answers = vec![answer];
        let mut book = self.0.book.lock();
        if let Some(running) = book.running.take() {
            if let Some(waiters) = running.waiters.upgrade() {
                answers.append(&mut waiters.lock());
            }
            running.task.abort();
        }
        let waiters = Arc::new(Mutex::new(answers));
        let (inner, held) = (self.0.clone(), waiters.clone());
        let task = self.0.runtime.spawn(async move {
            let state = inner.check(force).await;
            for answer in std::mem::take(&mut *held.lock()) {
                answer.send(Ok(state.clone()));
            }
        });
        book.running = Some(Running {
            task: task.abort_handle(),
            waiters: Arc::downgrade(&waiters),
        });
        reply
    }

    pub fn should_prompt(&self, version: &Version) -> Reply<bool, UpdatesError> {
        let version = version.clone();
        self.spawn(|inner| async move {
            let cache = inner.cache().await;
            cache.and_then(|cache| cache.skipped_version) != Some(version)
        })
    }

    pub fn skip(&self, version: &Version) -> Reply<bool, UpdatesError> {
        let version = version.clone();
        self.spawn(|inner| async move { inner.skip(version).await })
    }

    pub fn watch(&self, waker: Waker) {
        self.0.changes.watch(waker);
    }

    fn spawn<T: Send + 'static, F>(
        &self,
        work: impl FnOnce(Arc<Inner>) -> F,
    ) -> Reply<T, UpdatesError>
    where
        F: Future<Output = T> + Send + 'static,
    {
        let (answer, reply) = reply();
        let work = work(self.0.clone());
        self.0
            .runtime
            .spawn(async move { answer.send(Ok(work.await)) });
        reply
    }
}

enum Fetched {
    NotModified,
    Modified {
        release: Arc<Release>,
        etag: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
enum CheckError {
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error(transparent)]
    GitHub(#[from] GitHubError),
    #[error("Update check timed out.")]
    TimedOut,
    #[error("Release payload is invalid: {0}")]
    Payload(#[from] serde_json::Error),
    #[error("Latest release is not a stable release.")]
    NotStable,
    #[error("No published releases were found.")]
    NoReleases,
    #[error("Release payload is missing tag_name.")]
    MissingTag,
    #[error("Release payload has an invalid html_url.")]
    InvalidPage,
    #[error("Invalid semantic version: {0}.")]
    InvalidVersion(String),
}

impl Inner {
    async fn state(&self) -> UpdateState {
        if let Some(state) = self.book.lock().state.clone() {
            return state;
        }
        let enabled = self.settings.get().await;
        let initial = match enabled {
            Ok(settings) if !settings.preferences.check_for_updates => UpdateState::Disabled,
            _ => UpdateState::Idle,
        };
        self.book.lock().state.get_or_insert(initial).clone()
    }

    async fn check(&self, force: bool) -> UpdateState {
        match self.attempt(force).await {
            Ok(state) => state,
            Err(error) => {
                tracing::warn!(
                    component = "updates",
                    current_version = %self.current,
                    force,
                    %error,
                    "Failed to check for updates"
                );
                let state = UpdateState::Failed {
                    message: error.to_string(),
                    checked_at: (self.clock)(),
                };
                self.publish(state)
            }
        }
    }

    async fn attempt(&self, force: bool) -> Result<UpdateState, CheckError> {
        let settings = self.settings.get().await?;
        if !settings.preferences.check_for_updates && !force {
            return Ok(self.publish(UpdateState::Disabled));
        }
        let started_at = (self.clock)();
        self.publish(UpdateState::Checking { started_at });
        let cache = self.cache().await;
        let etag = cache.as_ref().and_then(|cache| cache.etag.clone());
        let fetched = tokio::time::timeout(CHECK_TIMEOUT, self.fetch(etag))
            .await
            .map_err(|_| CheckError::TimedOut)??;
        let checked_at = (self.clock)();
        let release = match (fetched, cache) {
            (Fetched::NotModified, None) => {
                let latest = self.current.clone();
                let state = UpdateState::Current { latest, checked_at };
                return Ok(self.publish(state));
            }
            (Fetched::NotModified, Some(cache)) => cache.release,
            (Fetched::Modified { release, etag }, cache) => {
                let skipped_version = cache
                    .and_then(|cache| cache.skipped_version)
                    .filter(|skipped| *skipped == release.version);
                self.save(ReleaseCache {
                    release: release.clone(),
                    etag,
                    skipped_version,
                })
                .await;
                release
            }
        };
        let state = if release.version > self.current {
            UpdateState::Available {
                release,
                checked_at,
            }
        } else {
            let latest = release.version.clone();
            UpdateState::Current { latest, checked_at }
        };
        Ok(self.publish(state))
    }

    fn publish(&self, state: UpdateState) -> UpdateState {
        self.book.lock().state = Some(state.clone());
        self.changes.notify();
        state
    }

    async fn fetch(&self, etag: Option<String>) -> Result<Fetched, CheckError> {
        if self.current.pre.is_empty() {
            self.latest_stable(etag).await
        } else {
            self.newest().await
        }
    }

    async fn latest_stable(&self, etag: Option<String>) -> Result<Fetched, CheckError> {
        let url = format!("{RELEASES_URL}/latest");
        let response = self.github.get(&request(&url, etag)).await?;
        let etag = response.header(ETAG).map(str::to_owned);
        if response.status == StatusCode::NOT_MODIFIED {
            return Ok(Fetched::NotModified);
        }
        let payload: Payload = parse(&response.body)?;
        if payload.draft || payload.prerelease {
            return Err(CheckError::NotStable);
        }
        let release = Arc::new(release(payload)?);
        Ok(Fetched::Modified { release, etag })
    }

    async fn newest(&self) -> Result<Fetched, CheckError> {
        let mut newest: Option<(Version, Payload)> = None;
        for page in 1.. {
            let url = format!("{RELEASES_URL}?per_page=100&page={page}");
            let response = self.github.get(&request(&url, None)).await?;
            let payloads: Vec<Payload> = parse(&response.body)?;
            for payload in payloads {
                if payload.draft {
                    continue;
                }
                let Some(version) = semver(&payload.tag_name) else {
                    continue;
                };
                if newest.as_ref().is_none_or(|(best, _)| version > *best) {
                    newest = Some((version, payload));
                }
            }
            let next = response
                .header(LINK)
                .is_some_and(|link| link.contains(r#"rel="next""#));
            if !next {
                break;
            }
        }
        let (_, payload) = newest.ok_or(CheckError::NoReleases)?;
        let release = Arc::new(release(payload)?);
        Ok(Fetched::Modified {
            release,
            etag: None,
        })
    }

    async fn cache(&self) -> Option<ReleaseCache> {
        if let Some(cache) = &self.book.lock().cache {
            return cache.clone();
        }
        let (answer, reply) = reply();
        self.app_data.submit(Request::ReleaseCache(answer));
        let loaded = reply.await.ok().flatten();
        self.book.lock().cache.get_or_insert(loaded).clone()
    }

    async fn save(&self, cache: ReleaseCache) {
        let (answer, reply) = reply();
        self.app_data
            .submit(Request::SaveReleaseCache(cache.clone(), answer));
        let _ = reply.await;
        self.book.lock().cache = Some(Some(cache));
    }

    async fn skip(&self, version: Version) -> bool {
        let release = match &self.book.lock().state {
            Some(UpdateState::Available { release, .. }) if release.version == version => {
                release.clone()
            }
            _ => return false,
        };
        let etag = self.cache().await.and_then(|cache| cache.etag);
        self.save(ReleaseCache {
            release,
            etag,
            skipped_version: Some(version),
        })
        .await;
        true
    }
}

fn request(url: &str, etag: Option<String>) -> Get {
    Get {
        url: Url::parse(url).expect("the releases URL parses"),
        etag,
        attempts: CHECK_ATTEMPTS,
        max_bytes: MAX_RESPONSE_BYTES,
        timeout: REQUEST_TIMEOUT,
    }
}

#[derive(Deserialize)]
struct Payload {
    #[serde(default)]
    body: Option<String>,
    draft: bool,
    html_url: String,
    #[serde(default)]
    name: Option<String>,
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    tag_name: String,
}

fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_str(&String::from_utf8_lossy(body))
}

fn release(payload: Payload) -> Result<Release, CheckError> {
    let tag_name = trim(&payload.tag_name).to_owned();
    let page = ReleasePage::parse(trim(&payload.html_url));
    if tag_name.is_empty() {
        return Err(CheckError::MissingTag);
    }
    let page = page.ok_or(CheckError::InvalidPage)?;
    let version = normalize(&tag_name).ok_or_else(|| {
        CheckError::InvalidVersion(serde_json::to_string(&tag_name).expect("a string serializes"))
    })?;
    let optional = |text: Option<String>| {
        text.map(|text| trim(&text).to_owned())
            .filter(|text| !text.is_empty())
    };
    Ok(Release {
        version,
        page,
        name: optional(payload.name),
        published_at: optional(payload.published_at),
        body: payload.body,
    })
}

fn normalize(version: &str) -> Option<Version> {
    let version = trim(version);
    semver(version.strip_prefix('v').unwrap_or(version))
}

fn semver(text: &str) -> Option<Version> {
    let text = trim(text);
    let mut version = Version::parse(text.strip_prefix('v').unwrap_or(text)).ok()?;
    version.build = semver::BuildMetadata::EMPTY;
    Some(version)
}

#[cfg(test)]
mod tests;
