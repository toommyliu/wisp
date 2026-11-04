use super::*;
use crate::app_settings::PreferencesPatch;
use crate::automation::test_logs::Logs;
use crate::github::fake::{Fake, Step, clock, response};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;
use tokio::sync::oneshot;

const LATEST: &str = "https://api.github.com/repos/toommyliu/lucent/releases/latest";

fn page_url(page: usize) -> String {
    format!("{RELEASES_URL}?per_page=100&page={page}")
}

fn tag_page(tag: &str) -> String {
    format!("https://github.com/toommyliu/lucent/releases/tag/{tag}")
}

struct Folder(PathBuf);

impl Folder {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "lucent-updates-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn cache_path(&self) -> PathBuf {
        self.0.join("update-cache.json")
    }

    fn cache(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.cache_path()).unwrap()).unwrap()
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Harness {
    updates: Updates,
    fake: Arc<Fake>,
    folder: Folder,
}

struct Options {
    enabled: bool,
    current: &'static str,
    cache: Option<Value>,
    start: SystemTime,
    steps: Vec<Step>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: true,
            current: "1.0.0",
            cache: None,
            start: UNIX_EPOCH + Duration::from_secs(1_785_542_400),
            steps: Vec::new(),
        }
    }
}

async fn harness(options: Options) -> Harness {
    harness_on(&Handle::current(), options).await
}

async fn harness_on(runtime: &Handle, options: Options) -> Harness {
    let folder = Folder::new();
    if let Some(cache) = &options.cache {
        std::fs::write(folder.cache_path(), cache.to_string()).unwrap();
    }
    let app_data = Arc::new(AppData::start(folder.0.clone(), folder.0.clone(), None).unwrap());
    let settings = AppSettingsStore {
        app_data: app_data.clone(),
    };
    settings
        .update_preferences(PreferencesPatch {
            check_for_updates: Some(options.enabled),
            ..PreferencesPatch::default()
        })
        .await
        .unwrap();
    let fake = Fake::new(options.steps);
    let updates = Updates::serving(
        runtime,
        app_data,
        fake.clone(),
        options.current,
        clock(options.start),
        || 0.5,
    );
    Harness {
        updates,
        fake,
        folder,
    }
}

fn release_json(tag: &str, prerelease: bool) -> Value {
    json!({
        "draft": false,
        "html_url": tag_page(tag),
        "prerelease": prerelease,
        "tag_name": tag,
    })
}

fn latest(tag: &str) -> Step {
    Step::Respond(Ok(response(
        200,
        &[],
        &release_json(tag, false).to_string(),
    )))
}

fn page(tags: &[&str], headers: &[(&str, &str)]) -> Step {
    let releases: Vec<Value> = tags
        .iter()
        .map(|tag| release_json(tag, tag.contains('-')))
        .collect();
    Step::Respond(Ok(response(
        200,
        headers,
        &Value::from(releases).to_string(),
    )))
}

fn next(page: usize) -> String {
    format!("<{}>; rel=\"next\"", page_url(page))
}

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn latest_version(state: &UpdateState) -> Option<String> {
    match state {
        UpdateState::Available { release, .. } => Some(release.version.to_string()),
        UpdateState::Current { latest, .. } => Some(latest.to_string()),
        _ => None,
    }
}

fn failure(state: &UpdateState) -> &str {
    match state {
        UpdateState::Failed { message, .. } => message,
        other => panic!("expected a failed check, got {other:?}"),
    }
}

fn available(state: &UpdateState) -> &Release {
    match state {
        UpdateState::Available { release, .. } => release,
        other => panic!("expected an available release, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn logs_one_warning_for_each_failed_check() {
    for force in [false, true] {
        let logs = Logs::default();
        let _logging = logs.enter();
        let harness = harness(Options {
            steps: (0..6)
                .map(|_| Step::Respond(Err(crate::github::HttpError::TooLarge(16))))
                .collect(),
            ..Options::default()
        })
        .await;

        let first = harness.updates.check(force).await.unwrap();
        let second = harness.updates.check(force).await.unwrap();

        assert_eq!(failure(&first), "HTTP response exceeds the 16 byte limit.");
        assert_eq!(failure(&second), failure(&first));
        assert_eq!(
            logs.messages()
                .iter()
                .filter(|message| *message == "Failed to check for updates")
                .count(),
            2
        );
    }
}

#[test]
fn shutdown_ends_checks_silently_and_answers_with_the_current_state() {
    let logs = Logs::default();
    let _logging = logs.enter();
    let dispatch = logs.dispatch();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .on_thread_start(move || std::mem::forget(tracing::dispatcher::set_default(&dispatch)))
        .enable_all()
        .build()
        .unwrap();
    let driver = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let harness = driver.block_on(harness_on(
        runtime.handle(),
        Options {
            steps: vec![Step::Hang, Step::Hang],
            ..Options::default()
        },
    ));

    let first = harness.updates.check(false);
    driver.block_on(harness.fake.reached(1));
    let second = harness.updates.check(true);
    driver.block_on(harness.fake.reached(2));
    let state = driver.block_on(harness.updates.state()).unwrap();
    assert!(matches!(state, UpdateState::Checking { .. }));
    assert_eq!(harness.fake.dropped(), [LATEST]);
    drop(runtime);

    assert_eq!(driver.block_on(first), Err(UpdatesError::Stopped));
    assert_eq!(driver.block_on(second), Err(UpdatesError::Stopped));
    assert_eq!(harness.fake.dropped(), [LATEST, LATEST]);
    assert_eq!(
        driver.block_on(harness.updates.check(true)),
        Err(UpdatesError::Stopped)
    );
    assert!(
        !logs
            .messages()
            .iter()
            .any(|message| message.contains("Failed"))
    );
}

#[tokio::test(start_paused = true)]
async fn rejects_an_oversized_release_response() {
    for current in ["1.0.0", "1.0.0-beta.1"] {
        let body = "x".repeat(16 * 1024 * 1024);
        let release = json!({
            "body": body,
            "draft": false,
            "html_url": tag_page("v1.1.0"),
            "prerelease": false,
            "tag_name": "v1.1.0",
        });
        let payload = if current.contains('-') {
            json!([release])
        } else {
            release
        };
        let harness = harness(Options {
            current,
            steps: vec![Step::Respond(Ok(response(200, &[], &payload.to_string())))],
            ..Options::default()
        })
        .await;

        let state = harness.updates.check(false).await.unwrap();

        assert_eq!(
            failure(&state),
            "HTTP response exceeds the 16777216 byte limit."
        );
        assert_eq!(harness.fake.requests(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn a_forced_check_runs_while_update_checks_are_off() {
    let harness = harness(Options {
        enabled: false,
        steps: vec![latest("v1.1.0")],
        ..Options::default()
    })
    .await;

    assert_eq!(
        harness.updates.check(false).await.unwrap(),
        UpdateState::Disabled
    );
    let state = harness.updates.check(true).await.unwrap();

    assert_eq!(available(&state).version, version("1.1.0"));
    assert_eq!(harness.fake.requests(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_check_keeps_running_when_its_caller_goes_away() {
    let harness = harness(Options {
        steps: vec![Step::Hang],
        ..Options::default()
    })
    .await;

    drop(harness.updates.check(false));
    harness.fake.reached(1).await;
    tokio::time::sleep(Duration::from_secs(29)).await;

    assert!(harness.fake.dropped().is_empty());
    assert!(matches!(
        harness.updates.state().await.unwrap(),
        UpdateState::Checking { .. }
    ));
}

#[tokio::test(start_paused = true)]
async fn starts_disabled_when_update_checks_are_off() {
    let harness = harness(Options {
        enabled: false,
        ..Options::default()
    })
    .await;

    assert_eq!(
        harness.updates.state().await.unwrap(),
        UpdateState::Disabled
    );
    assert_eq!(harness.fake.requests(), 0);
}

#[tokio::test(start_paused = true)]
async fn starts_idle_when_update_checks_are_on() {
    let harness = harness(Options::default()).await;

    assert_eq!(harness.updates.state().await.unwrap(), UpdateState::Idle);
}

#[tokio::test(start_paused = true)]
async fn keeps_the_newer_result_when_a_replacement_check_starts() {
    let (respond, older) = oneshot::channel();
    let harness = harness(Options {
        steps: vec![Step::Gate(older), latest("v1.1.0")],
        ..Options::default()
    })
    .await;

    let first = harness.updates.check(true);
    harness.fake.reached(1).await;
    let newer = harness.updates.check(true).await.unwrap();
    assert_eq!(latest_version(&newer).as_deref(), Some("1.1.0"));
    let _ = respond.send(Ok(response(
        200,
        &[],
        &release_json("v1.0.0", false).to_string(),
    )));
    assert_eq!(first.await.unwrap(), newer);

    let state = harness.updates.state().await.unwrap();
    assert_eq!(latest_version(&state).as_deref(), Some("1.1.0"));
    assert_eq!(harness.fake.dropped(), [LATEST]);
}

#[tokio::test(start_paused = true)]
async fn keeps_a_newer_available_result_after_an_older_check_would_have_timed_out() {
    let harness = harness(Options {
        steps: vec![Step::Hang, latest("v1.1.0")],
        ..Options::default()
    })
    .await;

    let startup = harness.updates.check(false);
    harness.fake.reached(1).await;
    let forced = harness.updates.check(true).await.unwrap();
    assert_eq!(available(&forced).version, version("1.1.0"));
    tokio::time::sleep(CHECK_TIMEOUT).await;
    assert_eq!(startup.await.unwrap(), forced);

    assert_eq!(harness.updates.state().await.unwrap(), forced);
    assert_eq!(available(&forced).page.url().as_str(), tag_page("v1.1.0"));
}

#[tokio::test(start_paused = true)]
async fn parses_stable_releases_and_reports_newer_versions() {
    let start = UNIX_EPOCH + Duration::from_millis(1_234_567_890_000);
    let (respond, gate) = oneshot::channel();
    let harness = harness(Options {
        current: "1.2.2",
        start,
        steps: vec![Step::Gate(gate)],
        ..Options::default()
    })
    .await;

    let check = harness.updates.check(false);
    harness.fake.reached(1).await;
    assert_eq!(
        harness.updates.state().await.unwrap(),
        UpdateState::Checking { started_at: start }
    );
    let body = json!({
        "draft": false,
        "html_url": tag_page("v1.2.3"),
        "name": "  Lucent 1.2.3 ",
        "prerelease": false,
        "published_at": "2026-06-23T00:00:00Z",
        "tag_name": "v1.2.3",
    });
    let _ = respond.send(Ok(response(200, &[("etag", "etag-2")], &body.to_string())));
    let state = check.await.unwrap();

    let UpdateState::Available {
        release,
        checked_at,
    } = &state
    else {
        panic!("expected an available release, got {state:?}");
    };
    assert_eq!(*checked_at, start);
    assert_eq!(release.version, version("1.2.3"));
    assert_eq!(release.name.as_deref(), Some("Lucent 1.2.3"));
    assert_eq!(
        release.published_at.as_deref(),
        Some("2026-06-23T00:00:00Z")
    );
    assert_eq!(harness.fake.urls(), [LATEST]);
    assert_eq!(
        harness.folder.cache(),
        json!({
            "release": {
                "version": "1.2.3",
                "page": tag_page("v1.2.3"),
                "name": "Lucent 1.2.3",
                "publishedAt": "2026-06-23T00:00:00Z",
            },
            "etag": "etag-2",
        })
    );
}

#[tokio::test(start_paused = true)]
async fn offers_prerelease_builds_the_newest_published_release() {
    let body = json!([
        {"draft": true, "html_url": tag_page("v0.2.0"), "prerelease": false, "tag_name": "v0.2.0"},
        release_json("v0.0.2", false),
        release_json("v0.1.0-beta.2", true),
    ]);
    let harness = harness(Options {
        current: "0.1.0-beta.1",
        steps: vec![Step::Respond(Ok(response(200, &[], &body.to_string())))],
        ..Options::default()
    })
    .await;

    let state = harness.updates.check(false).await.unwrap();

    assert_eq!(harness.fake.urls(), [page_url(1)]);
    let release = available(&state);
    assert_eq!(release.version, version("0.1.0-beta.2"));
    assert_eq!(release.page.url().as_str(), tag_page("v0.1.0-beta.2"));
}

#[tokio::test(start_paused = true)]
async fn finds_the_newest_published_version_across_release_pages() {
    let harness = harness(Options {
        current: "0.1.0-beta.1",
        steps: vec![
            page(
                &["nightly"],
                &[(
                    "link",
                    &format!("{}, <{}>; rel=\"last\"", next(2), page_url(3)),
                )],
            ),
            page(&["v0.1.0-beta.10"], &[("link", &next(3))]),
            page(
                &["v0.1.0-beta.9"],
                &[("link", &format!("<{}>; rel=\"prev\"", page_url(2)))],
            ),
        ],
        ..Options::default()
    })
    .await;

    let state = harness.updates.check(false).await.unwrap();

    let release = available(&state);
    assert_eq!(release.version, version("0.1.0-beta.10"));
    assert_eq!(release.page.url().as_str(), tag_page("v0.1.0-beta.10"));
    assert_eq!(harness.fake.urls(), [page_url(1), page_url(2), page_url(3)]);
}

#[tokio::test(start_paused = true)]
async fn bounds_the_whole_fetch_cancels_pagination_and_allows_a_retry() {
    let cache = json!({
        "skippedVersion": "0.1.0-beta.2",
        "release": {
            "page": tag_page("v0.1.0-beta.2"),
            "version": "0.1.0-beta.2",
        },
    });
    let slow_page = |number: usize| {
        let Step::Respond(Ok(response)) = page(&["v0.1.0-beta.3"], &[("link", &next(number + 1))])
        else {
            unreachable!()
        };
        Step::Delay(Duration::from_secs(9), response)
    };
    let harness = harness(Options {
        current: "0.1.0-beta.1",
        cache: Some(cache.clone()),
        steps: (1..=8usize).map(slow_page).collect(),
        ..Options::default()
    })
    .await;

    let check = harness.updates.check(false);
    harness.fake.reached(1).await;
    let overlapping = harness.updates.check(false).await.unwrap();

    assert_eq!(failure(&overlapping), "Update check timed out.");
    assert_eq!(check.await.unwrap(), overlapping);
    assert_eq!(harness.updates.state().await.unwrap(), overlapping);
    assert_eq!(harness.fake.dropped(), [page_url(1), page_url(4)]);
    assert_eq!(harness.folder.cache(), cache);
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(harness.fake.requests(), 5);

    harness.fake.clear();
    harness.fake.push(page(&["v0.1.0-beta.4"], &[]));
    let retried = harness.updates.check(false).await.unwrap();
    assert_eq!(latest_version(&retried).as_deref(), Some("0.1.0-beta.4"));
}

#[tokio::test(start_paused = true)]
async fn rechecks_later_pages_even_when_the_first_page_is_unchanged() {
    let first_page = || page(&["v0.0.2"], &[("etag", "first-page"), ("link", &next(2))]);
    let harness = harness(Options {
        current: "0.1.0-beta.1",
        cache: Some(json!({
            "etag": "legacy-first-page",
            "release": {
                "page": tag_page("v0.0.2"),
                "version": "0.0.2",
            },
        })),
        steps: vec![
            first_page(),
            page(&["v0.1.0-beta.2"], &[("etag", "second-page-1")]),
            first_page(),
            page(&["v0.1.0-beta.3"], &[("etag", "second-page-2")]),
        ],
        ..Options::default()
    })
    .await;

    let first = harness.updates.check(false).await.unwrap();
    let second = harness.updates.check(false).await.unwrap();

    assert_eq!(latest_version(&first).as_deref(), Some("0.1.0-beta.2"));
    assert_eq!(latest_version(&second).as_deref(), Some("0.1.0-beta.3"));
    for headers in harness.fake.headers() {
        assert!(!headers.contains_key("if-none-match"));
    }
    let cache = harness.folder.cache();
    assert_eq!(cache["release"]["version"], "0.1.0-beta.3");
    assert!(cache.get("etag").is_none());
}

#[tokio::test(start_paused = true)]
async fn preserves_the_previous_cache_when_a_later_release_page_fails() {
    let failures = [
        Step::Respond(Ok(response(404, &[], ""))),
        Step::Respond(Ok(response(200, &[], "invalid JSON"))),
    ];
    for failed in failures {
        let cache = json!({
            "etag": "legacy-first-page",
            "skippedVersion": "0.1.0-beta.2",
            "release": {
                "page": tag_page("v0.1.0-beta.2"),
                "version": "0.1.0-beta.2",
            },
        });
        let harness = harness(Options {
            current: "0.1.0-beta.1",
            cache: Some(cache.clone()),
            steps: vec![page(&["v0.1.0-beta.3"], &[("link", &next(2))]), failed],
            ..Options::default()
        })
        .await;

        let state = harness.updates.check(false).await.unwrap();

        assert!(matches!(state, UpdateState::Failed { .. }));
        assert_eq!(harness.folder.cache(), cache);
    }
}

#[tokio::test(start_paused = true)]
async fn offers_prerelease_builds_the_matching_stable_release() {
    let harness = harness(Options {
        current: "0.1.0-beta.2",
        steps: vec![page(&["v0.1.0", "v0.1.0-beta.2"], &[])],
        ..Options::default()
    })
    .await;

    let state = harness.updates.check(false).await.unwrap();

    assert_eq!(available(&state).version, version("0.1.0"));
}

#[tokio::test(start_paused = true)]
async fn rejects_draft_and_prerelease_payloads() {
    for (draft, prerelease) in [(true, false), (false, true)] {
        let body = json!({
            "draft": draft,
            "html_url": tag_page("v1.2.3"),
            "prerelease": prerelease,
            "tag_name": "v1.2.3",
        });
        let harness = harness(Options {
            current: "1.2.2",
            steps: vec![Step::Respond(Ok(response(200, &[], &body.to_string())))],
            ..Options::default()
        })
        .await;

        let state = harness.updates.check(false).await.unwrap();

        assert_eq!(failure(&state), "Latest release is not a stable release.");
    }
}

#[tokio::test(start_paused = true)]
async fn reuses_cached_etags_and_skips_network_work_while_updates_are_off() {
    let harness = harness(Options {
        cache: Some(json!({
            "etag": "etag-1",
            "release": {
                "page": tag_page("v1.0.0"),
                "version": "1.0.0",
            },
        })),
        steps: vec![Step::Respond(Ok(response(304, &[], "")))],
        ..Options::default()
    })
    .await;

    let state = harness.updates.check(false).await.unwrap();

    assert_eq!(latest_version(&state).as_deref(), Some("1.0.0"));
    assert!(matches!(state, UpdateState::Current { .. }));
    assert_eq!(
        harness.fake.headers()[0].get("if-none-match").unwrap(),
        "etag-1"
    );

    let disabled = self::harness(Options {
        enabled: false,
        ..Options::default()
    })
    .await;
    assert_eq!(
        disabled.updates.check(false).await.unwrap(),
        UpdateState::Disabled
    );
    assert_eq!(disabled.fake.requests(), 0);
}

#[tokio::test(start_paused = true)]
async fn an_unchanged_release_without_a_cache_reports_the_current_version() {
    let harness = harness(Options {
        steps: vec![Step::Respond(Ok(response(304, &[], "")))],
        ..Options::default()
    })
    .await;

    let state = harness.updates.check(false).await.unwrap();

    assert!(matches!(state, UpdateState::Current { .. }));
    assert_eq!(latest_version(&state).as_deref(), Some("1.0.0"));
}

#[tokio::test(start_paused = true)]
async fn skipping_a_release_stops_its_prompt_until_a_newer_one_appears() {
    let harness = harness(Options {
        steps: vec![
            latest("v1.1.0"),
            Step::Respond(Ok(response(
                200,
                &[("etag", "etag-3")],
                &release_json("v1.1.0", false).to_string(),
            ))),
            latest("v1.2.0"),
        ],
        ..Options::default()
    })
    .await;
    let updates = &harness.updates;

    assert!(!updates.skip(&version("1.1.0")).await.unwrap());
    updates.check(false).await.unwrap();
    assert!(updates.should_prompt(&version("1.1.0")).await.unwrap());
    assert!(!updates.skip(&version("1.2.0")).await.unwrap());
    assert!(updates.skip(&version("1.1.0")).await.unwrap());
    assert!(!updates.should_prompt(&version("1.1.0")).await.unwrap());
    assert_eq!(harness.folder.cache()["skippedVersion"], "1.1.0");

    updates.check(false).await.unwrap();
    assert!(!updates.should_prompt(&version("1.1.0")).await.unwrap());
    assert_eq!(harness.folder.cache()["etag"], "etag-3");

    let state = updates.check(false).await.unwrap();
    assert_eq!(available(&state).version, version("1.2.0"));
    assert!(updates.should_prompt(&version("1.1.0")).await.unwrap());
    assert!(harness.folder.cache().get("skippedVersion").is_none());
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_cache_is_ignored_with_a_warning() {
    let logs = Logs::default();
    let _logging = logs.enter();
    let skipped = |page: &str| {
        json!({
            "release": {"version": "1.0.0", "page": page},
            "skippedVersion": "1.0.0",
        })
    };
    let malformed = harness(Options::default()).await;
    std::fs::write(malformed.folder.cache_path(), "{").unwrap();
    let foreign = harness(Options {
        cache: Some(skipped(
            "https://example.com/toommyliu/lucent/releases/tag/v1.0.0",
        )),
        ..Options::default()
    })
    .await;
    let readable = harness(Options {
        cache: Some(skipped(&tag_page("v1.0.0"))),
        ..Options::default()
    })
    .await;

    let prompts = |harness: &Harness| harness.updates.should_prompt(&version("1.0.0"));
    assert!(prompts(&malformed).await.unwrap());
    assert!(prompts(&foreign).await.unwrap());
    assert!(!prompts(&readable).await.unwrap());

    let warnings = logs.messages();
    let warned = |text: &str| warnings.iter().filter(|line| *line == text).count();
    assert_eq!(warned("Failed to load release cache"), 2);
}

#[tokio::test(start_paused = true)]
async fn a_replaced_check_answers_with_the_newer_result_before_any_state_exists() {
    let harness = harness(Options {
        enabled: false,
        ..Options::default()
    })
    .await;

    let first = harness.updates.check(false);
    let second = harness.updates.check(false);

    assert_eq!(first.await, Ok(UpdateState::Disabled));
    assert_eq!(second.await, Ok(UpdateState::Disabled));
}

#[test]
fn release_pages_are_only_lucent_github_release_pages() {
    let page = |raw: &str| ReleasePage::parse(raw).map(|page| page.url().to_string());
    assert_eq!(
        page("https://github.com/toommyliu/lucent/releases/tag/v1.2.3").as_deref(),
        Some("https://github.com/toommyliu/lucent/releases/tag/v1.2.3")
    );
    for refused in [
        "https://github.com/toommyliu/other/releases/tag/v1.2.3",
        "https://github.com.evil.test/toommyliu/lucent/releases/tag/v1.2.3",
        "https://github.com/toommyliu/lucent/issues/1",
        "http://github.com/toommyliu/lucent/releases/tag/v1.2.3",
        "https://user@github.com/toommyliu/lucent/releases/tag/v1.2.3",
        "https://github.com:8443/toommyliu/lucent/releases/tag/v1.2.3",
        "https://github.com/toommyliu/lucent/releases/../issues/1",
        "not a url",
    ] {
        assert_eq!(page(refused), None, "{refused}");
    }
}

#[test]
fn versions_follow_node_semver_for_tags() {
    let parsed = |text: &str| semver(text).map(|version| version.to_string());
    assert_eq!(parsed("v1.2.3").as_deref(), Some("1.2.3"));
    assert_eq!(parsed(" 1.2.3-beta.1 ").as_deref(), Some("1.2.3-beta.1"));
    assert_eq!(parsed("1.2.3+build.5").as_deref(), Some("1.2.3"));
    assert_eq!(parsed("nightly"), None);
    assert_eq!(parsed("1.2"), None);
    assert_eq!(parsed("01.2.3"), None);
    assert!(semver("0.1.0-beta.10") > semver("0.1.0-beta.9"));
    assert!(semver("0.1.0") > semver("0.1.0-beta.2"));
}
