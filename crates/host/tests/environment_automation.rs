mod common;
use common::*;
use lucent_host::{
    Supervisor,
    bridge::{ScriptEvent, Value},
    testing,
};
use serde_json::json;
use std::{
    sync::Arc,
    task::{Wake, Waker},
    thread,
    time::Instant,
};

struct Notify(thread::Thread);
impl Wake for Notify {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

#[test]
fn registered_quests_load_without_a_script_while_settings_continue() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        quest_movie()
            .constant("quests.isInProgress", json!(false))
            .constant("quests.isAvailable", json!(true))
            .constant("world.isActionAvailable", json!(true))
            .responding("quests.accept", [(json!(true), vec![])])
            .recording("settings.setWalkSpeed"),
    );
    become_ready(&link);
    link.command(serde_json::from_value(json!({"kind":"refreshLoggedIn"})).unwrap())
        .recv_timeout(TIMEOUT)
        .unwrap();
    link.command(serde_json::from_value(json!({"kind":"updateEnvironment", "edit":{"kind":"addQuest","questId":802,"rewardItemId":5586}})).unwrap()).recv_timeout(TIMEOUT).unwrap();
    link.set_waker(Waker::from(Arc::new(Notify(thread::current()))));
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(4);
    let mut loaded = false;
    let mut settings = 0;
    let mut accepted = false;
    loop {
        while let Some(event) = link.try_event().unwrap() {
            if let ScriptEvent::Loader { event, .. } = event {
                if event.name != "onProbeCall" {
                    continue;
                }
                let name = event.args.first();
                if name == Some(&Value::String("quests.getMultiple".into())) {
                    loaded = true;
                }
                if accepted && name == Some(&Value::String("settings.setWalkSpeed".into())) {
                    settings += 1;
                }
                if name == Some(&Value::String("quests.accept".into())) {
                    assert!(loaded);
                    accepted = true;
                }
            }
        }
        if accepted && settings >= 2 {
            return;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(
            !left.is_zero(),
            "native quest automation did not accept or continue settings: accepted={accepted}, settings={settings}"
        );
        thread::park_timeout(left);
    }
}

fn emit(link: &lucent_host::ScriptLink, packet: serde_json::Value) {
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(json!({"type":"json","dataObj":packet}).to_string()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}
fn edit(link: &lucent_host::ScriptLink, edit: serde_json::Value) {
    link.command(serde_json::from_value(json!({"kind":"updateEnvironment","edit":edit})).unwrap())
        .recv_timeout(TIMEOUT)
        .unwrap();
}
fn next_call(link: &lucent_host::ScriptLink, method: &str) -> Vec<Value> {
    link.set_waker(Waker::from(Arc::new(Notify(thread::current()))));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        while let Some(event) = link.try_event().unwrap() {
            if let ScriptEvent::Loader { event, .. } = event
                && event.name == "onProbeCall"
                && event.args.first() == Some(&Value::String(method.into()))
            {
                return event.args;
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "no call to {method}");
        thread::park_timeout(left);
    }
}
#[test]
fn native_quest_turn_in_excludes_a_script_and_registers_late_targets() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        quest_movie()
            .constant("quests.isInProgress", json!(true))
            .constant("quests.canComplete", json!(true))
            .constant("world.isActionAvailable", json!(true))
            .constant("quests.getMaxTurnIns", json!(3))
            .responding("quests.complete", [(json!(null), vec![])]),
    );
    become_ready(&link);
    link.command(serde_json::from_value(json!({"kind":"refreshLoggedIn"})).unwrap())
        .recv_timeout(TIMEOUT)
        .unwrap();
    edit(
        &link,
        json!({"kind":"setQuestAutoRegister","requirements":true,"rewards":true}),
    );
    edit(
        &link,
        json!({"kind":"addQuest","questId":802,"rewardItemId":5586}),
    );
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    assert_eq!(
        next_call(&link, "quests.complete")[1],
        Value::List(vec![
            Value::Number(802.),
            Value::Number(3.),
            Value::Number(5586.),
            Value::Bool(false)
        ])
    );
    let state: serde_json::Value = serde_json::from_str(
        &link
            .query(lucent_protocol::query::Query::Environment {})
            .unwrap(),
    )
    .unwrap();
    assert_eq!(state["itemNames"], json!(["Elders Blood"]));
    let run = start(
        &mut session,
        loose(&[(
            "race.js",
            r#"
        import {quests} from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            if (yield* quests.complete(802)) throw new Error("overlapping turn-in");
            yield* script.stop("quest was held");
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        lucent_protocol::script::ScriptOutcome::StoppedByScript {
            reason: Some("quest was held".into())
        }
    );
    let mut duplicates = 0;
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && event.args.first() == Some(&Value::String("quests.complete".into()))
        {
            duplicates += 1;
        }
    }
    assert_eq!(duplicates, 0);
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onConnection".into()),
            Value::String("OnConnectionLost".into()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    assert!(link.read(|p| p.quests.all().is_empty()).unwrap());
    link.command(serde_json::from_value(json!({"kind":"refreshLoggedIn"})).unwrap())
        .recv_timeout(TIMEOUT)
        .unwrap();
    next_call(&link, "quests.getMultiple");
    next_call(&link, "quests.complete");
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    assert_eq!(
        link.call_loader("quests.complete", vec![Value::Number(802.)])
            .recv_timeout(TIMEOUT),
        Err(lucent_host::bridge::Error::Stopped)
    );
    let current = session
        .attach(lucent_host::bridge::EventInterests::ALL)
        .unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100., 100., 1.).unwrap(),
    ));
    testing::wait_loaded(&current);
    let state: serde_json::Value = serde_json::from_str(
        &current
            .query(lucent_protocol::query::Query::Environment {})
            .unwrap(),
    )
    .unwrap();
    assert_eq!(state["questIds"], json!([802.]));
    assert_eq!(state["itemNames"], json!(["Elders Blood"]));
    become_ready(&current);
    current
        .command(serde_json::from_value(json!({"kind":"refreshLoggedIn"})).unwrap())
        .recv_timeout(TIMEOUT)
        .unwrap();
    next_call(&current, "quests.getMultiple");
    next_call(&current, "quests.complete");
}

#[test]
fn issued_quest_actions_survive_kill_stop_and_fiber_timeout_until_confirmation() {
    for ending in ["kill", "stop", "timeout"] {
        let host = testing::headless_host();
        let (mut session, link) = testing::start_probe(
            &host,
            quest_movie()
                .constant("quests.isInProgress", json!(true))
                .constant("quests.canComplete", json!(true))
                .constant("world.isActionAvailable", json!(true))
                .constant("quests.getMaxTurnIns", json!(1))
                .responding("quests.complete", [(json!(null), vec![])]),
        );
        become_ready(&link);
        let source = if ending == "timeout" {
            r#"
            import {quests,packet} from "lucent/api";
            import { Effect } from "effect";
            import * as script from "lucent/script";
            export default function* () {
                yield* quests.complete(802).pipe(Effect.timeoutOption("100 millis"));
                yield* packet.sendToClient("timed out");
                yield* script.sleep("1 hour");
            }
        "#
        } else {
            r#"
            import {quests} from "lucent/api";
            export default function* () {yield* quests.complete(802);}
        "#
        };
        let run = start(
            &mut session,
            loose(&[("held.js", source)]),
            Default::default(),
        );
        next_call(&link, "quests.complete");
        assert!(link.acquire_quest_claim(802).unwrap().is_none());
        let mut run = Some(run);
        match ending {
            "kill" => {
                run.as_ref().unwrap().kill();
                let _ = run.as_ref().unwrap().wait(TIMEOUT);
                drop(run.take());
            }
            "stop" => {
                run.as_ref().unwrap().stop_script();
                let _ = run.as_ref().unwrap().wait(TIMEOUT);
                drop(run.take());
            }
            _ => {
                await_event(&link, "onDebug", run.as_ref());
            }
        }
        assert!(
            link.acquire_quest_claim(802).unwrap().is_none(),
            "{ending} released an unconfirmed turn-in"
        );
        assert!(
            link.call_loader(
                "quests.complete",
                vec![
                    Value::Number(802.),
                    Value::Number(1.),
                    Value::Number(-1.),
                    Value::Bool(false)
                ]
            )
            .recv_timeout(TIMEOUT)
            .is_err(),
            "{ending} permitted a second unconfirmed turn-in"
        );
        emit(&link, json!({"cmd":"ccqr","QuestID":802,"bSuccess":1}));
        let grant = link
            .acquire_quest_claim(802)
            .unwrap()
            .unwrap_or_else(|| panic!("{ending} retained its quest"));
        link.release_claim(grant.id);
        if let Some(run) = run {
            run.kill();
            let _ = run.wait(TIMEOUT);
            drop(run);
        }
        link.call_loader("probe.echo", vec![])
            .recv_timeout(TIMEOUT)
            .unwrap();
        while let Some(event) = link.try_event().unwrap() {
            if let ScriptEvent::Loader { event, .. } = event
                && event.name == "onProbeCall"
            {
                assert_ne!(
                    event.args.first(),
                    Some(&Value::String("quests.complete".into()))
                );
            }
        }
    }
}

fn quest_movie() -> testing::ProbeMovie {
    ready_movie().responding("quests.getMultiple",[(json!(null),vec![("onExtensionResponse".into(), json!({"type":"json","dataObj":{"cmd":"getQuests","quests":{"802":{"QuestID":802,"sName":"Elders Blood","Rewards":[{"ItemID":5586,"sName":"Elders Blood"}]}}}}).to_string())])])
}

fn become_ready(link: &lucent_host::ScriptLink) {
    emit(
        link,
        json!({"cmd":"loadInventoryBig","items":[],"hitems":[]}),
    );
    emit(
        link,
        json!({"cmd":"moveToArea","areaId":1,"areaName":"battleon-1","uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}),
    );
}

#[test]
fn registered_boosts_use_the_first_inactive_inventory_boost_without_a_script() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        ready_movie()
            .constant("player.hasActiveBoost", json!(false))
            .responding("inventory.use", [(json!(true), vec![])]),
    );
    become_ready(&link);
    link.command(serde_json::from_value(json!({"kind":"refreshLoggedIn"})).unwrap())
        .recv_timeout(TIMEOUT)
        .unwrap();
    emit(
        &link,
        json!({"cmd":"loadInventoryBig","items":[
        {"ItemID":1,"sName":"Gold Boost","sType":"ServerUse","sLink":"gboost::1","iQty":2},
        {"ItemID":2,"sName":"XP Boost","sType":"ServerUse","sLink":"xpboost::1","iQty":2}
    ],"hitems":[]}),
    );
    edit(&link, json!({"kind":"addBoost","name":"XP Boost"}));
    edit(&link, json!({"kind":"addBoost","name":"Gold Boost"}));
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    assert_eq!(
        next_call(&link, "inventory.use")[1],
        Value::List(vec![lucent_protocol::bridge::from_json(
            json!({"itemId":1})
        )])
    );
    let run = start(
        &mut session,
        loose(&[(
            "boost-race.js",
            r#"
        import {inventory} from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            if (yield* inventory.use(1)) throw new Error("overlapping boost use");
            yield* script.stop("boost was held");
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        lucent_protocol::script::ScriptOutcome::StoppedByScript {
            reason: Some("boost was held".into())
        }
    );
    emit(
        &link,
        json!({"cmd":"loadInventoryBig","items":[],"hitems":[]}),
    );
    link.call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    assert!(
        link.read(|p| p
            .items
            .all(lucent_protocol::item::ItemContainer::Inventory)
            .is_empty())
            .unwrap()
    );
}

#[test]
fn an_issued_item_use_remains_reserved_after_its_script_is_killed() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        ready_movie().responding("inventory.use", [(json!(true), vec![])]),
    );
    become_ready(&link);
    emit(
        &link,
        json!({"cmd":"loadInventoryBig","items":[{"ItemID":1,"sName":"XP Boost","sType":"ServerUse","sLink":"xpboost::1","iQty":2}],"hitems":[]}),
    );
    let run = start(
        &mut session,
        loose(&[(
            "use.js",
            r#"
        import {inventory} from "lucent/api";
        export default function* () {yield* inventory.use(1);}
    "#,
        )]),
        Default::default(),
    );
    next_call(&link, "inventory.use");
    run.kill();
    let _ = run.wait(TIMEOUT);
    drop(run);
    assert!(link.acquire_inventory_use_claim(1).unwrap().is_none());
    assert!(
        link.call_loader(
            "inventory.use",
            vec![lucent_protocol::bridge::from_json(json!({"itemId":1}))]
        )
        .recv_timeout(TIMEOUT)
        .is_err()
    );
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
        {
            assert_ne!(
                event.args.first(),
                Some(&Value::String("inventory.use".into()))
            );
        }
    }
}
