mod common;

use common::TIMEOUT;
use lucent_host::bridge::EventInterests;
use lucent_host::scripting::{Kill, Runtime, RuntimeError};
use lucent_host::testing;
use lucent_protocol::script::ScriptOutcome;
use serde_json::{Value, json};

fn area() -> String {
    json!({
        "type": "json",
        "dataObj": {
            "cmd": "moveToArea",
            "areaId": 5,
            "areaName": "testroom-1",
            "uoBranch": [
                {"entID": 8, "uoName": "player-b", "strFrame": "Enter", "intHP": 100, "intHPMax": 100},
                {"entID": 7, "uoName": "player-a", "strFrame": "Enter", "intHP": 100, "intHPMax": 100}
            ],
            "mondef": [
                {"MonID": 1, "strMonName": "Frogzard", "sRace": "Amphibian", "iLvl": 3}
            ],
            "monBranch": [
                {"MonID": 1, "MonMapID": 2, "intHP": 50, "intHPMax": 50}
            ],
            "monmap": [
                {"MonMapID": 2, "strFrame": "r2"}
            ]
        }
    })
    .to_string()
}

#[test]
fn the_api_layer_reads_and_changes_host_state_through_its_services() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .raising("flash.sendClientPacket", "onExtensionResponse", area())
            .constant("auth.isLoggedIn", json!(true))
            .constant("bank.loadItems", Value::Null)
            .constant("bank.isLoaded", json!(true))
            .constant(
                "bank.getItems",
                json!([
                    {"ItemID": 1, "sName": "Sword", "sType": "Sword"},
                    {"ItemID": 2, "sName": "Rogue", "sType": "Class"},
                    {"ItemID": 3, "sName": "Shield", "sType": "Shield"}
                ]),
            )
            .constant(
                "world.getMonster",
                json!({"MonMapID": 9, "MonID": 4, "strMonName": "Ghost", "intHP": 10, "intHPMax": 10}),
            )
            .constant(
                "auth.getServers",
                json!([
                    {"sName": "Artix", "iCount": 10, "iMax": 10, "bOnline": 1},
                    {"sName": "Yorumi", "iCount": 3, "iMax": 10, "bOnline": 1}
                ]),
            ),
    );
    let runtime = Runtime::start(link).unwrap();

    common::ready(&runtime);
    let script = common::loose(&[(
        "api.js",
        r#"
            import * as api from "lucent/api";
            import * as script from "lucent/script";
            export default function* () {
                const joined = yield* api.events.once(
                    { type: "join-map" },
                    { trigger: api.packet.sendToClient("join"), timeout: "5 seconds" },
                );
                const me = yield* api.player.get();
                const players = yield* api.players.getAll();
                const frogzard = yield* api.monsters.get("frogzard");
                const ghost = yield* api.monsters.get("id.9");
                const loaded = yield* api.bank.load();
                const bank = yield* api.bank.getAll();
                const servers = yield* api.auth.getServers();
                yield* script.stop(JSON.stringify({
                    joined,
                    map: yield* api.map.getName(),
                    me: me === null ? null : [me.entityId, me.username, me.alive],
                    players: players.map((player) => player.username),
                    frogzard: frogzard === null ? null : [frogzard.monsterMapId, frogzard.cell, frogzard.hpPercent],
                    ghost: ghost === null ? null : [ghost.monsterMapId, ghost.name],
                    loggedIn: yield* api.auth.isLoggedIn(),
                    loaded,
                    bank: bank.map((item) => [item.itemId, item.name, item.banked]),
                    servers: servers.map((server) => [server.name, server.full]),
                }));
            }
            "#,
    )]);
    let outcome = common::start(&mut session, script, Default::default())
        .wait(TIMEOUT)
        .unwrap();

    let observed: Value = match outcome {
        ScriptOutcome::StoppedByScript {
            reason: Some(reason),
        } => serde_json::from_str(&reason).unwrap(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        observed,
        json!({
            "joined": {"type": "join-map", "map": {"id": 5, "name": "testroom", "roomNumber": 1}},
            "map": "testroom",
            "me": [7, "player-a", true],
            "players": ["player-b", "player-a"],
            "frogzard": [2, "r2", 100],
            "ghost": [9, "Ghost"],
            "loggedIn": true,
            "loaded": true,
            "bank": [[1, "Sword", true], [3, "Shield", true]],
            "servers": [["Artix", true], ["Yorumi", false]],
        })
    );
}

fn read_settings() -> lucent_protocol::script::Script {
    common::loose(&[(
        "read.js",
        r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            const s = yield* api.settings.get();
            yield* script.stop(JSON.stringify({ walkSpeed: s.walkSpeed, animations: s.animations, customName: s.customName }));
        }
        "#,
    )])
}

#[test]
fn settings_survive_a_kill() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .constant("settings.setWalkSpeed", json!(null))
            .constant("settings.setAnimationsEnabled", json!(null))
            .constant("settings.setCustomName", json!(null)),
    );
    let runtime = Runtime::start(link).unwrap();
    common::ready(&runtime);
    let set = common::loose(&[(
        "set.js",
        r#"
        import * as api from "lucent/api";
        export default function* () {
            yield* api.settings.update({ walkSpeed: 20, animations: false, customName: "Tommy" });
        }
        "#,
    )]);
    let initial = common::start(&mut session, read_settings(), Default::default())
        .wait(TIMEOUT)
        .unwrap();
    assert_eq!(
        initial,
        ScriptOutcome::StoppedByScript {
            reason: Some(r#"{"walkSpeed":8,"animations":true,"customName":null}"#.into()),
        }
    );
    assert_eq!(
        common::start(&mut session, set, Default::default())
            .wait(TIMEOUT)
            .unwrap(),
        ScriptOutcome::Completed
    );
    let before = common::start(&mut session, read_settings(), Default::default())
        .wait(TIMEOUT)
        .unwrap();
    let error = common::start(
        &mut session,
        common::loose(&[("loop.js", "export default function* () { while (true) {} }")]),
        Default::default(),
    )
    .wait(TIMEOUT)
    .unwrap_err();
    assert!(matches!(error, RuntimeError::Killed(Kill::Watchdog)));
    common::ready(&runtime);
    let after = common::start(&mut session, read_settings(), Default::default())
        .wait(TIMEOUT)
        .unwrap();

    let expected = ScriptOutcome::StoppedByScript {
        reason: Some(r#"{"walkSpeed":20,"animations":false,"customName":"Tommy"}"#.into()),
    };
    assert_eq!(before, expected);
    assert_eq!(after, expected, "settings changed across the kill");
}

#[test]
fn settings_updates_report_rejected_patches_without_changing_the_record() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, common::ready_movie());
    let runtime = Runtime::start(link).unwrap();
    common::ready(&runtime);
    for (patch, expected) in [
        (
            r#"{ frameRate: "30", animations: false }"#,
            "host rejected settings patch",
        ),
        (
            r#"{ renderingMode: "invalid", walkSpeed: 20 }"#,
            "host rejected settings patch",
        ),
        (
            "{ frameRate: NaN, animations: false }",
            "frameRate must be a finite number",
        ),
        (
            "{ frameRate: Infinity, animations: false }",
            "frameRate must be a finite number",
        ),
        (
            "{ frameRate: -Infinity, animations: false }",
            "frameRate must be a finite number",
        ),
        (
            "{ walkSpeed: NaN, animations: false }",
            "walkSpeed must be a finite number",
        ),
        (
            "{ walkSpeed: Infinity, animations: false }",
            "walkSpeed must be a finite number",
        ),
        (
            "{ walkSpeed: -Infinity, animations: false }",
            "walkSpeed must be a finite number",
        ),
    ] {
        let source = format!(
            r#"
            import * as api from "lucent/api";
            export default function* () {{ yield* api.settings.update({patch}); }}
        "#
        );
        let outcome = common::start(
            &mut session,
            common::loose(&[("invalid.js", &source)]),
            Default::default(),
        )
        .wait(TIMEOUT)
        .unwrap();
        let ScriptOutcome::Failed { message, .. } = outcome else {
            panic!("accepted {patch}: {outcome:?}")
        };
        assert!(message.contains(expected), "{patch}: {message}");
    }
    let outcome = common::start(&mut session, common::loose(&[("read.js", r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () { yield* script.stop(JSON.stringify(yield* api.settings.get())); }
    "#)]), Default::default()).wait(TIMEOUT).unwrap();
    let ScriptOutcome::StoppedByScript {
        reason: Some(reason),
    } = outcome
    else {
        panic!("{outcome:?}")
    };
    let observed: Value = serde_json::from_str(&reason).unwrap();
    assert_eq!(observed["frameRate"], 24.0);
    assert_eq!(observed["walkSpeed"], 8.0);
    assert_eq!(observed["animations"], true);
}

#[test]
fn a_killed_runs_accept_finishes_before_the_next_starts() {
    use lucent_host::scripting::{Kill, RuntimeError};
    use std::{thread, time::Duration};

    let host = testing::headless_host();
    let packet = json!({"type":"json","dataObj":{
        "cmd":"getDrop", "ItemID":70, "bSuccess":0, "iQtyNow":1
    }})
    .to_string();
    let (mut session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .constant(
                "inventory.getItem",
                json!({"ItemID":70,"sName":"Drop","iQty":1}),
            )
            .accepting_drop(Duration::from_secs(2), packet),
    );
    let runtime = Runtime::start(link).unwrap();
    common::log_in_ready(&session.attach(EventInterests::ALL).unwrap());
    runtime
        .eval(
            "drop.js",
            r#"
        window.swf["probe.emit"]("onExtensionResponse", JSON.stringify({type:"json",dataObj:{
            cmd:"dropItem", items:{"70":{ItemID:70,sName:"Drop",iQty:1}}
        }}));
        null;
    "#,
        )
        .wait::<()>(TIMEOUT)
        .unwrap();
    let started = runtime.eval(
        "started.js",
        r#"
        new Promise(resolve => { window.onProbeAccept = () => resolve(true); })
    "#,
    );
    let script = common::loose(&[(
        "accept.js",
        r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* script.stop(String(yield* api.drops.accept(70)));
        }
    "#,
    )]);
    let first = common::start(&mut session, script.clone(), Default::default());
    assert!(started.wait::<bool>(TIMEOUT).unwrap());
    thread::sleep(Duration::from_millis(500));
    first.kill();
    assert!(matches!(
        first.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    drop(first);
    let order = runtime.eval(
        "order.js",
        r#"
        new Promise(resolve => {
            const events = [];
            const record = (event) => {
                events.push(event);
                if (events.length === 2) resolve(events);
            };
            window.onProbeAccept = () => record("accept");
            const previous = window.onExtensionResponse;
            window.onExtensionResponse = (...args) => {
                if (JSON.parse(args[0]).dataObj.cmd === "getDrop") record("packet");
                previous?.(...args);
            };
        })
    "#,
    );
    let second = common::start(&mut session, script, Default::default());
    assert_eq!(
        order.wait::<Vec<String>>(TIMEOUT).unwrap(),
        ["packet", "accept"]
    );
    assert_eq!(
        second.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("false".into())
        }
    );
}

#[test]
fn a_killed_runs_queued_accepts_never_reach_the_loader() {
    use lucent_host::bridge::{ScriptEvent, Value as BridgeValue};
    use std::time::{Duration, Instant};

    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        common::ready_movie().accepting_drop(
            Duration::ZERO,
            json!({"type":"json","dataObj":{"cmd":"unrelated"}}).to_string(),
        ),
    );
    let _runtime = Runtime::start(link).unwrap();
    let control = session.attach(EventInterests::ALL).unwrap();
    common::log_in_ready(&control);
    let emit = |data: Value| {
        control
            .call_loader(
                "probe.emit",
                vec![
                    BridgeValue::String("onExtensionResponse".into()),
                    BridgeValue::String(json!({"type":"json","dataObj":data}).to_string()),
                ],
            )
            .recv_timeout(TIMEOUT)
            .unwrap();
    };
    emit(json!({"cmd":"dropItem","items":{
        "70":{"ItemID":70,"sName":"First","iQty":1},
        "71":{"ItemID":71,"sName":"Queued A","iQty":1},
        "72":{"ItemID":72,"sName":"Queued B","iQty":1},
        "73":{"ItemID":73,"sName":"Next run","iQty":1}
    }}));
    let observer = session.attach(EventInterests::ALL).unwrap();
    let first = common::started_run(
        &mut session,
        r#"
        import { Effect } from "effect";
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* Effect.raceFirst(
                Effect.raceFirst(api.drops.accept(70), api.drops.accept(71)),
                Effect.raceFirst(api.drops.accept(72), Effect.gen(function* () {
                    yield* api.packet.sendToClient("started");
                    yield* script.sleep("1 hour");
                })),
            );
        }
    "#,
    );
    assert_eq!(
        common::await_event(&observer, "onProbeAccept", Some(&first)).args,
        vec![BridgeValue::Number(70.0)]
    );
    first.kill();
    assert!(matches!(
        first.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    drop(first);
    let second = common::started_run(
        &mut session,
        r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* api.packet.sendToClient("started");
            yield* script.stop(String(yield* api.drops.accept(73)));
        }
    "#,
    );
    emit(json!({"cmd":"getDrop","ItemID":70,"bSuccess":false}));
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut trace = vec![];
    while trace.len() < 2 {
        match observer.try_event().unwrap() {
            Some(ScriptEvent::Loader { event, .. }) if event.name == "onProbeAccept" => {
                let [BridgeValue::Number(id)] = event.args.as_slice() else {
                    panic!("unexpected accept arguments");
                };
                trace.push(("accept", *id));
            }
            Some(ScriptEvent::Loader { event, .. }) if event.name == "onExtensionResponse" => {
                let [BridgeValue::String(raw)] = event.args.as_slice() else {
                    panic!("unexpected packet arguments");
                };
                let packet: Value = serde_json::from_str(raw).unwrap();
                if packet["dataObj"]["cmd"] == "getDrop" {
                    trace.push(("packet", packet["dataObj"]["ItemID"].as_f64().unwrap()));
                }
            }
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(1)),
        }
        assert!(
            Instant::now() < deadline,
            "no accept after the first packet: {trace:?}"
        );
    }
    assert_eq!(trace, [("packet", 70.0), ("accept", 73.0)]);
    emit(json!({"cmd":"getDrop","ItemID":73,"bSuccess":false}));
    assert_eq!(
        second.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("false".into())
        }
    );
}
