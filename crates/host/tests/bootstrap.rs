mod common;

use common::{hold::hold, logs::Logs, *};
use lucent_host::bridge::{EventInterests, ScriptEvent};
use lucent_host::scripting::{Kill, RuntimeError, ScriptRun};
use lucent_host::testing;
use lucent_host::{ScriptLink, Session};
use lucent_protocol::bridge::Value;
use lucent_protocol::script::ScriptOutcome;
use serde_json::json;
use std::sync::{Arc, mpsc};
use std::task::{Wake, Waker};
use std::time::{Duration, Instant};
use tracing_subscriber::prelude::*;

const LOADING: &str = r#"
    import * as api from "lucent/api";
    console.log("module loading");
    export default function* () {
        yield* api.packet.sendToClient("started");
    }
"#;

const ACCEPTING: &str = r#"
    import * as api from "lucent/api";
    export default function* () {
        yield* api.drops.accept(70);
        yield* api.packet.sendToClient("started");
    }
"#;

fn held(message: &str) -> (common::hold::Gate, tracing::dispatcher::DefaultGuard) {
    let (layer, gate) = hold(message, Some("lucent-js-"), true);
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(Logs::default())
            .with(layer),
    );
    (gate, tracing::dispatcher::set_default(&dispatch))
}

fn accepting_session() -> (lucent_host::Host, Session, ScriptLink) {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(
        &host,
        ready_movie().accepting_drop(
            Duration::from_secs(3600),
            json!({"type":"json","dataObj":{"cmd":"getDrop","ItemID":70,"bSuccess":1,"iQtyNow":1}})
                .to_string(),
        ),
    );
    log_in(&link);
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(
                json!({"type":"json","dataObj":{"cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Drop","iQty":1}}}})
                    .to_string(),
            ),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    (host, session, link)
}

struct Changed(mpsc::Sender<()>);
impl Wake for Changed {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }
}

fn wait_last_end(session: &Session, expected: &str) {
    let (tx, changed) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    let deadline = Instant::now() + TIMEOUT;
    while session.script_status().last_end.as_deref() != Some(expected) {
        if changed
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .is_err()
        {
            panic!("{:?}", session.script_status());
        }
    }
}

fn supervised_session() -> (lucent_host::Host, Session, ScriptLink) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    session.supervise(None).unwrap();
    log_in(&observer);
    (host, session, observer)
}

fn replacement_link(session: &mut Session) -> ScriptLink {
    let observer = session.attach(EventInterests::ALL).unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    observer
}

fn accept_in_flight(session: &mut Session, observer: &ScriptLink) -> ScriptRun {
    let run = start(session, loose(&[("accept.js", ACCEPTING)]), Default::default());
    await_event(observer, "onProbeAccept", Some(&run));
    run
}

fn only_the_next_run_reaches_the_player(session: &mut Session, observer: &ScriptLink) {
    let marker = start(
        session,
        loose(&[(
            "marker.js",
            r#"import * as api from "lucent/api";
            export default function* () { yield* api.packet.sendToClient("marker"); }"#,
        )]),
        Default::default(),
    );
    assert_eq!(marker.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    let mut calls = 0;
    while let Some(event) = observer.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onDebug"
        {
            calls += 1;
        }
    }
    assert_eq!(calls, 1, "a dead run reached the player");
}

#[test]
fn a_kill_while_the_module_loads_issues_no_game_call() {
    let (gate, _guard) = held("module loading");
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(&mut session, loose(&[("load.js", LOADING)]), Default::default());
    gate.wait();
    run.kill();
    gate.open();
    let outcome = run.wait(TIMEOUT);
    assert!(matches!(outcome, Err(RuntimeError::Killed(Kill::Requested))), "{outcome:?}");
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_player_restart_while_the_module_loads_issues_no_game_call() {
    let (gate, _guard) = held("module loading");
    let (_host, mut session, _observer) = supervised_session();
    session.start_script(loose(&[("load.js", LOADING)])).unwrap();
    gate.wait();
    session.restart_player(testing::headless_surface()).unwrap();
    gate.open();
    wait_last_end(&session, "load.js ended because its player stopped");
    let observer = replacement_link(&mut session);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_watchdog_during_module_load_issues_no_game_call() {
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "spin.js",
            r#"
            import * as api from "lucent/api";
            while (true) {}
            export default function* () {
                yield* api.packet.sendToClient("started");
            }
        "#,
        )]),
        Default::default(),
    );
    let outcome = run.wait(TIMEOUT);
    assert!(matches!(outcome, Err(RuntimeError::Killed(Kill::Watchdog))), "{outcome:?}");
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_stop_during_the_first_action_issues_no_later_game_call() {
    let (_host, mut session, observer) = accepting_session();
    let run = accept_in_flight(&mut session, &observer);
    run.stop_script();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_kill_during_the_first_action_issues_no_later_game_call() {
    let (_host, mut session, observer) = accepting_session();
    let run = accept_in_flight(&mut session, &observer);
    run.kill();
    let outcome = run.wait(TIMEOUT);
    assert!(matches!(outcome, Err(RuntimeError::Killed(Kill::Requested))), "{outcome:?}");
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_player_restart_during_the_first_action_issues_no_later_game_call() {
    let (_host, mut session, observer) = accepting_session();
    session.supervise(None).unwrap();
    session.start_script(loose(&[("accept.js", ACCEPTING)])).unwrap();
    await_event(&observer, "onProbeAccept", None);
    session.restart_player(testing::headless_surface()).unwrap();
    wait_last_end(&session, "accept.js ended because its player stopped");
    drop(observer);
    let observer = replacement_link(&mut session);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_failing_event_handler_fails_the_run_with_its_error() {
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "handler.js",
            r#"
            import * as api from "lucent/api";
            import * as script from "lucent/script";
            export default function* () {
                yield* api.events.on({ type: "join-map" }, function* () {
                    throw new Error("handler broke");
                });
                yield* api.packet.sendToClient("started");
                yield* script.sleep("1 hour");
            }
        "#,
        )]),
        Default::default(),
    );
    await_event(&observer, "onDebug", Some(&run));
    observer
        .call_loader(
            "probe.emit",
            vec![
                Value::String("onExtensionResponse".into()),
                Value::String(
                    json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":2,"areaName":"battleon-2",
                        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}})
                    .to_string(),
                ),
            ],
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
    match run.wait(TIMEOUT).unwrap() {
        ScriptOutcome::Failed { message, details } => {
            assert_eq!(message, "handler.js:6:27: handler broke");
            assert!(details.unwrap().contains("handler broke"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_projected_read_does_not_ask_the_loader() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        ready_movie()
            .constant("player.getHp", json!(999))
            .game_object("world.myAvatar.dataLeaf.intHP", json!(999)),
    );
    log_in(&link);
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(
                json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":1,"areaName":"buyhouse-1",
                    "uoBranch":[{"entID":7,"uoName":"player-a","intHP":37,"intHPMax":100}]}})
                .to_string(),
            ),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "hp.js",
            r#"
            import * as api from "lucent/api";
            import * as script from "lucent/script";
            export default function* () {
                yield* script.stop(String(yield* api.player.getHp()));
            }
        "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("37".into())
        }
    );
}
