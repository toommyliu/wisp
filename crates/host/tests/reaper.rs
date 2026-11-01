#![cfg(any(target_os = "macos", target_os = "linux"))]

mod common;
use common::{hold::hold, logs::Logs, *};
use lucent_host::{Session, Supervisor, testing};
use std::{
    sync::{Mutex, mpsc},
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
