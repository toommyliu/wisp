mod common;

use common::{TIMEOUT, logs::Logs};
use lucent_host::scripting::Runtime;
use lucent_host::{
    ScriptLink,
    bridge::{EventInterests, ScriptEvent},
    testing,
};
use lucent_protocol::{
    bridge::Value, operation::Operation, query::ItemSelector, script::ScriptOutcome,
};
use serde_json::json;
use std::{
    sync::Arc,
    task::{Wake, Waker},
    time::{Duration, Instant},
};

fn emit(link: &ScriptLink, data: serde_json::Value) {
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(json!({"type":"json","dataObj":data}).to_string()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}

fn next_trace(link: &ScriptLink) -> String {
    trace_within(link, TIMEOUT).expect("missing loader trace")
}

fn trace_within(link: &ScriptLink, wait: Duration) -> Option<String> {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    link.set_waker(Waker::from(Arc::new(Unpark(std::thread::current()))));
    let deadline = Instant::now() + wait;
    loop {
        match link.try_event().unwrap() {
            Some(ScriptEvent::Loader { event, .. }) if event.name == "onProbeAccept" => {
                let [Value::Number(id)] = event.args.as_slice() else {
                    panic!("{event:?}")
                };
                return Some(format!("accept {id}"));
            }
            Some(ScriptEvent::Loader { event, .. }) if event.name == "onExtensionResponse" => {
                let [Value::String(raw)] = event.args.as_slice() else {
                    panic!("{event:?}")
                };
                let packet: serde_json::Value = serde_json::from_str(raw).unwrap();
                if packet["dataObj"]["cmd"] == "getDrop" {
                    return Some(format!("packet {}", packet["dataObj"]["ItemID"]));
                }
            }
            Some(_) => {}
            None => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return None;
                }
                std::thread::park_timeout(remaining);
            }
        }
    }
}

fn accepting_session() -> (lucent_host::Session, Runtime, ScriptLink, ScriptLink) {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        common::ready_movie().accepting_drop(
            Duration::ZERO,
            json!({"type":"json","dataObj":{"cmd":"unrelated"}}).to_string(),
        ),
    );
    let runtime = Runtime::start(link).unwrap();
    let control = session.attach(EventInterests::ALL).unwrap();
    common::log_in_ready(&control);
    emit(
        &control,
        json!({"cmd":"dropItem","items":{
            "70":{"ItemID":70,"sName":"First","iQty":1},
            "71":{"ItemID":71,"sName":"Queued","iQty":1},
            "73":{"ItemID":73,"sName":"Next","iQty":1}
        }}),
    );
    let observer = session.attach(EventInterests::ALL).unwrap();
    (session, runtime, control, observer)
}

#[test]
fn a_fiber_timeout_removes_a_queued_accept_before_dispatch() {
    let logs = Logs::default();
    let _trace = tracing::dispatcher::set_default(&logs.dispatch());
    let (mut session, _runtime, control, observer) = accepting_session();
    control
        .operate(Operation::AcceptDrop {
            selector: ItemSelector::Id(70.0),
        })
        .unwrap();
    let mut trace = vec![next_trace(&observer)];
    let run = common::start(
        &mut session,
        common::loose(&[(
            "cancel.js",
            r#"
        import { Effect, Option } from "effect";
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            let answers = 0;
            const result = yield* api.drops.accept(71).pipe(
                Effect.tap(() => Effect.sync(() => { answers++; })),
                Effect.timeoutOption("20 millis"),
            );
            console.log("queued timeout " + Option.isNone(result));
            const next = yield* api.drops.accept(73);
            yield* script.stop(JSON.stringify([Option.isNone(result), answers, next]));
        }
    "#,
        )]),
        Default::default(),
    );
    logs.wait_for(tracing::Level::INFO, "queued timeout true");
    emit(
        &control,
        json!({"cmd":"getDrop","ItemID":70,"bSuccess":false}),
    );
    trace.push(next_trace(&observer));
    trace.push(next_trace(&observer));
    assert_eq!(trace, ["accept 70", "packet 70", "accept 73"]);
    emit(
        &control,
        json!({"cmd":"getDrop","ItemID":73,"bSuccess":false}),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("[true,0,false]".into())
        }
    );
}

#[test]
fn a_fiber_timeout_detaches_an_issued_accept_until_settlement() {
    let logs = Logs::default();
    let _trace = tracing::dispatcher::set_default(&logs.dispatch());
    let (mut session, _runtime, control, observer) = accepting_session();
    let run = common::start(
        &mut session,
        common::loose(&[(
            "cancel.js",
            r#"
        import { Effect, Option } from "effect";
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            let answers = 0;
            const result = yield* api.drops.accept(70).pipe(
                Effect.tap(() => Effect.sync(() => { answers++; })),
                Effect.timeoutOption("1 second"),
            );
            console.log("issued timeout " + Option.isNone(result));
            const next = yield* api.drops.accept(73);
            yield* script.stop(JSON.stringify([Option.isNone(result), answers, next]));
        }
    "#,
        )]),
        Default::default(),
    );
    let mut trace = vec![next_trace(&observer)];
    logs.wait_for(tracing::Level::INFO, "issued timeout true");
    emit(
        &control,
        json!({"cmd":"getDrop","ItemID":70,"bSuccess":false}),
    );
    trace.push(next_trace(&observer));
    trace.push(next_trace(&observer));
    assert_eq!(trace, ["accept 70", "packet 70", "accept 73"]);
    emit(
        &control,
        json!({"cmd":"getDrop","ItemID":73,"bSuccess":false}),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("[true,0,false]".into())
        }
    );
}

fn cancel_at_issuance(issued: bool) {
    let logs = Logs::default();
    let _trace = tracing::dispatcher::set_default(&logs.dispatch());
    let (mut session, _runtime, control, observer) = accepting_session();
    let (entered, waiting) = std::sync::mpsc::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let blocked = std::sync::Mutex::new(blocked);
    let once = std::sync::atomic::AtomicBool::new(false);
    control.set_operation_probe(move |_, committed| {
        if committed == issued && !once.swap(true, std::sync::atomic::Ordering::SeqCst) {
            entered.send(()).unwrap();
            blocked.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
        }
    });
    let run = common::start(
        &mut session,
        common::loose(&[(
            "race.js",
            r#"
        import { Effect } from "effect";
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            let answers = 0;
            const result = yield* Effect.raceFirst(
                api.drops.accept(70).pipe(Effect.tap(() => Effect.sync(() => { answers++; }))),
                api.packet.once({ direction: "extension", command: "cancelAccept", encoding: "json" }).pipe(
                    Effect.as("cancelled"),
                ),
            );
            console.log("issuance " + result);
            yield* api.drops.accept(73);
            yield* script.stop(JSON.stringify([result === "cancelled", answers]));
        }
    "#,
        )]),
        Default::default(),
    );
    waiting.recv_timeout(TIMEOUT).unwrap();
    emit(&control, json!({"cmd":"cancelAccept"}));
    logs.wait_for(tracing::Level::INFO, "issuance cancelled");
    let mut trace = vec![];
    if issued {
        trace.push(next_trace(&observer));
        release.send(()).unwrap();
        assert_eq!(trace_within(&observer, Duration::from_millis(300)), None);
        emit(
            &control,
            json!({"cmd":"getDrop","ItemID":70,"bSuccess":false}),
        );
        trace.push(next_trace(&observer));
    } else {
        release.send(()).unwrap();
    }
    trace.push(next_trace(&observer));
    assert_eq!(
        trace,
        if issued {
            vec!["accept 70", "packet 70", "accept 73"]
        } else {
            vec!["accept 73"]
        }
    );
    emit(
        &control,
        json!({"cmd":"getDrop","ItemID":73,"bSuccess":false}),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("[true,0]".into())
        }
    );
}

#[test]
fn cancellation_winning_the_issuance_commit_never_dispatches() {
    cancel_at_issuance(false);
}

#[test]
fn issuance_winning_cancellation_keeps_the_fifo_until_settlement() {
    cancel_at_issuance(true);
}

#[test]
fn stale_and_foreign_run_handles_do_not_cancel_another_recipient() {
    let (mut session, _runtime, control, observer) = accepting_session();
    let (handles, received) = std::sync::mpsc::channel();
    control.set_operation_probe(move |id, issued| {
        if issued {
            handles.send(id).unwrap();
        }
    });
    let first_link = session.attach(EventInterests::ALL).unwrap();
    let cancel_first = first_link.operation_canceller();
    let first = lucent_host::scripting::ScriptRun::start(
        first_link,
        common::loose(&[(
            "first.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () { yield* script.stop(String(yield* api.drops.accept(70))); }
    "#,
        )]),
        Default::default(),
    )
    .unwrap();
    let first_id = received.recv_timeout(TIMEOUT).unwrap();
    let mut trace = vec![next_trace(&observer)];
    let second_link = session.attach(EventInterests::ALL).unwrap();
    let cancel_second = second_link.operation_canceller();
    let second = lucent_host::scripting::ScriptRun::start(
        second_link,
        common::loose(&[(
            "second.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () { yield* script.stop(String(yield* api.drops.accept(73))); }
    "#,
        )]),
        Default::default(),
    )
    .unwrap();
    cancel_second(first_id);
    cancel_second(first_id);
    cancel_first(u64::MAX);
    emit(
        &control,
        json!({"cmd":"getDrop","ItemID":70,"bSuccess":false}),
    );
    trace.push(next_trace(&observer));
    trace.push(next_trace(&observer));
    let second_id = received.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(
        first.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("false".into())
        }
    );
    cancel_first(first_id);
    cancel_first(first_id);
    cancel_second(first_id);
    cancel_first(second_id);
    emit(
        &control,
        json!({"cmd":"getDrop","ItemID":73,"bSuccess":false}),
    );
    trace.push(next_trace(&observer));
    assert_eq!(
        second.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("false".into())
        }
    );
    assert_eq!(trace, ["accept 70", "packet 70", "accept 73", "packet 73"]);
}
