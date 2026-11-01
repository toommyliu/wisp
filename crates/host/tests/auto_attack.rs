mod common;

use common::*;
use lucent_host::{Supervisor, bridge::Value, testing};
use lucent_protocol::query::Query;
use serde_json::json;
use std::time::{Duration, Instant};

fn packet(link: &lucent_host::ScriptLink, data: serde_json::Value) {
    let (event, payload) = if data.get("cmd").and_then(serde_json::Value::as_str) == Some("ct") {
        ("packetFromServer", json!({"t":"xt","b":{"o":data}}))
    } else {
        ("onExtensionResponse", json!({"type":"json","dataObj":data}))
    };
    link.call_loader(
        "probe.emit",
        vec![
            Value::String(event.into()),
            Value::String(payload.to_string()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}

fn setup() -> (lucent_host::Session, lucent_host::ScriptLink, Supervisor) {
    setup_with(|movie| movie)
}
fn setup_with(
    modify: impl FnOnce(testing::ProbeMovie) -> testing::ProbeMovie,
) -> (lucent_host::Session, lucent_host::ScriptLink, Supervisor) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        modify(
            ready_movie()
                .combat_facts()
                .constant("world.getAvailableMonsterMapIds", json!([2, 3]))
                .constant("combat.getConsumableSkillItem", json!(null))
                .responding("combat.attackMonster", [(json!(true), vec![])])
                .responding("combat.useSkill", [(json!(true), vec![])])
                .responding("combat.cancelAutoAttack", [(json!(null), vec![])])
                .responding("combat.cancelTarget", [(json!(null), vec![])])
                .responding("drops.accept", [(json!(true), vec![])])
                .recording("settings.setWalkSpeed")
                .recording("settings.enemyMagnet"),
        ),
    );
    log_in(&observer);
    packet(
        &observer,
        json!({"cmd":"moveToArea","areaId":1,"areaName":"test-1",
        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],
        "mondef":[{"MonID":1,"strMonName":"Frogzard"},{"MonID":2,"strMonName":"Slime"}],
        "monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50,"intState":1},{"MonID":2,"MonMapID":3,"intHP":50,"intHPMax":50,"intState":1}]}),
    );
    let supervisor = Supervisor::start(&mut session, None).unwrap();
    (session, observer, supervisor)
}

fn call(
    link: &lucent_host::ScriptLink,
    run: Option<&lucent_host::scripting::ScriptRun>,
    name: &str,
) -> Vec<Value> {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        assert!(std::time::Instant::now() < deadline, "missing {name}");
        let event = await_event(link, "onProbeCall", run);
        if event.args.first() == Some(&Value::String(name.into())) {
            return event.args;
        }
    }
}

fn enable(
    link: &lucent_host::ScriptLink,
    profile: serde_json::Value,
    priority: &[&str],
) -> serde_json::Value {
    edit(
        link,
        json!({"kind":"enableAutoAttack","profile":profile,"targetPriority":priority}),
    )
}
fn edit(link: &lucent_host::ScriptLink, value: serde_json::Value) -> serde_json::Value {
    serde_json::from_str(
        &link
            .command(serde_json::from_value(value).unwrap())
            .recv_timeout(TIMEOUT)
            .unwrap(),
    )
    .unwrap()
}
fn disable(link: &lucent_host::ScriptLink) -> serde_json::Value {
    edit(link, json!({"kind":"disableAutoAttack"}))
}
fn state(link: &lucent_host::ScriptLink) -> serde_json::Value {
    serde_json::from_str(
        &link
            .query(serde_json::from_value::<Query>(json!({"kind":"autoAttack"})).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn skill(args: &[Value]) -> &str {
    let Value::List(args) = &args[1] else {
        panic!("{args:?}")
    };
    let Value::String(skill) = &args[0] else {
        panic!("{args:?}")
    };
    skill
}
fn target(args: &[Value]) -> f64 {
    let Value::List(args) = &args[1] else {
        panic!("{args:?}")
    };
    lucent_protocol::bridge::to_json(args[0].clone()).unwrap()["monsterMapId"]
        .as_f64()
        .unwrap()
}
fn drain(link: &lucent_host::ScriptLink) -> Vec<String> {
    link.call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let mut calls = Vec::new();
    while let Some(event) = link.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && let Some(Value::String(name)) = event.args.first()
        {
            calls.push(name.clone());
        }
    }
    calls
}
fn connection(link: &lucent_host::ScriptLink) {
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onConnection".into()),
            Value::String("OnConnection".into()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}
fn area(link: &lucent_host::ScriptLink, player: bool) {
    packet(
        link,
        json!({"cmd":"moveToArea","areaId":1,"areaName":"test-1",
        "uoBranch":if player {json!([{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}])} else {json!([])},
        "mondef":[{"MonID":1,"strMonName":"Frogzard"}],
        "monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50,"intState":1}]}),
    );
}
#[test]
fn enabling_attacks_and_publishes_the_profile() {
    let (_session, observer, _supervisor) = setup();
    let record = enable(
        &observer,
        json!({"id":"test","label":"Test","steps":[{"skill":1}]}),
        &["Frogzard"],
    );
    assert_eq!(record["config"]["enabled"], true);
    assert_eq!(record["status"]["running"], true);
    assert_eq!(record["config"]["profile"]["id"], "test");
    assert_eq!(record["config"]["profile"]["label"], "Test");
    assert_eq!(target(&call(&observer, None, "combat.attackMonster")), 2.0);
    assert_eq!(skill(&call(&observer, None, "combat.useSkill")), "1");
    assert_eq!(state(&observer)["config"], record["config"]);
}
#[test]
fn priorities_precede_fallback_and_a_better_target_takes_over() {
    let (_session, observer, _supervisor) = setup();
    for (priority, expected) in [vec!["Slime"], vec!["id:3"], vec!["Nobody"], vec![]]
        .into_iter()
        .zip([3, 3, 2, 2])
    {
        enable(&observer, json!({"steps":[{"skill":1}]}), &priority);
        assert_eq!(
            target(&call(&observer, None, "combat.attackMonster")),
            f64::from(expected)
        );
        disable(&observer);
        drain(&observer);
    }
    packet(
        &observer,
        json!({"cmd":"ct","m":{"3":{"intHP":0,"intState":0}}}),
    );
    enable(&observer, json!({"steps":[{"skill":1}]}), &["Slime"]);
    assert_eq!(target(&call(&observer, None, "combat.attackMonster")), 2.0);
    packet(
        &observer,
        json!({"cmd":"ct","m":{"3":{"intHP":50,"intState":1}}}),
    );
    assert_eq!(target(&call(&observer, None, "combat.attackMonster")), 3.0);
}
#[test]
fn no_target_never_cancels_the_clients_combat() {
    let (session, observer, _supervisor) =
        setup_with(|movie| movie.constant("world.getAvailableMonsterMapIds", json!([])));
    let (observed, observations) = std::sync::mpsc::channel();
    session.set_profile_observation_probe(move |_| {
        let _ = observed.send(());
    });
    enable(&observer, json!({"steps":[{"skill":1}]}), &[]);
    for _ in 0..4 {
        observations.recv_timeout(TIMEOUT).unwrap();
    }
    let calls = drain(&observer);
    assert!(
        !calls.iter().any(|name| name.starts_with("combat.cancel")),
        "{calls:?}"
    );
    assert_eq!(state(&observer)["config"]["enabled"], true);
}

#[test]
fn disable_stops_combat_once_and_releases_the_claim() {
    let (session, observer, _supervisor) = setup();
    enable(&observer, json!({"steps":[{"skill":5}]}), &[]);
    call(&observer, None, "combat.useSkill");
    drain(&observer);
    let record = disable(&observer);
    assert_eq!(record["status"]["running"], false);
    call(&observer, None, "combat.cancelAutoAttack");
    call(&observer, None, "combat.cancelTarget");
    assert!(
        drain(&observer)
            .iter()
            .all(|name| !name.starts_with("combat."))
    );
    assert_eq!(session.live_profile_job_count(), 0);
    let claim = observer
        .acquire_consumable_claim(Some(777))
        .unwrap()
        .unwrap();
    observer.release_claim(claim.id);
    disable(&observer);
    assert!(
        drain(&observer)
            .iter()
            .all(|name| !name.starts_with("combat."))
    );
}
#[test]
fn enabling_again_replaces_the_rotation() {
    let (session, observer, _supervisor) = setup();
    enable(&observer, json!({"steps":[{"skill":1}]}), &[]);
    assert_eq!(skill(&call(&observer, None, "combat.useSkill")), "1");
    enable(&observer, json!({"steps":[{"skill":2}]}), &[]);
    call(&observer, None, "combat.cancelTarget");
    for _ in 0..3 {
        assert_eq!(skill(&call(&observer, None, "combat.useSkill")), "2");
    }
    assert_eq!(session.live_profile_job_count(), 1);
}
#[test]
fn persistent_failure_disables_with_a_sticky_error() {
    let (_session, observer, _supervisor) =
        setup_with(|movie| movie.game_object("world.actions.active", json!({})));
    enable(&observer, json!({"steps":[{"skill":1}]}), &[]);
    call(&observer, None, "combat.cancelAutoAttack");
    call(&observer, None, "combat.cancelTarget");
    let failed = state(&observer);
    assert_eq!(failed["config"]["enabled"], false);
    assert_eq!(failed["status"]["running"], false);
    assert!(failed["config"]["profile"].is_null());
    assert_eq!(failed["status"]["classNames"], json!([]));
    let error = failed["status"]["lastError"].as_str().unwrap();
    assert!(
        error.starts_with(
            "combat profile observation failed for 5 seconds: couldn't decode refreshCombatState:"
        ),
        "{error}"
    );
    assert_eq!(state(&observer), failed);
    assert!(
        drain(&observer)
            .iter()
            .all(|name| !name.starts_with("combat."))
    );
    let restarted = enable(&observer, json!({"steps":[]}), &[]);
    assert!(restarted["status"]["lastError"].is_null());
}
#[test]
fn script_kills_have_separate_cursors_and_leave_auto_attack_running() {
    let (mut session, observer, _supervisor) = setup();
    enable(&observer, json!({"steps":[{"skill":1},{"skill":2}]}), &[]);
    assert_eq!(skill(&call(&observer, None, "combat.useSkill")), "1");
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () {yield* api.combat.kill("Frogzard",{profile:{steps:[{skill:3},{skill:4}]}});}
    "#,
        )]),
        Default::default(),
    );
    let mut auto = vec!["1".to_owned()];
    let mut script = Vec::new();
    while auto.len() < 3 || script.len() < 3 {
        let args = call(&observer, Some(&run), "combat.useSkill");
        let cast = skill(&args).to_owned();
        if cast == "1" || cast == "2" {
            auto.push(cast);
        } else {
            script.push(cast);
        }
    }
    assert_eq!(&auto[..3], ["1", "2", "1"]);
    assert_eq!(&script[..3], ["3", "4", "3"]);
    run.kill();
    drop(run);
    call(&observer, None, "combat.cancelTarget");
    let args = call(&observer, None, "combat.useSkill");
    assert!(["1", "2"].contains(&skill(&args)));
    assert_eq!(state(&observer)["config"]["enabled"], true);
}
#[test]
fn player_restart_rebuilds_auto_attack_without_stopping_the_new_player() {
    let (mut session, observer, mut supervisor) = setup();
    enable(&observer, json!({"steps":[{"skill":1}]}), &[]);
    call(&observer, None, "combat.useSkill");
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    let observer = session
        .attach(lucent_host::bridge::EventInterests::ALL)
        .unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    assert_eq!(state(&observer)["config"]["enabled"], true);
    log_in(&observer);
    area(&observer, true);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        assert!(
            Instant::now() < deadline,
            "auto attack did not resume after player restart"
        );
        let event = await_event(&observer, "onProbeCall", None);
        assert!(
            !matches!(event.args.first(),Some(Value::String(name)) if name.starts_with("combat.cancel"))
        );
        if event.args.first() == Some(&Value::String("combat.useSkill".into())) {
            break;
        }
    }
}
#[test]
fn logout_idles_past_the_failure_bound_then_resumes() {
    let (session, observer, _supervisor) = setup_with(|movie| movie.recording("combat.getTarget"));
    enable(&observer, json!({"steps":[{"skill":1}]}), &[]);
    call(&observer, None, "combat.useSkill");
    let (observed, observations) = std::sync::mpsc::channel();
    session.set_profile_observation_probe(move |_| {
        let _ = observed.send(());
    });
    connection(&observer);
    drain(&observer);
    let deadline = Instant::now() + Duration::from_millis(5500);
    while Instant::now() < deadline {
        observations.recv_timeout(TIMEOUT).unwrap();
    }
    assert!(
        drain(&observer)
            .iter()
            .all(|name| name != "combat.getTarget" && !name.starts_with("combat.cancel"))
    );
    assert_eq!(state(&observer)["config"]["enabled"], true);
    assert!(state(&observer)["status"]["lastError"].is_null());
    log_in(&observer);
    area(&observer, true);
    call(&observer, None, "combat.useSkill");
}
#[test]
fn reconnect_replaces_preparation_warnings() {
    let (session, observer, _supervisor) = setup();
    enable(
        &observer,
        json!({"consumable":"Potion","steps":[{"skill":5}]}),
        &[],
    );
    call(&observer, None, "combat.attackMonster");
    assert_eq!(
        state(&observer)["status"]["warnings"],
        json!(["Potion was not found. Skill 5 will use whichever consumable is equipped."])
    );
    let (entered, barrier) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let resume = std::sync::Mutex::new(resume);
    let held = std::sync::atomic::AtomicBool::new(false);
    session.set_profile_dispatch_probe(move |_, attack| {
        if attack && !held.swap(true, std::sync::atomic::Ordering::SeqCst) {
            entered.send(()).unwrap();
            resume.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
        }
    });
    barrier.recv_timeout(TIMEOUT).unwrap();
    connection(&observer);
    log_in(&observer);
    area(&observer, true);
    packet(
        &observer,
        json!({"cmd":"loadInventoryBig","items":[{"ItemID":100,"sName":"Potion","sType":"Item","sLink":"none","iQty":2}]}),
    );
    drain(&observer);
    release.send(()).unwrap();
    call(&observer, None, "combat.attackMonster");
    assert_eq!(
        state(&observer)["status"]["warnings"],
        json!(["Potion cannot be equipped. Skill 5 will use whichever consumable is equipped."])
    );
}
#[test]
fn failed_equip_releases_the_consumable_for_a_sibling() {
    let (_session, observer, _supervisor) =
        setup_with(|movie| movie.constant("world.isActionAvailable", json!(false)));
    packet(
        &observer,
        json!({"cmd":"loadInventoryBig","items":[{"ItemID":100,"sName":"Potion","sType":"Item","sLink":"potion","sMeta":"400","iQty":2}]}),
    );
    enable(
        &observer,
        json!({"consumable":"Potion","steps":[{"skill":5}]}),
        &[],
    );
    call(&observer, None, "combat.attackMonster");
    let claim = observer
        .acquire_consumable_claim(Some(101))
        .unwrap()
        .expect("failed preparation retained its claim");
    observer.release_claim(claim.id);
}
#[test]
fn an_enabled_profile_keeps_settings_within_their_cadence_bounds() {
    let (_session, observer, _supervisor) = setup();
    edit(
        &observer,
        json!({"kind":"updateSettings","patch":{"enemyMagnetEnabled":true}}),
    );
    enable(&observer, json!({"delayMs":50,"steps":[{"skill":1}]}), &[]);
    call(&observer, None, "combat.useSkill");
    let mut times = [Vec::new(), Vec::new()];
    let mut casts = 0;
    while times[0].len() < 6 {
        let event = await_event(&observer, "onProbeCall", None);
        match event.args.first() {
            Some(Value::String(name)) if name == "settings.setWalkSpeed" => {
                times[0].push(Instant::now())
            }
            Some(Value::String(name)) if name == "settings.enemyMagnet" => {
                times[1].push(Instant::now())
            }
            Some(Value::String(name)) if name == "combat.useSkill" => casts += 1,
            _ => {}
        }
    }
    assert!(casts >= 20);
    for (index, (times, bound)) in times
        .into_iter()
        .zip([Duration::from_millis(1500), Duration::from_secs(1)])
        .enumerate()
    {
        assert!(times.len() >= 6);
        let max = times
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .max()
            .unwrap();
        println!("auto attack settings loop {index} maximum gap {max:?}, casts {casts}");
        assert!(max <= bound);
    }
}
#[test]
fn disabled_auto_attack_adds_no_combat_reads_or_actions() {
    let (_session, observer, _supervisor) = setup_with(|movie| movie.recording("combat.getTarget"));
    disable(&observer);
    drain(&observer);
    for _ in 0..3 {
        loop {
            let event = await_event(&observer, "onProbeCall", None);
            assert!(
                !matches!(event.args.first(),Some(Value::String(name)) if name.starts_with("combat."))
            );
            if event.args.first() == Some(&Value::String("settings.setWalkSpeed".into())) {
                break;
            }
        }
    }
}

#[test]
fn enabling_while_logged_out_waits_for_login() {
    let (_session, observer, _supervisor) = setup();
    connection(&observer);
    let enabled = enable(
        &observer,
        json!({"consumable":"Potion", "steps":[{"skill":5}]}),
        &[],
    );
    assert_eq!(enabled["config"]["enabled"], true);
    assert_eq!(enabled["status"]["running"], true);
    log_in(&observer);
    area(&observer, true);
    call(&observer, None, "combat.attackMonster");
    assert!(state(&observer)["status"]["lastError"].is_null());
}

#[test]
fn class_warnings_use_ascii_case_insensitive_names_at_preparation() {
    let (_session, observer, _supervisor) =
        setup_with(|movie| movie.constant("player.getClassName", json!("Warrior")));
    for (classes, expected) in [
        (json!(["warrior"]), json!([])),
        (
            json!(["Mage"]),
            json!(["This profile is for Mage, but Warrior is equipped."]),
        ),
        (
            json!(["Mage", "Bard", "Rogue"]),
            json!(["This profile is for Mage, Bard, or Rogue, but Warrior is equipped."]),
        ),
    ] {
        enable(
            &observer,
            json!({"classNames":classes, "steps":[{"skill":1}]}),
            &[],
        );
        call(&observer, None, "combat.attackMonster");
        assert_eq!(state(&observer)["status"]["warnings"], expected);
        disable(&observer);
        drain(&observer);
    }
}

#[test]
fn invalid_profile_input_falls_back_to_generic() {
    let (_session, observer, _supervisor) = setup();
    let enabled = enable(&observer, json!(null), &[]);
    assert_eq!(enabled["config"]["profile"]["id"], "generic-base");
    assert_eq!(skill(&call(&observer, None, "combat.useSkill")), "1");
}

#[test]
fn a_script_kill_waits_for_the_player_to_return() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            const killed = yield* api.combat.killForItem("Frogzard", {item:"Never drops",quantity:1}, {profile:{steps:[{skill:1}]}});
            yield* script.stop(String(killed));
        }
    "#,
        )]),
        Default::default(),
    );
    call(&observer, Some(&run), "combat.useSkill");
    let (observed, observations) = std::sync::mpsc::channel();
    session.set_profile_observation_probe(move |_| {
        let _ = observed.send(());
    });
    connection(&observer);
    area(&observer, false);
    assert!(
        observer
            .read(|state| state.world.get_me().is_none())
            .unwrap()
    );
    let deadline = Instant::now() + Duration::from_millis(5500);
    while Instant::now() < deadline {
        observations.recv_timeout(TIMEOUT).unwrap();
    }
    assert!(run.wait(Duration::ZERO).is_err());
    area(&observer, true);
    call(&observer, Some(&run), "combat.useSkill");
    packet(
        &observer,
        json!({"cmd":"loadInventoryBig","items":[{"ItemID":999,"sName":"Never drops","iQty":1}]}),
    );
    packet(
        &observer,
        json!({"cmd":"ct","m":{"2":{"intHP":0,"intState":0}}}),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        lucent_protocol::script::ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
}

#[test]
fn script_repreparation_logs_each_new_warning_once() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () {yield* api.combat.killForItem("Frogzard", {item:"Never drops",quantity:1}, {profile:{consumable:"Potion",steps:[{skill:5}]}});}
    "#,
        )]),
        Default::default(),
    );
    call(&observer, Some(&run), "combat.useSkill");
    for _ in 0..2 {
        let (entered, barrier) = std::sync::mpsc::channel();
        let (release, resume) = std::sync::mpsc::channel();
        let resume = std::sync::Mutex::new(resume);
        let held = std::sync::atomic::AtomicBool::new(false);
        session.set_profile_dispatch_probe(move |_, attack| {
            if attack && !held.swap(true, std::sync::atomic::Ordering::SeqCst) {
                entered.send(()).unwrap();
                resume.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
            }
        });
        barrier.recv_timeout(TIMEOUT).unwrap();
        connection(&observer);
        log_in(&observer);
        area(&observer, true);
        packet(
            &observer,
            json!({"cmd":"loadInventoryBig","items":[{"ItemID":100,"sName":"Potion","sType":"Item","sLink":"none","iQty":2}]}),
        );
        drain(&observer);
        release.send(()).unwrap();
        call(&observer, Some(&run), "combat.useSkill");
        logs.wait_for(
            tracing::Level::WARN,
            "Potion cannot be equipped. Skill 5 will use whichever consumable is equipped.",
        );
    }
    assert_eq!(
        logs.messages(tracing::Level::WARN)
            .iter()
            .filter(|line| *line
                == "Potion cannot be equipped. Skill 5 will use whichever consumable is equipped.")
            .count(),
        1
    );
}

#[test]
fn dispatch_errors_disable_auto_attack_but_leave_script_jobs_running() {
    for method in ["combat.attackMonster", "combat.useSkill"] {
        let (mut session, observer, _supervisor) =
            setup_with(|movie| movie.throwing(method, "dispatch failed").recording(method));
        enable(&observer, json!({"steps":[{"skill":1}]}), &[]);
        let mut stopped = Vec::new();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            assert!(
                Instant::now() < deadline,
                "dispatch error did not stop auto attack"
            );
            let event = await_event(&observer, "onProbeCall", None);
            if let Some(Value::String(name)) = event.args.first() {
                if name.starts_with("combat.cancel") {
                    stopped.push(name.clone());
                }
                if name == "combat.cancelTarget" {
                    break;
                }
            }
        }
        assert_eq!(stopped, ["combat.cancelAutoAttack", "combat.cancelTarget"]);
        let failed = state(&observer);
        assert_eq!(failed["config"]["enabled"], false);
        assert_eq!(failed["status"]["running"], false);
        let error = failed["status"]["lastError"].as_str().unwrap();
        assert!(
            error.contains(method) && error.contains("dispatch failed"),
            "{error}"
        );
        assert_eq!(state(&observer), failed);
        assert!(
            !drain(&observer)
                .iter()
                .any(|name| name.starts_with("combat.cancel"))
        );
        assert!(disable(&observer)["status"]["lastError"].is_null());
        let run = start(
            &mut session,
            loose(&[(
                "fight.js",
                r#"
            import * as api from "lucent/api";
            export default function* () {
                yield* api.combat.killForItem("Frogzard", {item:"Never drops",quantity:1}, {profile:{steps:[{skill:1}]}});
            }
        "#,
            )]),
            Default::default(),
        );
        for _ in 0..3 {
            call(&observer, Some(&run), method);
        }
        assert!(run.wait(Duration::ZERO).is_err());
    }
}

#[test]
fn an_unprojected_selected_monster_reports_an_observation_failure() {
    let (_session, observer, _supervisor) = setup_with(|movie| {
        movie.constant(
            "combat.getTarget",
            json!({"type":"monster","monsterMapId":999}),
        )
    });
    enable(&observer, json!({"steps":[{"skill":1}]}), &[]);
    call(&observer, None, "combat.cancelTarget");
    assert_eq!(
        state(&observer)["status"]["lastError"],
        "combat profile observation failed for 5 seconds: combat observation failed: selected monster has no projected generation"
    );
}

#[test]
fn an_area_change_during_observation_reports_what_changed() {
    let data = json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":1,"areaName":"test-1","uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],"monBranch":[]}});
    let (_session, observer, _supervisor) = setup_with(|movie| {
        movie.responding(
            "combat.getTarget",
            [(
                json!(null),
                vec![("onExtensionResponse".into(), data.to_string())],
            )],
        )
    });
    enable(&observer, json!({"steps":[{"skill":1}]}), &[]);
    call(&observer, None, "combat.cancelTarget");
    assert_eq!(
        state(&observer)["status"]["lastError"],
        "combat profile observation failed for 5 seconds: combat observation failed: area or connection changed"
    );
}
