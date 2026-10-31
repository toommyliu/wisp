use std::time::Duration;

use lucent_host::scripting::Runtime;
use lucent_host::testing::{self, ProbeMovie};
use serde_json::{Value, json};

#[test]
fn abort_signals_and_console_work_in_the_runtime() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let observed: Value = runtime
        .eval(
            "globals.js",
            r#"
            (() => {
                const controller = new AbortController();
                const calls = [];
                controller.signal.addEventListener("abort", () => calls.push("first"));
                controller.signal.onabort = () => calls.push("onabort");
                controller.signal.addEventListener("abort", () => calls.push("last"), { once: true });
                controller.abort();
                controller.abort("ignored");
                let thrown;
                try { controller.signal.throwIfAborted(); } catch (error) { thrown = error.message; }
                const cycle = {};
                cycle.self = cycle;
                console.log({ count: 2 }, new Error("broken"), cycle);
                return {
                    aborted: controller.signal.aborted,
                    reason: { name: controller.signal.reason.name, message: controller.signal.reason.message },
                    calls,
                    thrown,
                    staticReason: AbortSignal.abort("finished").reason,
                    encoded: [...new TextEncoder().encode("é")],
                    logged: true,
                };
            })()
            "#,
        )
        .wait(Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        observed,
        json!({
            "aborted": true,
            "reason": { "name": "AbortError", "message": "This operation was aborted" },
            "calls": ["first", "onabort", "last"],
            "thrown": "This operation was aborted",
            "staticReason": "finished",
            "encoded": [195, 169],
            "logged": true,
        })
    );
}

#[test]
fn web_globals_are_available_in_the_runtime() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let observed: Value = runtime
        .eval(
            "globals.js",
            "[typeof URL, typeof URLSearchParams, typeof TextDecoder, typeof Headers]",
        )
        .wait(Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        observed,
        json!(["function", "function", "function", "function"])
    );
}

#[test]
fn web_globals_match_node_corpus() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let hidden: Value = runtime
        .eval(
            "hide.js",
            "Reflect.deleteProperty(globalThis, '__host') && typeof __host === 'undefined'",
        )
        .wait(Duration::from_secs(10))
        .unwrap();
    assert_eq!(hidden, json!(true));
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/globals.json")).unwrap();
    for (index, case) in cases.iter().enumerate() {
        let source = format!(
            "(() => {{ try {{ return {{value: ({})}}; }} catch (e) {{ return {{error: e.name}}; }} }})()",
            case["source"].as_str().unwrap()
        );
        let observed: Value = runtime
            .eval("corpus.js", source)
            .wait(Duration::from_secs(10))
            .unwrap();
        assert_eq!(
            observed, case["expected"],
            "case {index}: {}",
            case["source"]
        );
    }
    eprintln!("replayed {} Node globals cases", cases.len());
}

#[test]
fn text_decoder_rejects_unsupported_modes() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();
    let observed: Value = runtime
        .eval(
            "decoder.js",
            r#"[
                () => new TextDecoder('utf-16'),
                () => new TextDecoder('latin1'),
                () => new TextDecoder().decode(new Uint8Array(), {stream: true}),
            ].map(run => { try { run(); return 'accepted'; } catch (error) { return error.name; } })"#,
        )
        .wait(Duration::from_secs(10))
        .unwrap();
    assert_eq!(observed, json!(["RangeError", "RangeError", "TypeError"]));
}
