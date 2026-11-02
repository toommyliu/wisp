mod common;

use common::TIMEOUT;
use lucent_host::{
    ScriptLink, Session,
    bridge::{Error, EventInterests, ScriptEvent, Value},
    scripting::{Kill, RuntimeError, ScriptRun},
    testing,
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
    let deadline = Instant::now() + Duration::from_secs(40);
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

fn emit(link: &ScriptLink, name: &str, payload: String) {
    link.call_loader(
        "probe.emit",
        vec![Value::String(name.into()), Value::String(payload)],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}

fn packet(data: Json) -> String {
    json!({"type":"json","dataObj":data}).to_string()
}

fn run(session: &mut Session, source: &str) -> ScriptRun {
    ScriptRun::start(
        session.attach(EventInterests::ALL).unwrap(),
        common::loose(&[("probe.js", source)]),
        Default::default(),
    )
    .unwrap()
}

struct Release(Option<std::sync::mpsc::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.send(());
        }
    }
}

fn hold_first_issue(link: &ScriptLink) -> Release {
    let (release, held) = std::sync::mpsc::channel();
    let held = std::sync::Mutex::new(held);
    let once = std::sync::atomic::AtomicBool::new(false);
    link.set_operation_probe(move |_, issued| {
        if issued && !once.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let _ = held.lock().unwrap().recv_timeout(Duration::from_secs(40));
        }
    });
    Release(Some(release))
}

fn trace_until_answer(link: &ScriptLink, id: u64) -> (Vec<String>, Result<Json, Error>) {
    let mut trace = vec![];
    loop {
        match next(link) {
            ScriptEvent::Loader { event, .. } if event.name == "onProbeCall" => {
                trace.push(format!("{:?}", event.args));
            }
            ScriptEvent::Operation { id: got, answer } if got == id => {
                return (
                    trace,
                    answer.map(|answer| serde_json::to_value(answer).unwrap()),
                );
            }
            _ => {}
        }
    }
}

#[test]
fn env_connection_change_stops_an_in_flight_quest_load() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        common::ready_movie().responding("quests.getMultiple", [(json!(null), vec![])]),
    );
    let id = operate(&link, json!({"kind":"loadQuests","ids":[55],"silent":true}));
    call(&link, "quests.getMultiple");
    emit(&link, "onConnection", "OnConnectionLost".into());
    let (trace, answer) = trace_until_answer(&link, id);
    assert_eq!(answer, Err(Error::Stopped));
    assert!(
        trace
            .iter()
            .all(|call| !call.contains("quests.getMultiple"))
    );
}

fn killed_quest_load(silent: bool) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie()
            .constant("quests.getMultiple", json!(null))
            .recording("quests.getMultiple")
            .constant("quests.loadMultiple", json!(null))
            .recording("quests.loadMultiple"),
    );
    common::log_in(&observer);
    let release = hold_first_issue(&observer);
    let source = format!(
        r#"
        import * as api from "lucent/api";
        export default function* () {{ yield* api.quests.loadBatch(Array.from({{length:31}}, (_, i) => i + 1), {silent}); }}
    "#
    );
    let first = run(&mut session, &source);
    call(
        &observer,
        if silent {
            "quests.getMultiple"
        } else {
            "quests.loadMultiple"
        },
    );
    first.kill();
    assert!(matches!(
        first.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    let sentinel = operate(
        &observer,
        json!({"kind":"loadQuests","ids":[],"silent":true}),
    );
    drop(release);
    let (trace, answer) = trace_until_answer(&observer, sentinel);
    assert_eq!(answer, Ok(json!({"kind":"loadQuests","value":[]})));
    assert!(
        trace
            .iter()
            .all(|call| !call.contains("quests.getMultiple")
                && !call.contains("quests.loadMultiple")),
        "detached load retried or sent another chunk: {trace:?}"
    );
}

#[test]
fn env_killed_silent_quest_load_sends_no_retry_or_later_chunk() {
    killed_quest_load(true);
}

#[test]
fn env_killed_visible_quest_load_sends_no_retry_or_later_chunk() {
    killed_quest_load(false);
}

#[test]
fn env_killed_bank_open_confirms_the_load_without_opening() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie()
            .recording("bank.loadItems")
            .recording("bank.open")
            .responding(
                "bank.isLoaded",
                [(json!(false), vec![]), (json!(true), vec![])],
            )
            .responding("bank.isOpen", [(json!(false), vec![])]),
    );
    common::log_in(&observer);
    let release = hold_first_issue(&observer);
    let first = run(
        &mut session,
        r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.bank.open({force:true}); }
    "#,
    );
    call(&observer, "bank.loadItems");
    first.kill();
    assert!(matches!(
        first.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    let sentinel = operate(
        &observer,
        json!({"kind":"loadQuests","ids":[],"silent":true}),
    );
    drop(release);
    let (trace, answer) = trace_until_answer(&observer, sentinel);
    assert_eq!(answer, Ok(json!({"kind":"loadQuests","value":[]})));
    assert_eq!(
        trace
            .iter()
            .filter(|call| call.contains("bank.isLoaded"))
            .count(),
        2,
        "issued load was not confirmed: {trace:?}"
    );
    assert!(
        trace.iter().all(|call| !call.contains("bank.open")),
        "killed caller opened bank: {trace:?}"
    );
}

#[test]
fn env_script_equip_and_drop_accept_complete_while_a_bank_load_is_held() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .recording("bank.loadItems")
            .constant("world.isActionAvailable", json!(true))
            .constant("player.isMember", json!(true))
            .constant("combat.getConsumableSkillItem", json!({"itemId":100}))
            .responding(
                "inventory.equip",
                [(
                    json!(true),
                    vec![(
                        "onExtensionResponse".into(),
                        packet(json!({"cmd":"seia","iRes":1,"o":{"id":400}})),
                    )],
                )],
            )
            .accepting_drop(
                Duration::ZERO,
                packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":2})),
            ),
    );
    common::log_in(&link);
    emit(
        &link,
        "onExtensionResponse",
        packet(
            json!({"cmd":"loadInventoryBig","hitems":[],"items":[{"ItemID":100,"sName":"Potion","sType":"Item","sLink":"potion","sMeta":"400","iQty":2}]}),
        ),
    );
    emit(
        &link,
        "onExtensionResponse",
        packet(
            json!({"cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Dropped","sType":"Item","iQty":2}}}),
        ),
    );
    let release = hold_first_issue(&link);
    let load = operate(&link, json!({"kind":"loadBank","force":true}));
    call(&link, "bank.loadItems");
    let script = run(
        &mut session,
        r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () { yield* script.stop(String(yield* api.inventory.equip(100))); }
    "#,
    );
    let outcome = script.wait(TIMEOUT);
    assert_eq!(
        outcome.unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
    let accept = operate(&link, json!({"kind":"acceptDrop","selector":70}));
    let (trace, answer) = trace_until_answer(&link, accept);
    drop(release);
    assert_eq!(answer, Ok(json!({"kind":"acceptDrop","value":true})));
    assert!(
        trace.iter().any(|call| call.contains("inventory.equip")),
        "no script equip: {trace:?}"
    );
    assert_eq!(
        trace_until_answer(&link, load).1,
        Ok(json!({"kind":"loadBank","value":true}))
    );
}
#[test]
fn env_profile_equips_before_fighting_while_a_quest_load_is_held() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .combat_facts()
            .constant("world.isActionAvailable", json!(true))
            .constant("world.getAvailableMonsterMapIds", json!([2]))
            .recording("world.getAvailableMonsterMapIds")
            .constant("combat.attackMonster", json!(true))
            .responding(
                "combat.getConsumableSkillItem",
                [(json!(null), vec![]), (json!({"itemId":100}), vec![])],
            )
            .responding(
                "inventory.equip",
                [(
                    json!(true),
                    vec![(
                        "onExtensionResponse".into(),
                        packet(json!({"cmd":"seia","iRes":1,"o":{"id":400}})),
                    )],
                )],
            )
            .responding("quests.getMultiple", [(json!(null), vec![])]),
    );
    let runtime = lucent_host::scripting::Runtime::start(link).unwrap();
    common::ready(&runtime);
    let control = session.attach(EventInterests::ALL).unwrap();
    emit(
        &control,
        "onExtensionResponse",
        packet(
            json!({"cmd":"loadInventoryBig","hitems":[{"ItemID":2,"sName":"Home","sType":"House","bEquip":1}],"items":[
        {"ItemID":100,"sName":"Potion","sType":"Item","sLink":"potion","sMeta":"400","iQty":2}]}),
        ),
    );
    emit(
        &control,
        "onExtensionResponse",
        packet(json!({"cmd":"moveToArea","areaId":1,"areaName":"test-1",
        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],
        "mondef":[{"MonID":1,"strMonName":"Frogzard"}],
        "monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50}]})),
    );
    let _supervisor = lucent_host::Supervisor::start(&mut session, None).unwrap();
    let trace = session.attach(EventInterests::ALL).unwrap();
    let release = hold_first_issue(&control);
    let quest = operate(
        &control,
        json!({"kind":"loadQuests","ids":[77],"silent":true}),
    );
    call(&trace, "quests.getMultiple");
    let fighter_observer = session.attach(EventInterests::ALL).unwrap();
    let fighter = common::start(
        &mut session,
        common::loose(&[(
            "fight.js",
            r#"
        import { Effect } from "effect";
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* api.packet.sendToClient("profile");
            yield* Effect.raceFirst(
                api.combat.kill("Frogzard", {profile:{steps:[{skill:5}],consumable:"Potion"}}),
                api.packet.once({command:"release",direction:"extension",encoding:"json"}, {timeout:"1 hour"}),
            );
            yield* script.sleep("1 hour");
        }
    "#,
        )]),
        Default::default(),
    );
    common::await_event(&fighter_observer, "onDebug", Some(&fighter));
    loop {
        let event = common::await_event(&fighter_observer, "onProbeCall", Some(&fighter));
        if event.args.first() == Some(&Value::String("world.getAvailableMonsterMapIds".into())) {
            break;
        }
    }
    let mut equips = 0;
    while let Some(event) = trace.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && event.args.first() == Some(&Value::String("inventory.equip".into()))
        {
            equips += 1;
        }
    }
    fighter.kill();
    let _ = fighter.wait(TIMEOUT);
    drop(release);
    emit(
        &control,
        "onExtensionResponse",
        packet(json!({"cmd":"getQuests","quests":{"77":{"QuestID":77,"sName":"Loaded"}}})),
    );
    assert_eq!(
        trace_until_answer(&control, quest).1,
        Ok(json!({"kind":"loadQuests","value":[true]}))
    );
    assert_eq!(equips, 1, "profile fought without equipping behind a load");
}
