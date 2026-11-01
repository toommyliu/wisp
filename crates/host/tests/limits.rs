mod common;

use lucent_host::bridge::EventInterests;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use lucent_host::Session;
use lucent_host::scripting::{Kill, PendingEval, Runtime, RuntimeError};
use lucent_host::testing::{self, ProbeMovie};
use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(10);
const STOP_LIMIT: Duration = Duration::from_millis(250);

#[test]
fn a_turn_waiting_off_cpu_keeps_its_cpu_budget() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    assert_eq!(
        runtime
            .eval(
                "wait.js",
                "Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 300)",
            )
            .wait::<String>(TIMEOUT)
            .unwrap(),
        "timed-out"
    );
}

#[test]
fn a_turn_waiting_without_cpu_is_killed_by_the_wall_backstop() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let began = Instant::now();
    let error = runtime
        .eval(
            "wait.js",
            "Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0)",
        )
        .wait::<Value>(TIMEOUT)
        .unwrap_err();
    assert!(began.elapsed() >= Duration::from_secs(5));
    assert!(
        matches!(error, RuntimeError::Killed(Kill::WallBackstop)),
        "{error}"
    );
    assert!(error.to_string().contains("wall-clock backstop"), "{error}");
}

fn stall_player(session: &Session) -> mpsc::Sender<()> {
    let (entered, stalled) = mpsc::channel();
    let (release, held) = mpsc::channel::<()>();
    let _ = session.read_projection(move |_| {
        let _ = entered.send(());
        let _ = held.recv();
    });
    stalled
        .recv_timeout(TIMEOUT)
        .expect("the player did not stall");
    release
}

fn let_the_script_reach_the_player() {
    thread::sleep(Duration::from_millis(100));
}

fn listen(runtime: &Runtime) -> PendingEval {
    runtime.eval(
        "listen.js",
        r#"
        new Promise(resolve => {
            const heard = [typeof globalThis.marker];
            window.onDebug = message => {
                heard.push(message);
                if (message === "after") resolve(heard);
            };
        })
        "#,
    )
}

fn emit_after(runtime: &Runtime) {
    assert!(
        runtime
            .eval(
                "after.js",
                r#"window.swf["probe.emit"]("onDebug", "after")"#
            )
            .wait::<bool>(TIMEOUT)
            .unwrap()
    );
}

#[test]
fn a_runaway_turn_is_killed_within_250_ms_of_cpu_and_a_new_runtime_works() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, ProbeMovie::new());
    drop(link);
    for source in [
        "while (true) {}",
        "while (true) { try { while (true) {} } catch {} finally { while (true) {} } }",
        "new Promise(() => setTimeout(() => { while (true) {} }, 0))",
        "(function again() { Promise.resolve().then(again); })()",
        "Promise.resolve().then(() => { while (true) {} }); while (true) {}",
        "for (let i = 0; i < 2; i++) Promise.reject({ get stack() { while (true) {} } })",
    ] {
        let runtime = Runtime::start(session.attach(EventInterests::ALL).unwrap()).unwrap();
        let waiting = runtime.eval("wait.js", "globalThis.marker = 1; new Promise(() => {})");
        let runaway = runtime.eval("runaway.js", source);
        let error = runaway.wait::<Value>(TIMEOUT).unwrap_err();
        assert!(
            matches!(error, RuntimeError::Killed(Kill::Watchdog)),
            "{source}: {error}"
        );
        let error = waiting.wait::<Value>(TIMEOUT).unwrap_err();
        assert!(
            matches!(error, RuntimeError::Killed(Kill::Watchdog)),
            "{source}: {error}"
        );

        #[cfg(feature = "testing")]
        let runtime = {
            let mut runtime = runtime;
            let cpu = runtime
                .join_cpu_time()
                .expect("the killed turn had no CPU measurement");
            assert!(
                cpu >= Duration::from_millis(200),
                "{source}: stopped after {cpu:?} CPU"
            );
            assert!(cpu < STOP_LIMIT, "{source}: stopped after {cpu:?} CPU");
            runtime
        };
        drop(runtime);
        assert_eq!(
            session.attached_link_count().recv_timeout(TIMEOUT).unwrap(),
            0
        );
        let runtime = Runtime::start(session.attach(EventInterests::ALL).unwrap()).unwrap();
        let heard = listen(&runtime);
        emit_after(&runtime);
        assert_eq!(
            heard.wait::<Value>(TIMEOUT).unwrap(),
            json!(["undefined", "after"]),
            "{source}"
        );
    }
}

#[test]
fn a_script_blocked_in_a_loader_call_is_killed_without_waiting_for_the_reply() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let release = stall_player(&session);
    let blocked = runtime.eval(
        "blocked.js",
        r#"globalThis.marker = 1; window.swf["probe.emit"]("onDebug", "issued")"#,
    );
    let_the_script_reach_the_player();

    let killed = Instant::now();
    runtime.kill();
    let error = blocked.wait::<Value>(TIMEOUT).unwrap_err();
    let elapsed = killed.elapsed();
    assert!(
        matches!(error, RuntimeError::Killed(Kill::Requested)),
        "{error}"
    );
    assert!(elapsed < STOP_LIMIT, "stopped after {elapsed:?}");

    drop(runtime);
    drop(release);
    let runtime = Runtime::start(session.attach(EventInterests::ALL).unwrap()).unwrap();
    drop(runtime);
    let runtime = Runtime::start(session.attach(EventInterests::ALL).unwrap()).unwrap();
    let heard = listen(&runtime);
    emit_after(&runtime);
    assert_eq!(
        heard.wait::<Value>(TIMEOUT).unwrap(),
        json!(["undefined", "after"])
    );
}

#[test]
fn a_script_that_catches_out_of_memory_is_killed_at_the_heap_limit() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let error = runtime
        .eval(
            "hoard.js",
            r#"
            globalThis.marker = 1;
            const hoard = [];
            for (;;) {
                try { hoard.push(new Array(100000).fill(1.5)); } catch {}
            }
            "#,
        )
        .wait::<Value>(TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(error, RuntimeError::Killed(Kill::HeapLimit)),
        "{error}"
    );

    drop(runtime);
    let runtime = Runtime::start(session.attach(EventInterests::ALL).unwrap()).unwrap();
    let heard = listen(&runtime);
    emit_after(&runtime);
    assert_eq!(
        heard.wait::<Value>(TIMEOUT).unwrap(),
        json!(["undefined", "after"])
    );
}

#[test]
fn dropping_a_runtime_blocked_in_a_loader_call_returns_promptly() {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let release = stall_player(&session);
    let blocked = runtime.eval(
        "blocked.js",
        r#"
        window.swf["probe.emit"]("onExtensionResponse", JSON.stringify({
            type: "json",
            dataObj: { cmd: "moveToArea", areaName: "issued-1", uoBranch: [] },
        }))
        "#,
    );
    let_the_script_reach_the_player();

    let dropped = Instant::now();
    drop(runtime);
    let elapsed = dropped.elapsed();
    assert!(elapsed < STOP_LIMIT, "dropped after {elapsed:?}");
    assert!(matches!(
        blocked.wait::<Value>(TIMEOUT),
        Err(RuntimeError::Stopped)
    ));

    drop(release);
    let map = session
        .read_projection(|projection| projection.world.map.clone())
        .recv_timeout(TIMEOUT)
        .unwrap();
    assert_eq!(map.name, "issued");
}

#[test]
fn a_killed_script_runs_none_of_its_queued_callbacks() {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let join = |area: &str| {
        format!(
            r#"window.swf["probe.emit"]("onExtensionResponse", JSON.stringify({{
                type: "json",
                dataObj: {{ cmd: "moveToArea", areaName: "{area}", uoBranch: [] }},
            }}))"#
        )
    };
    assert!(
        runtime
            .eval("before.js", join("before-1"))
            .wait::<bool>(TIMEOUT)
            .unwrap()
    );

    let error = runtime
        .eval(
            "leak.js",
            format!(
                "Promise.resolve().then(() => {}); while (true) {{}}",
                join("leaked-1")
            ),
        )
        .wait::<Value>(TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(error, RuntimeError::Killed(Kill::Watchdog)),
        "{error}"
    );

    let map = session
        .read_projection(|projection| projection.world.map.clone())
        .recv_timeout(TIMEOUT)
        .unwrap();
    assert_eq!(map.name, "before");
}

#[test]
fn evals_sent_before_a_kill_fail_with_it_and_the_runtime_ends() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let runaway = runtime.eval("runaway.js", "globalThis.marker = 1; while (true) {}");
    let queued = runtime.eval("queued.js", "typeof globalThis.marker");
    for pending in [runaway, queued] {
        let error = pending.wait::<Value>(TIMEOUT).unwrap_err();
        assert!(
            matches!(error, RuntimeError::Killed(Kill::Watchdog)),
            "{error}"
        );
    }
    assert!(matches!(
        runtime.eval("ended.js", "42").wait::<i64>(TIMEOUT),
        Err(RuntimeError::Stopped)
    ));
    drop(runtime);
    let runtime = Runtime::start(session.attach(EventInterests::ALL).unwrap()).unwrap();
    assert_eq!(
        runtime
            .eval("after.js", "typeof globalThis.marker")
            .wait::<String>(TIMEOUT)
            .unwrap(),
        "undefined"
    );
}

#[test]
fn killed_runs_use_less_than_250_ms_of_cpu_and_drop_their_threads() {
    if std::env::var_os("LUCENT_RUN_LIMIT_CHILD").is_none() {
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "killed_runs_use_less_than_250_ms_of_cpu_and_drop_their_threads"
                ])
                .env("LUCENT_RUN_LIMIT_CHILD", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let (_host, mut session, _runtime) = common::ready_runtime();
    #[cfg(target_os = "macos")]
    let before = common::js_threads();
    for (body, expected) in [
        ("while (true) {}", Kill::Watchdog),
        (
            "const hoard = []; for (;;) { try { hoard.push(new Array(100000).fill(1.5)); } catch {} }",
            Kill::HeapLimit,
        ),
    ] {
        let run = common::started_run(
            &mut session,
            &format!(
                r#"
            import * as api from "lucent/api";
            export default function* () {{
                yield* api.packet.sendToClient("started");
                {body}
            }}
        "#
            ),
        );
        assert!(matches!(run.wait(TIMEOUT), Err(RuntimeError::Killed(kill)) if kill == expected));
        #[cfg(feature = "testing")]
        let run = {
            let mut run = run;
            let cpu = run
                .join_cpu_time()
                .expect("the killed turn had no CPU measurement");
            assert!(cpu < STOP_LIMIT, "{expected}: stopped after {cpu:?} CPU");
            run
        };
        drop(run);
        assert_eq!(
            session.attached_link_count().recv_timeout(TIMEOUT).unwrap(),
            1
        );
        #[cfg(target_os = "macos")]
        {
            let deadline = Instant::now() + TIMEOUT;
            loop {
                let threads = common::js_threads();
                if threads == before {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "threads after drop: {threads:?}, before: {before:?}"
                );
                thread::yield_now();
            }
        }
        common::completed(&mut session);
    }
}
