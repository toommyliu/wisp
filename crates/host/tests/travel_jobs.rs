mod common;

use common::*;
use lucent_host::{
    Supervisor,
    bridge::Value,
    game::travel::Reads,
    scripting::{Kill, RuntimeError},
    testing,
};
use lucent_protocol::script::ScriptOutcome;
use serde_json::json;

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
        import { Effect } from "effect";
        export default function* () {
            yield* api.player.joinMap("doom").pipe(Effect.timeoutOption("300 millis"));
            yield* api.packet.sendToClient("started");
            yield* Effect.never;
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
    assert!(matches!(
        run.wait(std::time::Duration::ZERO),
        Err(RuntimeError::Timeout)
    ));
    run.kill();
    assert!(matches!(
        run.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
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
