mod common;

use common::*;
use lucent_host::accounts::{AccountEdit, AccountsError, AccountsSnapshot, Password, Reply};
use lucent_host::scripting::ScriptRun;
use lucent_host::{Host, HostError, testing};
use lucent_protocol::script::ScriptOutcome;
use lucent_protocol::script_options::{RoomPolicy, ScriptOptions, ScriptOptionsPatch};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::Instant;
use tracing::field::{Field, Visit};
use tracing_subscriber::{Layer, Registry, layer, prelude::*};

const CANARY: &str = "credential-canary-5f3c";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lucent-accounts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("scripts")).unwrap();
        Self(path)
    }

    fn scripts(&self) -> PathBuf {
        self.0.join("scripts")
    }

    fn app_data(&self) -> PathBuf {
        self.0.join("app-data")
    }

    fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.app_data(), |path, segment| path.join(segment))
    }

    fn host(&self) -> Host {
        testing::headless_host_with_roots(self.scripts(), self.app_data()).unwrap()
    }

    fn write(&self, relative: &str, contents: impl AsRef<[u8]>) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wait<T>(reply: Reply<T>) -> Result<T, AccountsError> {
    struct Unpark(thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut reply = std::pin::pin!(reply);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Poll::Ready(result) = reply.as_mut().poll(&mut Context::from_waker(&waker)) {
            return result;
        }
        let left = deadline
            .checked_duration_since(Instant::now())
            .expect("the account data service did not answer");
        thread::park_timeout(left);
    }
}

#[derive(Default)]
struct Wakes(AtomicUsize);

impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn watch(host: &Host) -> Arc<Wakes> {
    let wakes = Arc::new(Wakes::default());
    host.accounts().watch(Waker::from(wakes.clone()));
    wakes
}

fn count_writes(host: &Host) -> Arc<AtomicUsize> {
    let writes = Arc::new(AtomicUsize::new(0));
    let counter = writes.clone();
    host.set_app_data_probe(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    writes
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<BTreeMap<String, Value>>>>);

struct Fields(BTreeMap<String, Value>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().into(), format!("{value:?}").into());
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().into(), value.into());
    }
}

impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: layer::Context<'_, S>) {
        let mut fields = Fields(BTreeMap::new());
        event.record(&mut fields);
        fields
            .0
            .insert("level".into(), event.metadata().level().as_str().into());
        self.0.lock().unwrap().push(fields.0);
    }
}

impl Capture {
    fn start() -> (Self, tracing::dispatcher::DefaultGuard) {
        let capture = Self::default();
        let dispatch = tracing::Dispatch::new(Registry::default().with(capture.clone()));
        (capture, tracing::dispatcher::set_default(&dispatch))
    }

    fn take(&self) -> Vec<Value> {
        let events = std::mem::take(&mut *self.0.lock().unwrap());
        events
            .into_iter()
            .map(|event| Value::Object(event.into_iter().collect()))
            .collect()
    }
}

fn file(value: Value) -> String {
    serde_json::to_string_pretty(&value).unwrap() + "\n"
}

fn account(username: &str, password: &str) -> Value {
    json!({"label": username, "username": username, "password": password})
}

fn create(username: &str) -> AccountEdit {
    AccountEdit::CreateAccount {
        label: None,
        username: username.into(),
        password: Password::new("secret"),
    }
}

fn create_group(name: &str, usernames: &[&str]) -> AccountEdit {
    AccountEdit::CreateGroup {
        name: name.into(),
        usernames: usernames.iter().map(|name| (*name).into()).collect(),
    }
}

fn unchanged() -> AccountEdit {
    AccountEdit::DeleteAccounts { usernames: vec![] }
}

fn usernames(snapshot: &AccountsSnapshot) -> Vec<&str> {
    snapshot
        .accounts()
        .iter()
        .map(|account| account.username())
        .collect()
}

fn groups(snapshot: &AccountsSnapshot) -> Vec<(&str, Vec<&str>)> {
    snapshot
        .groups()
        .iter()
        .map(|group| {
            let members = group.usernames().iter().map(String::as_str);
            (group.name(), members.collect())
        })
        .collect()
}

fn read_failure(workspace: &Workspace, relative: &str, too_large: bool) -> String {
    let limit = if too_large {
        " The document exceeds the limit of 8388608 bytes."
    } else {
        ""
    };
    format!(
        "JSON file read failed at {}.{limit}",
        workspace.path(relative).display()
    )
}

fn file_failure<T: std::fmt::Debug>(result: Result<T, AccountsError>) -> String {
    match result {
        Err(error @ AccountsError::File(_)) => error.to_string(),
        other => panic!("expected a file failure, got {other:?}"),
    }
}

fn accounts_warning(message: &str, fields: Value) -> Value {
    let mut event = json!({
        "level": "WARN",
        "message": message,
        "component": "accounts",
        "path": "accounts.json",
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    event
}

fn settings_warning(message: &str, fields: Value) -> Value {
    let mut event = json!({
        "level": "WARN",
        "message": message,
        "component": "account-settings",
        "path": "account-settings/[account].json",
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    event
}

const UNREADABLE_ACCOUNTS: &str =
    "Saved accounts could not be read; using defaults until the next save";
const INVALID_ACCOUNTS: &str =
    "Saved accounts had invalid entries; ignoring them until the next save";
const UNREADABLE_SETTINGS: &str =
    "Saved account settings could not be read; using defaults until the next save";
const INVALID_SETTINGS: &str =
    "Saved account settings had invalid entries; ignoring them until the next save";

#[test]
fn notifies_only_changed_storage_and_never_the_first_load() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let wakes = watch(&host);
    let accounts = host.accounts();
    assert_eq!(
        wait(accounts.snapshot()).unwrap(),
        AccountsSnapshot::default()
    );
    assert_eq!(
        wait(accounts.snapshot()).unwrap(),
        AccountsSnapshot::default()
    );
    assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
    wait(accounts.edit(create_group("Raiders", &[]))).unwrap();
    wait(accounts.edit(unchanged())).unwrap();
    let raiders = file(json!({"accounts": [], "groups": {"Raiders": []}}));
    assert_eq!(workspace.read("accounts.json"), raiders);
    assert_eq!(
        groups(&wait(accounts.snapshot()).unwrap()),
        [("Raiders", vec![])]
    );
    workspace.write("accounts.json", r#"{"accounts":[],"groups":{"Alts":[]}}"#);
    assert_eq!(
        groups(&wait(accounts.snapshot()).unwrap()),
        [("Raiders", vec![])]
    );
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
}

#[test]
fn keeps_storage_when_a_save_fails_or_changes_nothing() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let accounts = host.accounts();
    assert_eq!(
        wait(accounts.edit(unchanged())).unwrap(),
        AccountsSnapshot::default()
    );
    assert!(!workspace.path("accounts.json").exists());
    wait(accounts.edit(create_group("Raiders", &[]))).unwrap();
    fs::remove_file(workspace.path("accounts.json")).unwrap();
    fs::create_dir(workspace.path("accounts.json")).unwrap();
    assert_eq!(
        groups(&wait(accounts.edit(unchanged())).unwrap()),
        [("Raiders", vec![])]
    );
    let error = wait(accounts.edit(create_group("Alts", &[]))).unwrap_err();
    assert_eq!(
        error.to_string(),
        format!(
            "JSON file rename failed at {}.",
            workspace.path("accounts.json").display()
        )
    );
    assert_eq!(
        groups(&wait(accounts.snapshot()).unwrap()),
        [("Raiders", vec![])]
    );
    assert!(workspace.path("accounts.json").is_dir());
    assert_eq!(fs::read_dir(workspace.app_data()).unwrap().count(), 1);
}

#[test]
fn keeps_valid_entries_and_preserves_invalid_source_until_a_changed_save() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let accounts = host.accounts();
    let source = json!({
        "accounts": [
            account("Hero", "secret"),
            {"label": "Duplicate", "username": "hero", "password": "private"},
            {"label": "Broken", "username": "Broken", "password": 42},
        ],
        "groups": {"Raiders": ["Hero", "Hero", "Missing", 42], " raiders ": ["Hero"], " ": ["Hero"]},
    })
    .to_string();
    workspace.write("accounts.json", &source);
    let snapshot = wait(accounts.snapshot()).unwrap();
    assert_eq!(usernames(&snapshot), ["Hero"]);
    assert_eq!(snapshot.accounts()[0].label(), "Hero");
    assert_eq!(
        snapshot.accounts()[0].password().reveal_for_editor(),
        "secret"
    );
    assert_eq!(groups(&snapshot), [("Raiders", vec!["Hero"])]);
    assert_eq!(
        capture.take(),
        [accounts_warning(
            INVALID_ACCOUNTS,
            json!({"ignored": r#"{"accounts":2,"groups":2,"groupMembers":5}"#})
        )]
    );
    assert_eq!(workspace.read("accounts.json"), source);
    wait(accounts.edit(unchanged())).unwrap();
    assert_eq!(workspace.read("accounts.json"), source);
    let saved = wait(accounts.edit(create_group("Alts", &[]))).unwrap();
    assert_eq!(
        groups(&saved),
        [("Raiders", vec!["Hero"]), ("Alts", vec![])]
    );
    assert_eq!(
        workspace.read("accounts.json"),
        file(json!({
            "accounts": [account("Hero", "secret")],
            "groups": {"Raiders": ["Hero"], "Alts": []},
        }))
    );
    assert_eq!(capture.take(), Vec::<Value>::new());
}

#[test]
fn uses_defaults_without_overwriting_unreadable_source() {
    for (source, position) in [
        ("{not-json", json!({"line": 1, "column": 2})),
        ("[]", json!({})),
        ("null", json!({})),
        ("42", json!({})),
    ] {
        let (capture, _guard) = Capture::start();
        let workspace = Workspace::new();
        let host = workspace.host();
        workspace.write("accounts.json", source);
        assert_eq!(
            wait(host.accounts().snapshot()).unwrap(),
            AccountsSnapshot::default()
        );
        let mut fields = json!({"ignored": r#"{"file":1}"#});
        fields
            .as_object_mut()
            .unwrap()
            .extend(position.as_object().unwrap().clone());
        assert_eq!(
            capture.take(),
            [accounts_warning(UNREADABLE_ACCOUNTS, fields)],
            "{source}"
        );
        assert_eq!(workspace.read("accounts.json"), source);
        wait(host.accounts().edit(create_group("Raiders", &[]))).unwrap();
        assert_eq!(
            workspace.read("accounts.json"),
            file(json!({"accounts": [], "groups": {"Raiders": []}}))
        );
    }
}

#[test]
fn loads_missing_storage_silently() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    assert_eq!(
        wait(host.accounts().snapshot()).unwrap(),
        AccountsSnapshot::default()
    );
    assert_eq!(capture.take(), Vec::<Value>::new());
    assert_eq!(fs::read_dir(workspace.app_data()).unwrap().count(), 0);
}

#[test]
fn loads_valid_optional_fields_silently() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = r#"{"accounts":[{"label":"Hero","username":"Hero","password":"secret"}]}"#;
    workspace.write("accounts.json", source);
    let snapshot = wait(host.accounts().snapshot()).unwrap();
    assert_eq!(usernames(&snapshot), ["Hero"]);
    assert!(snapshot.groups().is_empty());
    assert_eq!(capture.take(), Vec::<Value>::new());
    assert_eq!(workspace.read("accounts.json"), source);
}

#[test]
fn keeps_valid_account_data_when_a_collection_has_the_wrong_shape() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = r#"{"accounts":[{"label":"Hero","username":"Hero","password":"secret"}],"groups":{"Raiders":42}}"#;
    workspace.write("accounts.json", source);
    let snapshot = wait(host.accounts().snapshot()).unwrap();
    assert_eq!(usernames(&snapshot), ["Hero"]);
    assert_eq!(groups(&snapshot), [("Raiders", vec![])]);
    assert_eq!(
        capture.take(),
        [accounts_warning(
            INVALID_ACCOUNTS,
            json!({"reset": r#"["groups"]"#})
        )]
    );
    assert_eq!(workspace.read("accounts.json"), source);
}

#[test]
fn reports_both_ignored_accounts_and_reset_group_fields_in_one_warning() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = r#"{"accounts":[{"label":"Hero","username":"Hero","password":"secret"},null],"groups":{"Raiders":42}}"#;
    workspace.write("accounts.json", source);
    let snapshot = wait(host.accounts().snapshot()).unwrap();
    assert_eq!(usernames(&snapshot), ["Hero"]);
    assert_eq!(groups(&snapshot), [("Raiders", vec![])]);
    assert_eq!(
        capture.take(),
        [accounts_warning(
            INVALID_ACCOUNTS,
            json!({
                "ignored": r#"{"accounts":1,"groups":0,"groupMembers":0}"#,
                "reset": r#"["groups"]"#,
            })
        )]
    );
    assert_eq!(workspace.read("accounts.json"), source);
}

fn permissions_apply() -> bool {
    #[cfg(unix)]
    return unsafe { libc::geteuid() != 0 };
    #[cfg(not(unix))]
    return false;
}

fn set_mode(path: &std::path::Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

#[cfg(unix)]
#[test]
fn fails_reads_and_updates_without_replacing_unreadable_valid_accounts() {
    if !permissions_apply() {
        return;
    }
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source =
        r#"{"accounts":[{"label":"Hero","username":"Hero","password":"secret"}],"groups":{}}"#;
    workspace.write("accounts.json", source);
    set_mode(&workspace.path("accounts.json"), 0o000);
    let unreadable = read_failure(&workspace, "accounts.json", false);
    assert_eq!(file_failure(wait(host.accounts().snapshot())), unreadable);
    assert_eq!(
        file_failure(wait(host.accounts().edit(create_group("Raiders", &[])))),
        unreadable
    );
    assert_eq!(capture.take(), Vec::<Value>::new());
    set_mode(&workspace.path("accounts.json"), 0o600);
    assert_eq!(workspace.read("accounts.json"), source);
    assert_eq!(
        usernames(&wait(host.accounts().snapshot()).unwrap()),
        ["Hero"]
    );
}

#[test]
fn rejects_accounts_above_the_read_limit_without_replacing_them() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source =
        json!({"accounts": [], "groups": {}, "padding": "x".repeat(8 * 1024 * 1024)}).to_string();
    workspace.write("accounts.json", &source);
    let too_large = read_failure(&workspace, "accounts.json", true);
    assert_eq!(file_failure(wait(host.accounts().snapshot())), too_large);
    assert_eq!(
        file_failure(wait(host.accounts().edit(create_group("Raiders", &[])))),
        too_large
    );
    assert_eq!(capture.take(), Vec::<Value>::new());
    assert_eq!(workspace.read("accounts.json"), source);
}

#[test]
fn saves_accounts_without_recovery_warnings_before_loading() {
    for source in ["{", r#"{"accounts":[null],"groups":{}}"#] {
        let (capture, _guard) = Capture::start();
        let workspace = Workspace::new();
        let host = workspace.host();
        workspace.write("accounts.json", source);
        let saved = wait(host.accounts().edit(create_group("Raiders", &[]))).unwrap();
        assert!(saved.accounts().is_empty());
        assert_eq!(groups(&saved), [("Raiders", vec![])]);
        assert_eq!(
            workspace.read("accounts.json"),
            file(json!({"accounts": [], "groups": {"Raiders": []}}))
        );
        assert_eq!(capture.take(), Vec::<Value>::new(), "{source}");
    }
}

#[test]
fn publishes_nothing_for_unchanged_storage() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let wakes = watch(&host);
    let accounts = host.accounts();
    wait(accounts.snapshot()).unwrap();
    wait(accounts.edit(unchanged())).unwrap();
    assert_eq!(
        wait(accounts.edit(AccountEdit::CreateAccount {
            label: None,
            username: "".into(),
            password: Password::new("secret"),
        })),
        Err(AccountsError::Required("username"))
    );
    assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
    wait(accounts.edit(create("Alice"))).unwrap();
    wait(accounts.edit(AccountEdit::UpdateAccount {
        username: "Alice".into(),
        label: None,
        new_username: None,
        password: None,
    }))
    .unwrap();
    wait(accounts.edit(unchanged())).unwrap();
    wait(accounts.snapshot()).unwrap();
    assert_eq!(
        wait(accounts.edit(create("Alice"))),
        Err(AccountsError::UsernameTaken)
    );
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
    workspace.write(
        "accounts.json",
        r#"{"accounts":[{"label":"Bob","username":"Bob","password":"secret"}],"groups":{}}"#,
    );
    assert_eq!(usernames(&wait(accounts.snapshot()).unwrap()), ["Alice"]);
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
}

#[test]
fn persists_account_and_group_mutations() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let accounts = host.accounts();
    wait(accounts.edit(create("Alice"))).unwrap();
    assert_eq!(
        workspace.read("accounts.json"),
        file(json!({"accounts": [account("Alice", "secret")], "groups": {}}))
    );
    let state = wait(accounts.edit(create_group("Party", &["Alice"]))).unwrap();
    assert_eq!(usernames(&state), ["Alice"]);
    assert_eq!(state.accounts()[0].label(), "Alice");
    assert_eq!(state.accounts()[0].password().reveal_for_editor(), "secret");
    assert_eq!(groups(&state), [("Party", vec!["Alice"])]);
    let saved =
        file(json!({"accounts": [account("Alice", "secret")], "groups": {"Party": ["Alice"]}}));
    assert_eq!(workspace.read("accounts.json"), saved);
    assert_eq!(
        wait(accounts.edit(AccountEdit::CreateAccount {
            label: None,
            username: "alice".into(),
            password: Password::new("other"),
        })),
        Err(AccountsError::UsernameTaken)
    );
    assert_eq!(workspace.read("accounts.json"), saved);
}

#[test]
fn deletes_multiple_accounts_and_group_memberships_atomically() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let accounts = host.accounts();
    for username in ["Alice", "Bob", "Cara"] {
        wait(accounts.edit(create(username))).unwrap();
    }
    wait(accounts.edit(create_group("Party", &["Alice", "Bob", "Cara"]))).unwrap();
    let before = workspace.read("accounts.json");
    assert_eq!(
        wait(accounts.edit(AccountEdit::DeleteAccounts {
            usernames: vec!["Alice".into(), "Missing".into()],
        })),
        Err(AccountsError::AccountsNotFound)
    );
    assert_eq!(wait(accounts.snapshot()).unwrap().accounts().len(), 3);
    assert_eq!(workspace.read("accounts.json"), before);
    let state = wait(accounts.edit(AccountEdit::DeleteAccounts {
        usernames: vec!["Alice".into(), "Bob".into()],
    }))
    .unwrap();
    assert_eq!(usernames(&state), ["Cara"]);
    assert_eq!(groups(&state), [("Party", vec!["Cara"])]);
    assert_eq!(
        workspace.read("accounts.json"),
        file(json!({"accounts": [account("Cara", "secret")], "groups": {"Party": ["Cara"]}}))
    );
}

#[test]
fn renames_an_account_and_then_changes_only_its_case() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let accounts = host.accounts();
    wait(accounts.edit(create("Alice"))).unwrap();
    for (current, next) in [("Alice", "Alicia"), ("Alicia", "ALICIA")] {
        wait(accounts.edit(AccountEdit::UpdateAccount {
            username: current.into(),
            label: None,
            new_username: Some(next.into()),
            password: None,
        }))
        .unwrap();
    }
    assert_eq!(
        workspace.read("accounts.json"),
        file(json!({
            "accounts": [{"label": "Alice", "username": "ALICIA", "password": "secret"}],
            "groups": {},
        }))
    );
}

#[test]
fn a_dropped_edit_still_commits() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let (entered, rename) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    host.set_app_data_probe(move || {
        entered.send(()).unwrap();
        released.lock().unwrap().recv().unwrap();
    });
    let accounts = host.accounts();
    wait(accounts.snapshot()).unwrap();
    let wakes = watch(&host);
    drop(accounts.edit(create("Committed")));
    rename.recv_timeout(TIMEOUT).unwrap();
    release.send(()).unwrap();
    let current = wait(accounts.snapshot()).unwrap();
    assert_eq!(usernames(&current), ["Committed"]);
    assert_eq!(
        workspace.read("accounts.json"),
        file(json!({"accounts": [account("Committed", "secret")], "groups": {}}))
    );
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
    release.send(()).unwrap();
    let after = wait(accounts.edit(create("After"))).unwrap();
    assert_eq!(usernames(&after), ["Committed", "After"]);
    assert_eq!(
        workspace.read("accounts.json"),
        file(json!({
            "accounts": [account("Committed", "secret"), account("After", "secret")],
            "groups": {},
        }))
    );
    assert_eq!(wakes.0.load(Ordering::SeqCst), 2);
}

#[test]
fn queues_every_request_behind_a_held_save() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let (entered, rename) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    host.set_app_data_probe(move || {
        entered.send(()).unwrap();
        released.lock().unwrap().recv().unwrap();
    });
    let accounts = host.accounts();
    let held = accounts.edit(create("First"));
    rename.recv_timeout(TIMEOUT).unwrap();
    let reads: Vec<_> = (0..200).map(|_| accounts.snapshot()).collect();
    let edit = accounts.edit(create("Second"));
    release.send(()).unwrap();
    release.send(()).unwrap();
    assert_eq!(usernames(&wait(held).unwrap()), ["First"]);
    for read in reads {
        assert_eq!(usernames(&wait(read).unwrap()), ["First"]);
    }
    assert_eq!(usernames(&wait(edit).unwrap()), ["First", "Second"]);
}

#[test]
fn equivalent_edits_neither_write_nor_wake() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let writes = count_writes(&host);
    let wakes = watch(&host);
    let accounts = host.accounts();
    let counts = || {
        (
            writes.load(Ordering::SeqCst),
            wakes.0.load(Ordering::SeqCst),
        )
    };
    wait(accounts.edit(unchanged())).unwrap();
    assert_eq!(counts(), (0, 0));
    wait(accounts.edit(create("Alice"))).unwrap();
    wait(accounts.edit(create_group("Party", &["Alice"]))).unwrap();
    assert_eq!(counts(), (2, 2));
    for edit in [
        unchanged(),
        AccountEdit::UpdateAccount {
            username: " Alice ".into(),
            label: Some(" Alice ".into()),
            new_username: Some("Alice".into()),
            password: Some(Password::new(" secret ")),
        },
        AccountEdit::UpdateGroup {
            name: "party".into(),
            new_name: Some("Party".into()),
            usernames: Some(vec!["Alice".into()]),
        },
        AccountEdit::DeleteAccounts { usernames: vec![] },
    ] {
        wait(accounts.edit(edit)).unwrap();
    }
    assert_eq!(counts(), (2, 2));
    wait(accounts.edit(AccountEdit::UpdateGroup {
        name: "Party".into(),
        new_name: None,
        usernames: Some(vec![]),
    }))
    .unwrap();
    assert_eq!(counts(), (3, 3));
}

#[test]
fn unreadable_accounts_fail_reads_and_edits_without_changing_bytes() {
    let valid = r#"{"accounts":[{"label":"Hero","username":"Hero","password":"secret"}]}"#;
    let large =
        json!({"accounts": [account("Hero", "secret")], "padding": "x".repeat(8 * 1024 * 1024)})
            .to_string();
    for (name, contents, denied, too_large) in [
        ("a directory", None, false, false),
        ("over the limit", Some(large.into_bytes()), false, true),
        (
            "permission denied",
            Some(valid.as_bytes().to_vec()),
            true,
            false,
        ),
    ] {
        if denied && !permissions_apply() {
            continue;
        }
        let workspace = Workspace::new();
        let host = workspace.host();
        let writes = count_writes(&host);
        let path = workspace.path("accounts.json");
        match &contents {
            Some(bytes) => workspace.write("accounts.json", bytes),
            None => fs::create_dir_all(&path).unwrap(),
        }
        if denied {
            set_mode(&path, 0o000);
        }
        let failure = read_failure(&workspace, "accounts.json", too_large);
        assert_eq!(
            file_failure(wait(host.accounts().snapshot())),
            failure,
            "{name}"
        );
        for edit in [create("New"), unchanged(), create_group("Raiders", &[])] {
            assert_eq!(
                file_failure(wait(host.accounts().edit(edit))),
                failure,
                "{name}"
            );
        }
        assert_eq!(writes.load(Ordering::SeqCst), 0, "{name}");
        if denied {
            set_mode(&path, 0o600);
        }
        assert_eq!(fs::read(&path).ok(), contents, "{name}");
    }
}

#[test]
fn edits_from_two_threads_both_survive() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let sides = ["Left", "Right"].map(|side| {
        let accounts = host.accounts().clone();
        thread::spawn(move || {
            for index in 0..50 {
                wait(accounts.edit(create(&format!("{side}{index}")))).unwrap();
            }
        })
    });
    for side in sides {
        side.join().unwrap();
    }
    let expected = |snapshot: &AccountsSnapshot| {
        for side in ["Left", "Right"] {
            let names: Vec<_> = usernames(snapshot)
                .into_iter()
                .filter(|name| name.starts_with(side))
                .collect();
            let wanted: Vec<_> = (0..50).map(|index| format!("{side}{index}")).collect();
            assert_eq!(names, wanted);
        }
        assert_eq!(snapshot.accounts().len(), 100);
    };
    expected(&wait(host.accounts().snapshot()).unwrap());
    drop(host);
    expected(&wait(workspace.host().accounts().snapshot()).unwrap());
}

#[test]
fn credentials_stay_out_of_logs_errors_and_debug() {
    let (capture, _guard) = Capture::start();
    let mut texts = Vec::new();

    let workspace = Workspace::new();
    let host = workspace.host();
    let accounts = host.accounts();
    let edit = AccountEdit::CreateAccount {
        label: None,
        username: "Hero".into(),
        password: Password::new(format!(" {CANARY} ")),
    };
    texts.push(format!("{edit:?}"));
    let created = wait(accounts.edit(edit)).unwrap();
    assert_eq!(created.accounts()[0].password().reveal_for_editor(), CANARY);
    assert!(workspace.read("accounts.json").contains(CANARY));
    texts.push(format!("{created:?}"));
    texts.push(format!("{:?}", created.accounts()[0]));
    texts.push(format!("{:?}", Password::new(CANARY)));
    for edit in [
        AccountEdit::CreateAccount {
            label: None,
            username: "hero".into(),
            password: Password::new(CANARY),
        },
        AccountEdit::UpdateAccount {
            username: "Missing".into(),
            label: None,
            new_username: None,
            password: Some(Password::new(CANARY)),
        },
    ] {
        let error = wait(accounts.edit(edit)).unwrap_err();
        texts.push(error.to_string());
        texts.push(format!("{error:?}"));
    }

    for source in [
        format!(r#"{{"accounts":[{{"label":"Hero","username":"Hero","password":{CANARY}}}]}}"#),
        format!(r#"{{"accounts":[{{"label":"Hero","username":"Hero","password":"{CANARY}\q"}}]}}"#),
        format!(
            r#"{{"accounts":[{{"label":"Hero","username":"Hero","password":"{CANARY}"}},{{"label":1,"username":"Alt","password":"{CANARY}"}}],"groups":{{"Raiders":["{CANARY}"]}}}}"#
        ),
        format!(
            r#"{{"accounts":[],"padding":"{}{CANARY}"}}"#,
            "x".repeat(8 * 1024 * 1024)
        ),
    ] {
        let workspace = Workspace::new();
        workspace.write("accounts.json", &source);
        let host = workspace.host();
        match wait(host.accounts().snapshot()) {
            Ok(snapshot) => texts.push(format!("{snapshot:?}")),
            Err(error) => {
                texts.push(error.to_string());
                texts.push(format!("{error:?}"));
            }
        }
        let error = wait(host.accounts().edit(AccountEdit::UpdateAccount {
            username: "Nobody".into(),
            label: None,
            new_username: None,
            password: Some(Password::new(CANARY)),
        }))
        .unwrap_err();
        texts.push(error.to_string());
        texts.push(format!("{error:?}"));
    }

    let events = capture.take();
    assert!(events.len() >= 2, "{events:?}");
    assert!(
        events
            .iter()
            .any(|event| event.get("line").is_some() && event.get("column").is_some()),
        "{events:?}"
    );
    texts.push(serde_json::to_string(&events).unwrap());
    for text in texts {
        assert!(!text.contains("canary"), "{text}");
    }
}

fn run_script(host: &Host, body: &str) -> Value {
    let (_session, link) = testing::start_probe(host, ready_movie());
    log_in(&link);
    let source = format!(
        r#"
        import * as fs from "lucent/filesystem";
        import * as script from "lucent/script";
        import {{ Effect }} from "effect";
        const attempt = effect => effect.pipe(
            Effect.map(value => value === undefined ? "missing" : value),
            Effect.catch(error => Effect.succeed(error.reason)));
        export default function* () {{
            const result = yield* Effect.gen(function* () {{ {body} }});
            yield* script.stop(JSON.stringify(result));
        }}
    "#
    );
    let run = ScriptRun::start(link, loose(&[("files.js", &source)]), Default::default()).unwrap();
    match run.wait(TIMEOUT).unwrap() {
        ScriptOutcome::StoppedByScript {
            reason: Some(value),
        } => serde_json::from_str(&value).unwrap(),
        outcome => panic!("{outcome:?}"),
    }
}

#[test]
fn scripts_cannot_reach_app_data() {
    let workspace = Workspace::new();
    let host = workspace.host();
    wait(host.accounts().edit(create("Hero"))).unwrap();
    wait(
        host.accounts()
            .update_settings("Hero", ScriptOptionsPatch::default()),
    )
    .unwrap();
    let saved = workspace.read("accounts.json");
    #[cfg(unix)]
    std::os::unix::fs::symlink(workspace.app_data(), workspace.scripts().join("linked")).unwrap();
    let result = run_script(
        &host,
        r#"
        return [
            yield* fs.list(),
            yield* attempt(fs.readText("accounts.json")),
            yield* attempt(fs.readText("../app-data/accounts.json")),
            yield* attempt(fs.readJson("linked/accounts.json")),
            yield* attempt(fs.readText("linked/account-settings/hero.json")),
            yield* attempt(fs.list("linked")),
            yield* attempt(fs.writeText("../app-data/accounts.json", "{}")),
            yield* attempt(fs.remove("linked/accounts.json")),
            yield* fs.exists("accounts.json"),
        ];
    "#,
    );
    let linked = if cfg!(unix) {
        json!([{"name": "linked", "kind": "symbolic-link"}])
    } else {
        json!([])
    };
    let through_link = if cfg!(unix) {
        "invalid-path"
    } else {
        "missing"
    };
    assert_eq!(
        result,
        json!([
            linked,
            "missing",
            "invalid-path",
            through_link,
            through_link,
            if cfg!(unix) {
                "invalid-path"
            } else {
                "not-found"
            },
            "invalid-path",
            if cfg!(unix) {
                "invalid-path"
            } else {
                "missing"
            },
            false,
        ])
    );
    assert_eq!(workspace.read("accounts.json"), saved);
    assert!(workspace.path("account-settings/hero.json").is_file());
}

#[test]
fn roots_that_nest_are_refused() {
    let workspace = Workspace::new();
    let base = workspace.0.join("roots");
    for (scripts, app_data) in [
        (base.clone(), base.clone()),
        (base.clone(), base.join("app-data")),
        (base.join("scripts"), base.clone()),
        (
            base.join("a"),
            base.join("b").join("..").join("a").join("app"),
        ),
    ] {
        assert!(
            matches!(
                testing::headless_host_with_roots(scripts.clone(), app_data.clone()),
                Err(HostError::OverlappingRoots)
            ),
            "{scripts:?} {app_data:?}"
        );
    }
    assert!(testing::headless_host_with_roots(base.join("data"), base.join("database")).is_ok());
}

#[cfg(target_os = "macos")]
#[test]
fn roots_spelled_through_the_data_volume_are_refused() {
    let workspace = Workspace::new();
    let scripts = workspace.scripts().canonicalize().unwrap();
    let data_volume =
        PathBuf::from("/System/Volumes/Data").join(scripts.strip_prefix("/").unwrap());
    assert!(data_volume.is_dir());
    for (scripts, app_data) in [
        (scripts.clone(), data_volume.join("app")),
        (data_volume.clone(), scripts.join("app")),
    ] {
        assert!(
            matches!(
                testing::headless_host_with_roots(scripts.clone(), app_data.clone()),
                Err(HostError::OverlappingRoots)
            ),
            "{scripts:?} {app_data:?}"
        );
    }
}

#[cfg(windows)]
#[test]
fn roots_spelled_through_an_administrative_share_are_refused() {
    use std::path::{Component, Prefix};
    let workspace = Workspace::new();
    let scripts = workspace.scripts().canonicalize().unwrap();
    let mut components = scripts.components();
    let (Some(Component::Prefix(prefix)), Some(Component::RootDir)) =
        (components.next(), components.next())
    else {
        panic!("{scripts:?}");
    };
    let Prefix::VerbatimDisk(drive) = prefix.kind() else {
        panic!("{scripts:?}");
    };
    let share =
        PathBuf::from(format!(r"\\localhost\{}$\", char::from(drive))).join(components.as_path());
    assert!(share.is_dir(), "{share:?}");
    for (scripts, app_data) in [
        (scripts.clone(), share.join("app")),
        (share.clone(), scripts.join("app")),
    ] {
        assert!(
            matches!(
                testing::headless_host_with_roots(scripts.clone(), app_data.clone()),
                Err(HostError::OverlappingRoots)
            ),
            "{scripts:?} {app_data:?}"
        );
    }
}

#[test]
fn roots_that_differ_only_in_case_or_normalization_are_refused() {
    let workspace = Workspace::new();
    let base = workspace.0.join("roots");
    fs::create_dir(&base).unwrap();
    for (scripts, app_data) in [("Shared", "shared"), ("caf\u{e9}", "cafe\u{301}")] {
        fs::create_dir(base.join(scripts)).unwrap();
        let aliased = base.join(app_data).exists();
        fs::remove_dir(base.join(scripts)).unwrap();
        match testing::headless_host_with_roots(base.join(scripts), base.join(app_data).join("app"))
        {
            Err(HostError::OverlappingRoots) => assert!(aliased, "{scripts:?} {app_data:?}"),
            Err(error) => panic!("{error}"),
            Ok(host) => {
                wait(host.accounts().edit(create("Hero"))).unwrap();
                assert_eq!(
                    run_script(
                        &host,
                        r#"return yield* attempt(fs.readText("app/accounts.json"));"#
                    ),
                    "missing",
                    "{scripts:?} {app_data:?}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn an_app_data_root_behind_a_dangling_link_into_scripts_is_refused() {
    let workspace = Workspace::new();
    std::os::unix::fs::symlink(workspace.scripts().join("inner"), workspace.0.join("link"))
        .unwrap();
    let app_data = workspace.0.join("link").join("app-data");
    match testing::headless_host_with_roots(workspace.scripts(), app_data) {
        Err(HostError::OverlappingRoots | HostError::AppData(_)) => {}
        Err(error) => panic!("{error}"),
        Ok(host) => {
            let created = run_script(
                &host,
                r#"return yield* attempt(fs.writeText("inner/note.txt", ""));"#,
            );
            assert_eq!(created, "missing");
            wait(host.accounts().edit(create("Hero"))).unwrap();
            assert_eq!(
                run_script(
                    &host,
                    r#"return yield* attempt(fs.readText("inner/app-data/accounts.json"));"#
                ),
                "missing"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn follows_a_linked_app_data_folder_and_settings_folder() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new();
    let data = workspace.0.join("elsewhere").join("data");
    let settings = workspace.0.join("elsewhere").join("settings");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&settings).unwrap();
    symlink(&data, workspace.app_data()).unwrap();
    symlink(&settings, data.join("account-settings")).unwrap();
    fs::write(
        data.join("accounts.json"),
        file(json!({"accounts": [account("Hero", "secret")], "groups": {}})),
    )
    .unwrap();
    fs::write(
        settings.join("hero.json"),
        settings_file(json!({"safeStartStop": true})),
    )
    .unwrap();
    let host = workspace.host();
    assert_eq!(
        usernames(&wait(host.accounts().snapshot()).unwrap()),
        ["Hero"]
    );
    assert!(
        wait(host.accounts().settings("Hero"))
            .unwrap()
            .safe_start_stop
    );
    wait(host.accounts().edit(create("Sidekick"))).unwrap();
    wait(
        host.accounts()
            .update_settings("Hero", safe_start_stop(false)),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(data.join("accounts.json")).unwrap(),
        file(json!({
            "accounts": [account("Hero", "secret"), account("Sidekick", "secret")],
            "groups": {},
        }))
    );
    assert!(
        !wait(host.accounts().settings("Hero"))
            .unwrap()
            .safe_start_stop
    );
    assert!(
        fs::read_to_string(settings.join("hero.json"))
            .unwrap()
            .contains(r#""safeStartStop": false"#)
    );
    assert!(
        fs::symlink_metadata(workspace.app_data())
            .unwrap()
            .is_symlink()
    );
    assert!(
        fs::symlink_metadata(data.join("account-settings"))
            .unwrap()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn app_data_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let workspace = Workspace::new();
    let host = workspace.host();
    wait(host.accounts().edit(create("Hero"))).unwrap();
    wait(
        host.accounts()
            .update_settings("Hero", ScriptOptionsPatch::default()),
    )
    .unwrap();
    assert_eq!(mode(workspace.app_data()), 0o700);
    assert_eq!(mode(workspace.path("accounts.json")), 0o600);
    assert_eq!(mode(workspace.path("account-settings")), 0o700);
    assert_eq!(mode(workspace.path("account-settings/hero.json")), 0o600);

    let workspace = Workspace::new();
    workspace.write("accounts.json", r#"{"accounts":[],"groups":{}}"#);
    set_mode(&workspace.path("accounts.json"), 0o644);
    let host = workspace.host();
    wait(host.accounts().edit(create("Hero"))).unwrap();
    assert_eq!(mode(workspace.path("accounts.json")), 0o600);

    let workspace = Workspace::new();
    fs::create_dir_all(workspace.path("account-settings")).unwrap();
    set_mode(&workspace.app_data(), 0o755);
    set_mode(&workspace.path("account-settings"), 0o755);
    let host = workspace.host();
    wait(host.accounts().edit(create("Hero"))).unwrap();
    assert_eq!(mode(workspace.app_data()), 0o700);
    wait(
        host.accounts()
            .update_settings("Hero", ScriptOptionsPatch::default()),
    )
    .unwrap();
    assert_eq!(mode(workspace.path("account-settings")), 0o700);
}

fn settings_file(scripts: Value) -> String {
    file(json!({"version": 1, "scripts": scripts}))
}

fn options(restart: bool, room_policy: RoomPolicy, safe: bool) -> ScriptOptions {
    ScriptOptions {
        restart_after_reconnect: restart,
        room_policy,
        safe_start_stop: safe,
    }
}

fn safe_start_stop(value: bool) -> ScriptOptionsPatch {
    ScriptOptionsPatch {
        safe_start_stop: Some(value),
        ..Default::default()
    }
}

#[test]
fn uses_a_lowercase_username_filename_and_only_writes_after_an_edit() {
    let workspace = Workspace::new();
    let host = workspace.host();
    assert_eq!(
        wait(host.accounts().settings("Hero Name")).unwrap(),
        ScriptOptions::default()
    );
    assert!(!workspace.path("account-settings/hero name.json").exists());
    assert_eq!(
        wait(
            host.accounts()
                .update_settings("  Hero Name  ", safe_start_stop(false))
        )
        .unwrap(),
        options(false, RoomPolicy::RandomPrivate, false)
    );
    assert_eq!(
        workspace.read("account-settings/hero name.json"),
        settings_file(json!({
            "restartAfterReconnect": false,
            "roomPolicy": {"kind": "random-private"},
            "safeStartStop": false,
        }))
    );
}

#[test]
fn normalizes_fields_independently_and_preserves_siblings_on_update() {
    let workspace = Workspace::new();
    let host = workspace.host();
    workspace.write(
        "account-settings/hero.json",
        r#"{"scripts":{"restartAfterReconnect":"invalid","roomPolicy":{"kind":"specific","roomNumber":42},"safeStartStop":false}}"#,
    );
    let specific = RoomPolicy::Specific { room_number: 42 };
    assert_eq!(
        wait(host.accounts().settings("hero")).unwrap(),
        options(false, specific.clone(), false)
    );
    let updated = wait(host.accounts().update_settings(
        "HERO",
        ScriptOptionsPatch {
            restart_after_reconnect: Some(true),
            ..Default::default()
        },
    ))
    .unwrap();
    assert_eq!(updated, options(true, specific, false));
    assert_eq!(
        workspace.read("account-settings/hero.json"),
        settings_file(json!({
            "restartAfterReconnect": true,
            "roomPolicy": {"kind": "specific", "roomNumber": 42},
            "safeStartStop": false,
        }))
    );
}

#[test]
fn leaves_malformed_json_untouched_until_an_edit_heals_it() {
    let workspace = Workspace::new();
    let host = workspace.host();
    workspace.write("account-settings/hero.json", "{not-json");
    assert_eq!(
        wait(host.accounts().settings("Hero")).unwrap(),
        ScriptOptions::default()
    );
    assert_eq!(workspace.read("account-settings/hero.json"), "{not-json");
    let healed = wait(
        host.accounts()
            .update_settings("Hero", safe_start_stop(false)),
    )
    .unwrap();
    assert!(!healed.safe_start_stop);
    assert_eq!(
        workspace.read("account-settings/hero.json"),
        settings_file(json!({
            "restartAfterReconnect": false,
            "roomPolicy": {"kind": "random-private"},
            "safeStartStop": false,
        }))
    );
}

#[test]
fn warns_once_about_reset_fields_without_account_names_and_keeps_valid_siblings() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = r#"{"scripts":{"restartAfterReconnect":"invalid","roomPolicy":{"kind":"specific","roomNumber":42},"safeStartStop":false}}"#;
    workspace.write("account-settings/private hero.json", source);
    let specific = RoomPolicy::Specific { room_number: 42 };
    assert_eq!(
        wait(host.accounts().settings("Private Hero")).unwrap(),
        options(false, specific.clone(), false)
    );
    assert_eq!(
        capture.take(),
        [settings_warning(
            INVALID_SETTINGS,
            json!({"reset": r#"["scripts.restartAfterReconnect"]"#})
        )]
    );
    assert_eq!(workspace.read("account-settings/private hero.json"), source);
    let saved = wait(host.accounts().update_settings(
        "Private Hero",
        ScriptOptionsPatch {
            restart_after_reconnect: Some(true),
            ..Default::default()
        },
    ))
    .unwrap();
    assert_eq!(saved, options(true, specific, false));
    assert_eq!(capture.take(), Vec::<Value>::new());
    assert_eq!(
        workspace.read("account-settings/private hero.json"),
        settings_file(json!({
            "restartAfterReconnect": true,
            "roomPolicy": {"kind": "specific", "roomNumber": 42},
            "safeStartStop": false,
        }))
    );
}

#[test]
fn uses_defaults_and_a_private_path_without_overwriting_unreadable_settings() {
    for (source, position) in [
        ("{not-json", json!({"line": 1, "column": 2})),
        ("[]", json!({})),
        ("null", json!({})),
        ("42", json!({})),
    ] {
        let (capture, _guard) = Capture::start();
        let workspace = Workspace::new();
        let host = workspace.host();
        workspace.write("account-settings/private hero.json", source);
        assert_eq!(
            wait(host.accounts().settings("Private Hero")).unwrap(),
            ScriptOptions::default()
        );
        let mut fields = json!({"ignored": r#"{"file":1}"#});
        fields
            .as_object_mut()
            .unwrap()
            .extend(position.as_object().unwrap().clone());
        assert_eq!(
            capture.take(),
            [settings_warning(UNREADABLE_SETTINGS, fields)],
            "{source}"
        );
        assert_eq!(workspace.read("account-settings/private hero.json"), source);
        assert_eq!(
            wait(
                host.accounts()
                    .update_settings("Private Hero", safe_start_stop(false))
            )
            .unwrap(),
            options(false, RoomPolicy::RandomPrivate, false)
        );
        assert_eq!(capture.take(), Vec::<Value>::new());
        assert_eq!(
            workspace.read("account-settings/private hero.json"),
            settings_file(json!({
                "restartAfterReconnect": false,
                "roomPolicy": {"kind": "random-private"},
                "safeStartStop": false,
            }))
        );
    }
}

#[test]
fn fails_unreadable_settings_without_overwriting_them_on_update() {
    let source = r#"{"version":1,"scripts":{"restartAfterReconnect":true,"roomPolicy":{"kind":"specific","roomNumber":42},"safeStartStop":false}}
"#;
    for (name, denied) in [("a directory", false), ("permission denied", true)] {
        if denied && !permissions_apply() {
            continue;
        }
        let (capture, _guard) = Capture::start();
        let workspace = Workspace::new();
        let host = workspace.host();
        let writes = count_writes(&host);
        let path = workspace.path("account-settings/hero.json");
        if denied {
            workspace.write("account-settings/hero.json", source);
            set_mode(&path, 0o000);
        } else {
            fs::create_dir_all(&path).unwrap();
        }
        let failure = read_failure(&workspace, "account-settings/hero.json", false);
        assert_eq!(
            file_failure(wait(host.accounts().settings("Hero"))),
            failure,
            "{name}"
        );
        assert_eq!(
            file_failure(wait(
                host.accounts()
                    .update_settings("Hero", safe_start_stop(true))
            )),
            failure,
            "{name}"
        );
        assert_eq!(writes.load(Ordering::SeqCst), 0, "{name}");
        assert_eq!(capture.take(), Vec::<Value>::new(), "{name}");
        if denied {
            set_mode(&path, 0o600);
        } else {
            assert!(path.is_dir());
            fs::remove_dir(&path).unwrap();
            workspace.write("account-settings/hero.json", source);
        }
        assert_eq!(workspace.read("account-settings/hero.json"), source);
        assert_eq!(
            wait(host.accounts().settings("Hero")).unwrap(),
            options(true, RoomPolicy::Specific { room_number: 42 }, false),
            "{name}"
        );
    }
}

#[test]
fn loads_missing_files_and_valid_optional_fields_silently() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    assert_eq!(
        wait(host.accounts().settings("Hero")).unwrap(),
        ScriptOptions::default()
    );
    assert_eq!(capture.take(), Vec::<Value>::new());
    workspace.write(
        "account-settings/hero.json",
        r#"{"scripts":{"safeStartStop":false}}"#,
    );
    assert_eq!(
        wait(host.accounts().settings("Hero")).unwrap(),
        options(false, RoomPolicy::RandomPrivate, false)
    );
    assert_eq!(capture.take(), Vec::<Value>::new());
}

#[test]
fn reports_only_fixed_field_paths_when_nested_account_settings_are_invalid() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = r#"{"version":"wrong","scripts":{"roomPolicy":{"kind":"specific","roomNumber":0},"safeStartStop":null}}"#;
    workspace.write("account-settings/private hero.json", source);
    assert_eq!(
        wait(host.accounts().settings("Private Hero")).unwrap(),
        ScriptOptions::default()
    );
    assert_eq!(
        capture.take(),
        [settings_warning(
            INVALID_SETTINGS,
            json!({"reset": r#"["version", "scripts.roomPolicy", "scripts.safeStartStop"]"#})
        )]
    );
    assert_eq!(workspace.read("account-settings/private hero.json"), source);
}

#[test]
fn reads_newer_account_settings_silently_and_writes_the_current_version_on_update() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = r#"{"version":2,"scripts":{"safeStartStop":false},"futureOnly":"retain"}"#;
    workspace.write("account-settings/hero.json", source);
    assert_eq!(
        wait(host.accounts().settings("Hero")).unwrap(),
        options(false, RoomPolicy::RandomPrivate, false)
    );
    assert_eq!(workspace.read("account-settings/hero.json"), source);
    assert_eq!(
        wait(
            host.accounts()
                .update_settings("Hero", safe_start_stop(true))
        )
        .unwrap(),
        ScriptOptions::default()
    );
    assert_eq!(capture.take(), Vec::<Value>::new());
    assert_eq!(
        workspace.read("account-settings/hero.json"),
        settings_file(json!({
            "restartAfterReconnect": false,
            "roomPolicy": {"kind": "random-private"},
            "safeStartStop": true,
        }))
    );
}

#[test]
fn settings_reject_invalid_usernames() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let writes = count_writes(&host);
    for username in ["", " \u{feff}", ".", " .. ", "a/b", "a\\b", "a\0b"] {
        let error = wait(host.accounts().settings(username)).unwrap_err();
        assert_eq!(error, AccountsError::InvalidUsername, "{username:?}");
        assert_eq!(error.to_string(), "Invalid account username.");
        assert_eq!(
            wait(
                host.accounts()
                    .update_settings(username, safe_start_stop(false))
            ),
            Err(AccountsError::InvalidUsername),
            "{username:?}"
        );
    }
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read_dir(workspace.app_data()).unwrap().count(), 0);
}

#[test]
fn settings_reads_never_write_and_updates_always_write() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let writes = count_writes(&host);
    workspace.write("account-settings/hero.json", "{not-json");
    for _ in 0..2 {
        wait(host.accounts().settings("Hero")).unwrap();
    }
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(workspace.read("account-settings/hero.json"), "{not-json");
    for expected in 1..=2 {
        assert_eq!(
            wait(
                host.accounts()
                    .update_settings("Hero", ScriptOptionsPatch::default())
            )
            .unwrap(),
            ScriptOptions::default()
        );
        assert_eq!(writes.load(Ordering::SeqCst), expected);
    }
    assert_eq!(
        workspace.read("account-settings/hero.json"),
        settings_file(json!({
            "restartAfterReconnect": false,
            "roomPolicy": {"kind": "random-private"},
            "safeStartStop": true,
        }))
    );
}

#[test]
fn settings_updates_keep_room_numbers_in_range() {
    let workspace = Workspace::new();
    let host = workspace.host();
    for (room_number, expected) in [
        (
            99_999,
            RoomPolicy::Specific {
                room_number: 99_999,
            },
        ),
        (0, RoomPolicy::RandomPrivate),
        (100_000, RoomPolicy::RandomPrivate),
    ] {
        let patch = ScriptOptionsPatch {
            room_policy: Some(RoomPolicy::Specific { room_number }),
            ..Default::default()
        };
        assert_eq!(
            wait(host.accounts().update_settings("Hero", patch))
                .unwrap()
                .room_policy,
            expected
        );
    }
}

#[test]
fn account_renames_and_deletes_leave_settings_files_alone() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let accounts = host.accounts();
    wait(accounts.edit(create("Alice"))).unwrap();
    wait(accounts.update_settings("Alice", safe_start_stop(false))).unwrap();
    let saved = workspace.read("account-settings/alice.json");
    wait(accounts.edit(AccountEdit::UpdateAccount {
        username: "Alice".into(),
        label: None,
        new_username: Some("Alicia".into()),
        password: None,
    }))
    .unwrap();
    wait(accounts.edit(AccountEdit::DeleteAccounts {
        usernames: vec!["Alicia".into()],
    }))
    .unwrap();
    assert_eq!(workspace.read("account-settings/alice.json"), saved);
    assert!(!workspace.path("account-settings/alicia.json").exists());
    assert_eq!(
        wait(accounts.settings("ALICE")).unwrap(),
        options(false, RoomPolicy::RandomPrivate, false)
    );
}
