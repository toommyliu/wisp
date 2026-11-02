mod common;

use common::{hold::hold, logs::Logs, *};
use lucent_host::Supervisor;
use lucent_protocol::bridge::Value;
const INFO: tracing::Level = tracing::Level::INFO;
use lucent_host::{Session, testing};
use lucent_protocol::script::ScriptPhase;
use std::{
    sync::{Arc, mpsc},
    task::{Wake, Waker},
    time::{Duration, Instant},
};
use tracing_subscriber::prelude::*;

struct Changed(mpsc::Sender<()>);
impl Wake for Changed {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }
}
fn wait_status(session: &Session, ready: impl Fn(&Session) -> bool) {
    let (tx, rx) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if ready(session) {
            return;
        }
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}
fn wait_idle(session: &Session) {
    wait_status(session, |s| s.script_status().phase == ScriptPhase::Idle);
}
#[test]
fn a_nonreturning_finalizer_is_forced_after_the_stop_grace() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        ready_movie().recording("settings.setWalkSpeed").responding(
            "flash.callGameFunction0",
            [
                (serde_json::json!("true"), vec![]),
                (
                    serde_json::json!("false"),
                    vec![("onDebug".into(), "finalizer entered".into())],
                ),
            ],
        ),
    );
    session.supervise(None).unwrap();
    log_in(&observer);
    session
        .start_script(loose(&[(
            "finalizer.js",
            r#"
        import * as script from "lucent/script";
        export default function* () {
            console.log("body entered");
            yield* script.sleep("1 hour");
        }
    "#,
        )]))
        .unwrap();
    logs.wait_for(tracing::Level::INFO, "body entered");
    observer.call_loader("probe.emit", vec![
        lucent_protocol::bridge::Value::String("onExtensionResponse".into()),
        lucent_protocol::bridge::Value::String(serde_json::json!({"type":"json","dataObj":{"cmd":"cb","p":{"player-a":{"intHP":0,"intState":0}}}}).to_string()),
    ]).recv_timeout(TIMEOUT).unwrap();
    let player = observer
        .read(|p| p.world.get_me().unwrap().snapshot())
        .unwrap();
    assert_eq!(player["alive"], serde_json::json!(false), "{player}");
    let began = Instant::now();
    session.stop_script();
    assert_eq!(
        await_event(&observer, "onDebug", None).args,
        vec![lucent_protocol::bridge::Value::String(
            "finalizer entered".into()
        )]
    );
    let (tx, changes) = mpsc::channel();
    let waker = Waker::from(Arc::new(Changed(tx)));
    session.watch(waker.clone());
    observer.set_waker(waker);
    let mut settings = Vec::new();
    while session.script_status().phase != ScriptPhase::Idle {
        while let Some(event) = observer.try_event().unwrap() {
            if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
                && event.name == "onProbeCall"
                && event.args.first()
                    == Some(&lucent_protocol::bridge::Value::String(
                        "settings.setWalkSpeed".into(),
                    ))
            {
                settings.push(Instant::now());
            }
        }
        changes
            .recv_timeout((began + TIMEOUT).saturating_duration_since(Instant::now()))
            .unwrap();
    }
    assert!(began.elapsed() <= Duration::from_millis(5250));
    assert!(settings.len() >= 4, "{settings:?}");
    assert!(
        settings
            .windows(2)
            .all(|pair| pair[1] - pair[0] < Duration::from_millis(1250)),
        "{settings:?}"
    );
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("finalizer.js was stopped: did not stop within 5 s")
    );
    assert_eq!(
        logs.messages(tracing::Level::INFO)
            .iter()
            .filter(|line| line.as_str()
                == "session 1: finalizer.js was stopped: did not stop within 5 s")
            .count(),
        1
    );
}

#[test]
fn readiness_polls_loader_answers_until_a_run_can_start() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let movie = ready_movie()
        .responding(
            "player.isLoaded",
            [
                (serde_json::json!(false), vec![]),
                (serde_json::json!(false), vec![]),
                (serde_json::json!(true), vec![]),
            ],
        )
        .recording("player.isLoaded");
    let (mut session, observer) = testing::start_probe(&host, movie);
    session.supervise(None).unwrap();
    session
        .start_script(loose(&[(
            "ready.js",
            "export default function* () { console.log('ready body'); }",
        )]))
        .unwrap();
    log_in(&observer);
    logs.wait_for(tracing::Level::INFO, "ready body");
    wait_idle(&session);
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("ready.js completed")
    );
}

#[test]
fn connection_loss_ends_the_run_without_waiting_for_a_finalizer() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    session.supervise(None).unwrap();
    log_in(&observer);
    session
        .start_script(loose(&[(
            "disconnect.js",
            r#"
        import * as script from "lucent/script";
        export default function* () {
            console.log("disconnect body");
            yield* script.sleep("1 hour");
        }
    "#,
        )]))
        .unwrap();
    logs.wait_for(tracing::Level::INFO, "disconnect body");
    let began = Instant::now();
    observer
        .call_loader(
            "probe.emit",
            vec![
                lucent_protocol::bridge::Value::String("onConnection".into()),
                lucent_protocol::bridge::Value::String("OnConnectionLost".into()),
            ],
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
    wait_idle(&session);
    assert!(began.elapsed() <= Duration::from_millis(250));
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("disconnect.js was stopped: Connection lost")
    );
}

#[test]
fn each_worker_ending_publishes_one_terminal_outcome() {
    for (source, action, expected, level) in [
        (
            "export default function* () {}",
            "none",
            "outcome.js completed",
            tracing::Level::INFO,
        ),
        (
            "export default function* () { throw new Error('lifecycle boom'); }",
            "none",
            "outcome.js failed: lifecycle boom",
            tracing::Level::WARN,
        ),
        (
            "export default function* () { while (true) {} }",
            "none",
            "outcome.js was killed: a turn used more than 200 ms of CPU time",
            tracing::Level::WARN,
        ),
        (
            "export default function* () { const held = []; for (;;) { try { held.push(new Array(100000).fill(1.5)); } catch {} } }",
            "none",
            "outcome.js was killed: it reached the 64 MiB heap limit",
            tracing::Level::WARN,
        ),
        (
            "import * as script from 'lucent/script'; export default function* () { console.log('control body'); yield* script.sleep('1 hour'); }",
            "kill",
            "outcome.js was killed",
            tracing::Level::WARN,
        ),
        (
            "import * as script from 'lucent/script'; export default function* () { console.log('control body'); yield* script.sleep('1 hour'); }",
            "stop",
            "outcome.js was stopped",
            tracing::Level::INFO,
        ),
    ] {
        let logs = Logs::default();
        let dispatch = logs.dispatch();
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (mut session, observer) = testing::start_probe(&host, ready_movie());
        session.supervise(None).unwrap();
        log_in(&observer);
        session
            .start_script(loose(&[("outcome.js", source)]))
            .unwrap();
        if action != "none" {
            logs.wait_for(tracing::Level::INFO, "control body");
            if action == "kill" {
                session.kill_script();
            } else {
                session.stop_script();
            }
        }
        logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
        wait_idle(&session);
        let actual = session.script_status().last_end.unwrap();
        if source.contains("lifecycle boom") {
            assert!(
                actual.starts_with("outcome.js failed: outcome.js:1:37: lifecycle boom"),
                "{actual}"
            );
        } else {
            assert!(actual.starts_with(expected), "{actual}");
        }
        assert!(
            logs.messages(level)
                .iter()
                .any(|line| line == &format!("session 1: {actual}")),
            "terminal outcome was not logged at {level}"
        );
        let endings = [tracing::Level::INFO, tracing::Level::WARN]
            .into_iter()
            .flat_map(|level| logs.messages(level))
            .filter(|line| {
                line.starts_with("session 1: outcome.js ")
                    && (line.contains("completed")
                        || line.contains("failed:")
                        || line.contains("was killed")
                        || line.contains("was stopped"))
            })
            .collect::<Vec<_>>();
        assert_eq!(endings.len(), 1, "{endings:?}");
    }
}

fn emit_connection(observer: &lucent_host::ScriptLink, status: &str) {
    observer
        .call_loader(
            "probe.emit",
            vec![
                lucent_protocol::bridge::Value::String("onConnection".into()),
                lucent_protocol::bridge::Value::String(status.into()),
            ],
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
}
fn restart_option(session: &Session, enabled: bool) {
    session
        .update_script_options(lucent_protocol::script_options::ScriptOptionsPatch {
            restart_after_reconnect: Some(enabled),
            safe_start_stop: Some(false),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn reconnect_allocates_a_new_run_and_thread_with_reset_globals() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    restart_option(&session, true);
    session.supervise(None).unwrap();
    log_in(&observer);
    session
        .start_script(loose(&[(
            "reconnect.js",
            r#"
        import * as script from "lucent/script";
        export default function* () {
            globalThis.lifecycleCounter = (globalThis.lifecycleCounter ?? 0) + 1;
            console.log("fresh global " + globalThis.lifecycleCounter);
            yield* script.sleep("1 hour");
        }
    "#,
        )]))
        .unwrap();
    logs.wait_for(tracing::Level::INFO, "fresh global 1");
    let first = session.script_lifecycle().active.unwrap().run;
    emit_connection(&observer, "OnConnectionLost");
    logs.wait_for(
        tracing::Level::INFO,
        "session 1: reconnect.js was stopped: Connection lost",
    );
    wait_status(&session, |s| {
        s.script_lifecycle().active.is_none()
            && s.script_lifecycle().waiting_to_restart
            && s.script_status().phase == ScriptPhase::Waiting
    });
    assert_eq!(session.script_status().phase, ScriptPhase::Waiting);
    emit_connection(&observer, "OnConnection");
    log_in(&observer);
    logs.wait_for_count(tracing::Level::INFO, "fresh global 1", 2);
    assert_ne!(session.script_lifecycle().active.unwrap().run, first);
    let workers = logs
        .messages(tracing::Level::DEBUG)
        .into_iter()
        .filter(|s| s.starts_with("script worker lucent-js-"))
        .collect::<Vec<_>>();
    assert_eq!(workers.len(), 2);
    assert_ne!(workers[0], workers[1], "{workers:?}");
    assert!(
        logs.messages(tracing::Level::INFO)
            .iter()
            .all(|s| s != "fresh global 2")
    );
    session.kill_script();
    wait_idle(&session);
}

#[test]
fn two_queued_runs_finish_in_order_in_separate_isolates() {
    use lucent_host::lifecycle::{Entry, EntryId, QueueCommand, Request};
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    restart_option(&session, false);
    session.supervise(None).unwrap();
    log_in(&observer);
    for (id, name) in [(1, "a.js"), (2, "b.js")] {
        let source = format!(
            "export default function* () {{ if (globalThis.queuedRun) throw new Error('old global'); globalThis.queuedRun = true; console.log('{name} body'); }}"
        );
        session
            .script_queue(QueueCommand::Add(Entry {
                id: EntryId(id),
                request: Request {
                    name: name.into(),
                    reference: name.into(),
                    script: Arc::new(loose(&[(name, &source)])),
                    inputs: Default::default(),
                },
                inputs_available: false,
            }))
            .unwrap();
    }
    session.script_queue(QueueCommand::Start).unwrap();
    logs.wait_for(tracing::Level::INFO, "session 1: b.js completed");
    wait_idle(&session);
    assert_eq!(
        logs.messages(tracing::Level::INFO)
            .into_iter()
            .filter(|s| s.ends_with(" body"))
            .collect::<Vec<_>>(),
        ["a.js body", "b.js body"]
    );
    let status = session.script_lifecycle();
    assert_eq!(
        status.queue.latest.unwrap().status,
        lucent_host::lifecycle::queue::QueueRunStatus::Completed
    );
    assert_eq!(status.latest.unwrap().run.0, 2);
}

fn wait_lifecycle(
    session: &Session,
    matches: impl Fn(&lucent_host::lifecycle::Status) -> bool,
) -> lucent_host::lifecycle::Status {
    let (tx, rx) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let status = session.script_lifecycle();
        if matches(&status) {
            return status;
        }
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}

#[test]
fn disabled_restart_account_changes_and_player_replacement_cancel_reconnect() {
    for invalidator in ["off", "disable", "account", "player"] {
        let logs = Logs::default();
        let dispatch = logs.dispatch();
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (mut session, observer) = testing::start_probe(
            &host,
            ready_movie().constant("auth.login", serde_json::json!(true)),
        );
        restart_option(&session, invalidator != "off");
        session.supervise(None).unwrap();
        log_in(&observer);
        session.start_script(loose(&[("cancel.js", r#"
            import * as script from "lucent/script";
            export default function* () { console.log("cancel body"); yield* script.sleep("1 hour"); }
        "#)])).unwrap();
        logs.wait_for(tracing::Level::INFO, "cancel body");
        emit_connection(&observer, "OnConnectionLost");
        wait_lifecycle(&session, |status| status.active.is_none());
        if invalidator == "off" {
            assert!(!session.script_lifecycle().waiting_to_restart);
        } else {
            assert!(session.script_lifecycle().waiting_to_restart);
            match invalidator {
                "disable" => restart_option(&session, false),
                "account" => {
                    session
                        .command(lucent_protocol::command::Command::Login {
                            username: "other-account".into(),
                            password: "fixture".into(),
                        })
                        .recv_timeout(TIMEOUT)
                        .unwrap();
                }
                "player" => session.restart_player(testing::headless_surface()).unwrap(),
                _ => unreachable!(),
            }
            wait_lifecycle(&session, |status| {
                !status.waiting_to_restart && status.active.is_none()
            });
        }
        if invalidator != "player" {
            emit_connection(&observer, "OnConnection");
            log_in(&observer);
        }
        assert_eq!(session.script_lifecycle().latest.unwrap().run.0, 1);
        assert_eq!(
            logs.messages(tracing::Level::INFO)
                .iter()
                .filter(|s| s.as_str() == "cancel body")
                .count(),
            1
        );
    }
}

#[test]
fn incomplete_projection_facts_allocate_no_worker_or_loader_readiness_calls() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        ready_movie()
            .recording("player.isLoaded")
            .recording("world.isLoaded"),
    );
    session.supervise(None).unwrap();
    let attachments = session.attached_link_count().recv_timeout(TIMEOUT).unwrap();
    session
        .start_script(loose(&[("waiting.js", "export default function* () {}")]))
        .unwrap();
    logs.wait_for(
        tracing::Level::INFO,
        "session 1: waiting.js is waiting for login",
    );
    assert_eq!(
        session.attached_link_count().recv_timeout(TIMEOUT).unwrap(),
        attachments
    );
    assert!(
        !logs
            .messages(tracing::Level::DEBUG)
            .iter()
            .any(|s| s == "script worker booting")
    );
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
        {
            assert!(
                !matches!(event.args.first(), Some(lucent_protocol::bridge::Value::String(method)) if matches!(method.as_str(), "player.isLoaded" | "world.isLoaded"))
            );
        }
    }
    session.stop_script();
    wait_idle(&session);
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("waiting.js was stopped")
    );
}

#[test]
fn an_active_run_is_not_requeued_after_player_replacement() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    restart_option(&session, true);
    session.supervise(None).unwrap();
    log_in(&observer);
    session
        .start_script(loose(&[(
            "replace.js",
            r#"
        import * as script from "lucent/script";
        export default function* () { console.log("replace body"); yield* script.sleep("1 hour"); }
    "#,
        )]))
        .unwrap();
    logs.wait_for(tracing::Level::INFO, "replace body");
    session.restart_player(testing::headless_surface()).unwrap();
    logs.wait_for_count(tracing::Level::INFO, "session 1: supervisor started", 2);
    let observer = session
        .attach(lucent_host::bridge::EventInterests::ALL)
        .unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    log_in(&observer);
    wait_idle(&session);
    let status = session.script_lifecycle();
    assert!(!status.waiting_to_restart);
    assert_eq!(status.latest.unwrap().run.0, 1);
    assert_eq!(
        logs.messages(tracing::Level::INFO)
            .iter()
            .filter(|s| s.as_str() == "replace body")
            .count(),
        1
    );
}

#[test]
fn exit_flags_reach_the_host_and_logout_finishes_before_idle() {
    use lucent_host::lifecycle::Outcome;
    for close_client in [false, true] {
        let logs = Logs::default();
        let dispatch = logs.dispatch();
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (mut session, observer) = testing::start_probe(
            &host,
            ready_movie().raising_sequence(
                "auth.logout",
                [
                    ("onDebug".into(), "host logout".into()),
                    ("onConnection".into(), "OnConnectionLost".into()),
                ],
            ),
        );
        session.supervise(None).unwrap();
        log_in(&observer);
        let source = format!(
            "import * as script from 'lucent/script'; export default function* () {{ yield* script.exit({{ logout: true, closeClient: {close_client} }}); }}"
        );
        session
            .start_script(loose(&[("exit.js", &source)]))
            .unwrap();
        assert_eq!(
            await_event(&observer, "onDebug", None).args,
            vec![lucent_protocol::bridge::Value::String("host logout".into())]
        );
        wait_idle(&session);
        assert_eq!(
            session.script_lifecycle().latest.unwrap().outcome,
            Outcome::Exited {
                logout: true,
                close_client
            }
        );
        assert!(
            !session
                .read_projection(|p| p.auth.logged_in)
                .recv_timeout(TIMEOUT)
                .unwrap()
        );
    }
}

#[test]
fn a_readiness_reply_from_an_old_connection_is_ignored() {
    let logs = Logs::default();
    let (ctl_h, ctl) = hold(
        "session 1: o.js is waiting for login",
        Some("lucent-supervisor"),
        true,
    );
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(ctl_h),
    );
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) =
        testing::start_probe(&host, ready_movie().recording("player.isLoaded"));
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    log_in(&observer);
    supervisor
        .start_script(
            &mut session,
            loose(&[("p.js", "export default function* () {}")]),
        )
        .unwrap();
    logs.wait_for(INFO, "session 1: p.js completed");
    wait_idle(&session);
    while observer.try_event().unwrap().is_some() {}
    std::thread::scope(|scope| {
        let starting = scope.spawn(|| {
            supervisor.start_script(
                &mut session,
                loose(&[(
                    "o.js",
                    "export default function* () { console.log('o body'); }",
                )]),
            )
        });
        ctl.wait();
        emit_connection(&observer, "OnConnection");
        log_in(&observer);
        ctl.open();
        starting.join().unwrap().unwrap();
    });
    wait_status(&session, |s| {
        s.script_status()
            .last_end
            .is_some_and(|end| end.starts_with("o.js"))
    });
    wait_idle(&session);
    let mut reads = 0;
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && event.args.first() == Some(&Value::String("player.isLoaded".into()))
        {
            reads += 1;
        }
    }
    println!(
        "last_end {:?} body {} isLoaded reads {reads}",
        session.script_status().last_end,
        logs.messages(INFO)
            .iter()
            .filter(|line| *line == "o body")
            .count()
    );
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("o.js completed")
    );
}

#[test]
fn readiness_times_out_through_the_controller_without_booting_a_worker() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        ready_movie().responding("player.isLoaded", [(serde_json::json!(false), vec![])]),
    );
    session.supervise(None).unwrap();
    log_in(&observer);
    session
        .start_script(loose(&[(
            "timeout.js",
            "export default function* () { console.log('unexpected body'); }",
        )]))
        .unwrap();
    wait_status(&session, |s| s.script_status().last_end.is_some());
    wait_idle(&session);
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("timeout.js failed: Script readiness timed out after 10 s")
    );
    assert!(
        !logs
            .messages(tracing::Level::DEBUG)
            .iter()
            .any(|line| line == "script worker booting")
    );
    assert!(
        !logs
            .messages(tracing::Level::INFO)
            .iter()
            .any(|line| line == "unexpected body")
    );
}

#[test]
fn stop_cancels_a_pending_reconnect_restart() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    restart_option(&session, true);
    session.supervise(None).unwrap();
    log_in(&observer);
    let source = "import * as script from 'lucent/script'; export default function* () { console.log('restart body'); yield* script.sleep('1 hour'); }";
    session
        .start_script(loose(&[("restart.js", source)]))
        .unwrap();
    logs.wait_for(tracing::Level::INFO, "restart body");
    emit_connection(&observer, "OnConnectionLost");
    wait_status(&session, |s| {
        s.script_lifecycle().active.is_none()
            && s.script_lifecycle().waiting_to_restart
            && s.script_status().phase == ScriptPhase::Waiting
    });
    assert_eq!(session.script_status().phase, ScriptPhase::Waiting);
    assert_eq!(
        session.script_status().script.as_deref(),
        Some("restart.js")
    );
    session.stop_script();
    wait_status(&session, |s| !s.script_lifecycle().waiting_to_restart);
    assert_eq!(session.script_status().phase, ScriptPhase::Idle);
    emit_connection(&observer, "OnConnection");
    log_in(&observer);
    session
        .start_script(loose(&[("next.js", "export default function* () {}")]))
        .unwrap();
    logs.wait_for(tracing::Level::INFO, "session 1: next.js completed");
    wait_idle(&session);
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("next.js completed")
    );
    assert_eq!(
        logs.messages(tracing::Level::INFO)
            .iter()
            .filter(|line| *line == "restart body")
            .count(),
        1
    );
}

#[test]
fn a_killed_run_cannot_issue_queued_work_before_its_thread_exits() {
    use lucent_protocol::{operation::Operation, query::ItemSelector};
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    let logs = Logs::default();
    let (held_thread, thread_gate) = hold("queued operation held", Some("lucent-js-"), true);
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(held_thread),
    );
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    session.supervise(None).unwrap();
    log_in(&observer);
    observer.call_loader("probe.emit", vec![Value::String("onExtensionResponse".into()), Value::String(serde_json::json!({"type":"json","dataObj":{"cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Dropped Item","iQty":2}}}}).to_string())]).recv_timeout(TIMEOUT).unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let resume = Mutex::new(resume);
    let once = AtomicBool::new(false);
    let (issued, issuance) = mpsc::channel();
    observer.set_operation_probe(move |id, dispatched| {
        if dispatched {
            let _ = issued.send(id);
        } else if !once.swap(true, Ordering::AcqRel) {
            let _ = entered.send(id);
            let _ = resume.lock().unwrap().recv_timeout(TIMEOUT);
        }
    });
    struct Release(Option<mpsc::Sender<()>>);
    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }
    let release = Release(Some(release));
    session.start_script(loose(&[("queued.js", r#"
        import { Effect } from "effect";
        import * as api from "lucent/api";
        export default function* () {
            yield* Effect.raceFirst(api.drops.accept(70), Effect.sync(() => console.log("queued operation held")));
        }
    "#)])).unwrap();
    thread_gate.wait();
    let queued = waiting.recv_timeout(TIMEOUT).unwrap();
    session.kill_script();
    wait_status(&session, |s| {
        s.script_status().last_end.as_deref() == Some("queued.js was killed")
    });
    assert_eq!(session.script_status().phase, ScriptPhase::Finishing);
    drop(release);
    let sentinel = observer
        .operate(Operation::AcceptDrop {
            selector: ItemSelector::Id(999.0),
        })
        .unwrap();
    let (tx, changed) = mpsc::channel();
    observer.set_waker(Waker::from(Arc::new(Changed(tx))));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match observer.try_event().unwrap() {
            Some(lucent_host::bridge::ScriptEvent::Operation { id, .. }) if id == sentinel => break,
            Some(_) => continue,
            None => changed
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap(),
        }
    }
    assert!(!issuance.try_iter().any(|id| id == queued));
    thread_gate.open();
    wait_idle(&session);
}

#[test]
fn admission_publishes_waiting_before_returning_to_its_caller() {
    let logs = Logs::default();
    let (publishing, gate) = hold(
        "session 1: admitted.js is waiting for login",
        Some("lucent-supervisor"),
        true,
    );
    let dispatch =
        tracing::Dispatch::new(tracing_subscriber::registry().with(logs).with(publishing));
    let (accepted, answer) = mpsc::channel();
    let caller = std::thread::spawn(move || {
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (mut session, _observer) = testing::start_probe(&host, ready_movie());
        session.supervise(None).unwrap();
        session
            .start_script(loose(&[("admitted.js", "export default function* () {}")]))
            .unwrap();
        accepted.send(session.script_status().phase).unwrap();
        session.stop_script();
        wait_idle(&session);
        session.script_status().last_end
    });
    gate.wait();
    let early = answer.recv_timeout(Duration::from_secs(1));
    gate.open();
    let phase = match early {
        Ok(phase) => phase,
        Err(mpsc::RecvTimeoutError::Timeout) => answer.recv_timeout(TIMEOUT).unwrap(),
        Err(error) => panic!("admission failed: {error}"),
    };
    let end = caller.join().unwrap();
    assert_eq!(phase, ScriptPhase::Waiting);
    assert_eq!(end.as_deref(), Some("admitted.js was stopped"));
}

#[test]
fn queue_admission_publishes_waiting_before_returning_to_its_caller() {
    let logs = Logs::default();
    let (publishing, gate) = hold(
        "session 1: admitted.js is waiting for login",
        Some("lucent-supervisor"),
        true,
    );
    let dispatch =
        tracing::Dispatch::new(tracing_subscriber::registry().with(logs).with(publishing));
    let (accepted, answer) = mpsc::channel();
    let caller = std::thread::spawn(move || {
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (mut session, _observer) = testing::start_probe(&host, ready_movie());
        session.supervise(None).unwrap();
        use lucent_host::lifecycle::{Entry, EntryId, QueueCommand, Request};
        session
            .script_queue(QueueCommand::Add(Entry {
                id: EntryId(1),
                request: Request {
                    name: "admitted.js".into(),
                    reference: "admitted.js".into(),
                    script: Arc::new(loose(&[("admitted.js", "export default function* () {}")])),
                    inputs: Default::default(),
                },
                inputs_available: false,
            }))
            .unwrap();
        session.script_queue(QueueCommand::Start).unwrap();
        accepted.send(session.script_status().phase).unwrap();
        session.stop_script();
        wait_idle(&session);
        session.script_status().last_end
    });
    gate.wait();
    let early = answer.recv_timeout(Duration::from_secs(1));
    gate.open();
    let phase = match early {
        Ok(phase) => phase,
        Err(mpsc::RecvTimeoutError::Timeout) => answer.recv_timeout(TIMEOUT).unwrap(),
        Err(error) => panic!("admission failed: {error}"),
    };
    let end = caller.join().unwrap();
    assert_eq!(phase, ScriptPhase::Waiting);
    assert_eq!(end.as_deref(), Some("admitted.js was stopped"));
}
