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
        BarAction::ZoneMap(lucent_protocol::auto_zone::AutoZoneMap::Ultradage),
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
