use lucent_host::projection::combat_state::CombatHydration;
use lucent_host::projection::{PacketProjection, Projection};
use lucent_protocol::{Packet, PacketEncoding, query::MonsterSelector};
use serde_json::{Value, json};

fn packet(state: &mut Projection, command: &str, data: Value, now: f64) {
    let input = Packet::Extension {
        command: command.into(),
        data,
        raw: String::new(),
        encoding: PacketEncoding::Json,
    };
    assert!(matches!(
        state.packet(&input, now, false),
        PacketProjection::Applied(_)
    ));
}

fn area(state: &mut Projection) {
    state.auth.username = "Hero".into();
    packet(
        state,
        "moveToArea",
        json!({
            "areaId":12,"areaName":"battleon-42",
            "uoBranch":[{"entID":10,"strUsername":"Hero","intHP":100,"strFrame":"Enter"}],
            "monBranch":[
                {"MonMapID":1,"MonID":5,"strMonName":" Slime ","intHP":100,"intHPMax":100,"intState":1,"strFrame":"Enter"},
                {"MonMapID":2,"MonID":6,"strMonName":"id.1","intHP":100,"intHPMax":100,"intState":1,"strFrame":"Enter"},
                {"MonMapID":3,"MonID":7,"strMonName":"Slime","intHP":100,"intHPMax":100,"intState":1,"strFrame":"Boss"}
            ]
        }),
        1000.0,
    );
}

fn snapshot(state: &Projection, now: f64) -> Value {
    json!({"monsters": state.combat_monsters(
        now,
        &[
            MonsterSelector::Name("ID:2".into()),
            MonsterSelector::Named { name: "id.1".into() },
            MonsterSelector::Name("\u{feff}SLIME\u{a0}".into()),
        ],
        true,
    )})
}

fn refresh(input: CombatHydration, now: f64) -> Value {
    serde_json::to_value(input.snapshot(now).unwrap()).unwrap()
}

fn input(actions: Value, target: Value, haste: f64, gcd: f64) -> CombatHydration {
    CombatHydration {
        actions,
        haste: json!(haste),
        global_cooldown: json!(gcd),
        global_used_at: json!(1000),
        potions_locked: json!(false),
        target,
        available_monster_ids: json!([1, 2]),
    }
}

#[test]
fn cooldowns_use_old_cooldown_haste_and_the_global_deadline() {
    let actions = json!([
        {"cd":2000,"OldCD":1000,"ts":1000},
        {"cd":1000,"ts":800}, null, null, null,
        {"cd":1000,"ts":1000,"ref":"i1","sArg1":"42","isOK":true,"skillLock":false,"lock":false}
    ]);
    let at = |now| refresh(input(actions.clone(), Value::Null, 0.5, 750.0), now);
    assert_eq!(
        at(1000.0)["skills"][0],
        json!({"status":"cooldown","remainingMs":750})
    );
    assert_eq!(at(1000.0)["skills"][2], json!({"status":"ready"}));
    assert_eq!(at(1000.0)["consumable"], json!({"itemId":42,"ready":false}));
    assert_eq!(at(1749.0)["skills"][0]["remainingMs"], 1);
    assert_eq!(at(1750.0)["skills"][0], json!({"status":"ready"}));
    assert_eq!(at(1750.0)["consumable"], json!({"itemId":42,"ready":true}));
    assert_eq!(
        refresh(
            input(json!([{"cd":1000,"ts":1000}]), Value::Null, -2.0, 0.0),
            1000.0
        )["skills"][0]["remainingMs"],
        2000
    );
}

#[test]
fn generations_change_only_on_revival_and_priority_ranks_follow_selectors() {
    let mut state = Projection::default();
    area(&mut state);
    let original = state.combat_monsters(1000.0, &[], true);
    for (command, data, expected) in [
        ("cb", json!({"m":{"1":{"intHP":0,"intState":0}}}), 0),
        ("respawnMon", json!(["xt", "respawnMon", "1"]), 1),
        ("respawnMon", json!(["xt", "respawnMon", "1"]), 1),
        ("mtls", json!({"id":1,"o":{"intHP":0,"intState":0}}), 1),
        ("mtls", json!({"id":1,"o":{"intHP":100,"intState":1}}), 2),
        ("cb", json!({"m":{"1":{"intHP":0,"intState":0}}}), 2),
        ("cb", json!({"m":{"1":{"intHP":100,"intState":1}}}), 3),
    ] {
        packet(&mut state, command, data, 1010.0);
        assert_eq!(
            snapshot(&state, 1010.0)["monsters"][0]["generation"],
            expected,
            "{command}"
        );
    }
    assert_eq!(original[0].generation, 0);
    assert_eq!(snapshot(&state, 1020.0)["monsters"][0]["priorityRank"], 2);
    assert_eq!(snapshot(&state, 1020.0)["monsters"][1]["priorityRank"], 0);
    assert_eq!(snapshot(&state, 1020.0)["monsters"][2]["priorityRank"], 2);
    for (selector, expected) in [
        (MonsterSelector::Id(1.0), vec![Some(0), None, None]),
        (
            MonsterSelector::Named {
                name: "id.1".into(),
            },
            vec![None, Some(0), None],
        ),
        (
            MonsterSelector::Name("id.1".into()),
            vec![Some(0), None, None],
        ),
        (MonsterSelector::Name("*".into()), vec![Some(0); 3]),
    ] {
        assert_eq!(
            state
                .combat_monsters(1000.0, &[selector], false)
                .iter()
                .map(|monster| monster.priority_rank)
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn counter_protection_outlives_cast_time_and_ends_on_lifecycle_signals() {
    let mut state = Projection::default();
    area(&mut state);
    packet(
        &mut state,
        "cb",
        json!({"a":[{"cmd":"aura+","tInf":"m:1","aura":{"nam":"Counter Attack","t":"s","dur":2}}]}),
        1000.0,
    );
    assert_eq!(
        snapshot(&state, 9999.0)["monsters"][0]["attackBlocked"],
        true
    );
    assert_eq!(
        snapshot(&state, 10000.0)["monsters"][0]["attackBlocked"],
        false
    );
    packet(
        &mut state,
        "cb",
        json!({"a":[{"cmd":"aura-","tInf":"m:1","aura":{"nam":"Counter Attack"}}]}),
        1200.0,
    );
    assert_eq!(
        snapshot(&state, 1200.0)["monsters"][0]["attackBlocked"],
        false
    );
}

#[test]
fn counter_windows_extend_without_shortening_and_obey_death_and_setting_changes() {
    let mut state = Projection::default();
    area(&mut state);
    let warning = json!({"anims":[{"cInf":"m:1","msg":"prepares a counter attack!"}]});
    let mut long_warning = warning.clone();
    long_warning["sara"] = json!([{"actionResult":{"cInf":"m:1","ct":2000}}]);
    packet(&mut state, "cb", long_warning, 1000.0);
    packet(&mut state, "cb", warning.clone(), 1100.0);
    assert_eq!(
        snapshot(&state, 9999.0)["monsters"][0]["attackBlocked"],
        true
    );
    assert_eq!(
        snapshot(&state, 7999.0)["monsters"][0]["attackBlocked"],
        true
    );
    packet(&mut state, "cb", warning, 7000.0);
    assert_eq!(
        snapshot(&state, 13999.0)["monsters"][0]["attackBlocked"],
        true
    );
    assert_eq!(
        snapshot(&state, 14000.0)["monsters"][0]["attackBlocked"],
        false
    );
    assert!(!state.combat_monsters(7000.0, &[], false)[0].attack_blocked);
    packet(
        &mut state,
        "cb",
        json!({"m":{"1":{"intHP":0,"intState":0}}}),
        7010.0,
    );
    assert_eq!(
        snapshot(&state, 7010.0)["monsters"][0]["attackBlocked"],
        false
    );
    packet(
        &mut state,
        "mtls",
        json!({"id":1,"o":{"intHP":100,"intState":1}}),
        7020.0,
    );
    assert_eq!(snapshot(&state, 7020.0)["monsters"][0]["generation"], 1);
}

#[test]
fn skill_rounding_and_consumable_locks_follow_the_client_formula() {
    let slot = json!({"cd":1001,"OldCD":null,"ts":1000,"ref":"i1","sArg1":42,"isOK":true});
    for (field, value) in [
        ("isOK", json!(false)),
        ("skillLock", json!(true)),
        ("lock", json!(true)),
        ("ref", json!("a1")),
        ("sArg1", Value::Null),
    ] {
        let mut action = slot.clone();
        action[field] = value;
        let current = refresh(
            input(
                json!([null, null, null, null, null, action]),
                Value::Null,
                0.9,
                0.0,
            ),
            2000.0,
        );
        assert!(
            current["consumable"].is_null() || current["consumable"]["ready"] == false,
            "{field}"
        );
    }
    let at = |now| {
        refresh(
            input(
                json!([null, null, null, null, null, slot.clone()]),
                Value::Null,
                0.9,
                0.0,
            ),
            now,
        )
    };
    assert_eq!(at(1000.0)["skills"][5]["remainingMs"], 501);
    assert_eq!(
        at(1500.5)["skills"][5],
        json!({"status":"cooldown","remainingMs":1})
    );
    assert_eq!(at(1500.5)["consumable"]["ready"], false);
    assert_eq!(at(1501.0)["consumable"]["ready"], true);
    let mut locked = input(
        json!([null, null, null, null, null, slot]),
        Value::Null,
        0.0,
        0.0,
    );
    locked.potions_locked = json!(true);
    assert_eq!(refresh(locked, 5000.0)["consumable"]["ready"], false);
}

#[test]
fn invalid_client_inputs_return_errors() {
    for invalid in [
        json!({}),
        json!([{"cd":"bad","ts":1000}]),
        json!([{"cd":1e30,"ts":1000}]),
    ] {
        assert!(
            input(invalid, Value::Null, 0.0, 0.0)
                .snapshot(1000.0)
                .is_err()
        );
    }
}

#[test]
fn missing_and_null_timestamps_follow_client_number_coercion() {
    for (timestamp, expected) in [
        (None, json!({"status":"ready"})),
        (
            Some(Value::Null),
            json!({"status":"cooldown","remainingMs":500}),
        ),
        (Some(json!("bad")), json!({"status":"ready"})),
    ] {
        let mut action = json!({"cd":1000});
        if let Some(timestamp) = timestamp {
            action["ts"] = timestamp;
        }
        let mut observation = input(json!([action]), Value::Null, 0.0, 0.0);
        observation.global_used_at = json!(0);
        assert_eq!(refresh(observation, 500.0)["skills"][0], expected);
    }
    for (timestamp, expected) in [
        (Value::Null, json!({"status":"cooldown","remainingMs":500})),
        (json!("bad"), json!({"status":"ready"})),
        (json!("undefined"), json!({"status":"ready"})),
    ] {
        let mut observation = input(json!([{"cd":0,"ts":0}]), Value::Null, 0.0, 1000.0);
        observation.global_used_at = timestamp;
        assert_eq!(refresh(observation, 500.0)["skills"][0], expected);
    }
}

#[test]
fn connection_and_area_changes_clear_packet_combat_facts() {
    for status in [
        "OnConnection",
        "OnConnectionLost",
        "OnConnectionFailed",
        "area",
    ] {
        let mut state = Projection::default();
        area(&mut state);
        packet(
            &mut state,
            "cb",
            json!({"anims":[{"cInf":"m:1","msg":"prepares a counter attack!"}]}),
            1000.0,
        );
        assert!(state.combat_monsters(1000.0, &[], true)[0].attack_blocked);
        if status == "area" {
            area(&mut state);
            assert!(!state.combat_monsters(1000.0, &[], true)[0].attack_blocked);
        } else {
            state.connection(status);
            assert!(state.combat_monsters(1000.0, &[], true).is_empty());
            area(&mut state);
            assert!(!state.combat_monsters(1000.0, &[], true)[0].attack_blocked);
        }
    }
}

fn probe_query(link: &lucent_host::ScriptLink) -> Value {
    serde_json::from_str(
        &link
            .query(lucent_protocol::query::Query::CombatState { priorities: vec![] })
            .unwrap(),
    )
    .unwrap()
}

fn probe_emit(link: &lucent_host::ScriptLink, event: &str, data: Value) {
    use lucent_host::bridge::Value as LoaderValue;
    assert_eq!(
        link.call_loader(
            "probe.emit",
            vec![
                LoaderValue::String(event.into()),
                LoaderValue::String(data.to_string())
            ]
        )
        .recv_timeout(std::time::Duration::from_secs(5)),
        Ok(LoaderValue::Bool(true))
    );
}

fn probe_calls(link: &lucent_host::ScriptLink) -> Vec<Value> {
    let mut calls = Vec::new();
    while let Some(event) = link.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
        {
            calls.push(
                lucent_protocol::bridge::to_json(lucent_host::bridge::Value::List(event.args))
                    .unwrap(),
            );
        }
    }
    calls
}

fn combat_probe(
    inputs: &[Value],
    events: Vec<(String, String)>,
) -> lucent_host::testing::ProbeMovie {
    let mut reads = Vec::new();
    for input in inputs {
        for field in [
            "actions",
            "haste",
            "globalCooldown",
            "globalUsedAt",
            "potionsLocked",
        ] {
            reads.push((
                json!(input[field].to_string()),
                if field == "globalCooldown" {
                    events.clone()
                } else {
                    vec![]
                },
            ));
        }
    }
    lucent_host::testing::ProbeMovie::new()
        .responding("flash.getGameObject", reads)
        .responding(
            "combat.getTarget",
            inputs.iter().map(|input| (input["target"].clone(), vec![])),
        )
        .constant("player.getUserId", json!(10))
        .responding(
            "world.getAvailableMonsterMapIds",
            inputs
                .iter()
                .map(|input| (input["availableMonsterIds"].clone(), vec![])),
        )
}

fn ready_input() -> Value {
    json!({
        "actions":[null,null,null,null,null,{"cd":0,"ts":0,"ref":"i1","sArg1":42,"isOK":true}],
        "haste":0,"globalCooldown":0,"globalUsedAt":0,"potionsLocked":false,
        "target":{"type":"monster","monsterMapId":1},"availableMonsterIds":[1]
    })
}

#[test]
fn typed_combat_refresh_reads_fresh_client_facts_without_storing_them() {
    use lucent_host::testing;
    use lucent_protocol::command::Command;
    let first = ready_input();
    let mut next = first.clone();
    next["target"] = json!({"type":"player","entityId":10,"hp":0,"state":0});
    next["actions"][5]["sArg1"] = json!(7);
    next["potionsLocked"] = json!(true);
    next["availableMonsterIds"] = json!([3]);
    let mut invalid = first.clone();
    invalid["actions"] = json!({});
    let host = testing::headless_host();
    let (_session, link) =
        testing::start_probe(&host, combat_probe(&[first, next, invalid], vec![]));
    probe_emit(
        &link,
        "onExtensionResponse",
        json!({"type":"json","dataObj":{
            "cmd":"moveToArea","areaName":"battleon-42",
            "uoBranch":[{"entID":10,"uoName":"Hero","intHP":100,"strFrame":"Enter"}],
            "monBranch":[{"MonID":5,"MonMapID":1,"intHP":100,"intState":1,"strFrame":"Enter"}]
        }}),
    );
    let before = probe_query(&link);
    assert_eq!(
        before,
        json!([{"monsterMapId":1,"generation":0,"priorityRank":null,"attackBlocked":false}])
    );
    for (target, consumable, available) in [
        (
            json!({"type":"monster","monsterMapId":1}),
            json!({"itemId":42,"ready":true}),
            json!([1]),
        ),
        (
            json!({"type":"player","entityId":10,"alive":false}),
            json!({"itemId":7,"ready":false}),
            json!([3]),
        ),
    ] {
        let reply = link
            .command(Command::RefreshCombatState {})
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let observed: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(observed["selectedTarget"], target);
        assert_eq!(observed["consumable"], consumable);
        assert_eq!(observed["availableMonsterIds"], available);
        assert_eq!(
            observed["skills"],
            json!(vec![json!({"status":"ready"}); 6])
        );
        assert!(observed["observedAtMs"].as_f64().unwrap() > 0.0);
        assert_eq!(
            probe_calls(&link),
            vec![
                json!(["flash.getGameObject", ["world.actions.active"]]),
                json!(["flash.getGameObject", ["world.myAvatar.dataLeaf.sta.$tha"]]),
                json!(["flash.getGameObject", ["world.GCD"]]),
                json!(["flash.getGameObject", ["world.GCDTS"]]),
                json!(["flash.getGameObject", ["world.lockdownPots"]]),
                json!(["combat.getTarget", []]),
                json!(["world.getAvailableMonsterMapIds", []]),
            ]
        );
        assert_eq!(probe_query(&link), before);
        assert_eq!(link.try_event().unwrap(), None);
    }
    assert!(
        link.command(Command::RefreshCombatState {})
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_err()
    );
    assert_eq!(probe_query(&link), before);
}

#[test]
fn typed_combat_refresh_returns_client_facts_across_packet_updates() {
    use lucent_host::testing;
    use lucent_protocol::command::Command;
    for event in [
        ("onConnection".into(), "OnConnectionLost".into()),
        (
            "onExtensionResponse".into(),
            json!({"type":"json","dataObj":{"cmd":"cb","p":{}}}).to_string(),
        ),
        (
            "onExtensionResponse".into(),
            json!({"type":"json","dataObj":{"cmd":"moveToArea","areaName":"battleon-42"}})
                .to_string(),
        ),
    ] {
        let host = testing::headless_host();
        let (_session, link) =
            testing::start_probe(&host, combat_probe(&[ready_input()], vec![event]));
        let reply = link
            .command(Command::RefreshCombatState {})
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let observed: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(observed["consumable"], json!({"itemId":42,"ready":true}));
        assert_eq!(
            observed["selectedTarget"],
            json!({"type":"monster","monsterMapId":1})
        );
        assert_eq!(probe_calls(&link).len(), 7);
        assert_eq!(probe_query(&link), json!([]));
    }
}

#[test]
fn combat_refresh_matches_the_cooldown_reference_corpus() {
    let fixtures: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/projection/combat-state.json")).unwrap();
    for fixture in fixtures {
        let input: CombatHydration = serde_json::from_value(fixture["hydration"].clone()).unwrap();
        let current = refresh(input, fixture["nowMs"].as_f64().unwrap());
        assert_eq!(current["skills"], fixture["skills"], "{}", fixture["name"]);
        assert_eq!(
            current["consumable"], fixture["consumable"],
            "{}",
            fixture["name"]
        );
        assert_eq!(current["selectedTarget"], json!({"type":"none"}));
    }
}

#[test]
#[ignore]
fn measure_typed_combat_refresh_through_headless_host() {
    use lucent_host::testing;
    use lucent_protocol::command::Command;
    let mut input = ready_input();
    input["actions"] = json!([
        {"cd":1000,"ts":0}, {"cd":2000,"OldCD":1500,"ts":0},
        {"cd":3000,"ts":0}, {"cd":4000,"ts":0}, {"cd":5000,"ts":0},
        {"cd":1000,"ts":0,"ref":"i1","sArg1":42,"isOK":true,"skillLock":false,"lock":false}
    ]);
    let host = testing::headless_host();
    let mut times = Vec::new();
    for _ in 0..100 {
        let (_session, link) =
            testing::start_probe(&host, combat_probe(&[input.clone(), input.clone()], vec![]));
        link.command(Command::RefreshCombatState {})
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(probe_calls(&link).len(), 7);
        let start = std::time::Instant::now();
        let reply = link
            .command(Command::RefreshCombatState {})
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        times.push(start.elapsed().as_secs_f64() * 1_000_000.0);
        let observed: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(
            observed["skills"],
            json!(vec![json!({"status":"ready"}); 6])
        );
        assert_eq!(
            observed["selectedTarget"],
            json!({"type":"monster","monsterMapId":1})
        );
        assert_eq!(observed["availableMonsterIds"], json!([1]));
        assert_eq!(observed["consumable"], json!({"itemId":42,"ready":true}));
        assert_eq!(probe_calls(&link).len(), 7);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "refreshCombatState calls=100 callbacks=700 min_us={:.3} median_us={:.3} p99_us={:.3}",
        times[0],
        (times[49] + times[50]) / 2.0,
        times[98]
    );
}
