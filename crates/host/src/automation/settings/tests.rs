use super::*;
use crate::automation::test_logs::Logs;
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    Full,
    Actions,
}
#[derive(Clone)]
enum Fault {
    Error,
    Panic,
    Json(String),
    Hold,
}
struct Host {
    ready: Cell<bool>,
    faults: RefCell<Vec<(Boundary, Fault)>>,
    applied: RefCell<Vec<bool>>,
    held: RefCell<Vec<crate::bridge::Reply<String>>>,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            ready: Cell::new(true),
            faults: RefCell::default(),
            applied: RefCell::default(),
            held: RefCell::default(),
        }
    }
}
impl Host {
    fn fault(&self, boundary: Boundary) -> Option<Fault> {
        self.faults
            .borrow()
            .iter()
            .find(|(at, _)| *at == boundary)
            .map(|(_, fault)| fault.clone())
    }
}
impl SettingsHost for Host {
    fn command(&self, command: Command) -> Pending<String> {
        let (boundary, value) = match command {
            Command::ReapplySettings { actions_only } => {
                self.applied.borrow_mut().push(actions_only);
                (
                    if actions_only {
                        Boundary::Actions
                    } else {
                        Boundary::Full
                    },
                    "null",
                )
            }
            _ => unreachable!(),
        };
        let (reply, pending) = Pending::channel();
        let result = match self.fault(boundary) {
            Some(Fault::Error) => Err(Error::Unavailable("temporary host failure".into())),
            Some(Fault::Panic) => panic!("broken native boundary"),
            Some(Fault::Json(json)) => Ok(json),
            Some(Fault::Hold) => {
                self.held.borrow_mut().push(reply);
                return pending;
            }
            _ => Ok(value.into()),
        };
        reply.send(result).unwrap();
        pending
    }
}

fn advance(settings: &mut Settings, host: &Host, now: Instant) -> Instant {
    settings
        .advance(host, host.ready.get(), Waker::noop(), now)
        .unwrap_or(now + Kind::Actions.interval())
}

fn retries(boundary: Boundary, fault: Fault) {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = Host::default();
    host.faults.borrow_mut().push((boundary, fault.clone()));
    let mut now = Instant::now();
    let mut settings = Settings::new(now);
    for _ in 0..7 {
        let next = advance(&mut settings, &host, now);
        assert!(
            next >= now + Kind::Actions.interval(),
            "retried after {:?}",
            next - now
        );
        now = next;
    }
    assert_eq!(logs.count("settings policy tick failed"), 1);
    assert_eq!(logs.count("settings policy recovered"), 0);
    let cause = logs.cause("settings policy tick failed: ");
    match (&fault, boundary) {
        (Fault::Error, _) => assert!(cause.contains("temporary host failure"), "{cause}"),
        (Fault::Panic, _) => assert!(cause.contains("broken native boundary"), "{cause}"),
        _ => assert!(!cause.is_empty()),
    }
    host.faults.borrow_mut().clear();
    for _ in 0..4 {
        now = advance(&mut settings, &host, now);
    }
    assert_eq!(logs.count("settings policy recovered"), 1);
    assert_eq!(logs.count("settings policy tick failed"), 1);
    assert!(host.applied.borrow().contains(&false));
    assert!(host.applied.borrow().contains(&true));
    host.faults.borrow_mut().push((boundary, Fault::Error));
    for _ in 0..3 {
        now = advance(&mut settings, &host, now);
    }
    assert_eq!(logs.count("settings policy tick failed"), 2);
}

#[test]
fn failed_settings_ticks_warn_and_retry() {
    for boundary in [Boundary::Full, Boundary::Actions] {
        retries(boundary, Fault::Error);
    }
}
#[test]
fn settings_policy_code_errors_warn_once_and_retry() {
    retries(Boundary::Full, Fault::Panic);
}
#[test]
fn settings_policy_malformed_host_replies_warn_once_and_retry() {
    retries(Boundary::Full, Fault::Json("not json".into()));
}
#[test]
fn settings_policy_invalid_reapply_reply_warns_once_and_retries() {
    for boundary in [Boundary::Full, Boundary::Actions] {
        for value in ["false", "{}"] {
            retries(boundary, Fault::Json(value.into()));
        }
    }
}
#[test]
fn settings_policy_uses_player_readiness() {
    let host = Host::default();
    let now = Instant::now();
    let mut settings = Settings::new(now);
    host.ready.set(false);
    assert_eq!(settings.advance(&host, false, Waker::noop(), now), None);
    assert!(host.applied.borrow().is_empty());
    assert!(settings.failing.is_empty());
    host.ready.set(true);
    advance(&mut settings, &host, now);
    assert_eq!(*host.applied.borrow(), [false, true]);
    host.ready.set(false);
    assert_eq!(settings.advance(&host, false, Waker::noop(), now), None);
    host.applied.borrow_mut().clear();
    advance(&mut settings, &host, now);
    assert!(host.applied.borrow().is_empty());
    host.ready.set(true);
    advance(&mut settings, &host, now);
    assert_eq!(*host.applied.borrow(), [false, true]);
}
#[test]
fn settings_policy_recovery_waits_for_every_failing_loop() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = Host::default();
    *host.faults.borrow_mut() = vec![
        (Boundary::Full, Fault::Error),
        (Boundary::Actions, Fault::Error),
    ];
    let mut now = Instant::now();
    let mut settings = Settings::new(now);
    for _ in 0..5 {
        now = advance(&mut settings, &host, now);
    }
    assert_eq!(logs.count("tick failed"), 1);
    host.faults
        .borrow_mut()
        .retain(|(boundary, _)| *boundary != Boundary::Actions);
    for _ in 0..4 {
        now = advance(&mut settings, &host, now);
    }
    assert_eq!(logs.count("recovered"), 0);
    assert_eq!(settings.failing, BTreeSet::from([Kind::Full]));
    host.faults.borrow_mut().clear();
    for _ in 0..3 {
        now = advance(&mut settings, &host, now);
    }
    assert_eq!(logs.count("recovered"), 1);
}
#[test]
fn deadlines_follow_completion_and_unanswered_ticks_time_out() {
    let host = Host::default();
    let now = Instant::now();
    let mut settings = Settings::new(now);
    host.faults.borrow_mut().extend([
        (Boundary::Full, Fault::Hold),
        (Boundary::Actions, Fault::Hold),
    ]);
    assert_eq!(advance(&mut settings, &host, now), now + REPLY_TIMEOUT);
    let done = now + Duration::from_secs(3);
    for reply in host.held.borrow_mut().drain(..) {
        reply.send(Ok("null".into())).unwrap();
    }
    assert_eq!(
        advance(&mut settings, &host, done),
        done + Duration::from_millis(500)
    );
    assert_eq!(*host.applied.borrow(), [false, true]);
    assert_eq!(
        advance(&mut settings, &host, done + Duration::from_millis(500)),
        done + Duration::from_secs(1)
    );
    advance(&mut settings, &host, done + Duration::from_secs(6));
    assert!(settings.failing.contains(&Kind::Actions));
}

#[test]
fn independent_settings_policies_keep_separate_failure_streaks() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let first = Host::default();
    let second = Host::default();
    let now = Instant::now();
    let mut one = Settings::new(now);
    let mut two = Settings::new(now);
    first
        .faults
        .borrow_mut()
        .push((Boundary::Full, Fault::Error));
    advance(&mut one, &first, now);
    advance(&mut two, &second, now);
    assert_eq!(logs.count("tick failed"), 1);
    second
        .faults
        .borrow_mut()
        .push((Boundary::Full, Fault::Error));
    advance(&mut two, &second, now + Duration::from_secs(1));
    assert_eq!(logs.count("tick failed"), 2);
    first.faults.borrow_mut().clear();
    advance(&mut one, &first, now + Duration::from_secs(1));
    assert_eq!(logs.count("recovered"), 1);
    assert!(!two.failing.is_empty());
}

#[test]
fn settings_before_the_first_viewport_skip_without_warning() {
    let logs = Logs::default();
    let _guard = logs.enter();
    let host = crate::testing::headless_host();
    let (_session, link) =
        crate::testing::start_probe_without_viewport(&host, crate::testing::ProbeMovie::new());
    let mut policy = SettingsPolicy::new(&link, Waker::noop().clone());
    policy.advance(Instant::now());
    assert_eq!(
        link.command(Command::ReapplySettings {
            actions_only: false
        })
        .recv_timeout(REPLY_TIMEOUT),
        Err(Error::NotReady)
    );
    policy.advance(Instant::now());
    assert_eq!(
        logs.count("settings policy tick failed"),
        0,
        "{:?}",
        logs.messages()
    );
}
