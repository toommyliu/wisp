mod common;

use common::*;
use lucent_host::{
    Supervisor,
    bridge::Value,
    game::travel::Reads,
    scripting::{Kill, RuntimeError},
    testing,
};
use lucent_protocol::script::{ScriptDialog, ScriptDialogResponse, ScriptOutcome};
use serde_json::json;
use std::{
    sync::{Arc, Mutex, mpsc},
    task::{Wake, Waker},
    time::{Duration, Instant},
};

fn setup() -> (lucent_host::Session, lucent_host::ScriptLink, Supervisor) {
    setup_with(|movie| movie)
}

fn setup_with(
    modify: impl FnOnce(testing::ProbeMovie) -> testing::ProbeMovie,
) -> (lucent_host::Session, lucent_host::ScriptLink, Supervisor) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        modify(
            ready_movie()
                .constant("world.isActionAvailable", json!(true))
                .responding("player.jump", [(json!(null), vec![])])
                .responding("player.getCell", [(json!("r2"), vec![])])
                .responding("player.getPad", [(json!("Left"), vec![])])
                .responding("world.isCellReady", [(json!(true), vec![])])
                .responding("player.joinMap", [(json!(null), vec![])])
                .responding("combat.cancelAutoAttack", [(json!(null), vec![])])
                .responding("combat.cancelTarget", [(json!(null), vec![])]),
        ),
    );
    log_in(&observer);
    session
        .update_script_options(
            serde_json::from_value(json!({"roomPolicy":{"kind":"public"}})).unwrap(),
        )
        .unwrap();
    let supervisor = Supervisor::start(&mut session, None).unwrap();
    (session, observer, supervisor)
}

fn await_no_travel(session: &lucent_host::Session) {
    let deadline = std::time::Instant::now() + TIMEOUT;
    while session.live_travel_job_count() != 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "travel jobs were not disposed"
        );
        std::thread::yield_now();
    }
}

fn await_dialog(
    session: &lucent_host::Session,
    run: &lucent_host::scripting::ScriptRun,
    timeout: Duration,
) -> ScriptDialog {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    session.watch(Waker::from(Arc::new(Unpark(std::thread::current()))));
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(dialog) = session.script_dialog() {
            return dialog;
        }
        let outcome = run.wait(Duration::ZERO);
        assert!(matches!(outcome, Err(RuntimeError::Timeout)), "{outcome:?}");
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "the script did not publish its dialog"
        );
        std::thread::park_timeout(remaining.min(Duration::from_millis(10)));
    }
}

#[test]
fn a_jump_reads_its_location_once_per_readiness_poll() {
    let (mut session, observer, _supervisor) = setup_with(|movie| {
        movie.responding(
            "world.isCellReady",
            [false, false, true].map(|ready| (json!(ready), vec![])),
        )
    });
    let run = start(
        &mut session,
        loose(&[(
            "poll.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* script.stop(String(yield* api.player.jumpToCell("r2", "Left")));
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let mut calls = Vec::new();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
        {
            calls.push(event.args[0].clone());
        }
    }
    let expected = std::iter::once("player.jump")
        .chain(["player.getCell", "player.getPad", "world.isCellReady"].repeat(3))
        .map(|method| Value::String(method.into()))
        .collect::<Vec<_>>();
    assert_eq!(calls, expected);
    await_no_travel(&session);
}

#[test]
fn an_unanswered_travel_action_returns_false_near_five_seconds() {
    let (mut session, _observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "deadline.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* script.alert("ready");
            yield* script.alert(String(yield* api.player.jumpToCell("r2", "Left")));
        }
    "#,
        )]),
        Default::default(),
    );
    let ready = await_dialog(&session, &run, TIMEOUT);
    assert_eq!(ready.message, "ready");
    let (entered, arrived) = mpsc::channel();
    let (release, held) = mpsc::channel::<()>();
    let _hold = session.read_projection(move |_| {
        let _ = entered.send(());
        let _ = held.recv();
    });
    arrived.recv_timeout(TIMEOUT).unwrap();
    let began = Instant::now();
    session.respond_dialog(ready.id, ScriptDialogResponse::Dismissed);
    let result = await_dialog(&session, &run, Duration::from_secs(8));
    let elapsed = began.elapsed();
    drop(release);
    assert_eq!(result.message, "false");
    assert!(elapsed >= Duration::from_secs(5), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(8), "{elapsed:?}");
    session.respond_dialog(result.id, ScriptDialogResponse::Dismissed);
    assert_eq!(run.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    await_no_travel(&session);
}

#[test]
fn a_travel_completion_reaches_its_script_while_the_player_is_held() {
    let (mut session, _observer, _supervisor) = setup();
    let (entered, arrived) = mpsc::channel();
    let (release, held) = mpsc::channel::<()>();
    let held = Mutex::new(held);
    session.set_travel_observation_probe(move |reads, _| {
        if reads.contains(Reads::LIVE) {
            let _ = entered.send(());
            let _ = held.lock().unwrap().recv();
        }
    });
    let run = start(
        &mut session,
        loose(&[(
            "completion.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* script.alert(String(yield* api.player.jumpToCell("r2", "Left")));
        }
    "#,
        )]),
        Default::default(),
    );
    let release_player = release;
    arrived.recv_timeout(TIMEOUT).unwrap();
    await_no_travel(&session);
    let result = await_dialog(&session, &run, TIMEOUT);
    drop(release_player);
    assert_eq!(result.message, "false");
    session.respond_dialog(result.id, ScriptDialogResponse::Dismissed);
    assert_eq!(run.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
}

#[test]
fn script_jump_runs_as_a_host_job() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "jump.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* script.stop(String(yield* api.player.jumpToCell("r2", "Left")));
        }
    "#,
        )]),
        Default::default(),
    );
    for method in [
        "player.jump",
        "player.getCell",
        "player.getPad",
        "world.isCellReady",
    ] {
        let event = await_event(&observer, "onProbeCall", Some(&run));
        let args = if matches!(method, "player.jump" | "world.isCellReady") {
            vec![Value::String("r2".into()), Value::String("Left".into())]
        } else {
            vec![]
        };
        assert_eq!(
            event.args,
            vec![Value::String(method.into()), Value::List(args)]
        );
    }
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
    await_no_travel(&session);
}

#[test]
fn killing_a_join_revokes_the_host_job() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "join.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.player.joinMap("doom"); }
    "#,
        )]),
        Default::default(),
    );
    let event = await_event(&observer, "onProbeCall", Some(&run));
    assert_eq!(event.args[0], Value::String("player.joinMap".into()));
    assert_eq!(session.live_travel_job_count(), 1);
    run.kill();
    assert!(matches!(
        run.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    await_no_travel(&session);
}

#[test]
fn restarting_the_player_cancels_a_join() {
    let (mut session, observer, mut supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "join.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.player.joinMap("doom"); }
    "#,
        )]),
        Default::default(),
    );
    await_event(&observer, "onProbeCall", Some(&run));
    assert_eq!(session.live_travel_job_count(), 1);
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    assert!(matches!(run.wait(TIMEOUT), Err(RuntimeError::Stopped)));
    await_no_travel(&session);
}

#[test]
fn combat_exit_refreshes_cells_before_cancelling_and_jumping() {
    let (mut session, observer, _supervisor) = setup_with(|movie| {
        movie
        .responding("world.getCells", [(json!(["r2"]), vec![])])
        .responding("player.jump", [(json!(null), vec![("onExtensionResponse".into(), json!({"type":"json","dataObj":{"cmd":"uotls","unm":"player-a","o":{"intState":1}}}).to_string())])])
    });
    observer.call_loader("probe.emit", vec![Value::String("onExtensionResponse".into()), Value::String(json!({"type":"json","dataObj":{"cmd":"uotls","unm":"player-a","o":{"intState":2,"strFrame":"r1","strPad":"Left"}}}).to_string())]).recv_timeout(TIMEOUT).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "exit.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () { yield* script.stop(String(yield* api.combat.exit())); }
    "#,
        )]),
        Default::default(),
    );
    for method in [
        "world.getCells",
        "combat.cancelAutoAttack",
        "combat.cancelTarget",
        "player.jump",
        "player.getCell",
        "player.getPad",
        "world.isCellReady",
    ] {
        assert_eq!(
            await_event(&observer, "onProbeCall", Some(&run)).args[0],
            Value::String(method.into())
        );
    }
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
    await_no_travel(&session);
}

#[test]
fn an_already_loaded_join_uses_the_native_readiness_order() {
    let (mut session, observer, _supervisor) = setup();
    let (observed, observations) = std::sync::mpsc::channel();
    session.set_travel_observation_probe(move |reads, calls| {
        if reads.contains(Reads::READY) {
            let _ = observed.send(calls);
        }
    });
    let run = start(
        &mut session,
        loose(&[(
            "join.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () { yield* script.stop(String(yield* api.player.joinMap("buyhouse"))); }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        observations.recv_timeout(TIMEOUT).unwrap(),
        vec![
            "auth.isLoggedIn".to_owned(),
            "world.isLoaded".to_owned(),
            "player.isLoaded".to_owned()
        ]
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert!(
                event.name != "onProbeCall"
                    || event.args.first() != Some(&Value::String("player.joinMap".into()))
            );
        }
    }
    await_no_travel(&session);
}

#[test]
fn a_native_join_reads_only_loading_fields_between_polls() {
    let (mut session, observer, _supervisor) = setup();
    let (observed, observations) = std::sync::mpsc::channel();
    session.set_travel_observation_probe(move |reads, calls| {
        if reads.contains(Reads::LOAD) {
            let _ = observed.send(calls);
        }
    });
    let run = start(
        &mut session,
        loose(&[(
            "join.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.player.joinMap("doom"); }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        await_event(&observer, "onProbeCall", Some(&run)).args[0],
        Value::String("player.joinMap".into())
    );
    for _ in 0..4 {
        assert_eq!(
            observations.recv_timeout(TIMEOUT).unwrap(),
            vec!["world.isLoaded".to_owned()]
        );
    }
    assert_eq!(session.live_travel_job_count(), 1);
    run.kill();
    assert!(matches!(
        run.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    await_no_travel(&session);
}

#[test]
fn a_fiber_timeout_cancels_travel_while_its_run_continues() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "timeout.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        import { Effect } from "effect";
        export default function* () {
            yield* api.player.joinMap("doom").pipe(Effect.timeoutOption("300 millis"));
            yield* api.packet.sendToClient("started");
            yield* Effect.raceFirst(
                Effect.sleep("1 hour"),
                script.confirm("continue")
            );
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        await_event(&observer, "onProbeCall", Some(&run)).args[0],
        Value::String("player.joinMap".into())
    );
    await_event(&observer, "onDebug", Some(&run));
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    await_no_travel(&session);
    let dialog = await_dialog(&session, &run, TIMEOUT);
    assert_eq!(dialog.message, "continue");
    let outcome = run.wait(std::time::Duration::ZERO);
    assert!(matches!(outcome, Err(RuntimeError::Timeout)), "{outcome:?}");
    session.respond_dialog(dialog.id, ScriptDialogResponse::Confirmed(true));
    assert_eq!(run.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
}

#[test]
fn oversized_travel_input_returns_false_without_a_loader_call() {
    let (mut session, observer, _supervisor) = setup();
    while observer.try_event().unwrap().is_some() {}
    let run = start(
        &mut session,
        loose(&[(
            "limit.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* script.stop(String(yield* api.player.jumpToCell("x".repeat(4097))));
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("false".into())
        }
    );
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        assert!(
            !matches!(event, lucent_host::bridge::ScriptEvent::Loader { event, .. } if event.name == "onProbeCall")
        );
    }
    await_no_travel(&session);
}
