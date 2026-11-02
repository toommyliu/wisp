use super::*;
use crate::automation::test_logs::Logs;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    sync::atomic::{AtomicBool, Ordering},
};

struct Host {
    username: RefCell<String>,
    password: RefCell<String>,
    ready: Cell<bool>,
    logged_in: Cell<bool>,
    select: Cell<bool>,
    kicked: Cell<bool>,
    failures: RefCell<Vec<(&'static str, &'static str)>>,
    calls: RefCell<Vec<String>>,
    connections: RefCell<Vec<String>>,
    held: RefCell<Vec<crate::bridge::Reply<String>>>,
    selection: RefCell<serde_json::Value>,
    connect_ready: Cell<bool>,
    manual_connection: Cell<bool>,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            username: RefCell::new("Hero".into()),
            password: RefCell::new("pw".into()),
            ready: Cell::new(false),
            logged_in: Cell::new(false),
            select: Cell::new(false),
            kicked: Cell::new(false),
            failures: RefCell::default(),
            calls: RefCell::default(),
            connections: RefCell::default(),
            held: RefCell::default(),
            selection: RefCell::new(json!({"ok":true,"status":"selected","message":"Connected"})),
            connect_ready: Cell::new(true),
            manual_connection: Cell::new(false),
        }
    }
}
fn reply<T>(value: Result<T, Error>) -> Pending<T> {
    let (reply, pending) = Pending::channel();
    reply.send(value).unwrap();
    pending
}
impl Host {
    fn failure(&self, key: &str) -> Option<&'static str> {
        self.failures
            .borrow()
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    }
    fn count(&self, key: &str) -> usize {
        self.calls
            .borrow()
            .iter()
            .filter(|s| s.as_str() == key)
            .count()
    }
}
impl ReloginHost for Host {
    fn snapshot(&self) -> Pending<Snapshot> {
        if self.failure("snapshot") == Some("error") {
            return reply(Err(Error::ReadPanicked));
        }
        reply(Ok(Snapshot {
            username: self.username.borrow().clone(),
            password: self.password.borrow().clone(),
            connected: Some(self.logged_in.get()),
            logged_in: self.logged_in.get(),
            ready: self.ready.get() && self.logged_in.get(),
            servers: vec!["Artix".into()],
        }))
    }
    fn command(&self, command: Command) -> Pending<String> {
        let (key, output) = match command {
            Command::RefreshCredentials {} => (
                "capture",
                json!({"username":*self.username.borrow(),"password":*self.password.borrow()}),
            ),
            Command::RefreshServers {} => ("servers", json!([{ "name":"Artix" }])),
            Command::Login { username, password } => {
                assert_eq!((username.as_str(), password.as_str()), ("Hero", "pw"));
                if self.failure("login").is_none() {
                    self.select.set(true);
                }
                ("login", json!(true))
            }
            Command::Logout {} => {
                if self.failure("logout").is_none() {
                    self.select.set(false);
                }
                self.logged_in.set(false);
                ("logout", json!(null))
            }
            _ => panic!("unexpected command"),
        };
        self.calls.borrow_mut().push(key.into());
        match self.failure(key) {
            Some("hold") => {
                let (send, pending) = Pending::channel();
                self.held.borrow_mut().push(send);
                pending
            }
            Some("error") => reply(Err(Error::ReadPanicked)),
            Some("panic") => panic!("broken native boundary"),
            Some(json) => reply(Ok(json.into())),
            None => reply(Ok(output.to_string())),
        }
    }
    fn call(&self, method: &'static str, args: Vec<Value>) -> Pending<Value> {
        self.calls.borrow_mut().push(method.into());
        if self.failure(method) == Some("error") {
            return reply(Err(Error::ReadPanicked));
        }
        reply(Ok(match method {
            "flash.isNull" => Value::Bool(!self.select.get()),
            "auth.isTemporarilyKicked" => Value::Bool(self.kicked.get()),
            "flash.getGameObject" => Value::String("1".into()),
            "auth.connectTo" => {
                if let Some(Value::String(server)) = args.first() {
                    self.connections.borrow_mut().push(server.clone());
                }
                assert!(
                    matches!(args.first(),Some(Value::String(server)) if server == "Artix" || server == "Yulgar" || server == "Upgrade")
                );
                let selected = self.selection.borrow().clone();
                if self.manual_connection.get() {
                    self.logged_in.set(true);
                }
                if selected["ok"] == true {
                    self.logged_in.set(true);
                    self.ready.set(self.connect_ready.get());
                }
                Value::String(selected.to_string())
            }
            _ => panic!("unexpected call {method}"),
        }))
    }
}
struct Harness {
    host: Host,
    shared: Arc<Shared>,
    policy: Policy,
    now: Instant,
}
impl Default for Harness {
    fn default() -> Self {
        Self {
            host: Host::default(),
            shared: Arc::default(),
            policy: Policy::default(),
            now: Instant::now(),
        }
    }
}
impl Harness {
    fn tick(&mut self) {
        for _ in 0..8 {
            self.policy
                .advance(&self.host, &self.shared, Waker::noop(), self.now);
        }
    }
    fn after(&mut self, duration: Duration) {
        self.now += duration;
        self.tick();
    }
    fn set(&mut self, edit: Edit) -> Result<serde_json::Value, Error> {
        let pending = self
            .shared
            .submit(edit, Arc::new(AtomicBool::new(false)), self.now);
        self.tick();
        pending
            .recv_timeout(Duration::ZERO)
            .map(|s| serde_json::from_str(&s).unwrap())
    }
    fn enabled(&mut self) {
        self.set(Edit::Server(Some("  artix  ".into()))).unwrap();
        self.set(Edit::Delay(0.0)).unwrap();
        assert_eq!(
            self.set(Edit::Enabled(true)).unwrap()["status"]["username"],
            "Hero"
        );
    }
    fn lost(&mut self) {
        self.host.ready.set(false);
        self.host.logged_in.set(false);
        self.policy.connection("OnConnectionLost", self.now);
        self.tick();
    }
}

#[test]
fn setters_capture_then_publish_sanitized_records_before_answering() {
    let mut h = Harness::default();
    h.enabled();
    assert_eq!(*h.shared.records.read(), h.policy.records);
    let records = h.set(Edit::Enabled(true)).unwrap();
    assert_eq!(records["status"]["captured"], true);
    assert_eq!(records["config"]["server"], "Artix");
    assert!(!records.to_string().contains("password"));
    assert!(!records.to_string().contains("pw"));
    assert_eq!(h.host.count("capture"), 1);
    h.set(Edit::Enabled(false)).unwrap();
    assert!(h.shared.credentials.lock().is_none());
    assert_eq!(h.policy.records.status.username, None);
}
#[test]
fn delay_inputs_are_bounded_and_nonfinite_values_use_the_default() {
    let mut h = Harness::default();
    for (input, expected) in [
        (-1.0, 0.0),
        (300_001.0, 300_000.0),
        (1.6, 1.6),
        (f64::INFINITY, 3000.0),
        (f64::NAN, 3000.0),
    ] {
        assert_eq!(
            h.set(Edit::Delay(input)).unwrap()["config"]["delayMs"],
            expected
        );
    }
}
#[test]
fn capture_errors_and_malformed_replies_use_cached_credentials_and_allow_later_setters() {
    for fault in ["error", "{"] {
        let mut h = Harness::default();
        h.host.failures.borrow_mut().push(("capture", fault));
        h.enabled();
        assert_eq!(
            h.set(Edit::Enabled(false)).unwrap()["status"]["captured"],
            false
        );
        h.host.failures.borrow_mut().clear();
        assert_eq!(
            h.set(Edit::Enabled(true)).unwrap()["status"]["captured"],
            true
        );
    }
}
#[test]
fn missing_credentials_are_captured_later_without_logging_in() {
    let mut h = Harness::default();
    h.host.username.replace(String::new());
    h.host.password.replace(String::new());
    assert_eq!(
        h.set(Edit::Enabled(true)).unwrap()["status"]["captured"],
        false
    );
    h.host.username.replace("Hero".into());
    h.host.password.replace("pw".into());
    h.after(Duration::from_secs(5));
    assert!(h.policy.records.status.captured);
    assert_eq!(h.host.count("login"), 0);
}
#[test]
fn delay_edits_preserve_elapsed_disconnect_time_and_server_edits_preserve_recovery() {
    let mut h = Harness::default();
    h.enabled();
    h.set(Edit::Delay(10_000.0)).unwrap();
    h.lost();
    h.after(Duration::from_secs(2));
    h.set(Edit::Server(Some("Yulgar".into()))).unwrap();
    h.set(Edit::Delay(3000.0)).unwrap();
    assert_eq!(h.policy.records.status.phase, Phase::WaitingDelay);
    assert_eq!(h.host.count("login"), 0);
    h.after(Duration::from_secs(1));
    assert_eq!(h.host.count("login"), 1);
    h.after(Duration::from_millis(500));
    h.after(Duration::from_millis(250));
    h.after(POLL);
    assert_eq!(h.policy.records.status.phase, Phase::Armed);
    assert_eq!(h.host.count("auth.connectTo"), 1);
    assert_eq!(*h.host.connections.borrow(), vec!["Yulgar"]);
}
#[test]
fn recovery_logs_in_without_a_server_and_returns_to_armed_at_server_selection() {
    let mut h = Harness::default();
    h.set(Edit::Enabled(true)).unwrap();
    h.set(Edit::Delay(0.0)).unwrap();
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    assert_eq!(h.host.count("login"), 1);
    assert_eq!(h.host.count("auth.connectTo"), 0);
    assert_eq!(h.policy.records.status.phase, Phase::Armed);
}
#[test]
fn manual_readiness_cancels_delayed_recovery_and_keeps_the_captured_account() {
    for event in [true, false] {
        let mut h = Harness::default();
        h.enabled();
        h.set(Edit::Delay(10_000.0)).unwrap();
        h.lost();
        h.host.username.replace("Other".into());
        h.host.ready.set(true);
        h.host.logged_in.set(true);
        if event {
            h.policy.connection("OnConnection", h.now);
        }
        h.after(POLL);
        assert_eq!(h.policy.records.status.phase, Phase::Armed);
        assert_eq!(h.host.count("login"), 0);
        assert_eq!(h.policy.records.status.username.as_deref(), Some("Hero"));
    }
}
#[test]
fn login_failures_back_off_for_three_retries_then_remain_failed() {
    let mut h = Harness::default();
    h.enabled();
    h.host.failures.borrow_mut().push(("login", "error"));
    h.lost();
    assert_eq!(
        h.policy.records.status.phase,
        Phase::Backoff { delay_ms: 5000.0 }
    );
    assert_eq!(h.policy.records.status.attempts_remaining, Some(3.0));
    for (attempt, delay) in [5, 10, 20].into_iter().enumerate() {
        assert_eq!(
            h.policy.records.status.phase,
            Phase::Backoff {
                delay_ms: delay as f64 * 1000.0
            }
        );
        h.after(Duration::from_millis(delay * 1000 - 1));
        assert_eq!(h.host.count("login"), attempt + 1);
        h.after(Duration::from_millis(1));
        assert_eq!(h.host.count("login"), attempt + 2);
    }
    assert_eq!(h.host.count("login"), 4);
    assert_eq!(h.policy.records.status.phase, Phase::Failed);
    assert_eq!(h.policy.records.status.attempts_remaining, Some(0.0));
    assert_eq!(
        h.policy.records.status.last_error.as_deref(),
        Some("Login could not be submitted.")
    );
    h.policy.connection("OnConnectionLost", h.now);
    h.set(Edit::Delay(1.0)).unwrap();
    h.after(Duration::from_secs(60));
    assert_eq!(h.host.count("login"), 4);
}
#[test]
fn full_servers_refresh_login_between_retries_and_logout_failures_back_off() {
    for logout_failure in [false, true] {
        let mut h = Harness::default();
        h.enabled();
        h.host.selection.replace(
            json!({"ok":false,"status":"blocked","reason":"full","message":"Server is full"}),
        );
        if logout_failure {
            h.host.failures.borrow_mut().push(("logout", "error"));
        }
        h.lost();
        for attempt in 0..4 {
            h.after(POLL);
            h.after(Duration::from_millis(250));
            assert_eq!(h.host.count("auth.connectTo"), attempt + 1);
            assert_eq!(h.host.count("logout"), attempt + 1);
            assert_eq!(
                h.policy.records.status.last_error.as_deref(),
                Some(if logout_failure {
                    "Logout could not be submitted."
                } else {
                    "Server is full."
                })
            );
            if attempt < 3 {
                h.after(Duration::from_secs(5 << attempt));
            }
        }
        assert_eq!(h.host.count("login"), 4);
        assert_eq!(h.policy.records.status.phase, Phase::Failed);
        let actions: Vec<_> = h
            .host
            .calls
            .borrow()
            .iter()
            .filter(|s| matches!(s.as_str(), "login" | "auth.connectTo" | "logout"))
            .cloned()
            .collect();
        assert_eq!(
            actions,
            vec![
                "login",
                "auth.connectTo",
                "logout",
                "login",
                "auth.connectTo",
                "logout",
                "login",
                "auth.connectTo",
                "logout",
                "login",
                "auth.connectTo",
                "logout"
            ]
        );
    }
}
#[test]
fn account_restrictions_are_terminal_and_a_changed_server_retries_immediately() {
    let mut h = Harness::default();
    h.enabled();
    h.host.selection.replace(json!({"ok":false,"status":"blocked","message":"account is not authorized for member-only servers"}));
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    assert_eq!(h.policy.records.status.phase, Phase::Failed);
    assert_eq!(h.policy.records.status.attempts_remaining, None);
    assert_eq!(
        h.policy.records.status.last_error.as_deref(),
        Some("Account is not authorized for member-only servers.")
    );
    h.after(Duration::from_secs(60));
    assert_eq!(h.host.count("auth.connectTo"), 1);
    h.set(Edit::Server(Some("Artix".into()))).unwrap();
    assert_eq!(h.policy.records.status.phase, Phase::Failed);
    h.host
        .selection
        .replace(json!({"ok":true,"status":"selected","message":"Connected"}));
    assert_eq!(
        h.set(Edit::Server(Some("Yulgar".into()))).unwrap()["status"]["phase"]["tag"],
        "waiting-delay"
    );
    h.after(Duration::from_millis(250));
    h.after(POLL);
    assert_eq!(h.policy.records.status.phase, Phase::Armed);
}
#[test]
fn connected_but_unready_players_log_out_before_retrying() {
    let mut h = Harness::default();
    h.enabled();
    h.host.connect_ready.set(false);
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    assert_eq!(h.policy.records.status.phase, Phase::AwaitingPlayerReady);
    for attempt in 0..4 {
        h.after(Duration::from_secs(10));
        assert_eq!(h.host.count("logout"), attempt + 1);
        assert_eq!(
            h.policy.records.status.last_error.as_deref(),
            Some("Player did not become ready.")
        );
        if attempt < 3 {
            assert_eq!(
                h.policy.records.status.phase,
                Phase::Backoff {
                    delay_ms: (5 << attempt) as f64 * 1000.0
                }
            );
            h.after(Duration::from_secs(5 << attempt));
            h.after(POLL);
            h.after(Duration::from_millis(250));
        }
    }
    assert_eq!(h.policy.records.status.phase, Phase::Failed);
    assert_eq!(h.policy.records.status.attempts_remaining, Some(0.0));
    assert_eq!(h.host.count("login"), 4);
}
#[test]
fn temporary_kicks_wait_until_clear_and_manual_readiness_wins() {
    for manual in [false, true] {
        let mut h = Harness::default();
        h.enabled();
        h.host.kicked.set(true);
        h.lost();
        assert_eq!(h.policy.records.status.phase, Phase::WaitingKick);
        assert_eq!(h.host.count("login"), 0);
        if manual {
            h.host.ready.set(true);
            h.host.logged_in.set(true);
            h.policy.connection("OnConnection", h.now);
            h.after(POLL);
            assert_eq!(h.policy.records.status.phase, Phase::Armed);
        } else {
            h.host.kicked.set(false);
            h.after(Duration::from_secs(2));
            assert_eq!(h.host.count("login"), 1);
        }
    }
}
#[test]
fn repeated_connection_failures_do_not_restart_a_running_recovery() {
    let mut h = Harness::default();
    h.enabled();
    h.host.failures.borrow_mut().push(("login", "error"));
    h.lost();
    h.lost();
    assert_eq!(h.host.count("login"), 1);
    h.after(Duration::from_secs(5));
    assert_eq!(h.host.count("login"), 2);
}
#[test]
fn successful_connections_do_not_cancel_recovery() {
    for connected in [true, false] {
        let mut h = Harness::default();
        h.enabled();
        h.lost();
        h.host.logged_in.set(connected);
        h.after(POLL);
        if !connected {
            h.after(Duration::from_millis(250));
        }
        h.host.ready.set(true);
        h.host.logged_in.set(true);
        h.policy.connection("OnConnection", h.now);
        h.after(POLL);
        assert_eq!(h.policy.records.status.phase, Phase::Armed);
        assert_eq!(h.host.count("login"), 1);
        assert_eq!(h.host.count("logout"), 0);
        assert_eq!(h.host.count("auth.connectTo"), usize::from(!connected));
    }
}
#[test]
fn a_hung_login_is_bounded() {
    let mut h = Harness::default();
    h.enabled();
    h.host.failures.borrow_mut().push(("login", "hold"));
    h.lost();
    assert_eq!(h.policy.records.status.phase, Phase::LoggingIn);
    h.after(REPLY_TIMEOUT);
    assert_eq!(
        h.policy.records.status.phase,
        Phase::Backoff { delay_ms: 5000.0 }
    );
}
#[test]
fn cancelled_and_expired_setters_are_skipped_and_a_failed_setter_frees_the_queue() {
    let mut h = Harness::default();
    let dead = Arc::new(AtomicBool::new(false));
    let first = h.shared.submit(Edit::Delay(1.0), dead.clone(), h.now);
    dead.store(true, Ordering::Release);
    let expired = h.shared.submit(Edit::Delay(2.0), Arc::default(), h.now);
    h.now += Duration::from_secs(6);
    let next = h.shared.submit(Edit::Delay(7000.0), Arc::default(), h.now);
    h.tick();
    assert_eq!(first.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    assert_eq!(expired.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&next.recv_timeout(Duration::ZERO).unwrap())
            .unwrap()["config"]["delayMs"],
        7000.0
    );
}
#[test]
fn a_blocked_setter_counts_toward_capacity_and_late_answers_have_separate_receivers() {
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("capture", "hold"));
    let dead = Arc::new(AtomicBool::new(false));
    let old = h.shared.submit(Edit::Enabled(true), dead.clone(), h.now);
    h.tick();
    let requests: Vec<_> = (0..1023)
        .map(|_| h.shared.submit(Edit::Delay(1.0), Arc::default(), h.now))
        .collect();
    let overflow = h.shared.submit(Edit::Delay(2.0), Arc::default(), h.now);
    assert_eq!(overflow.recv_timeout(Duration::ZERO), Err(Error::Overflow));
    dead.store(true, Ordering::Release);
    drop(old);
    drop(requests);
    h.host
        .held
        .borrow_mut()
        .pop()
        .unwrap()
        .send(Ok("{\"username\":\"Hero\",\"password\":\"pw\"}".into()))
        .unwrap();
    for _ in 0..20 {
        h.tick();
    }
    assert_eq!(
        h.set(Edit::Delay(9000.0)).unwrap()["config"]["delayMs"],
        9000.0
    );
}

#[test]
fn a_boot_disable_wins_before_credential_capture() {
    let mut h = Harness::default();
    h.policy.records.config.enabled = true;
    assert_eq!(
        h.set(Edit::Enabled(false)).unwrap()["config"]["enabled"],
        false
    );
    h.after(Duration::from_secs(30));
    assert_eq!(h.host.count("capture"), 0);
}

#[test]
fn server_list_failures_keep_the_requested_server_and_later_setters_succeed() {
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("servers", "error"));
    assert_eq!(
        h.set(Edit::Server(Some("  Artix  ".into()))).unwrap()["config"]["server"],
        "Artix"
    );
    h.host.failures.borrow_mut().clear();
    assert_eq!(
        h.set(Edit::Server(Some(" artix ".into()))).unwrap()["config"]["server"],
        "Artix"
    );
}

#[test]
fn malformed_capture_records_fail_once_and_free_the_next_setter() {
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("capture", "{}"));
    assert!(matches!(
        h.set(Edit::Enabled(true)),
        Err(Error::CommandDecode { .. })
    ));
    h.host.failures.borrow_mut().clear();
    assert_eq!(
        h.set(Edit::Enabled(true)).unwrap()["status"]["captured"],
        true
    );
}
#[test]
fn native_boundary_panics_fail_the_caller_and_allow_the_next_setter() {
    let logs = Logs::default();
    let _guard = logs.enter();
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("servers", "panic"));
    assert!(h.set(Edit::Server(Some("Artix".into()))).is_err());
    assert!(
        logs.messages()
            .iter()
            .any(|s| s.contains("setter panicked: broken native boundary"))
    );
    h.host.failures.borrow_mut().clear();
    assert_eq!(
        h.set(Edit::Server(Some(" artix ".into()))).unwrap()["config"]["server"],
        "Artix"
    );
}

#[test]
fn a_disconnect_during_missing_capture_recovers_after_cached_credentials_arrive() {
    let mut h = Harness::default();
    h.host.username.replace(String::new());
    h.host.password.replace(String::new());
    h.host.failures.borrow_mut().push(("capture", "error"));
    h.set(Edit::Enabled(true)).unwrap();
    h.set(Edit::Delay(0.0)).unwrap();
    h.lost();
    h.host.username.replace("Hero".into());
    h.host.password.replace("pw".into());
    h.policy.connection("OnConnection", h.now);
    h.tick();
    assert!(h.policy.records.status.captured);
    assert_eq!(h.host.count("login"), 1);
}

#[test]
fn queued_setters_do_not_spin_while_capture_is_waiting() {
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("capture", "hold"));
    let _first = h.shared.submit(Edit::Enabled(true), Arc::default(), h.now);
    let _next = h.shared.submit(Edit::Delay(7000.0), Arc::default(), h.now);
    let deadline = h
        .policy
        .advance(&h.host, &h.shared, Waker::noop(), h.now)
        .unwrap();
    assert!(deadline > h.now);
}

#[test]
fn a_persistent_temporary_kick_is_bounded_before_retry() {
    let mut h = Harness::default();
    h.enabled();
    h.host.kicked.set(true);
    h.lost();
    h.after(Duration::from_secs(70));
    assert_eq!(h.host.count("login"), 0);
    assert_eq!(
        h.policy.records.status.phase,
        Phase::Backoff { delay_ms: 5000.0 }
    );
}

#[test]
fn failed_background_capture_logs_once_per_streak_and_recovers() {
    let logs = Logs::default();
    let _guard = logs.enter();
    let mut h = Harness::default();
    h.policy.records.config.enabled = true;
    h.host
        .failures
        .borrow_mut()
        .extend([("capture", "error"), ("snapshot", "error")]);
    h.tick();
    h.after(Duration::from_secs(5));
    assert_eq!(logs.count("capture failed"), 1);
    h.policy.connection("OnConnectionLost", h.now);
    h.policy.connection("OnConnection", h.now);
    h.tick();
    assert_eq!(logs.count("capture failed"), 2);
    assert!(
        logs.messages()
            .iter()
            .filter(|s| s.contains("capture failed"))
            .all(|s| s.contains("capture failed: the projection read panicked"))
    );
    h.host.failures.borrow_mut().clear();
    h.after(Duration::from_secs(5));
    assert!(h.policy.records.status.captured);
    assert_eq!(h.host.count("login"), 1);
}

#[test]
fn a_repeated_enable_returns_readiness_after_automation_acts() {
    let mut h = Harness::default();
    h.enabled();
    h.lost();
    h.host.ready.set(true);
    h.host.logged_in.set(true);
    let records = h.set(Edit::Enabled(true)).unwrap();
    assert_eq!(records["status"]["phase"]["tag"], "armed");
    assert_eq!(
        records["status"]["attemptsRemaining"],
        serde_json::Value::Null
    );
    assert_eq!(h.host.count("capture"), 1);
}

#[test]
fn a_recovery_panic_ends_the_attempt_without_refilling_retries() {
    let logs = Logs::default();
    let _guard = logs.enter();
    let mut h = Harness::default();
    h.enabled();
    h.host.failures.borrow_mut().push(("login", "panic"));
    h.lost();
    assert!(
        logs.messages()
            .iter()
            .any(|s| s.contains("transition panicked: broken native boundary"))
    );
    assert_eq!(h.policy.records.status.phase, Phase::Failed);
    assert_eq!(h.policy.records.status.attempts_remaining, None);
    assert_eq!(
        h.policy.records.status.last_error.as_deref(),
        Some("Auto relogin failed unexpectedly.")
    );
    h.lost();
    h.set(Edit::Delay(1000.0)).unwrap();
    h.after(Duration::from_secs(60));
    assert_eq!(h.host.count("login"), 1);
}

#[test]
fn replacing_a_player_rebinds_active_recovery_without_refilling_retries() {
    let mut h = Harness::default();
    h.enabled();
    h.host.failures.borrow_mut().push(("login", "error"));
    h.lost();
    h.after(Duration::from_secs(5));
    assert_eq!(h.policy.records.status.attempts_remaining, Some(2.0));
    let records = h.policy.records.clone();
    h.policy = Policy::for_player(records.clone(), h.now);
    assert_eq!(h.policy.records, records);
    h.after(Duration::from_secs(10));
    assert_eq!(h.policy.records.status.attempts_remaining, Some(1.0));
    assert_eq!(h.host.count("login"), 3);
}

#[test]
fn a_manual_connection_wins_over_a_failed_server_selection_before_player_readiness() {
    let mut h = Harness::default();
    h.enabled();
    h.host.manual_connection.set(true);
    h.host
        .selection
        .replace(json!({"ok":false,"status":"blocked","reason":"full","message":"Server is full"}));
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    assert_eq!(h.policy.records.status.phase, Phase::AwaitingPlayerReady);
    assert_eq!(h.host.count("logout"), 0);
    h.host.ready.set(true);
    h.policy.connection("OnConnection", h.now);
    h.tick();
    assert_eq!(h.policy.records.status.phase, Phase::Armed);
}

#[test]
fn a_capture_panic_allows_a_later_enable_to_arm_recovery() {
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("capture", "panic"));
    assert!(h.set(Edit::Enabled(true)).is_err());
    h.host.failures.borrow_mut().clear();
    assert_eq!(
        h.set(Edit::Enabled(true)).unwrap()["status"]["captured"],
        true
    );
    h.set(Edit::Delay(0.0)).unwrap();
    h.lost();
    assert_eq!(h.host.count("login"), 1);
}

#[test]
fn the_maximum_delay_waits_for_the_full_disconnect_interval() {
    let mut h = Harness::default();
    h.enabled();
    h.set(Edit::Delay(300_000.0)).unwrap();
    h.lost();
    h.after(Duration::from_millis(299_999));
    assert_eq!(h.host.count("login"), 0);
    h.after(Duration::from_millis(1));
    assert_eq!(h.host.count("login"), 1);
}

#[test]
fn a_changed_failed_server_returns_armed_if_the_player_is_already_ready() {
    let mut h = Harness::default();
    h.enabled();
    h.host.failures.borrow_mut().push(("login", "error"));
    h.lost();
    for delay in [5, 10, 20] {
        h.after(Duration::from_secs(delay));
    }
    assert_eq!(h.policy.records.status.phase, Phase::Failed);
    h.host.ready.set(true);
    h.host.logged_in.set(true);
    assert_eq!(
        h.set(Edit::Server(Some("Yulgar".into()))).unwrap()["status"]["phase"]["tag"],
        "armed"
    );
    assert_eq!(h.host.count("login"), 4);
}

#[test]
fn server_refresh_errors_preserve_the_cached_canonical_name() {
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("servers", "error"));
    assert_eq!(
        h.set(Edit::Server(Some(" aRtIx ".into()))).unwrap()["config"]["server"],
        "Artix"
    );
}

#[test]
fn a_timed_out_capture_finishes_automation_before_the_next_setter() {
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("capture", "hold"));
    let pending = h.shared.submit(Edit::Enabled(true), Arc::default(), h.now);
    h.tick();
    h.after(REPLY_TIMEOUT);
    assert_eq!(pending.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    assert!(h.policy.records.status.captured);
    assert_eq!(
        h.set(Edit::Delay(7000.0)).unwrap()["status"]["captured"],
        true
    );
}

#[test]
fn failed_commands_cannot_reuse_a_full_servers_stale_selection() {
    let mut h = Harness::default();
    h.enabled();
    h.host
        .selection
        .replace(json!({"ok":false,"status":"blocked","reason":"full","message":"Server is full"}));
    h.host.failures.borrow_mut().push(("logout", "error"));
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    assert_eq!(
        h.policy.records.status.phase,
        Phase::Backoff { delay_ms: 5000.0 }
    );
    h.host.failures.borrow_mut().push(("login", "error"));
    h.after(Duration::from_secs(5));
    assert_eq!(h.host.count("login"), 2);
    h.after(Duration::from_secs(10));
    assert_eq!(h.host.count("login"), 3);
    h.after(Duration::from_secs(20));
    assert_eq!(h.host.count("login"), 4);
    assert_eq!(h.host.count("auth.connectTo"), 1);
    assert_eq!(h.policy.records.status.phase, Phase::Failed);
}

#[test]
fn setter_wakes_observe_the_answered_records() {
    struct Check {
        shared: Arc<Shared>,
        seen: std::sync::Mutex<Vec<AutoReloginRecords>>,
    }
    impl std::task::Wake for Check {
        fn wake(self: Arc<Self>) {
            let records = self.shared.records.read().clone();
            self.seen.lock().unwrap().push(records);
        }
    }
    let mut h = Harness::default();
    for edit in [
        Edit::Delay(7000.0),
        Edit::Server(Some(" artix ".into())),
        Edit::Enabled(true),
        Edit::Enabled(false),
    ] {
        let pending = h
            .shared
            .submit(edit, Arc::new(AtomicBool::new(false)), h.now);
        let check = Arc::new(Check {
            shared: h.shared.clone(),
            seen: Default::default(),
        });
        pending.set_waker(Waker::from(check.clone()));
        h.tick();
        let answer: AutoReloginRecords =
            serde_json::from_str(&pending.recv_timeout(Duration::ZERO).unwrap()).unwrap();
        assert_eq!(check.seen.lock().unwrap().first(), Some(&answer));
    }
}

#[test]
fn a_connection_during_the_player_wait_keeps_the_retry() {
    let mut h = Harness::default();
    h.enabled();
    h.host.connect_ready.set(false);
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    assert_eq!(h.policy.records.status.phase, Phase::AwaitingPlayerReady);
    h.policy.connection("OnConnection", h.now);
    h.tick();
    h.after(Duration::from_secs(10));
    assert_eq!(h.host.count("logout"), 1);
    assert_eq!(
        h.policy.records.status.phase,
        Phase::Backoff { delay_ms: 5000.0 }
    );
}

#[test]
fn surviving_setters_apply_in_fifo_order() {
    let mut h = Harness::default();
    let requests: Vec<_> = [1000.0, 7000.0, 9000.0]
        .into_iter()
        .map(|delay| h.shared.submit(Edit::Delay(delay), Arc::default(), h.now))
        .collect();
    h.tick();
    for (request, expected) in requests.into_iter().zip([1000.0, 7000.0, 9000.0]) {
        let records: AutoReloginRecords =
            serde_json::from_str(&request.recv_timeout(Duration::ZERO).unwrap()).unwrap();
        assert_eq!(records.config.delay_ms, expected);
    }
    assert_eq!(h.shared.records.read().config.delay_ms, 9000.0);
}

#[test]
fn disabling_during_boot_capture_discards_the_late_credentials() {
    let mut h = Harness::default();
    h.policy.records.config.enabled = true;
    h.host.failures.borrow_mut().push(("capture", "hold"));
    h.tick();
    assert_eq!(h.host.count("capture"), 1);
    h.set(Edit::Enabled(false)).unwrap();
    let _ = h
        .host
        .held
        .borrow_mut()
        .pop()
        .unwrap()
        .send(Ok("{\"username\":\"Hero\",\"password\":\"pw\"}".into()));
    h.host.failures.borrow_mut().clear();
    h.after(REPLY_TIMEOUT);
    assert!(!h.shared.records.read().status.captured);
    assert!(h.shared.credentials.lock().is_none());
    assert_eq!(h.shared.records.read().status.username, None);
    h.host.username.replace(String::new());
    h.host.password.replace(String::new());
    h.set(Edit::Enabled(true)).unwrap();
    assert_eq!(h.host.count("capture"), 2);
    assert!(!h.policy.records.status.captured);
    assert!(h.shared.credentials.lock().is_none());
    h.host.username.replace("Hero".into());
    h.host.password.replace("pw".into());
    h.after(REPLY_TIMEOUT);
    assert!(h.policy.records.status.captured);
}

#[test]
fn snapshot_errors_prevent_arming_a_logged_in_player() {
    let mut h = Harness::default();
    h.enabled();
    h.host.failures.borrow_mut().push(("snapshot", "error"));
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    assert!(h.host.logged_in.get());
    assert!(h.host.ready.get());
    h.after(POLL);
    assert_eq!(h.policy.records.status.phase, Phase::AwaitingPlayerReady);
    h.after(Duration::from_secs(10));
    assert_eq!(h.host.count("logout"), 1);
    assert_eq!(
        h.policy.records.status.phase,
        Phase::Backoff { delay_ms: 5000.0 }
    );
}

#[test]
fn a_full_login_keeps_passwords_out_of_every_log_message() {
    let logs = Logs::default();
    let _guard = logs.enter();
    let mut h = Harness::default();
    h.enabled();
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    h.after(POLL);
    assert_eq!(h.host.count("login"), 1);
    assert_eq!(h.host.count("auth.connectTo"), 1);
    assert_eq!(h.policy.records.status.phase, Phase::Armed);
    assert!(
        logs.messages().iter().all(|s| !s.contains("pw")),
        "{:?}",
        logs.messages()
    );
}

#[test]
fn cached_credential_fallback_does_not_warn() {
    let logs = Logs::default();
    let _guard = logs.enter();
    let mut h = Harness::default();
    h.host.failures.borrow_mut().push(("capture", "error"));
    h.enabled();
    h.lost();
    h.after(POLL);
    h.after(Duration::from_millis(250));
    h.after(POLL);
    assert_eq!(h.policy.records.status.phase, Phase::Armed);
    assert!(
        logs.messages().iter().all(|s| !s.contains("auto relogin")),
        "{:?}",
        logs.messages()
    );
}

#[test]
fn unchanged_records_and_setters_do_not_wait_for_a_record_writer() {
    let host = crate::testing::headless_host();
    let (mut session, writer) =
        crate::testing::start_probe(&host, crate::testing::ProbeMovie::new());
    let mut policy = ReloginPolicy::new(
        session.attach(ReloginPolicy::INTERESTS).unwrap(),
        Waker::noop().clone(),
    );
    let shared_records = policy.link.records.auto_relogin.clone();
    let records = shared_records.read();
    let pending = writer.command(Command::SetAutoReloginDelay { delay_ms: 3000.0 });
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        policy.advance(Instant::now());
        send.send(()).unwrap();
    });
    let finished = receive.recv_timeout(REPLY_TIMEOUT);
    drop(records);
    worker.join().unwrap();
    finished.unwrap();
    let answer: AutoReloginRecords =
        serde_json::from_str(&pending.recv_timeout(Duration::ZERO).unwrap()).unwrap();
    assert_eq!(answer.config.delay_ms, 3000.0);
}

#[test]
fn immediate_recovery_uses_the_supplied_clock() {
    let mut h = Harness::default();
    h.enabled();
    h.now += Duration::from_secs(60);
    h.policy.recovery.state = RecoveryState::Attempt { resume: false };
    assert_eq!(h.policy.recovery_deadline(h.now), Some(h.now));
}

#[test]
fn pending_logouts_report_the_recovery_they_are_finishing() {
    for full in [true, false] {
        let mut h = Harness::default();
        h.enabled();
        h.host.failures.borrow_mut().push(("logout", "hold"));
        if full {
            h.host.selection.replace(
                json!({"ok":false,"status":"blocked","reason":"full","message":"Server is full"}),
            );
        } else {
            h.host.connect_ready.set(false);
        }
        h.lost();
        h.after(POLL);
        h.after(Duration::from_millis(250));
        if !full {
            h.after(Duration::from_secs(10));
        }
        assert_eq!(h.host.count("logout"), 1);
        assert_eq!(
            h.policy.records.status.phase,
            if full {
                Phase::SelectingServer
            } else {
                Phase::AwaitingPlayerReady
            }
        );
    }
}

#[test]
fn a_replacement_preserves_failure_while_credentials_are_pending() {
    let mut h = Harness::default();
    h.policy.records.config.enabled = true;
    h.host.failures.borrow_mut().push(("capture", "panic"));
    h.tick();
    assert_eq!(h.policy.records.status.phase, Phase::Failed);
    let records = h.policy.records.clone();
    h.host.failures.borrow_mut().clear();
    h.host.failures.borrow_mut().push(("capture", "hold"));
    h.policy = Policy::for_player(records.clone(), h.now);
    h.tick();
    assert_eq!(*h.shared.records.read(), records);
}

#[test]
fn relogin_events_ignore_unrelated_loaders_and_keep_connection_and_invalidation() {
    use crate::bridge::{Event, ScriptEvent};
    struct Counter(std::sync::atomic::AtomicUsize);
    impl std::task::Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let (sender, events) = crate::bridge::script_events(2, ReloginPolicy::INTERESTS);
    let count = Arc::new(Counter(std::sync::atomic::AtomicUsize::new(0)));
    events.set_waker(Waker::from(count.clone()));
    for _ in 0..20_000 {
        sender.send(ScriptEvent::Loader {
            event: Event {
                name: "onDebug".into(),
                args: vec![],
            },
            projected: vec![],
        });
    }
    assert_eq!(count.0.load(Ordering::Relaxed), 0);
    assert_eq!(events.try_recv(), Ok(None));
    let connection = ScriptEvent::Loader {
        event: Event {
            name: "onConnection".into(),
            args: vec![Value::String("OnConnectionLost".into())],
        },
        projected: vec![],
    };
    sender.send(connection.clone());
    sender.send(ScriptEvent::Invalidated);
    assert_eq!(count.0.load(Ordering::Relaxed), 2);
    assert_eq!(events.try_recv(), Ok(Some(connection)));
    assert_eq!(events.try_recv(), Ok(Some(ScriptEvent::Invalidated)));
    assert_eq!(events.try_recv(), Ok(None));
}
