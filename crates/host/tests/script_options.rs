mod common;
use common::*;
use lucent_host::{
    bridge::EventInterests,
    scripting::{Kill, RuntimeError},
    testing,
};
use lucent_protocol::{
    command::Command,
    query::Query,
    script::ScriptOutcome,
    script_options::{RoomPolicy, ScriptOptions, ScriptOptionsPatch},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::task::{Wake, Waker};

fn defaults() -> serde_json::Value {
    json!({"restartAfterReconnect":false,"roomPolicy":{"kind":"random-private"},"safeStartStop":true})
}

#[test]
fn defaults_match_the_account_settings() {
    let host = testing::headless_host();
    let (mut session, _) = testing::start_probe(&host, ready_movie());
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        defaults()
    );
    let reader = session.attach(EventInterests::ALL).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reader.query(Query::ScriptOptions {}).unwrap())
            .unwrap(),
        defaults()
    );
}

#[test]
fn script_updates_are_visible_to_the_session_and_the_next_run() {
    let (_host, mut session, _runtime) = ready_runtime();
    assert_eq!(start(&mut session, loose(&[("update.js", r#"
        import * as script from "lucent/script";
        export default function* () {
            yield* script.options.update({restartAfterReconnect:true,roomPolicy:{kind:"specific",roomNumber:42},safeStartStop:false});
            const detached = yield* script.options.get(); detached.roomPolicy.roomNumber = 99;
        }
    "#)]), Default::default()).wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    let expected = json!({"restartAfterReconnect":true,"roomPolicy":{"kind":"specific","roomNumber":42},"safeStartStop":false});
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        expected
    );
    assert_eq!(start(&mut session, loose(&[("read.js", r#"
        import * as script from "lucent/script";
        export default function* () {
            const value = yield* script.options.get();
            if (!value.restartAfterReconnect || value.safeStartStop || value.roomPolicy.roomNumber !== 42) throw new Error(JSON.stringify(value));
            yield* script.stop("options retained");
        }
    "#)]), Default::default()).wait(TIMEOUT).unwrap(), ScriptOutcome::StoppedByScript {reason:Some("options retained".into())});
}

#[test]
fn options_survive_a_script_kill_and_player_restart() {
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "keep.js",
            r#"
        import * as script from "lucent/script";
        import { packet } from "lucent/api";
        export default function* () {
            yield* script.options.update({roomPolicy:{kind:"public"},safeStartStop:false});
            yield* packet.sendToClient("stored");
            yield* script.sleep("1 hour");
        }
    "#,
        )]),
        Default::default(),
    );
    await_event(&observer, "onDebug", Some(&run));
    run.kill();
    assert!(matches!(
        run.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    let expected =
        json!({"restartAfterReconnect":false,"roomPolicy":{"kind":"public"},"safeStartStop":false});
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        expected
    );
    session.restart(testing::headless_surface()).unwrap();
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        expected
    );
    let reader = session.attach(EventInterests::ALL).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reader.query(Query::ScriptOptions {}).unwrap())
            .unwrap(),
        expected
    );
}

#[test]
fn reset_restores_every_default_through_both_writers() {
    let (_host, mut session, _runtime) = ready_runtime();
    let patch = || ScriptOptionsPatch {
        restart_after_reconnect: Some(true),
        room_policy: Some(RoomPolicy::Specific { room_number: 42 }),
        safe_start_stop: Some(false),
    };
    session.update_script_options(patch()).unwrap();
    assert_eq!(
        start(
            &mut session,
            loose(&[(
                "reset.js",
                r#"
        import * as script from "lucent/script";
        export default function* () { yield* script.options.reset(); }
    "#
            )]),
            Default::default()
        )
        .wait(TIMEOUT)
        .unwrap(),
        ScriptOutcome::Completed
    );
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        defaults()
    );
    session.update_script_options(patch()).unwrap();
    session.reset_script_options();
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        defaults()
    );
}

#[test]
fn invalid_room_numbers_reject_the_entire_patch() {
    let (_host, mut session, _runtime) = ready_runtime();
    for room_number in [0, 100_000, u32::MAX] {
        let error = session
            .update_script_options(ScriptOptionsPatch {
                restart_after_reconnect: Some(true),
                room_policy: Some(RoomPolicy::Specific { room_number }),
                safe_start_stop: Some(false),
            })
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "script.options.update requires valid runtime options."
        );
        assert_eq!(
            serde_json::to_value(session.script_options()).unwrap(),
            defaults()
        );
    }
    let link = session.attach(EventInterests::ALL).unwrap();
    assert!(
        link.command(Command::UpdateScriptOptions {
            patch: ScriptOptionsPatch {
                room_policy: Some(RoomPolicy::Specific { room_number: 0 }),
                ..Default::default()
            }
        })
        .recv_timeout(TIMEOUT)
        .is_err()
    );
    assert_eq!(start(&mut session, loose(&[("invalid.js", r#"
        import * as script from "lucent/script";
        import { Effect } from "effect";
        export default function* () {
            for (const roomNumber of [0,100000,-1,1.5,NaN,Infinity]) {
                const error = yield* script.options.update({safeStartStop:false,roomPolicy:{kind:"specific",roomNumber}}).pipe(Effect.map(() => null), Effect.catch((error) => Effect.succeed(error)));
                if (error === null || error.message !== "script.options.update requires valid runtime options.") throw new Error(String(error));
            }
        }
    "#)]), Default::default()).wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        defaults()
    );
}

struct Counter(AtomicUsize);
impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn watchers_wake_once_per_real_change() {
    let host = testing::headless_host();
    let (mut session, _) = testing::start_probe(&host, ready_movie());
    let counter = Arc::new(Counter(AtomicUsize::new(0)));
    session.watch(Waker::from(counter.clone()));
    session
        .update_script_options(ScriptOptionsPatch::default())
        .unwrap();
    session.reset_script_options();
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
    let patch = || ScriptOptionsPatch {
        safe_start_stop: Some(false),
        ..Default::default()
    };
    session.update_script_options(patch()).unwrap();
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    session.update_script_options(patch()).unwrap();
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    let link = session.attach(EventInterests::ALL).unwrap();
    link.command(Command::UpdateScriptOptions { patch: patch() })
        .recv_timeout(TIMEOUT)
        .unwrap();
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    link.command(Command::ResetScriptOptions {})
        .recv_timeout(TIMEOUT)
        .unwrap();
    assert_eq!(counter.0.load(Ordering::SeqCst), 2);
    session.reset_script_options();
    assert_eq!(counter.0.load(Ordering::SeqCst), 2);
}

#[test]
fn patches_and_invalid_values_match_the_shared_corpus() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packages/schemas/test-fixtures/script-options.json"
    ))
    .unwrap();
    let host = testing::headless_host();
    let (mut session, _) = testing::start_probe(&host, ready_movie());
    let link = session.attach(EventInterests::ALL).unwrap();
    for case in fixtures.as_array().unwrap() {
        session.reset_script_options();
        for step in case["steps"].as_array().unwrap() {
            let result = serde_json::from_value::<ScriptOptionsPatch>(step["patch"].clone())
                .map_err(|e| e.to_string())
                .and_then(|patch| {
                    session
                        .update_script_options(patch)
                        .map_err(|e| e.to_string())
                });
            assert_eq!(
                result.is_ok(),
                step["valid"].as_bool().unwrap(),
                "{} {}",
                case["name"],
                step["patch"]
            );
            assert_eq!(
                serde_json::to_value(session.script_options()).unwrap(),
                step["expected"],
                "{} {}",
                case["name"],
                step["patch"]
            );
            let command = serde_json::from_value::<Command>(
                json!({"kind":"updateScriptOptions","patch":step["patch"]}),
            );
            let result = command.map_err(|e| e.to_string()).and_then(|c| {
                link.command(c)
                    .recv_timeout(TIMEOUT)
                    .map_err(|e| e.to_string())
            });
            assert_eq!(result.is_ok(), step["valid"].as_bool().unwrap());
        }
    }
    assert_eq!(
        serde_json::to_value(ScriptOptions::default()).unwrap(),
        defaults()
    );
}

#[test]
fn concurrent_writers_preserve_disjoint_fields() {
    let host = testing::headless_host();
    let (mut session, _) = testing::start_probe(&host, ready_movie());
    let link = session.attach(EventInterests::ALL).unwrap();
    let other = session.attach(EventInterests::ALL).unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let barrier = &barrier;
        scope.spawn(move || {
            barrier.wait();
            other
                .command(Command::UpdateScriptOptions {
                    patch: ScriptOptionsPatch {
                        safe_start_stop: Some(false),
                        ..Default::default()
                    },
                })
                .recv_timeout(TIMEOUT)
                .unwrap();
        });
        scope.spawn(move || {
            barrier.wait();
            link.command(Command::UpdateScriptOptions {
                patch: ScriptOptionsPatch {
                    restart_after_reconnect: Some(true),
                    ..Default::default()
                },
            })
            .recv_timeout(TIMEOUT)
            .unwrap();
        });
    });
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        json!({"restartAfterReconnect":true,"roomPolicy":{"kind":"random-private"},"safeStartStop":false})
    );
}

#[test]
fn account_changes_keep_the_session_options() {
    let host = testing::headless_host();
    let (mut session, link) =
        testing::start_probe(&host, ready_movie().constant("auth.login", json!(true)));
    let login = |username: &str| {
        link.command(Command::Login {
            username: username.into(),
            password: "secret".into(),
        })
        .recv_timeout(TIMEOUT)
        .unwrap();
        let auth: serde_json::Value =
            serde_json::from_str(&link.query(Query::Auth {}).unwrap()).unwrap();
        assert_eq!(auth["username"], username);
    };
    login("Artix");
    session
        .update_script_options(ScriptOptionsPatch {
            safe_start_stop: Some(false),
            room_policy: Some(RoomPolicy::Public),
            ..Default::default()
        })
        .unwrap();
    login("artix");
    login("Gravelyn");
    assert_eq!(
        serde_json::to_value(session.script_options()).unwrap(),
        json!({"restartAfterReconnect":false,"roomPolicy":{"kind":"public"},"safeStartStop":false})
    );
    let reader = session.attach(EventInterests::ALL).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reader.query(Query::ScriptOptions {}).unwrap())
            .unwrap(),
        json!({"restartAfterReconnect":false,"roomPolicy":{"kind":"public"},"safeStartStop":false})
    );
}
