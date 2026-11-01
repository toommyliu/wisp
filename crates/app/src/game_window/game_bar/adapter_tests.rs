use super::*;
use core::prelude::v1::test;
use lucent_host::testing::{self, ProbeMovie};
use std::time::Duration;

#[test]
fn session_records_follow_bar_actions() {
    let host = testing::headless_host();
    let movie = [
        "settings.setOtherPlayersVisible",
        "settings.setWalkSpeed",
        "settings.setFrameRate",
        "settings.setCustomName",
        "settings.setCustomGuild",
        "settings.resetCustomName",
        "settings.resetCustomGuild",
        "settings.setLagKillerEnabled",
    ]
    .into_iter()
    .fold(
        ProbeMovie::new()
            .constant("auth.isLoggedIn", serde_json::json!(false))
            .constant("world.isLoaded", serde_json::json!(false))
            .constant("player.isLoaded", serde_json::json!(false))
            .constant("flash.isNull", serde_json::json!(true)),
        |movie, callback| movie.constant(callback, serde_json::Value::Null),
    );
    let (mut session, _link) = testing::start_probe(&host, movie);
    session.supervise(None).unwrap();
    let mut state = BarState::default();
    state.read(&session);
    assert_eq!(state.script_action(), None);
    assert_eq!(state.settings.walk_speed, 8.);
    for action in [
        BarAction::Toggle(Toggle::HidePlayers, true),
        BarAction::WalkSpeed(99.),
        BarAction::Fps(60.),
        BarAction::CustomName(Some("Test".into())),
        BarAction::CustomGuild(Some("Guild".into())),
        BarAction::Rendering(RenderingMode::Minimal),
        BarAction::ZoneEnabled(true),
        BarAction::ZoneMap(Some(lucent_protocol::auto_zone::AutoZoneMap::Ultradage)),
        BarAction::Delay(5.),
        BarAction::ReloginEnabled(true),
        BarAction::Attack(true),
    ] {
        apply(&mut session, action.clone())
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .unwrap_or_else(|error| panic!("{action:?}: {error}"));
    }
    state.read(&session);
    assert_eq!(state.settings.walk_speed, 99.);
    assert_eq!(state.settings.frame_rate, 60.);
    assert!(!state.settings.other_players_visible);
    assert_eq!(state.settings.custom_name, "Test");
    assert_eq!(state.settings.custom_guild, "Guild");
    assert_eq!(state.settings.rendering_mode, RenderingMode::Minimal);
    assert!(state.zone_enabled);
    assert_eq!(
        state.zone_map,
        Some(lucent_protocol::auto_zone::AutoZoneMap::Ultradage)
    );
    assert!(state.relogin_enabled);
    assert_eq!(state.delay, 5.);
    assert!(state.attack_enabled);
    assert_eq!(state.attack_profile, "Generic");
    apply(&mut session, BarAction::Attack(false))
        .unwrap()
        .recv_timeout(Duration::from_secs(15))
        .unwrap();
    state.read(&session);
    assert!(!state.attack_enabled);
    for action in [BarAction::CustomName(None), BarAction::CustomGuild(None)] {
        apply(&mut session, action.clone())
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .unwrap_or_else(|error| panic!("{action:?}: {error}"));
    }
    state.read(&session);
    assert!(!state.settings.custom_name_configured);
    assert!(!state.settings.custom_guild_configured);
}

#[gpui::test]
fn adapter_controls_stop_kill_and_cancel_reconnect(cx: &mut gpui::TestAppContext) {
    use lucent_protocol::{
        bridge::Value,
        script::{ModulePath, Script},
    };
    use std::{
        collections::BTreeMap,
        sync::{Arc, mpsc},
        task::{Wake, Waker},
        time::Instant,
    };
    struct Changed(mpsc::Sender<()>);
    impl Wake for Changed {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    let host = testing::headless_host();
    let movie = [
        "settings.setAnimationsEnabled",
        "settings.setCollisionsEnabled",
        "settings.resetCustomGuild",
        "settings.resetCustomName",
        "settings.setDeathAdsVisible",
        "settings.setOtherPlayersVisible",
        "settings.setWalkSpeed",
        "settings.setLagKillerEnabled",
        "settings.setFrameRate",
        "settings.enemyMagnet",
        "settings.infiniteRange",
        "settings.provokeCell",
        "settings.skipCutscenes",
    ]
    .into_iter()
    .fold(
        ProbeMovie::new()
            .constant("auth.isLoggedIn", serde_json::json!(true))
            .constant("world.isLoaded", serde_json::json!(true))
            .constant("player.isLoaded", serde_json::json!(true))
            .constant("flash.getGameObjectS", serde_json::json!("\"player-a\""))
            .constant("flash.callGameFunction0", serde_json::json!("true")),
        |movie, callback| movie.constant(callback, serde_json::Value::Null),
    );
    let (mut session, observer) = testing::start_probe(&host, movie);
    for packet in [
        serde_json::json!({"type":"json","dataObj":{"cmd":"loadInventoryBig","items":[],"hitems":[]}}),
        serde_json::json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":1,"areaName":"battleon-1","uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}}),
    ] {
        observer
            .call_loader(
                "probe.emit",
                vec![
                    Value::String("onExtensionResponse".into()),
                    Value::String(packet.to_string()),
                ],
            )
            .recv_timeout(Duration::from_secs(15))
            .unwrap();
    }
    session.supervise(None).unwrap();
    let (tx, changes) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    for (action, name, outcome) in [
        (BarAction::Stop, "stop.js", "stop.js was stopped"),
        (BarAction::Kill, "kill.js", "kill.js was killed"),
    ] {
        let entry = ModulePath::loose(name).unwrap();
        let source = "import * as script from 'lucent/script'; export default function* () { yield* script.sleep('1 hour'); }";
        session
            .start_script(
                Script::new(
                    entry.clone(),
                    BTreeMap::from([(entry, source.into())]),
                    BTreeMap::new(),
                )
                .unwrap(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while session.script_status().phase != Phase::Running {
            changes
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        }
        assert!(apply(&mut session, action).is_none());
        while session.script_status().phase != Phase::Idle {
            changes
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        }
        assert_eq!(session.script_status().last_end.as_deref(), Some(outcome));
    }
    session
        .update_script_options(lucent_protocol::script_options::ScriptOptionsPatch {
            restart_after_reconnect: Some(true),
            ..Default::default()
        })
        .unwrap();
    let entry = ModulePath::loose("restart.js").unwrap();
    session.start_script(Script::new(entry.clone(), BTreeMap::from([(entry,
        "import * as script from 'lucent/script'; export default function* () { yield* script.sleep('1 hour'); }".into())]), BTreeMap::new()).unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while session.script_status().phase != Phase::Running {
        changes
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    observer
        .call_loader(
            "probe.emit",
            vec![
                Value::String("onConnection".into()),
                Value::String("OnConnectionLost".into()),
            ],
        )
        .recv_timeout(Duration::from_secs(15))
        .unwrap();
    while session.script_lifecycle().active.is_some()
        || !session.script_lifecycle().waiting_to_restart
        || session.script_status().phase != Phase::Waiting
    {
        changes
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    use std::{cell::RefCell, rc::Rc};
    let actions = Rc::new(RefCell::new(Vec::new()));
    let recorded = actions.clone();
    let (bar, cx) = cx.add_window_view(|window, cx| {
        let mut bar = GameBar::new(cx.focus_handle(), window, cx);
        bar.state.loaded = Some("restart.js".into());
        bar.state.read(&session);
        cx.subscribe_self(move |_, action: &BarAction, _| {
            recorded.borrow_mut().push(action.clone())
        })
        .detach();
        bar
    });
    assert_eq!(bar.read_with(cx, |bar, _| bar.state.phase), Phase::Waiting);
    cx.run_until_parked();
    let stop = cx.debug_bounds("Stop").unwrap();
    cx.simulate_click(stop.center(), Default::default());
    assert_eq!(*actions.borrow(), vec![BarAction::Stop]);
    apply(&mut session, actions.borrow_mut().pop().unwrap());
    while session.script_lifecycle().waiting_to_restart {
        changes
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    assert_eq!(session.script_status().phase, Phase::Idle);
}

#[test]
fn disabled_relogin_has_no_waiting_status() {
    let host = testing::headless_host();
    let session = host.start_session(testing::headless_surface()).unwrap();
    let mut state = BarState::default();
    state.read(&session);
    assert_eq!(state.relogin_status, "");
}
