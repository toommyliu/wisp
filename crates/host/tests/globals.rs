mod common;

use common::*;
use lucent_protocol::script::ScriptOutcome;
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
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/globals.json")).unwrap();
    let observed = run_globals(&format!(
        r#"
        const cases = {};
        const out = [];
        if (typeof __host !== 'undefined') throw new Error('Private host is visible');
        for (let i = 0; i < cases.length; i++) {{
            try {{ out.push({{value: Function('return (' + cases[i].source + ')')()}}); }}
            catch (error) {{ out.push({{error: error.name}}); }}
            if (i % 20 === 19) yield* script.sleep(0);
        }}
        yield* script.stop(JSON.stringify(out));
        "#,
        serde_json::to_string(&cases).unwrap()
    ));
    let mismatches: Vec<_> = cases
        .iter()
        .enumerate()
        .filter(|(index, case)| observed[*index] != case["expected"])
        .map(|(index, case)| {
            format!(
                "case {index}: {} expected {} observed {}",
                case["source"], case["expected"], observed[index]
            )
        })
        .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    assert_eq!(observed.as_array().unwrap().len(), cases.len());
    eprintln!(
        "replayed {} Node globals cases through a script run",
        cases.len()
    );
}

fn run_globals(body: &str) -> Value {
    let (_host, mut session, _runtime) = ready_runtime();
    let source = format!(
        "import * as script from 'lucent/script'; export default function* () {{ {body} }}"
    );
    let outcome = start(
        &mut session,
        loose(&[("globals.js", &source)]),
        Default::default(),
    )
    .wait(TIMEOUT)
    .unwrap();
    match outcome {
        ScriptOutcome::StoppedByScript {
            reason: Some(reason),
        } => serde_json::from_str(&reason).unwrap(),
        other => panic!("unexpected outcome {other:?}"),
    }
}

#[test]
fn headers_iterate_three_thousand_names_in_one_script_turn() {
    let observed = run_globals(
        r#"
        const headers = new Headers();
        for (let i = 2999; i >= 0; i--) headers.set('x-' + String(i).padStart(4, '0'), String(i));
        let count = 0;
        let sum = 0;
        let previous = '';
        for (const [name, value] of headers) {
            if (name <= previous) throw new Error('Headers out of order');
            previous = name;
            count++;
            sum += Number(value);
        }
        yield* script.stop(JSON.stringify([count, sum, previous]));
    "#,
    );
    assert_eq!(observed, json!([3000, 4498500, "x-2999"]));
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
