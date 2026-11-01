mod common;

use std::time::{Duration, Instant};

use lucent_host::ScriptLink;
use lucent_host::bridge::Value;
#[cfg(feature = "testing")]
use lucent_host::scripting::RuntimeError;
use lucent_host::testing;
use lucent_protocol::command::Command;
use serde_json::json;

use common::TIMEOUT;

fn become_ready(link: &ScriptLink) {
    for packet in [
        json!({"cmd":"loadInventoryBig","items":[],"hitems":[]}),
        json!({"cmd":"moveToArea","areaId":1,"areaName":"battleon-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}),
    ] {
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
}

fn update(link: &ScriptLink, patch: serde_json::Value) {
    let command: Command =
        serde_json::from_value(json!({"kind":"updateSettings","patch":patch})).unwrap();
    link.command(command).recv_timeout(TIMEOUT).unwrap();
}

fn next_call(link: &ScriptLink, method: &str) -> Vec<Value> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        assert!(Instant::now() < deadline, "did not receive {method}");
        let event = common::await_event(link, "onProbeCall", None);
        if event.args.first() == Some(&Value::String(method.into())) {
            return event.args;
        }
    }
}

fn drain(link: &ScriptLink) {
    while link.try_event().unwrap().is_some() {}
}

fn counter_session() -> (
    lucent_host::Host,
    lucent_host::Session,
    ScriptLink,
    lucent_host::Supervisor,
) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie()
            .constant(
                "combat.getTarget",
                json!({
                    "type":"monster","monsterMapId":1,"monsterId":1,"name":"Boss",
                    "cell":"Enter","hp":100,"maxHp":100,"level":1,"race":"None","state":1
                }),
            )
            .recording("combat.cancelAutoAttack")
            .recording("combat.cancelTarget"),
    );
    become_ready(&observer);
    update(&observer, json!({"antiCounterEnabled":true}));
    let supervisor = lucent_host::Supervisor::start(&mut session, None).unwrap();
    drain(&observer);
    (host, session, observer, supervisor)
}

fn counter(link: &ScriptLink, monster_map_id: i64) {
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(
                json!({"type":"json","dataObj":{
                    "cmd":"cb","anims":[{
                        "msg":"Boss prepares a counter attack","animStr":"cast",
                        "cInf":"p:7","tInf":format!("m:{monster_map_id}")
                    }]
                }})
                .to_string(),
            ),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}

fn assert_no_cancel(link: &ScriptLink) {
    let until = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < until {
        let Some(lucent_host::bridge::ScriptEvent::Loader { event: call, .. }) =
            link.try_event().unwrap()
        else {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        };
        if call.name != "onProbeCall" {
            continue;
        }
        let Some(Value::String(method)) = call.args.first() else {
            panic!("missing method")
        };
        assert!(!method.starts_with("combat.cancel"), "called {method}");
    }
}

#[test]
fn anti_counter_stops_attacking_the_countered_target_without_a_script() {
    let (_host, _session, observer, _supervisor) = counter_session();
    counter(&observer, 1);
    next_call(&observer, "combat.cancelAutoAttack");
    next_call(&observer, "combat.cancelTarget");
}

#[test]
fn anti_counter_stops_attacking_the_countered_target_in_a_script() {
    let (_host, mut session, observer, _supervisor) = counter_session();
    let run = common::yielding(&mut session);
    counter(&observer, 1);
    next_call(&observer, "combat.cancelAutoAttack");
    next_call(&observer, "combat.cancelTarget");
    run.stop_script();
    run.wait(TIMEOUT).unwrap();
}

#[test]
fn anti_counter_ignores_other_targets_and_disabled_sessions() {
    let (_host, _session, observer, _supervisor) = counter_session();
    counter(&observer, 2);
    assert_no_cancel(&observer);
    update(&observer, json!({"antiCounterEnabled":false}));
    drain(&observer);
    counter(&observer, 1);
    assert_no_cancel(&observer);
}

fn assert_only_scripts_request_monster_drops(local_move: bool) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie()
            .recording("world.requestMonsterDrops")
            .recording("settings.setWalkSpeed"),
    );
    become_ready(&observer);
    let _supervisor = lucent_host::Supervisor::start(&mut session, None).unwrap();
    drain(&observer);
    let emit = |packet| {
        observer
            .call_loader(
                "probe.emit",
                vec![
                    Value::String("onExtensionResponse".into()),
                    Value::String(json!({"type":"json","dataObj":packet}).to_string()),
                ],
            )
            .recv_timeout(TIMEOUT)
            .unwrap();
    };
    let area = json!({
        "cmd":"moveToArea","areaId":5,"areaName":"testroom-1",
        "uoBranch":[{"entID":7,"uoName":"player-a","strFrame":"Enter",
            "intHP":100,"intHPMax":100}],
        "mondef":[{"MonID":1,"strMonName":"Frogzard","sRace":"Amphibian","iLvl":3}],
        "monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50}],
        "monmap":[{"MonMapID":2,"strFrame":if local_move { "r2" } else { "Enter" }}]
    });
    let location = json!({"cmd":"uotls","unm":"player-a","o":{"strFrame":"r2"}});
    emit(area.clone());
    if local_move {
        emit(location.clone());
    }
    let until = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < until {
        let call = common::await_event(&observer, "onProbeCall", None);
        assert_ne!(
            call.args.first(),
            Some(&Value::String("world.requestMonsterDrops".into())),
            "idle session requested monster drops, local_move={local_move}"
        );
    }
    let run = common::yielding(&mut session);
    if local_move {
        emit(location);
    } else {
        emit(area);
    }
    assert_eq!(
        next_call(&observer, "world.requestMonsterDrops"),
        vec![
            Value::String("world.requestMonsterDrops".into()),
            Value::Number(2.0)
        ],
        "script did not request the current monster, local_move={local_move}"
    );
    run.stop_script();
    run.wait(TIMEOUT).unwrap();
}

#[test]
fn only_scripts_request_monster_drops_on_join_map() {
    assert_only_scripts_request_monster_drops(false);
}

#[test]
fn only_scripts_request_monster_drops_on_player_location() {
    assert_only_scripts_request_monster_drops(true);
}

#[cfg(feature = "testing")]
#[test]
fn overflow_logs_identify_the_script_runtime() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, common::ready_movie());
    let flood = r#"
        for (let i = 0; i < 1100; i++) window.swf["probe.emit"]("onDebug", String(i));
        new Promise(() => {});
    "#;
    let runtime = lucent_host::scripting::Runtime::start(link).unwrap();
    assert!(matches!(
        runtime.eval("flood.js", flood).wait::<()>(TIMEOUT),
        Err(RuntimeError::EventsLost)
    ));
    assert!(
        logs.messages(tracing::Level::ERROR)
            .iter()
            .any(|text| text == "script events overflowed and were lost")
    );
}
