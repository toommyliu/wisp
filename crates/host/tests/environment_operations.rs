mod common;

use common::{TIMEOUT, logs::Logs};
use lucent_host::{
    ScriptLink, Session,
    bridge::{EventInterests, ScriptEvent, Value},
    scripting::{Kill, RuntimeError, ScriptRun},
    testing::{self, ProbeMovie},
};
use lucent_protocol::{operation::Operation, script::ScriptOutcome};
use serde_json::{Value as Json, json};
use std::{
    sync::Arc,
    task::Wake,
    time::{Duration, Instant},
};

fn next(link: &ScriptLink) -> ScriptEvent {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    link.set_waker(std::task::Waker::from(Arc::new(Unpark(
        std::thread::current(),
    ))));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(event) = link.try_event().unwrap() {
            return event;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "missing event");
        std::thread::park_timeout(remaining);
    }
}

fn call(link: &ScriptLink, method: &str) -> Vec<Value> {
    loop {
        if let ScriptEvent::Loader { event, .. } = next(link)
            && event.name == "onProbeCall"
            && event.args.first() == Some(&Value::String(method.into()))
        {
            return event.args;
        }
    }
}

fn operate(link: &ScriptLink, operation: Json) -> u64 {
    link.operate(serde_json::from_value::<Operation>(operation).unwrap())
        .unwrap()
}

fn answer(link: &ScriptLink, expected: u64) -> Json {
    loop {
        if let ScriptEvent::Operation { id, answer } = next(link) {
            assert_eq!(id, expected);
            return serde_json::to_value(answer.unwrap()).unwrap();
        }
    }
}

fn run(session: &mut Session, source: &str) -> ScriptRun {
    ScriptRun::start(
        session.attach(EventInterests::ALL).unwrap(),
        common::loose(&[("environment.js", source)]),
        Default::default(),
    )
    .unwrap()
}

fn quest_packet(ids: &[i64]) -> String {
    let quests: serde_json::Map<String, Json> = ids
        .iter()
        .map(|id| {
            (
                id.to_string(),
                json!({"QuestID":id,"sName":format!("Quest {id}")}),
            )
        })
        .collect();
    json!({"type":"json","dataObj":{"cmd":"getQuests","quests":quests}}).to_string()
}

struct Release(Option<std::sync::mpsc::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.send(());
        }
    }
}

fn hold_first_issue(observer: &ScriptLink) -> Release {
    let (release, held) = std::sync::mpsc::channel();
    let held = std::sync::Mutex::new(held);
    let once = std::sync::atomic::AtomicBool::new(false);
    observer.set_operation_probe(move |_, issued| {
        if issued && !once.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let _ = held.lock().unwrap().recv_timeout(TIMEOUT);
        }
    });
    Release(Some(release))
}

#[test]
fn env_quest_loads_share_spacing_across_script_runs() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie().responding(
            "quests.getMultiple",
            [
                (
                    json!(null),
                    vec![("onExtensionResponse".into(), quest_packet(&[10]))],
                ),
                (
                    json!(null),
                    vec![("onExtensionResponse".into(), quest_packet(&[11]))],
                ),
            ],
        ),
    );
    common::log_in(&observer);
    let first = run(
        &mut session,
        r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.quests.load(10, true); }
    "#,
    );
    call(&observer, "quests.getMultiple");
    let at = Instant::now();
    assert_eq!(first.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    let second = run(
        &mut session,
        r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.quests.load(11, true); }
    "#,
    );
    call(&observer, "quests.getMultiple");
    let spacing = at.elapsed();
    assert!(
        spacing >= Duration::from_millis(1400),
        "spacing reset with the run: {spacing:?}"
    );
    assert_eq!(second.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
}

#[test]
fn env_quest_load_returns_partial_flags_and_retries_only_missing_ids() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new().responding(
            "quests.loadMultiple",
            [
                (
                    json!(null),
                    vec![("onExtensionResponse".into(), quest_packet(&[10]))],
                ),
                (
                    json!(null),
                    vec![("onExtensionResponse".into(), quest_packet(&[11]))],
                ),
            ],
        ),
    );
    let at = Instant::now();
    let id = operate(
        &link,
        json!({"kind":"loadQuests","ids":[10,11,12,10,0,-1,1.5],"silent":false}),
    );
    assert_eq!(
        call(&link, "quests.loadMultiple")[1],
        Value::List(vec![Value::String("10,11,12".into())])
    );
    assert_eq!(
        call(&link, "quests.loadMultiple")[1],
        Value::List(vec![Value::String("11,12".into())])
    );
    assert_eq!(
        answer(&link, id),
        json!({"kind":"loadQuests","value":[true,true,false]})
    );
    eprintln!("partial quest load lane hold: {:?}", at.elapsed());
}

#[test]
fn env_bank_load_skips_hydrated_unless_forced() {
    let host = testing::headless_host();
    let (_session, link) =
        testing::start_probe(&host, common::ready_movie().recording("bank.loadItems"));
    let at = Instant::now();
    let first = operate(&link, json!({"kind":"loadBank","force":false}));
    call(&link, "bank.loadItems");
    assert_eq!(
        answer(&link, first),
        json!({"kind":"loadBank","value":true})
    );
    eprintln!("hydrating bank load lane hold: {:?}", at.elapsed());
    let skipped = operate(&link, json!({"kind":"loadBank","force":false}));
    loop {
        match next(&link) {
            ScriptEvent::Loader { event, .. } if event.name == "onProbeCall" => {
                panic!("hydrated load issued: {event:?}")
            }
            ScriptEvent::Operation { id, answer } => {
                assert_eq!(id, skipped);
                assert_eq!(
                    serde_json::to_value(answer.unwrap()).unwrap(),
                    json!({"kind":"loadBank","value":true})
                );
                break;
            }
            _ => {}
        }
    }
    let forced = operate(&link, json!({"kind":"loadBank","force":true}));
    call(&link, "bank.loadItems");
    assert_eq!(
        answer(&link, forced),
        json!({"kind":"loadBank","value":true})
    );
}

#[test]
fn env_bank_open_waits_until_the_loader_reports_open() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        common::ready_movie().recording("bank.open").responding(
            "bank.isOpen",
            [
                (json!(false), vec![]),
                (json!(false), vec![]),
                (json!(true), vec![]),
            ],
        ),
    );
    let id = operate(&link, json!({"kind":"openBank","force":false}));
    call(&link, "bank.isOpen");
    call(&link, "bank.open");
    call(&link, "bank.isOpen");
    call(&link, "bank.isOpen");
    assert_eq!(answer(&link, id), json!({"kind":"openBank","value":true}));
}

#[test]
fn env_killed_bank_caller_settles_before_the_next_load() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie()
            .recording("bank.loadItems")
            .responding(
                "bank.isLoaded",
                [(json!(false), vec![]), (json!(true), vec![])],
            ),
    );
    common::log_in(&observer);
    let release = hold_first_issue(&observer);
    let first = run(
        &mut session,
        r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.bank.load(true); }
    "#,
    );
    call(&observer, "bank.loadItems");
    first.kill();
    assert!(matches!(
        first.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    let next_id = operate(&observer, json!({"kind":"loadBank","force":true}));
    drop(release);
    let mut trace = vec![];
    loop {
        match next(&observer) {
            ScriptEvent::Loader { event, .. } if event.name == "onProbeCall" => {
                trace.push(event.args[0].clone());
            }
            ScriptEvent::Operation { id, answer } => {
                assert_eq!(id, next_id);
                assert!(answer.is_ok());
                break;
            }
            _ => {}
        }
    }
    assert_eq!(
        trace,
        [
            "bank.isLoaded",
            "bank.isLoaded",
            "bank.loadItems",
            "bank.isLoaded"
        ]
        .map(|s| Value::String(s.into()))
    );
}

fn cancelled_queued_load(interrupt: bool) {
    let logs = Logs::default();
    let _trace = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie()
            .recording("bank.loadItems")
            .responding(
                "quests.getMultiple",
                [(
                    json!(null),
                    vec![("onExtensionResponse".into(), quest_packet(&[99]))],
                )],
            ),
    );
    common::log_in(&observer);
    let release = hold_first_issue(&observer);
    let held = operate(&observer, json!({"kind":"loadBank","force":true}));
    call(&observer, "bank.loadItems");
    let source = if interrupt {
        r#"
        import { Effect } from "effect";
        import * as api from "lucent/api";
        export default function* () {
            const result = yield* Effect.raceFirst(
                api.bank.load(true),
                Effect.sync(() => "cancelled"),
            );
            if (result !== "cancelled") throw new Error("load escaped interruption");
            console.log("load cancelled");
        }
    "#
    } else {
        r#"
        import { Effect } from "effect";
        import * as api from "lucent/api";
        export default function* () {
            yield* api.quests.load(99, true).pipe(Effect.timeoutOption("20 millis"));
            console.log("load cancelled");
        }
    "#
    };
    let cancelled = run(&mut session, source);
    logs.wait_for(tracing::Level::INFO, "load cancelled");
    assert_eq!(cancelled.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    drop(release);
    assert_eq!(
        answer(&observer, held),
        json!({"kind":"loadBank","value":true})
    );
    let sentinel = operate(&observer, json!({"kind":"loadBank","force":true}));
    let mut bank_calls = 0;
    loop {
        match next(&observer) {
            ScriptEvent::Loader { event, .. } if event.name == "onProbeCall" => {
                assert_ne!(event.args[0], Value::String("quests.getMultiple".into()));
                if event.args[0] == Value::String("bank.loadItems".into()) {
                    bank_calls += 1;
                }
            }
            ScriptEvent::Operation { id, answer } => {
                assert_eq!(id, sentinel);
                assert!(answer.is_ok());
                break;
            }
            _ => {}
        }
    }
    assert_eq!(bank_calls, 1);
}

#[test]
fn env_fiber_timeout_cancels_a_queued_quest_load() {
    cancelled_queued_load(false);
}

#[test]
fn env_fiber_interruption_cancels_a_queued_bank_load() {
    cancelled_queued_load(true);
}

#[test]
fn env_queued_bank_load_does_not_cross_a_connection_change() {
    let host = testing::headless_host();
    let (_session, link) =
        testing::start_probe(&host, common::ready_movie().recording("bank.loadItems"));
    let release = hold_first_issue(&link);
    let held = operate(&link, json!({"kind":"loadBank","force":true}));
    call(&link, "bank.loadItems");
    let queued = operate(&link, json!({"kind":"loadBank","force":true}));
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onConnection".into()),
            Value::String("OnConnectionLost".into()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    drop(release);
    loop {
        if let ScriptEvent::Operation { id, answer } = next(&link) {
            assert_eq!(id, held);
            assert_eq!(answer, Err(lucent_host::bridge::Error::Stopped));
            break;
        }
    }
    loop {
        match next(&link) {
            ScriptEvent::Loader { event, .. } if event.name == "onProbeCall" => {
                panic!("stale load issued: {event:?}")
            }
            ScriptEvent::Operation { id, answer } => {
                assert_eq!(id, queued);
                assert_eq!(answer, Err(lucent_host::bridge::Error::Stopped));
                break;
            }
            _ => {}
        }
    }
}

#[test]
fn env_quest_load_chunks_thirty_ids_and_skips_cached_quests() {
    let host = testing::headless_host();
    let ids: Vec<i64> = (1..=31).collect();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new().responding(
            "quests.getMultiple",
            [
                (
                    json!(null),
                    vec![("onExtensionResponse".into(), quest_packet(&ids[..30]))],
                ),
                (
                    json!(null),
                    vec![("onExtensionResponse".into(), quest_packet(&ids[30..]))],
                ),
            ],
        ),
    );
    let id = operate(&link, json!({"kind":"loadQuests","ids":ids,"silent":true}));
    assert_eq!(
        call(&link, "quests.getMultiple")[1],
        Value::List(vec![Value::String(
            (1..=30)
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )])
    );
    assert_eq!(
        call(&link, "quests.getMultiple")[1],
        Value::List(vec![Value::String("31".into())])
    );
    assert_eq!(
        answer(&link, id),
        json!({"kind":"loadQuests","value":vec![true;31]})
    );
    let cached = operate(
        &link,
        json!({"kind":"loadQuests","ids":[31,31,0],"silent":true}),
    );
    loop {
        match next(&link) {
            ScriptEvent::Loader { event, .. } if event.name == "onProbeCall" => {
                panic!("cached quests reloaded: {event:?}")
            }
            ScriptEvent::Operation { id, answer } => {
                assert_eq!(id, cached);
                assert_eq!(
                    serde_json::to_value(answer.unwrap()).unwrap(),
                    json!({"kind":"loadQuests","value":[true]})
                );
                break;
            }
            _ => {}
        }
    }
}

#[test]
fn env_bank_timeout_releases_the_load_lane() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .constant("bank.isLoaded", json!(false))
            .recording("bank.loadItems"),
    );
    let at = Instant::now();
    let load = operate(&link, json!({"kind":"loadBank","force":true}));
    call(&link, "bank.loadItems");
    let sentinel = operate(&link, json!({"kind":"loadQuests","ids":[],"silent":true}));
    assert_eq!(
        answer(&link, load),
        json!({"kind":"loadBank","value":false})
    );
    let held = at.elapsed();
    eprintln!("timed out bank load lane hold: {held:?}");
    assert!(held >= Duration::from_secs(10));
    assert_eq!(
        answer(&link, sentinel),
        json!({"kind":"loadQuests","value":[]})
    );
}
