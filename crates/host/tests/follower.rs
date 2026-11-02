mod common;

use common::*;
use lucent_host::{ScriptLink, Session, Supervisor, bridge::Value, testing};
use lucent_protocol::query::Query;
use serde_json::{Value as Json, json};
use std::{
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

type Packet = (String, String);

fn extension(data: Json) -> Packet {
    (
        "onExtensionResponse".into(),
        json!({"type":"json","dataObj":data}).to_string(),
    )
}
fn server(text: &str) -> Packet {
    ("packetFromServer".into(), text.into())
}
fn send(link: &ScriptLink, (event, payload): Packet) {
    link.call_loader(
        "probe.emit",
        vec![Value::String(event), Value::String(payload)],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}
fn player(id: i64, name: &str, cell: &str) -> Json {
    json!({"entID":id,"uoName":name,"strFrame":cell,"strPad":"Left","intHP":100,"intHPMax":100,"intState":1})
}
fn me(cell: &str) -> Json {
    player(7, "player-a", cell)
}
fn target(cell: &str) -> Json {
    player(8, "player-b", cell)
}
fn area(map: &str, players: Vec<Json>) -> Packet {
    extension(
        json!({"cmd":"moveToArea","areaId":1,"areaName":map,"uoBranch":players,
        "mondef":[{"MonID":1,"strMonName":"Frogzard"}],
        "monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50,"intState":1}]}),
    )
}
fn moved(name: &str, cell: &str) -> Packet {
    extension(json!({"cmd":"uotls","unm":name,"o":{"strFrame":cell,"strPad":"Left"}}))
}
fn arrived(id: i64, name: &str, cell: &str) -> Packet {
    extension(json!({"cmd":"uotls","unm":name,"o":player(id, name, cell)}))
}
fn log_in_at(link: &ScriptLink, players: Vec<Json>) {
    send(link, ("onConnection".into(), "OnConnection".into()));
    send(
        link,
        extension(json!({"cmd":"loadInventoryBig",
            "items":[{"ItemID":5586,"sName":"Elders Blood","sType":"Item","iQty":20,"iStk":20}],
            "hitems":[{"ItemID":2,"sName":"Home","sType":"House","bEquip":1}]})),
    );
    send(link, area("battleon-1", players));
    for command in [
        lucent_protocol::command::Command::RefreshCredentials {},
        lucent_protocol::command::Command::RefreshLoggedIn {},
    ] {
        link.command(command).recv_timeout(TIMEOUT).unwrap();
    }
}
const LOCKED: &str = "%xt%warning%-1%Cannot goto player in a locked zone.%";

fn movie() -> testing::ProbeMovie {
    ready_movie()
        .combat_facts()
        .constant("world.getAvailableMonsterMapIds", json!([2]))
        .constant("combat.getConsumableSkillItem", json!(null))
        .constant("world.isActionAvailable", json!(true))
        .responding("combat.attackMonster", [(json!(true), vec![])])
        .responding("combat.useSkill", [(json!(true), vec![])])
        .responding("combat.cancelAutoAttack", [(json!(null), vec![])])
        .responding("combat.cancelTarget", [(json!(null), vec![])])
        .responding(
            "player.jump",
            [(json!(null), vec![moved("player-a", "r2")])],
        )
        .responding("player.getCell", [(json!("r2"), vec![])])
        .responding("player.getPad", [(json!("Left"), vec![])])
        .responding("world.isCellReady", [(json!(true), vec![])])
        .responding(
            "player.goToPlayer",
            [(
                json!(null),
                vec![area("battleon-2", vec![me("r1"), target("r1")])],
            )],
        )
        .responding("player.walkTo", [(json!(true), vec![])])
        .responding(
            "player.joinMap",
            [(json!(null), vec![area("doom-1", vec![me("r1")])])],
        )
}

fn setup(players: Vec<Json>) -> (Session, ScriptLink, Supervisor) {
    setup_with(|movie| movie, players)
}
fn setup_with(
    modify: impl FnOnce(testing::ProbeMovie) -> testing::ProbeMovie,
    players: Vec<Json>,
) -> (Session, ScriptLink, Supervisor) {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, modify(movie()));
    log_in(&link);
    send(&link, area("battleon-1", players));
    session
        .update_script_options(
            serde_json::from_value(json!({"roomPolicy":{"kind":"public"}})).unwrap(),
        )
        .unwrap();
    let supervisor = Supervisor::start(&mut session, None).unwrap();
    await_ready(&link);
    drain(&link);
    (session, link, supervisor)
}
fn await_ready(link: &ScriptLink) {
    let deadline = Instant::now() + TIMEOUT;
    while !link
        .read(|projection| projection.readiness.can_act())
        .unwrap()
    {
        assert!(Instant::now() < deadline, "the player never became ready");
        std::thread::yield_now();
    }
}

fn config(overrides: Json) -> Json {
    let mut config = json!({"targetName":"player-b","combatEnabled":true,"copyWalk":false,
        "retryEnabled":true,"maxAttempts":3,"selectedProfileId":"generic-base","attackPriority":[],
        "lockedZoneFallbacks":[],"lockedZoneRoomOverride":""});
    config
        .as_object_mut()
        .unwrap()
        .extend(overrides.as_object().unwrap().clone());
    config
}
fn command(link: &ScriptLink, value: Json) -> Json {
    serde_json::from_str(
        &link
            .command(serde_json::from_value(value).unwrap())
            .recv_timeout(TIMEOUT)
            .unwrap(),
    )
    .unwrap()
}
fn start(link: &ScriptLink, overrides: Json) -> Json {
    command(
        link,
        json!({"kind":"startFollower","config":config(overrides),"profile":{"steps":[{"skill":1}]}}),
    )
}
fn stop(link: &ScriptLink) -> Json {
    command(link, json!({"kind":"stopFollower"}))
}
fn record(link: &ScriptLink) -> Json {
    serde_json::from_str(
        &link
            .query(serde_json::from_value::<Query>(json!({"kind":"follower"})).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn await_record(link: &ScriptLink, done: impl Fn(&Json) -> bool) -> Json {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let record = record(link);
        if done(&record) {
            return record;
        }
        assert!(Instant::now() < deadline, "record never matched: {record}");
        std::thread::yield_now();
    }
}
fn name(args: &[Value]) -> String {
    match args.first() {
        Some(Value::String(name)) => name.clone(),
        other => panic!("{other:?}"),
    }
}
fn args(call: &[Value]) -> Json {
    lucent_protocol::bridge::to_json(call[1].clone()).unwrap_or_default()
}
fn until(link: &ScriptLink, method: &str) -> Vec<(String, Json)> {
    let deadline = Instant::now() + TIMEOUT;
    let mut calls = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "missing {method}: {calls:?}");
        let event = await_event(link, "onProbeCall", None);
        let call = (name(&event.args), args(&event.args));
        let done = call.0 == method;
        calls.push(call);
        if done {
            return calls;
        }
    }
}
fn drain(link: &ScriptLink) -> Vec<String> {
    link.call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let mut calls = Vec::new();
    while let Some(event) = link.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
        {
            calls.push(name(&event.args));
        }
    }
    calls
}
fn names(calls: &[(String, Json)]) -> Vec<&str> {
    calls.iter().map(|(name, _)| name.as_str()).collect()
}
fn issues(session: &Session) -> mpsc::Receiver<(String, Instant)> {
    let (sender, issued) = mpsc::channel();
    session.set_follower_issue_probe(move |name| {
        let _ = sender.send((name.to_owned(), Instant::now()));
    });
    issued
}

#[test]
fn a_target_in_another_cell_is_followed_by_jump_before_combat() {
    let (_session, link, _supervisor) = setup(vec![me("r1"), target("r2")]);
    let started = start(&link, json!({}));
    assert_eq!(started["status"]["enabled"], true);
    assert_eq!(started["status"]["running"], true);
    assert_eq!(started["config"]["targetName"], "player-b");
    let calls = until(&link, "combat.attackMonster");
    assert_eq!(
        names(&calls),
        [
            "player.jump",
            "player.getCell",
            "player.getPad",
            "world.isCellReady",
            "combat.attackMonster"
        ]
    );
    assert_eq!(calls[0].1, json!(["r2", "Left"]));
    let fighting = await_record(&link, |record| record["status"]["phase"] == "combat");
    assert_eq!(fighting["status"]["attemptsRemaining"], 3);
    assert!(fighting["status"]["lastError"].is_null());
}

#[test]
fn an_absent_target_is_reached_by_goto_before_combat() {
    let (_session, link, _supervisor) = setup(vec![me("r1")]);
    start(&link, json!({}));
    let calls = until(&link, "combat.attackMonster");
    assert_eq!(names(&calls), ["player.goToPlayer", "combat.attackMonster"]);
    assert_eq!(calls[0].1, json!(["player-b"]));
    await_record(&link, |record| record["status"]["phase"] == "combat");
}

#[test]
fn a_locked_zone_warning_raised_inside_the_goto_moves_to_the_fallback() {
    let (_session, link, _supervisor) = setup_with(
        |movie| {
            movie
                .responding("player.goToPlayer", [(json!(null), vec![server(LOCKED)])])
                .responding(
                    "player.joinMap",
                    [(
                        json!(null),
                        vec![area("doom-42", vec![me("r1"), target("r1")])],
                    )],
                )
        },
        vec![me("r1")],
    );
    start(
        &link,
        json!({"lockedZoneFallbacks":["doom","dage"],"lockedZoneRoomOverride":"42"}),
    );
    let calls = until(&link, "combat.attackMonster");
    assert_eq!(
        names(&calls),
        [
            "player.goToPlayer",
            "player.joinMap",
            "combat.attackMonster"
        ]
    );
    assert_eq!(calls[1].1, json!(["doom-42"]));
    let record = await_record(&link, |record| record["status"]["phase"] == "combat");
    assert_eq!(record["status"]["attemptsRemaining"], 3);
}

#[test]
fn fallbacks_without_the_target_join_each_map_once_then_stop() {
    let (_session, link, _supervisor) = setup_with(
        |movie| {
            movie
                .responding("player.goToPlayer", [(json!(null), vec![server(LOCKED)])])
                .responding(
                    "player.joinMap",
                    [
                        (json!(null), vec![area("doom-1", vec![me("r1")])]),
                        (json!(null), vec![area("dage-1", vec![me("r1")])]),
                    ],
                )
        },
        vec![me("r1")],
    );
    start(
        &link,
        json!({"maxAttempts":1,"lockedZoneFallbacks":["doom","dage"]}),
    );
    let stopped = await_record(&link, |record| record["status"]["running"] == false);
    assert_eq!(stopped["status"]["enabled"], false);
    assert_eq!(stopped["status"]["phase"], "stopped");
    assert_eq!(stopped["status"]["stoppedReason"], "Room join failed");
    assert_eq!(
        stopped["status"]["lastError"],
        "Destination room is full or unreachable"
    );
    let calls = until(&link, "combat.cancelTarget");
    let joins: Vec<_> = calls
        .iter()
        .filter(|(name, _)| name == "player.joinMap")
        .map(|(_, args)| args.clone())
        .collect();
    assert_eq!(joins, [json!(["doom"]), json!(["dage"])]);
    assert!(
        drain(&link)
            .iter()
            .all(|name| !name.starts_with("player.") && !name.starts_with("combat."))
    );
}

#[test]
fn stop_sends_one_cancel_pair_and_nothing_after() {
    let (session, link, _supervisor) = setup(vec![me("r1"), target("r1")]);
    start(&link, json!({}));
    until(&link, "combat.attackMonster");
    let stopped = stop(&link);
    assert_eq!(stopped["status"]["enabled"], false);
    assert_eq!(stopped["status"]["running"], false);
    assert_eq!(stopped["status"]["phase"], "stopped");
    assert_eq!(stopped["status"]["stoppedReason"], "Stopped by user");
    until(&link, "combat.cancelTarget");
    let after = drain(&link);
    assert!(
        after
            .iter()
            .all(|name| !name.starts_with("player.") && !name.starts_with("combat.")),
        "{after:?}"
    );
    assert_eq!(session.live_profile_job_count(), 0);
    assert_eq!(session.live_travel_job_count(), 0);
    let pairs = stop(&link);
    assert_eq!(pairs["status"]["stoppedReason"], "Stopped by user");
    assert!(
        drain(&link).iter().all(|name| !name.starts_with("combat.")),
        "a second stop sent another pair"
    );
}

#[test]
fn an_empty_target_stops_with_the_release_texts() {
    let (_session, link, _supervisor) = setup(vec![me("r1")]);
    let stopped = start(&link, json!({"targetName":"  "}));
    assert_eq!(stopped["status"]["enabled"], false);
    assert_eq!(stopped["status"]["phase"], "stopped");
    assert_eq!(stopped["status"]["lastError"], "Target name is required");
    assert_eq!(stopped["status"]["stoppedReason"], "Target not found");
    let calls = until(&link, "combat.cancelTarget");
    assert_eq!(
        names(&calls),
        ["combat.cancelAutoAttack", "combat.cancelTarget"]
    );
}

#[test]
fn configuring_while_running_changes_nothing() {
    let (_session, link, _supervisor) = setup(vec![me("r1"), target("r1")]);
    start(&link, json!({}));
    let unchanged = command(
        &link,
        json!({"kind":"configureFollower","config":config(json!({"targetName":"player-c"}))}),
    );
    assert_eq!(unchanged["config"]["targetName"], "player-b");
    stop(&link);
    let configured = command(
        &link,
        json!({"kind":"configureFollower","config":config(json!({"targetName":" Player-C ","attackPriority":["id:3","Frogzard"]}))}),
    );
    assert_eq!(configured["config"]["targetName"], "player-c");
    assert_eq!(
        configured["config"]["attackPriority"],
        json!([3, "Frogzard"])
    );
    assert!(configured["profile"].is_null());
    assert!(configured["status"]["stoppedReason"].is_null());
}

#[test]
fn a_readiness_edge_wakes_a_waiting_follower_without_readiness_calls() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        movie()
            .recording("world.isLoaded")
            .recording("auth.isLoggedIn"),
    );
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    let issued = issues(&session);
    let started = Instant::now();
    let waiting = start(&link, json!({}));
    assert_eq!(waiting["status"]["running"], true);
    log_in_at(&link, vec![me("r1"), target("r2")]);
    let (first, at) = issued.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(first, "jump");
    assert!(
        at - started < Duration::from_secs(4),
        "the follower waited for its watchdog instead of the readiness edge"
    );
    for _ in 0..3 {
        until(&link, "combat.attackMonster");
    }
    drain(&link);
    let mut window = Vec::new();
    for _ in 0..6 {
        window.extend(
            names(&until(&link, "combat.attackMonster"))
                .into_iter()
                .map(String::from),
        );
    }
    window.extend(drain(&link));
    assert!(
        window
            .iter()
            .all(|name| name != "world.isLoaded" && name != "auth.isLoggedIn"),
        "{window:?}"
    );
}

#[test]
fn a_silent_location_change_is_caught_by_the_five_second_watchdog() {
    let (session, link, _supervisor) = setup(vec![me("r1"), target("r1")]);
    let issued = issues(&session);
    let started = Instant::now();
    start(&link, json!({"combatEnabled":false}));
    send(&link, moved("player-a", "r3"));
    let (first, at) = issued.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(first, "jump");
    assert!(
        at - started >= Duration::from_millis(5000),
        "the follower reacted to its own move before the watchdog"
    );
    let calls = until(&link, "player.jump");
    assert_eq!(calls, [("player.jump".into(), json!(["r1", "Left"]))]);
}

#[test]
fn unrelated_arrivals_never_restart_an_inflight_goto() {
    let (session, link, _supervisor) = setup_with(
        |movie| movie.responding("player.goToPlayer", [(json!(null), vec![])]),
        vec![me("r1")],
    );
    let issued = issues(&session);
    start(&link, json!({"lockedZoneFallbacks":["doom"]}));
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "exit");
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "goto");
    until(&link, "player.goToPlayer");
    send(&link, arrived(9, "player-c", "r1"));
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "join");
    assert_eq!(record(&link)["status"]["attemptsRemaining"], 3);
}

#[test]
fn the_target_arriving_preempts_an_inflight_goto() {
    let (session, link, _supervisor) = setup_with(
        |movie| movie.responding("player.goToPlayer", [(json!(null), vec![])]),
        vec![me("r1")],
    );
    let issued = issues(&session);
    start(&link, json!({"lockedZoneFallbacks":["doom"]}));
    until(&link, "player.goToPlayer");
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "exit");
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "goto");
    send(&link, arrived(8, "player-b", "r2"));
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "jump");
    let calls = until(&link, "player.jump");
    assert_eq!(calls.last().unwrap().1, json!(["r2", "Left"]));
    assert_eq!(record(&link)["status"]["attemptsRemaining"], 3);
}

#[test]
fn a_target_moving_during_a_jump_redirects_it() {
    let (session, link, _supervisor) = setup_with(
        |movie| movie.responding("world.isCellReady", [(json!(false), vec![])]),
        vec![me("r1"), target("r2")],
    );
    let issued = issues(&session);
    start(&link, json!({"combatEnabled":false}));
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "jump");
    until(&link, "world.isCellReady");
    send(&link, moved("player-b", "r3"));
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "jump");
    let jumps: Vec<_> = until(&link, "player.jump")
        .into_iter()
        .filter(|(name, _)| name == "player.jump")
        .collect();
    assert_eq!(jumps.last().unwrap().1, json!(["r3", "Left"]));
    assert_eq!(record(&link)["status"]["attemptsRemaining"], 3);
}

#[test]
fn copy_walk_walks_to_the_target_destination() {
    let (_session, link, _supervisor) = setup(vec![me("r1"), target("r1")]);
    start(&link, json!({"copyWalk":true,"combatEnabled":false}));
    send(
        &link,
        extension(json!({"cmd":"uotls","unm":"player-b","o":{"tx":5.7,"ty":6.2}})),
    );
    let calls = until(&link, "player.walkTo");
    assert_eq!(calls.last().unwrap().1, json!([5.0, 6.0]));
}

#[test]
fn script_kills_leave_the_follower_running() {
    let (mut session, link, _supervisor) = setup(vec![me("r1"), target("r1")]);
    start(&link, json!({}));
    until(&link, "combat.attackMonster");
    let run = common::start(
        &mut session,
        loose(&[(
            "spin.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { while (true) { yield* api.player.isReady(); } }
    "#,
        )]),
        Default::default(),
    );
    run.kill();
    drop(run);
    send(&link, moved("player-b", "r2"));
    let calls = until(&link, "player.jump");
    assert_eq!(calls.last().unwrap().1, json!(["r2", "Left"]));
    let record = record(&link);
    assert_eq!(record["status"]["enabled"], true);
    assert!(record["status"]["lastError"].is_null());
}

#[test]
fn a_player_restart_rebuilds_the_follower_from_its_record() {
    let (mut session, link, mut supervisor) = setup(vec![me("r1"), target("r1")]);
    start(&link, json!({}));
    until(&link, "combat.attackMonster");
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    let link = session
        .attach(lucent_host::bridge::EventInterests::ALL)
        .unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&link);
    let rebuilt = record(&link);
    assert_eq!(rebuilt["status"]["enabled"], true);
    assert_eq!(rebuilt["status"]["running"], true);
    log_in_at(&link, vec![me("r1"), target("r2")]);
    let calls = until(&link, "player.jump");
    assert_eq!(calls.last().unwrap().1, json!(["r2", "Left"]));
    until(&link, "combat.attackMonster");
}

#[test]
fn a_long_logout_idles_then_follows_after_login() {
    let (session, link, _supervisor) = setup(vec![me("r1"), target("r1")]);
    start(&link, json!({}));
    until(&link, "combat.attackMonster");
    let (observed, observations) = mpsc::channel();
    session.set_profile_observation_probe(move |_| {
        let _ = observed.send(());
    });
    send(&link, ("onConnection".into(), "OnConnectionLost".into()));
    drain(&link);
    let deadline = Instant::now() + Duration::from_millis(5500);
    while Instant::now() < deadline {
        observations.recv_timeout(TIMEOUT).unwrap();
    }
    let calls = drain(&link);
    assert!(
        calls
            .iter()
            .all(|name| !name.starts_with("player.") && !name.starts_with("combat.")),
        "{calls:?}"
    );
    let idle = record(&link);
    assert_eq!(idle["status"]["enabled"], true);
    assert_eq!(idle["status"]["attemptsRemaining"], 3);
    assert!(idle["status"]["lastError"].is_null());
    log_in_at(&link, vec![me("r1"), target("r2")]);
    let calls = until(&link, "player.jump");
    assert_eq!(calls.last().unwrap().1, json!(["r2", "Left"]));
}

#[test]
fn a_script_moving_the_player_and_the_follower_keep_their_own_results() {
    let (mut session, link, _supervisor) = setup_with(
        |movie| {
            movie.responding(
                "player.joinMap",
                [(
                    json!(null),
                    vec![area("doom-1", vec![me("r1"), target("r1")])],
                )],
            )
        },
        vec![me("r1"), target("r1")],
    );
    start(&link, json!({}));
    until(&link, "combat.attackMonster");
    let run = common::start(
        &mut session,
        loose(&[(
            "move.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () { yield* script.stop(String(yield* api.player.joinMap("doom"))); }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        lucent_protocol::script::ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
    drop(run);
    until(&link, "combat.attackMonster");
    let record = record(&link);
    assert_eq!(record["status"]["enabled"], true);
    assert_eq!(record["status"]["attemptsRemaining"], 3);
    assert!(record["status"]["lastError"].is_null());
}

#[test]
fn native_auto_attack_and_the_follower_keep_separate_cursors() {
    let (_session, link, _supervisor) = setup(vec![me("r1"), target("r1")]);
    command(
        &link,
        json!({"kind":"enableAutoAttack","profile":{"steps":[{"skill":1},{"skill":2}]},"targetPriority":[]}),
    );
    command(
        &link,
        json!({"kind":"startFollower","config":config(json!({})),"profile":{"steps":[{"skill":3},{"skill":4}]}}),
    );
    let skill = |call: &(String, Json)| call.1[0].as_str().unwrap().to_owned();
    let mut auto = Vec::new();
    let mut follower = Vec::new();
    while auto.len() < 3 || follower.len() < 3 {
        let call = until(&link, "combat.useSkill").pop().unwrap();
        match skill(&call).as_str() {
            "1" | "2" => auto.push(skill(&call)),
            _ => follower.push(skill(&call)),
        }
    }
    assert_eq!(auto[..3], ["1", "2", "1"]);
    assert_eq!(follower[..3], ["3", "4", "3"]);
    command(&link, json!({"kind":"disableAutoAttack"}));
    drain(&link);
    let call = until(&link, "combat.useSkill").pop().unwrap();
    assert!(["3", "4"].contains(&skill(&call).as_str()));
    assert_eq!(record(&link)["status"]["running"], true);
    command(
        &link,
        json!({"kind":"enableAutoAttack","profile":{"steps":[{"skill":1}]},"targetPriority":[]}),
    );
    stop(&link);
    drain(&link);
    let call = until(&link, "combat.useSkill").pop().unwrap();
    assert_eq!(skill(&call), "1");
    let auto_attack: Json = serde_json::from_str(
        &link
            .query(serde_json::from_value::<Query>(json!({"kind":"autoAttack"})).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(auto_attack["config"]["enabled"], true);
}

struct Hold {
    release: Option<mpsc::Sender<()>>,
    _read: lucent_host::bridge::PendingRead<()>,
}
impl Drop for Hold {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}
fn hold(session: &Session) -> Hold {
    let (entered, held) = mpsc::channel();
    let (release, resume) = mpsc::channel::<()>();
    let read = session.read_projection(move |_| {
        let _ = entered.send(());
        let _ = resume.recv_timeout(TIMEOUT);
    });
    held.recv_timeout(TIMEOUT).unwrap();
    Hold {
        release: Some(release),
        _read: read,
    }
}
fn gate() -> (
    impl Fn() + Send + Sync + 'static,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let (reached, arrivals) = mpsc::channel();
    let (proceed, waits) = mpsc::channel::<()>();
    let reached = Mutex::new(reached);
    let waits = Mutex::new(waits);
    (
        move || {
            let _ = reached.lock().unwrap().send(());
            let _ = waits.lock().unwrap().recv_timeout(TIMEOUT);
        },
        arrivals,
        proceed,
    )
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Jump,
    Goto,
    Combat,
}
#[derive(Clone, Copy, Debug)]
enum Ending {
    Stop,
    ConnectionLost,
}

#[test]
fn stopping_or_losing_the_connection_mid_operation_issues_no_queued_call() {
    for phase in [Phase::Jump, Phase::Goto, Phase::Combat] {
        for ending in [Ending::Stop, Ending::ConnectionLost] {
            let players = match phase {
                Phase::Jump => vec![me("r1"), target("r2")],
                Phase::Goto => vec![me("r1")],
                Phase::Combat => vec![me("r1"), target("r1")],
            };
            let (session, link, _supervisor) = setup_with(
                |movie| movie.responding("player.goToPlayer", [(json!(null), vec![])]),
                players,
            );
            let (probe, arrivals, proceed) = gate();
            let armed = std::sync::atomic::AtomicBool::new(true);
            let once = move |hit: bool| {
                if hit && armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    probe();
                }
            };
            match phase {
                Phase::Combat => session.set_profile_dispatch_probe(move |_, attack| once(attack)),
                Phase::Jump => session.set_follower_issue_probe(move |name| once(name == "jump")),
                Phase::Goto => session.set_follower_issue_probe(move |name| once(name == "goto")),
            }
            let started = link.command(
                serde_json::from_value(json!({"kind":"startFollower","config":config(json!({})),
                    "profile":{"steps":[{"skill":1}]}}))
                .unwrap(),
            );
            arrivals.recv_timeout(TIMEOUT).unwrap();
            let held = hold(&session);
            if matches!(ending, Ending::ConnectionLost) {
                let _ = link.call_loader(
                    "probe.emit",
                    vec![
                        Value::String("onConnection".into()),
                        Value::String("OnConnectionLost".into()),
                    ],
                );
            }
            proceed.send(()).unwrap();
            started.recv_timeout(TIMEOUT).unwrap();
            if matches!(ending, Ending::Stop) {
                assert_eq!(stop(&link)["status"]["running"], false);
            }
            drop(held);
            let forbidden = match phase {
                Phase::Jump => "player.jump",
                Phase::Goto => "player.goToPlayer",
                Phase::Combat => "combat.attackMonster",
            };
            if matches!(ending, Ending::ConnectionLost) && matches!(phase, Phase::Jump) {
                let deadline = Instant::now() + TIMEOUT;
                while session.live_travel_job_count() != 0 {
                    assert!(
                        Instant::now() < deadline,
                        "the jump outlived the connection"
                    );
                    std::thread::yield_now();
                }
                drain(&link);
                continue;
            }
            let calls = drain(&link);
            assert!(
                !calls.iter().any(|name| name == forbidden),
                "{phase:?} {ending:?}: {calls:?}"
            );
            if matches!(ending, Ending::Stop) {
                assert_eq!(
                    calls
                        .iter()
                        .filter(|name| *name == "combat.cancelAutoAttack")
                        .count(),
                    1,
                    "{phase:?}: {calls:?}"
                );
            }
        }
    }
}

#[test]
fn a_player_restart_mid_goto_rebuilds_without_the_old_goto() {
    let (mut session, link, mut supervisor) = setup_with(
        |movie| movie.responding("player.goToPlayer", [(json!(null), vec![])]),
        vec![me("r1")],
    );
    start(&link, json!({}));
    until(&link, "player.goToPlayer");
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    let link = session
        .attach(lucent_host::bridge::EventInterests::ALL)
        .unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&link);
    let issued = issues(&session);
    assert!(
        drain(&link)
            .iter()
            .all(|name| !name.starts_with("player.") && !name.starts_with("combat."))
    );
    log_in_at(&link, vec![me("r1"), target("r2")]);
    assert_eq!(issued.recv_timeout(TIMEOUT).unwrap().0, "jump");
    let calls = until(&link, "player.jump");
    assert!(
        names(&calls)
            .iter()
            .all(|name| *name != "player.goToPlayer" && !name.starts_with("combat.cancel")),
        "the old goto reached the new player: {calls:?}"
    );
    assert_eq!(calls.last().unwrap().1, json!(["r2", "Left"]));
    assert_eq!(record(&link)["status"]["attemptsRemaining"], 3);
}

#[test]
fn combat_dispatch_errors_keep_the_follower_running() {
    let (_session, link, _supervisor) = setup_with(
        |movie| {
            movie
                .throwing("combat.attackMonster", "attack failed")
                .recording("combat.attackMonster")
        },
        vec![me("r1"), target("r1")],
    );
    start(&link, json!({}));
    for _ in 0..3 {
        until(&link, "combat.attackMonster");
    }
    let record = record(&link);
    assert_eq!(record["status"]["running"], true);
    assert!(record["status"]["lastError"].is_null());
}

#[test]
fn a_failing_combat_profile_stops_the_follower() {
    let (_session, link, _supervisor) = setup_with(
        |movie| movie.game_object("world.actions.active", json!({})),
        vec![me("r1"), target("r1")],
    );
    start(&link, json!({}));
    let stopped = await_record(&link, |record| record["status"]["running"] == false);
    assert_eq!(stopped["status"]["enabled"], false);
    assert_eq!(stopped["status"]["stoppedReason"], "Combat profile failed");
    let error = stopped["status"]["lastError"].as_str().unwrap();
    assert!(
        error.starts_with("combat profile observation failed for 5 seconds"),
        "{error}"
    );
    let calls = until(&link, "combat.cancelTarget");
    assert_eq!(
        calls
            .iter()
            .filter(|(name, _)| name == "combat.cancelAutoAttack")
            .count(),
        1
    );
    assert!(drain(&link).iter().all(|name| !name.starts_with("combat.")),);
}
