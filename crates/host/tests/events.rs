mod common;

use lucent_host::bridge::EventInterests;
use std::time::Duration;

use lucent_host::scripting::{Kill, Runtime, RuntimeError};
use lucent_host::testing::{self, ProbeMovie};
use lucent_host::{ViewEvent, Viewport};
use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn logout_reaches_a_fresh_runtime_on_a_logged_in_session() {
    for kill_previous in [false, true] {
        let host = testing::headless_host();
        let (mut session, link) = testing::start_probe(&host, common::ready_movie());
        let runtime = Runtime::start(link).unwrap();
        runtime
            .eval(
                "connect.js",
                r#"window.swf["probe.emit"]("onConnection", "OnConnection"); null"#,
            )
            .wait::<()>(TIMEOUT)
            .unwrap();
        common::ready(&runtime);
        session
            .attach(EventInterests::new(&[]))
            .unwrap()
            .command(lucent_protocol::command::Command::RefreshLoggedIn {})
            .recv_timeout(TIMEOUT)
            .unwrap();
        common::completed(&mut session);
        common::ready(&runtime);
        if kill_previous {
            assert!(matches!(
                runtime
                    .eval("runaway.js", "while (true) {}")
                    .wait::<Value>(TIMEOUT),
                Err(RuntimeError::Killed(Kill::Watchdog))
            ));
        }
        drop(runtime);
        let runtime = Runtime::start(session.attach(EventInterests::ALL).unwrap()).unwrap();
        let observed: Value = runtime
            .eval(
                "logout.js",
                r#"
            new Promise(resolve => {
                const loggedIn = JSON.parse(__host.read('{"kind":"auth"}')).loggedIn;
                const seen = [];
                const onConnection = window.onConnection;
                const onProjectionEvent = window.onProjectionEvent;
                window.onConnection = status => {
                    onConnection(status);
                    seen.push(status);
                };
                window.onProjectionEvent = event => {
                    onProjectionEvent(event);
                    seen.push(event.type);
                };
                window.onDebug = () => resolve({ loggedIn, seen });
                window.swf["probe.emit"]("onConnection", "OnConnectionLost");
                window.swf["probe.emit"]("onDebug", "after");
            })
        "#,
            )
            .wait(TIMEOUT)
            .unwrap();
        assert_eq!(
            observed,
            json!({"loggedIn":true,"seen":["OnConnectionLost","logout"]}),
            "kill_previous={kill_previous}"
        );
    }
}

#[test]
fn a_loader_callback_reaches_scripts_before_the_projection_events_it_produced() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();

    let seen: Value = runtime
        .eval(
            "order.js",
            r#"
            new Promise(resolve => {
                const seen = [];
                window.onExtensionResponse = raw => seen.push(["onExtensionResponse", JSON.parse(raw).dataObj.cmd]);
                window.onProjectionEvent = event => seen.push([event.type]);
                window.onDebug = message => {
                    seen.push(["onDebug", message]);
                    resolve(seen);
                };
                window.swf["probe.emit"]("onExtensionResponse", JSON.stringify({
                    type: "json",
                    dataObj: { cmd: "moveToArea", areaName: "testroom-1", uoBranch: [{ entID: 7, uoName: "player-a", intHP: 100 }] },
                }));
                window.swf["probe.emit"]("onDebug", "after");
            })
            "#,
        )
        .wait(TIMEOUT)
        .unwrap();

    assert_eq!(
        seen,
        json!([
            ["onExtensionResponse", "moveToArea"],
            ["join-map"],
            ["players-changed"],
            ["onDebug", "after"],
        ])
    );
}

#[test]
fn event_overflow_fails_the_active_script_and_the_runtime_keeps_running() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();

    let error = runtime
        .eval(
            "flood.js",
            r#"
            for (let index = 0; index < 1100; index++) {
                window.swf["probe.emit"]("onDebug", String(index));
            }
            new Promise(() => {});
            "#,
        )
        .wait::<Value>(TIMEOUT)
        .unwrap_err();
    assert!(matches!(error, RuntimeError::EventsLost), "{error}");

    let delivered: String = runtime
        .eval(
            "after.js",
            r#"
            new Promise(resolve => {
                window.onDebug = resolve;
                window.swf["probe.emit"]("onDebug", "after");
            })
            "#,
        )
        .wait(TIMEOUT)
        .unwrap();
    assert_eq!(delivered, "after");
}

#[test]
fn restart_stops_the_runtime_on_the_previous_link() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let active = runtime.eval("wait.js", "new Promise(() => {})");

    session.restart(testing::headless_surface()).unwrap();

    assert!(matches!(
        active.wait::<Value>(TIMEOUT),
        Err(RuntimeError::Stopped)
    ));
    assert!(matches!(
        runtime.eval("after.js", "1").wait::<i64>(TIMEOUT),
        Err(RuntimeError::Stopped)
    ));

    let runtime = Runtime::start(session.attach(EventInterests::ALL).unwrap()).unwrap();
    let echoed = runtime.eval(
        "loaded.js",
        r#"new Promise(resolve => { window.onLoaded = () => resolve(window.swf["probe.echo"](42)); })"#,
    );
    session.send(ViewEvent::Resized(
        Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    assert_eq!(echoed.wait::<i64>(TIMEOUT).unwrap(), 42);
}

#[test]
fn timers_fire_in_deadline_order_without_starving_commands() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    runtime
        .eval(
            "spin.js",
            "globalThis.spins = 0; setInterval(() => spins++, 0); undefined",
        )
        .wait::<()>(TIMEOUT)
        .unwrap();

    let observed: Value = runtime
        .eval(
            "timers.js",
            r#"
            new Promise(resolve => {
                const fired = [];
                const started = __host.now();
                setTimeout(() => fired.push("late"), 40);
                const cancelled = setTimeout(() => fired.push("cancelled"), 10);
                setTimeout(label => fired.push(label), 10, "early");
                clearTimeout(cancelled);
                setTimeout(() => resolve({ fired, waited: __host.now() - started >= 60, spun: spins > 0 }), 60);
            })
            "#,
        )
        .wait(TIMEOUT)
        .unwrap();

    assert_eq!(
        observed,
        json!({ "fired": ["early", "late"], "waited": true, "spun": true })
    );
}

#[test]
fn a_script_settled_after_events_were_lost_still_fails() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let waiting = runtime.eval(
        "wait.js",
        "new Promise(resolve => { globalThis.finish = resolve; })",
    );
    runtime
        .eval(
            "flood.js",
            r#"
            for (let index = 0; index < 1100; index++) {
                window.swf["probe.emit"]("onDebug", String(index));
            }
            finish("done");
            "#,
        )
        .wait::<()>(TIMEOUT)
        .unwrap();
    let error = waiting.wait::<Value>(TIMEOUT).unwrap_err();
    assert!(matches!(error, RuntimeError::EventsLost), "{error}");
}

#[test]
fn a_script_whose_result_loses_events_while_serializing_fails() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let waiting = runtime.eval(
        "wait.js",
        "new Promise(resolve => { globalThis.finish = resolve; })",
    );
    runtime
        .eval(
            "finish.js",
            r#"
            finish({
                toJSON() {
                    for (let index = 0; index < 1100; index++) {
                        window.swf["probe.emit"]("onDebug", String(index));
                    }
                    return "done";
                },
            });
            "#,
        )
        .wait::<()>(TIMEOUT)
        .unwrap();
    let error = waiting.wait::<Value>(TIMEOUT).unwrap_err();
    assert!(matches!(error, RuntimeError::EventsLost), "{error}");
}

#[test]
fn a_throwing_turn_delivers_settled_results_and_logs_rejections() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let pending = runtime.eval(
        "throwing-turn.js",
        r#"
        new Promise(resolve => {
            setTimeout(() => {
                const filter = Array.prototype.filter;
                Array.prototype.filter = function () {
                    Array.prototype.filter = filter;
                    resolve("settled during throw");
                    Promise.reject(new Error("rejection during throw"));
                    throw new Error("one broken filter");
                };
                setTimeout(() => {}, 20);
            }, 20);
        });
    "#,
    );
    assert_eq!(
        pending.wait::<String>(Duration::from_secs(3)).unwrap(),
        "settled during throw"
    );
    let errors = logs.messages(tracing::Level::ERROR);
    assert!(
        errors
            .iter()
            .any(|text| text == "unhandled script rejection"),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|text| text == "script turn failed"),
        "{errors:?}"
    );
}
