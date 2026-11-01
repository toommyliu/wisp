mod common;
use common::*;
use lucent_host::bridge::EventInterests;
use lucent_host::scripting::{Kill, RuntimeError};
use lucent_protocol::script::ScriptOutcome;
use std::time::Duration;

#[test]
fn the_daily_package_runs_and_reports_its_maxed_reward() {
    let (_host, mut session, _runtime) = ready_runtime();
    assert_eq!(
        start(
            &mut session,
            package("lucent-dailies", "scripts/farm-elders-blood.js", &[]),
            Default::default()
        )
        .wait(TIMEOUT)
        .unwrap(),
        ScriptOutcome::Completed
    );
    let wrapper = package(
        "lucent-dailies",
        "tests/wrapper.js",
        &[(
            "tests/wrapper.js",
            r#"
            import * as Api from "lucent/api";
            import * as script from "lucent/script";
            import run from "../scripts/farm-elders-blood";
            export default function* () {
                const result = yield* run();
                yield* script.stop(JSON.stringify({
                    ...result,
                    map: yield* Api.map.getName(),
                }));
            }
        "#,
        )],
    );
    assert_eq!(
        start(&mut session, wrapper, Default::default())
            .wait(TIMEOUT)
            .unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some(r#"{"status":"maxed","map":"buyhouse"}"#.into()),
        },
    );
}

#[test]
fn inputs_apply_defaults_and_report_missing_required_values() {
    let (_host, mut session, _runtime) = ready_runtime();
    let defaults = loose(&[(
        "inputs.js",
        r#"
        import * as script from "lucent/script";
        export const inputs = {
            id: "test",
            fields: [{ key: "name", label: "Name", type: "string", default: "Tommy" }],
        };
        export default function* () {
            yield* script.stop(yield* script.inputs.get("name"));
        }
    "#,
    )]);
    assert_eq!(
        start(&mut session, defaults, Default::default())
            .wait(TIMEOUT)
            .unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("Tommy".into())
        },
    );
    let required = loose(&[(
        "required.js",
        r#"
        export const inputs = {
            id: "test",
            fields: [{ key: "name", label: "Name", type: "string", required: true }],
        };
        export default function* () {}
    "#,
    )]);
    let outcome = start(&mut session, required, Default::default())
        .wait(TIMEOUT)
        .unwrap();
    match outcome {
        ScriptOutcome::Failed { message, .. } => {
            assert_eq!(message, "required.js is missing required inputs: name.")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn module_errors_name_the_script_and_location() {
    let (_host, mut session, _runtime) = ready_runtime();
    for (path, source, expected) in [
        (
            "syntax.js",
            "export const x = ;",
            "syntax.js:1:18: SyntaxError: Unexpected token ';'",
        ),
        (
            "missing.js",
            "import './absent';",
            "missing.js:1:8: Cannot find module \"./absent\"",
        ),
        (
            "export.js",
            "import { absent } from 'lucent/api';",
            "export.js:1:10: SyntaxError: The requested module 'lucent/api' does not provide an export named 'absent'",
        ),
        (
            "default.js",
            "export default 1;",
            "default.js must export a generator function as its default export.",
        ),
        (
            "evaluate.js",
            "throw new Error('broken module');",
            "evaluate.js:1:7: broken module",
        ),
        (
            "await.js",
            "await 0;",
            "await.js: Scripts can't use top-level await.",
        ),
    ] {
        match start(&mut session, loose(&[(path, source)]), Default::default())
            .wait(TIMEOUT)
            .unwrap()
        {
            ScriptOutcome::Failed { message, .. } => assert_eq!(message, expected),
            other => panic!("{other:?}"),
        }
    }
    let broken = package(
        "lucent-dailies",
        "tests/broken.js",
        &[("tests/broken.js", "export const x = ;")],
    );
    match start(&mut session, broken, Default::default())
        .wait(TIMEOUT)
        .unwrap()
    {
        ScriptOutcome::Failed { message, .. } => assert_eq!(
            message,
            "@lucent/dailies/tests/broken.js:1:18: SyntaxError: Unexpected token ';'"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_nested_module_evaluation_error_keeps_the_throwing_module_in_its_details() {
    let (_host, mut session, _runtime) = ready_runtime();
    let script = package(
        "lucent-dailies",
        "tests/entry.js",
        &[
            ("tests/entry.js", "import './middle.js';"),
            ("tests/middle.js", "import './throwing.js';"),
            ("tests/throwing.js", "throw new Error('nested failure');"),
        ],
    );
    match start(&mut session, script, Default::default())
        .wait(TIMEOUT)
        .unwrap()
    {
        ScriptOutcome::Failed { message, details } => {
            assert_eq!(
                message,
                "@lucent/dailies/tests/throwing.js:1:7: nested failure"
            );
            assert!(
                details
                    .unwrap()
                    .contains("at @lucent/dailies/tests/throwing.js:1:7")
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn two_importers_evaluate_their_shared_module_once() {
    let (_host, mut session, _runtime) = ready_runtime();
    let script = loose(&[
        (
            "entry.js",
            r#"
            import "./first.js";
            import "./second.js";
            import * as script from "lucent/script";
            export default function* () {
                yield* script.stop(String(globalThis.sharedModuleEvaluations));
            }
            "#,
        ),
        ("first.js", "import './shared.js';"),
        ("second.js", "import './shared.js';"),
        (
            "shared.js",
            "globalThis.sharedModuleEvaluations = (globalThis.sharedModuleEvaluations ?? 0) + 1;",
        ),
    ]);
    assert_eq!(
        start(&mut session, script, Default::default())
            .wait(TIMEOUT)
            .unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("1".into())
        }
    );
}

#[test]
fn builtin_default_named_and_namespace_exports_share_members() {
    let (_host, mut session, _runtime) = ready_runtime();
    let script = loose(&[(
        "builtins.js",
        r#"
        import { enum as e, null as n } from "lucent/schema";
        import schema from "lucent/schema";
        import Api from "lucent/api";
        import * as Namespace from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* script.stop(JSON.stringify([
                e === schema.enum,
                n === schema.null,
                Api.player === Namespace.player,
                Api === Namespace.default,
                yield* Api.map.getName(),
            ]));
        }
    "#,
    )]);
    assert_eq!(
        start(&mut session, script, Default::default())
            .wait(TIMEOUT)
            .unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some(r#"[true,true,true,true,"buyhouse"]"#.into())
        },
    );
}

#[test]
fn replacing_a_run_drops_it_before_starting_the_next() {
    let (_host, mut session, _runtime) = ready_runtime();
    let first = yielding(&mut session);
    drop(first);
    completed(&mut session);
    assert_eq!(
        session.attached_link_count().recv_timeout(TIMEOUT).unwrap(),
        1
    );
}

#[test]
fn a_cooperative_stop_survives_an_async_abort_listener_rejection() {
    let (_host, mut session, _runtime) = ready_runtime();
    let first = started_run(
        &mut session,
        r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            script.signal.addEventListener("abort", async () => { throw new Error("late"); });
            yield* api.packet.sendToClient("started");
            yield* script.sleep("1 hour");
        }
    "#,
    );
    first.stop_script();
    assert_eq!(
        first.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    drop(first);
    completed(&mut session);
}

#[test]
fn a_script_outcome_survives_an_unhandled_rejection_during_serialization() {
    let (_host, mut session, _runtime) = ready_runtime();
    let run = start(
        &mut session,
        loose(&[(
            "serialize.js",
            r#"
            export default function* () {
                Object.prototype.toJSON = function () {
                    if (this.kind === "completed") {
                        delete Object.prototype.toJSON;
                        Promise.reject(new Error("unrelated"));
                    }
                    return this;
                };
            }
            "#,
        )]),
        Default::default(),
    );
    assert_eq!(run.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    drop(run);
    completed(&mut session);
}

#[test]
fn cooperative_stop_settles_a_yielding_script_and_allows_another_run() {
    let (_host, mut session, _runtime) = ready_runtime();
    let run = yielding(&mut session);
    run.stop_script();
    run.stop_script();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    drop(run);
    completed(&mut session);
}

#[test]
fn a_timed_out_wait_leaves_the_script_run_usable() {
    let (_host, mut session, _runtime) = ready_runtime();
    let run = yielding(&mut session);
    assert!(matches!(
        run.wait(Duration::from_millis(1)),
        Err(RuntimeError::Timeout)
    ));
    run.stop_script();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
}

#[test]
fn the_watchdog_kills_a_script_and_a_fresh_run_completes() {
    let (_host, mut session, _runtime) = ready_runtime();
    for source in [
        "while (true) {} export default function* () {}",
        "export const inputs = { get id() { while (true) {} } }; export default function* () {}",
        "export default function* () { while (true) {} }",
    ] {
        let error = start(
            &mut session,
            loose(&[("loop.js", source)]),
            Default::default(),
        )
        .wait(TIMEOUT)
        .unwrap_err();
        assert!(
            matches!(error, RuntimeError::Killed(Kill::Watchdog)),
            "{error:?}"
        );
        completed(&mut session);
    }
}

#[test]
fn an_explicit_kill_ends_a_script_and_a_fresh_run_completes() {
    let (_host, mut session, _runtime) = ready_runtime();
    let run = yielding(&mut session);
    run.kill();
    assert!(matches!(
        run.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    drop(run);
    completed(&mut session);
}

#[test]
fn event_overflow_finishes_script_cleanup_before_reporting_loss() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = lucent_host::testing::headless_host();
    let (mut session, link) = lucent_host::testing::start_probe(
        &host,
        ready_movie().raising_many(
            "flash.sendClientPacket",
            "onConnection",
            "OnConnection",
            600,
        ),
    );
    let setup = lucent_host::scripting::Runtime::start(link).unwrap();
    ready(&setup);
    let run = start(
        &mut session,
        loose(&[(
            "overflow.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            script.signal.addEventListener("abort", () => console.log("overflow cleanup"));
            yield* script.options.update({ safeStartStop: false });
            yield* api.packet.sendToClient("burst");
            yield* script.sleep("1 hour");
        }
    "#,
        )]),
        Default::default(),
    );
    let outcome = run.wait(TIMEOUT);
    assert!(
        matches!(outcome, Err(RuntimeError::EventsLost)),
        "{outcome:?}"
    );
    assert!(
        logs.messages(tracing::Level::INFO)
            .iter()
            .any(|text| text.contains("overflow cleanup"))
    );
    drop(run);
    ready(&setup);
    completed(&mut session);
}

#[test]
fn dropping_a_script_run_detaches_its_link() {
    let (_host, mut session, _runtime) = ready_runtime();
    let run = yielding(&mut session);
    drop(run);
    assert_eq!(
        session.attached_link_count().recv_timeout(TIMEOUT).unwrap(),
        1
    );
    completed(&mut session);
}

#[test]
fn an_unavailable_host_service_fails_the_script_with_its_name() {
    let (_host, mut session, _runtime) = ready_runtime();
    let script = loose(&[(
        "army.js",
        r#"
        import * as api from "lucent/api";
        export default function* () {
            yield* api.army.start("group");
        }
    "#,
    )]);
    match start(&mut session, script, Default::default())
        .wait(TIMEOUT)
        .unwrap()
    {
        ScriptOutcome::Failed { message, .. } => {
            assert_eq!(
                message,
                "army.js:4:29: The army service is not available yet."
            )
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_stopped_run_leaves_no_globals_or_timers() {
    let (_host, mut session, _runtime) = ready_runtime();
    let run = started_run(
        &mut session,
        r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            globalThis.leak = 1;
            globalThis.ticks = 0;
            setInterval(() => globalThis.ticks++, 1);
            yield* api.packet.sendToClient("started");
            yield* script.sleep("1 hour");
        }
    "#,
    );
    run.stop_script();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    drop(run);
    let next = start(
        &mut session,
        loose(&[(
            "inspect.js",
            r#"
        import * as script from "lucent/script";
        export default function* () {
            yield* script.sleep(20);
            yield* script.stop(JSON.stringify([typeof globalThis.leak, typeof globalThis.ticks]));
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        next.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some(r#"["undefined","undefined"]"#.into()),
        }
    );
}

#[test]
fn user_modules_cannot_reach_host_internals() {
    let (_host, mut session, _runtime) = ready_runtime();
    let run = start(
        &mut session,
        loose(&[(
            "internals.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        const internals = [typeof __host, typeof __lucent, typeof window.swf];
        export default function* () {
            const player = yield* api.player.get();
            yield* script.stop(JSON.stringify([...internals, player?.username]));
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some(r#"["undefined","undefined","undefined","player-a"]"#.into()),
        }
    );
}

#[test]
fn restarting_the_player_stops_a_run_and_a_new_link_runs_again() {
    let (_host, mut session, runtime) = ready_runtime();
    let run = yielding(&mut session);
    session
        .restart(lucent_host::testing::headless_surface())
        .unwrap();
    assert!(matches!(run.wait(TIMEOUT), Err(RuntimeError::Stopped)));
    drop(run);
    drop(runtime);
    let link = session.attach(EventInterests::ALL).unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    await_event(&link, "onLoaded", None);
    let setup = lucent_host::scripting::Runtime::start(link).unwrap();
    ready(&setup);
    completed(&mut session);
}

#[test]
#[should_panic(expected = "run ended before onDebug: Ok(Completed)")]
fn waiting_for_start_reports_a_script_that_completed_without_the_event() {
    let (_host, mut session, _runtime) = ready_runtime();
    started_run(&mut session, "export default function* () {}");
}

#[test]
#[should_panic(expected = "run ended before onDebug: Err(Killed(Watchdog))")]
fn waiting_for_start_reports_a_script_that_was_killed_before_the_event() {
    let (_host, mut session, _runtime) = ready_runtime();
    started_run(
        &mut session,
        "export default function* () { while (true) {} }",
    );
}

#[test]
fn a_fresh_run_can_import_the_api_after_each_kill() {
    let (_host, mut session, _runtime) = ready_runtime();
    for _ in 0..10 {
        let run = yielding(&mut session);
        run.kill();
        assert!(matches!(
            run.wait(TIMEOUT),
            Err(RuntimeError::Killed(Kill::Requested))
        ));
        drop(run);
        let outcome = start(
            &mut session,
            loose(&[(
                "fresh.js",
                r#"
                import * as api from "lucent/api";
                import * as script from "lucent/script";
                export default function* () {
                    yield* script.stop(String(yield* api.auth.isLoggedIn()));
                }
                "#,
            )]),
            Default::default(),
        )
        .wait(TIMEOUT);
        assert!(
            matches!(outcome, Ok(ScriptOutcome::StoppedByScript { reason: Some(ref reason) }) if reason == "true"),
            "{outcome:?}"
        );
    }
}

#[test]
fn a_throwing_turn_does_not_stop_a_script_timer() {
    let (_host, mut session, runtime) = ready_runtime();
    drop(runtime);
    let run = start(
        &mut session,
        loose(&[(
            "throwing-turn.js",
            r#"
            import * as script from "lucent/script";
            export default function* () {
                yield* script.sleep("20 millis");
                const filter = Array.prototype.filter;
                Array.prototype.filter = function (...args) {
                    Array.prototype.filter = filter;
                    throw new Error("one broken filter");
                };
                yield* script.sleep("40 millis");
            }
        "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(Duration::from_secs(3)).unwrap(),
        ScriptOutcome::Completed
    );
}

#[test]
fn invalid_inputs_exports_report_the_script_name() {
    let (_host, mut session, _runtime) = ready_runtime();
    let outcome = start(
        &mut session,
        loose(&[(
            "invalid.js",
            "export const inputs = 42; export default function* () {}",
        )]),
        Default::default(),
    )
    .wait(TIMEOUT)
    .unwrap();
    match outcome {
        ScriptOutcome::Failed { message, .. } => {
            assert_eq!(message, "invalid.js has an invalid inputs export.")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn generator_errors_keep_the_script_frame_and_cause_stack() {
    let (_host, mut session, _runtime) = ready_runtime();
    let script = loose(&[(
        "throw.js",
        "export default function* () {\n  throw new Error('outer failure', { cause: new Error('inner failure') });\n}",
    )]);
    let outcome = start(&mut session, script, Default::default())
        .wait(TIMEOUT)
        .unwrap();
    match outcome {
        ScriptOutcome::Failed { message, details } => {
            assert_eq!(message, "throw.js:2:9: outer failure");
            let details = details.unwrap();
            assert!(
                details.starts_with("Error: outer failure\n    at default (throw.js:2:9)"),
                "{details}"
            );
            assert!(
                details
                    .contains("\nCaused by:\nError: inner failure\n    at default (throw.js:2:45)"),
                "{details}"
            );
        }
        other => panic!("{other:?}"),
    }
}
