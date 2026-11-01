mod common;

use common::{logs::Logs, *};
use lucent_host::{Session, testing};
use lucent_protocol::script::ScriptPhase;
use std::{
    sync::{Arc, mpsc},
    task::{Wake, Waker},
    time::{Duration, Instant},
};

struct Changed(mpsc::Sender<()>);
impl Wake for Changed {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }
}
fn wait_idle(session: &Session) {
    let (tx, rx) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if session.script_status().phase == ScriptPhase::Idle {
            return;
        }
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
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
            "export default function* () { const held = []; while (true) held.push(new Array(100000).fill('heap')); }",
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
        let _ = level;
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
    wait_idle(&session);
    assert!(session.script_lifecycle().waiting_to_restart);
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
        wait_idle(&session);
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
