#![cfg(any(target_os = "macos", target_os = "linux"))]

mod common;
use common::{hold::hold, logs::Logs, *};
use lucent_host::{Session, Supervisor, testing};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    task::{Wake, Waker},
    time::{Duration, Instant},
};
use tracing_subscriber::prelude::*;

static SERIAL: Mutex<()> = Mutex::new(());

fn quiescent_threads(prefixes: &[&str], deadline: Instant) -> Vec<String> {
    loop {
        let names: Vec<_> = named_threads()
            .into_iter()
            .filter(|name| !name.starts_with('<'))
            .collect();
        if !names
            .iter()
            .any(|name| prefixes.iter().any(|prefix| name.starts_with(prefix)))
        {
            return names;
        }
        assert!(
            Instant::now() < deadline,
            "workers remained after the join deadline: {names:?}"
        );
        std::thread::yield_now();
    }
}
fn start(host: &lucent_host::Host) -> (Session, lucent_host::ScriptLink) {
    let (session, observer) = testing::start_probe(host, ready_movie());
    log_in(&observer);
    (session, observer)
}
fn waiting() -> lucent_protocol::script::Script {
    loose(&[(
        "wait.js",
        "import * as script from 'lucent/script'; export default function* () { console.log('body entered'); yield* script.sleep('1 hour'); }",
    )])
}

#[test]
fn a_held_teardown_does_not_delay_another_sessions_next_run() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let logs = Logs::default();
    let (joining, gate) = hold("script worker joining", Some("lucent-reaper"), true);
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(joining),
    );
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut a, _a_observer) = start(&host);
    let (mut b, _b_observer) = start(&host);
    a.supervise(None).unwrap();
    b.supervise(None).unwrap();
    a.start_script(loose(&[("a.js", "export default function* () {}")]))
        .unwrap();
    gate.wait();
    b.start_script(loose(&[("b.js", "export default function* () {}")]))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while b.script_status().phase != lucent_protocol::script::ScriptPhase::Idle {
        assert!(
            Instant::now() < deadline,
            "session B could not finish while session A was held"
        );
        std::thread::yield_now();
    }
    b.start_script(waiting())
        .expect("session B's next run must be admitted while A is held");
    logs.wait_for(tracing::Level::INFO, "body entered");
    gate.open();
}

#[test]
fn twenty_stop_cycles_and_host_drop_join_all_script_workers() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let logs = Logs::default();
    let (joining, gate) = hold("script worker joining", Some("lucent-reaper"), false);
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(joining),
    );
    let (done, dropped) = mpsc::channel();
    let dropper = std::thread::spawn(move || {
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (mut session, _observer) = start(&host);
        session.supervise(None).unwrap();
        for cycle in 1..=20 {
            session.start_script(waiting()).unwrap();
            logs.wait_for_count(tracing::Level::INFO, "body entered", cycle);
            let stopped = Instant::now();
            session.stop_script();
            logs.wait_for_count(
                tracing::Level::DEBUG,
                "session 1: script worker reaped",
                cycle,
            );
            assert!(stopped.elapsed() < Duration::from_secs(2));
        }
        session.start_script(waiting()).unwrap();
        logs.wait_for_count(tracing::Level::INFO, "body entered", 21);
        assert!(done.send((host, session)).is_ok());
    });
    let (host, session) = dropped.recv_timeout(TIMEOUT).unwrap();
    dropper.join().unwrap();
    gate.arm();
    let (done, dropped) = mpsc::channel();
    let began = Instant::now();
    let dropper = std::thread::spawn(move || {
        drop(session);
        done.send(()).unwrap();
        drop(host);
        done.send(()).unwrap();
    });
    gate.wait();
    let early = dropped.recv_timeout(Duration::from_millis(100));
    gate.open();
    if early.is_err() {
        dropped.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    dropped.recv_timeout(Duration::from_secs(2)).unwrap();
    dropper.join().unwrap();
    assert!(
        matches!(early, Err(mpsc::RecvTimeoutError::Timeout)),
        "drop returned before the worker joined"
    );
    assert!(began.elapsed() < Duration::from_secs(2));
    quiescent_threads(
        &["lucent-js", "lucent-reaper"],
        began + Duration::from_secs(2),
    );
}

#[test]
fn a_stuck_worker_is_warned_about_and_abandoned_during_shutdown() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let logs = Logs::default();
    let (booting, gate) = hold("script worker booting", Some("lucent-js-"), true);
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(booting),
    );
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, _observer) = start(&host);
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    supervisor.start_script(&mut session, waiting()).unwrap();
    gate.wait();
    let (done, dropped) = mpsc::channel();
    let began = Instant::now();
    let dropper = std::thread::spawn(move || {
        drop(session);
        drop(host);
        done.send(()).unwrap();
    });
    let result = dropped.recv_timeout(Duration::from_secs(2));
    let elapsed = began.elapsed();
    let warnings = logs.messages(tracing::Level::WARN);
    gate.open();
    result.expect("shutdown exceeded its bound");
    dropper.join().unwrap();
    drop(supervisor);
    quiescent_threads(&["lucent-js", "lucent-reaper"], Instant::now() + TIMEOUT);
    assert!(elapsed < Duration::from_secs(2));
    assert!(
        warnings
            .iter()
            .any(|line| line.contains("abandoning script worker")),
        "missing abandonment warning: {warnings:?}"
    );
}

#[test]
fn a_hundred_runs_leave_the_same_named_threads_as_one_run() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, _observer) = start(&host);
    let mut after_one = Vec::new();
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    for run in 1..=100 {
        supervisor
            .start_script(
                &mut session,
                loose(&[("done.js", "export default function* () {}")]),
            )
            .unwrap();
        logs.wait_for_count(
            tracing::Level::DEBUG,
            "session 1: script worker reaped",
            run,
        );
        if run == 1 {
            after_one = quiescent_threads(&["lucent-js"], Instant::now() + Duration::from_secs(2));
        }
    }
    let after_hundred = quiescent_threads(&["lucent-js"], Instant::now() + Duration::from_secs(2));
    assert_eq!(after_hundred, after_one);
    drop(supervisor);
    drop(session);
    drop(host);
    quiescent_threads(
        &["lucent-js", "lucent-reaper"],
        Instant::now() + Duration::from_secs(2),
    );
}

struct PanicOn {
    message: &'static str,
    thread: &'static str,
    armed: AtomicBool,
}
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for PanicOn {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        let name = std::thread::current().name().unwrap_or("").to_string();
        if message.0 == self.message
            && name.starts_with(self.thread)
            && self.armed.swap(false, Ordering::AcqRel)
        {
            panic!("injected teardown panic in {name}");
        }
    }
}

#[test]
fn a_panicking_reap_completes_the_run_and_admits_the_next_script() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let logs = Logs::default();
    let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(logs.clone()).with(
        PanicOn {
            message: "script worker joining",
            thread: "lucent-reaper",
            armed: AtomicBool::new(true),
        },
    ));
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = start(&host);
    session.supervise(None).unwrap();
    session
        .start_script(loose(&[("done.js", "export default function* () {}")]))
        .unwrap();
    logs.wait_for(tracing::Level::ERROR, "a worker join panicked");
    let deadline = Instant::now() + Duration::from_secs(2);
    while session.script_status().phase != lucent_protocol::script::ScriptPhase::Idle
        && Instant::now() < deadline
    {
        std::thread::yield_now();
    }
    let phase = session.script_status().phase;
    let next = session.start_script(waiting());
    if next.is_ok() {
        logs.wait_for(tracing::Level::INFO, "body entered");
    }
    let serving = session.wait_for_script_reaping(TIMEOUT);
    drop(observer);
    drop(session);
    drop(host);
    quiescent_threads(
        &["lucent-js", "lucent-reaper", "lucent-supervisor"],
        Instant::now() + TIMEOUT,
    );
    assert_eq!(phase, lucent_protocol::script::ScriptPhase::Idle);
    assert!(next.is_ok(), "next start failed: {:?}", next.err());
    assert!(serving, "the reaper stopped serving after a panic");
}

struct BlockOnSupervisor {
    armed: Arc<AtomicBool>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
    entered: Mutex<mpsc::Sender<()>>,
}
impl Wake for BlockOnSupervisor {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let name = std::thread::current().name().unwrap_or("").to_string();
        if name.starts_with("lucent-supervisor") && self.armed.swap(false, Ordering::AcqRel) {
            let _ = self.entered.lock().unwrap().send(());
            let _ = self.release.lock().unwrap().recv_timeout(TIMEOUT);
        }
    }
}

#[test]
fn a_stuck_controller_and_reaper_share_the_session_shutdown_deadline() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let logs = Logs::default();
    let (joining, reaper_gate) = hold("script worker joining", Some("lucent-reaper"), false);
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(joining),
    );
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = start(&host);
    session.supervise(None).unwrap();
    session.start_script(waiting()).unwrap();
    logs.wait_for(tracing::Level::INFO, "body entered");
    let armed = Arc::new(AtomicBool::new(false));
    let (release, resume) = mpsc::channel();
    let (entered, blocked) = mpsc::channel();
    session.watch(Waker::from(Arc::new(BlockOnSupervisor {
        armed: armed.clone(),
        release: Arc::new(Mutex::new(resume)),
        entered: Mutex::new(entered),
    })));
    reaper_gate.arm();
    armed.store(true, Ordering::Release);
    let (done, dropped) = mpsc::channel();
    let dropper = std::thread::spawn(move || {
        let began = Instant::now();
        drop(observer);
        drop(session);
        drop(host);
        let _ = done.send(began.elapsed());
    });
    let controller_blocked = blocked.recv_timeout(TIMEOUT);
    reaper_gate.wait();
    let result = dropped.recv_timeout(Duration::from_secs(4));
    let warnings = logs.messages(tracing::Level::WARN);
    drop(release);
    reaper_gate.open();
    dropper.join().unwrap();
    quiescent_threads(
        &["lucent-js", "lucent-reaper", "lucent-supervisor"],
        Instant::now() + TIMEOUT,
    );
    controller_blocked.expect("the controller was not held");
    let elapsed = result.expect("session drop did not return");
    println!("session shutdown with both joins held: {elapsed:?}");
    assert!(
        elapsed >= Duration::from_secs(1),
        "shutdown did not wait for its workers"
    );
    assert!(
        elapsed < Duration::from_millis(1500),
        "session joins used separate deadlines: {elapsed:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|line| line.contains("abandoning session controller")),
        "{warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|line| line.contains("abandoning script worker teardown")),
        "{warnings:?}"
    );
}

#[test]
fn an_abandoned_controller_does_not_warn_about_an_idle_reaper() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let logs = Logs::default();
    let (killing, gate) = hold(
        "session 1: killing wait.js",
        Some("lucent-supervisor"),
        false,
    );
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(killing),
    );
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = start(&host);
    session.supervise(None).unwrap();
    session.start_script(waiting()).unwrap();
    logs.wait_for(tracing::Level::INFO, "body entered");
    gate.arm();
    let (done, dropped) = mpsc::channel();
    let dropper = std::thread::spawn(move || {
        drop(observer);
        drop(session);
        drop(host);
        let _ = done.send(());
    });
    gate.wait();
    let result = dropped.recv_timeout(Duration::from_secs(2));
    let warnings = logs.messages(tracing::Level::WARN);
    gate.open();
    dropper.join().unwrap();
    quiescent_threads(
        &["lucent-js", "lucent-reaper", "lucent-supervisor"],
        Instant::now() + TIMEOUT,
    );
    result.expect("session shutdown exceeded its bound");
    assert!(
        warnings
            .iter()
            .any(|line| line.contains("abandoning session controller")),
        "{warnings:?}"
    );
    assert!(
        !warnings
            .iter()
            .any(|line| line.contains("abandoning script worker teardown")),
        "idle reaper reported as abandoned: {warnings:?}"
    );
}

#[test]
fn a_second_supervisor_reports_that_the_session_is_already_supervised() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let host = testing::headless_host();
    let (mut session, _observer) = start(&host);
    let first = Supervisor::start(&mut session, None).unwrap();
    let second = Supervisor::start(&mut session, None);
    let error = second.err().expect("duplicate supervision was accepted");
    assert_eq!(error.to_string(), "the session already has a supervisor");
    drop(first);
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let mut next = Supervisor::start(&mut session, None).unwrap();
    next.start_script(&mut session, waiting()).unwrap();
    logs.wait_for(tracing::Level::INFO, "body entered");
}
