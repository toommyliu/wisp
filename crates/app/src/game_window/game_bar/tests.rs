use super::*;
use core::prelude::v1::test;
use lucent_protocol::{auto_zone::AutoZoneMap, command::Command};

#[test]
fn script_controls_follow_phase() {
    let mut state = BarState::default();
    assert_eq!(state.script_action(), None);
    state.loaded = Some("test.js".into());
    assert_eq!(state.script_action(), Some(BarAction::Start));
    for phase in [Phase::Waiting, Phase::Starting, Phase::Running] {
        state.phase = phase;
        assert_eq!(state.script_action(), Some(BarAction::Stop));
    }
    state.phase = Phase::Stopping;
    assert_eq!(state.script_action(), Some(BarAction::Kill));
    assert_eq!(state.script_label(), "Kill");
}

#[test]
fn hide_players_inverts_visibility() {
    let Command::UpdateSettings { patch } =
        command(BarAction::Toggle(Toggle::HidePlayers, true)).unwrap()
    else {
        panic!()
    };
    assert_eq!(patch.other_players_visible, Some(false));
}

#[test]
fn settings_preserve_unedited_fields() {
    for (toggle, key) in [
        (Toggle::InfiniteRange, "infiniteRangeEnabled"),
        (Toggle::ProvokeCell, "provokeCellEnabled"),
        (Toggle::EnemyMagnet, "enemyMagnetEnabled"),
        (Toggle::HidePlayers, "otherPlayersVisible"),
        (Toggle::SkipCutscenes, "skipCutscenesEnabled"),
        (Toggle::AntiCounter, "antiCounterEnabled"),
        (Toggle::Animations, "animationsEnabled"),
        (Toggle::Collisions, "collisionsEnabled"),
        (Toggle::DeathAds, "deathAdsVisible"),
    ] {
        for enabled in [true, false] {
            let Command::UpdateSettings { patch } =
                command(BarAction::Toggle(toggle, enabled)).unwrap()
            else {
                panic!()
            };
            let value = serde_json::to_value(patch).unwrap();
            assert_eq!(value.as_object().unwrap().len(), 1);
            assert_eq!(
                &value[key],
                &(if toggle == Toggle::HidePlayers {
                    !enabled
                } else {
                    enabled
                })
            );
        }
    }
}

#[test]
fn automation_actions_emit_partial_updates() {
    let Command::PatchAutoZone { patch } =
        command(BarAction::ZoneMap(Some(AutoZoneMap::Ultradage))).unwrap()
    else {
        panic!()
    };
    assert_eq!(patch.map, Some(Some(AutoZoneMap::Ultradage)));
    assert_eq!(patch.enabled, None);
    let Command::PatchAutoZone { patch } = command(BarAction::ZoneEnabled(false)).unwrap() else {
        panic!()
    };
    assert_eq!(patch.enabled, Some(false));
    assert_eq!(patch.map, None);
    assert!(matches!(
        command(BarAction::ReloginEnabled(true)),
        Some(Command::SetAutoReloginEnabled { enabled: true })
    ));
    assert!(matches!(
        command(BarAction::Delay(5.)),
        Some(Command::SetAutoReloginDelay { delay_ms: 5000. })
    ));
    assert!(
        matches!(command(BarAction::Server(Some("Twilly".into()))), Some(Command::SetAutoReloginServer { server: Some(s) }) if s == "Twilly")
    );
}

#[test]
fn custom_identity_reset_is_explicit() {
    let Command::UpdateSettings { patch } = command(BarAction::CustomName(None)).unwrap() else {
        panic!()
    };
    assert_eq!(patch.custom_name, Some(None));
    let Command::UpdateSettings { patch } =
        command(BarAction::CustomGuild(Some("Guild".into()))).unwrap()
    else {
        panic!()
    };
    assert_eq!(patch.custom_guild, Some(Some("Guild".into())));
}

#[test]
fn numeric_and_rendering_actions_emit_settings() {
    let Command::UpdateSettings { patch } = command(BarAction::WalkSpeed(99.)).unwrap() else {
        panic!()
    };
    assert_eq!(patch.walk_speed, Some(99.));
    let Command::UpdateSettings { patch } = command(BarAction::Fps(60.)).unwrap() else {
        panic!()
    };
    assert_eq!(patch.frame_rate, Some(60.));
    for mode in [
        RenderingMode::Full,
        RenderingMode::InterfaceOnly,
        RenderingMode::Minimal,
    ] {
        let Command::UpdateSettings { patch } = command(BarAction::Rendering(mode)).unwrap() else {
            panic!()
        };
        assert_eq!(patch.rendering_mode, Some(mode));
    }
}

#[gpui::test]
fn clicks_emit_script_and_zone_actions(cx: &mut gpui::TestAppContext) {
    use std::{cell::RefCell, rc::Rc};
    let actions = Rc::new(RefCell::new(Vec::new()));
    let recorded = actions.clone();
    let (bar, cx) = cx.add_window_view(|window, cx| {
        let focus = cx.focus_handle();
        let bar = GameBar::new(focus, window, cx);
        cx.subscribe_self(move |_, action: &BarAction, _| {
            recorded.borrow_mut().push(action.clone())
        })
        .detach();
        bar
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("Start").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    assert!(actions.borrow().is_empty());
    bar.update(cx, |bar, cx| {
        bar.state.loaded = Some("test.js".into());
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Default::default());
    assert_eq!(*actions.borrow(), vec![BarAction::Start]);
    bar.update(cx, |bar, cx| {
        bar.state.phase = Phase::Stopping;
        cx.notify();
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("Kill").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    assert_eq!(actions.borrow().last(), Some(&BarAction::Kill));
    let bounds = cx.debug_bounds("Load").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    assert_eq!(actions.borrow().last(), Some(&BarAction::Kill));
    let bounds = cx.debug_bounds("Auto Zone ▾").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    let bounds = cx.debug_bounds("Ultra Dage").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    assert_eq!(
        actions.borrow().last(),
        Some(&BarAction::ZoneMap(Some(AutoZoneMap::Ultradage)))
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let bounds = cx.debug_bounds("Options ▾").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let keystroke = Keystroke::parse("space").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    assert_eq!(
        actions.borrow().last(),
        Some(&BarAction::Toggle(Toggle::InfiniteRange, true))
    );
}

#[gpui::test]
fn numeric_fields_commit_and_escape_reverts(cx: &mut gpui::TestAppContext) {
    use std::{cell::RefCell, rc::Rc};
    let actions = Rc::new(RefCell::new(Vec::new()));
    let recorded = actions.clone();
    cx.update(lucent_ui::text_field::init);
    let (_bar, cx) = cx.add_window_view(|window, cx| {
        let focus = cx.focus_handle();
        let bar = GameBar::new(focus, window, cx);
        cx.subscribe_self(move |_, action: &BarAction, _| {
            recorded.borrow_mut().push(action.clone())
        })
        .detach();
        bar
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let bounds = cx.debug_bounds("Options ▾").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    let bounds = cx.debug_bounds("walk-speed").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("99");
    cx.simulate_keystrokes("enter");
    assert_eq!(*actions.borrow(), vec![BarAction::WalkSpeed(99.)]);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("abc");
    cx.simulate_keystrokes("escape");
    assert_eq!(actions.borrow().len(), 1);
    cx.run_until_parked();
    _bar.read_with(cx, |bar, _| assert!(bar.open.is_none()));
    let bounds = cx.debug_bounds("Options ▾").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    let bounds = cx.debug_bounds("walk-speed").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.simulate_keystrokes("enter");
    assert_eq!(actions.borrow().last(), Some(&BarAction::WalkSpeed(99.)));
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("NaN");
    cx.simulate_keystrokes("enter");
    assert_eq!(actions.borrow().len(), 1);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("30");

    let bounds = cx.debug_bounds("Infinite Range").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    assert!(
        actions.borrow().contains(&BarAction::WalkSpeed(30.)),
        "{:?}",
        actions.borrow()
    );
}

struct OverlayHarness {
    bar: Entity<GameBar>,
    game_clicks: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Render for OverlayHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.game_clicks.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.bar.clone())
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                    move |_, hitbox, window, _| {
                        let clicks = clicks.clone();
                        window.on_mouse_event(move |_: &MouseDownEvent, phase, window, _| {
                            if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                                clicks.set(clicks.get() + 1);
                            }
                        });
                    },
                )
                .flex_1()
                .w_full(),
            )
    }
}

#[gpui::test]
fn menus_occlude_game_input_and_escape_restores_focus(cx: &mut gpui::TestAppContext) {
    let clicks = std::rc::Rc::new(std::cell::Cell::new(0));
    let recorded = clicks.clone();
    let (harness, cx) = cx.add_window_view(|window, cx| {
        let focus = cx.focus_handle();
        OverlayHarness {
            bar: cx.new(|cx| GameBar::new(focus, window, cx)),
            game_clicks: recorded,
        }
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("Options ▾").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    let bounds = cx.debug_bounds("Infinite Range").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    assert_eq!(clicks.get(), 0);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    harness.read_with(cx, |harness, cx| {
        let bar = harness.bar.read(cx);
        assert!(bar.open.is_none());
    });
    cx.simulate_click(point(px(500.), px(500.)), Default::default());
    assert_eq!(clicks.get(), 1);
}

#[test]
fn numeric_values_stay_within_game_control_ranges() {
    let Command::UpdateSettings { patch } = command(BarAction::WalkSpeed(1000.)).unwrap() else {
        panic!()
    };
    assert_eq!(patch.walk_speed, Some(99.));
    let Command::UpdateSettings { patch } = command(BarAction::Fps(0.)).unwrap() else {
        panic!()
    };
    assert_eq!(patch.frame_rate, Some(1.));
    assert!(matches!(
        command(BarAction::Delay(-1.)),
        Some(Command::SetAutoReloginDelay { delay_ms: 0. })
    ));
    assert!(matches!(
        command(BarAction::Delay(999.)),
        Some(Command::SetAutoReloginDelay { delay_ms: 300000. })
    ));
}

#[test]
fn auto_attack_uses_the_generic_profile() {
    let Some(Command::EnableAutoAttack {
        profile,
        target_priority,
    }) = command(BarAction::Attack(true))
    else {
        panic!()
    };
    assert_eq!(profile, serde_json::Value::Null);
    assert!(target_priority.is_empty());
    assert!(matches!(
        command(BarAction::Attack(false)),
        Some(Command::DisableAutoAttack {})
    ));
}

#[test]
fn number_controls_round_to_release_integers() {
    for (action, key, expected) in [
        (BarAction::WalkSpeed(8.5), "walkSpeed", 9.),
        (BarAction::WalkSpeed(8.49), "walkSpeed", 8.),
        (BarAction::Fps(24.5), "frameRate", 25.),
    ] {
        let Command::UpdateSettings { patch } = command(action).unwrap() else {
            panic!()
        };
        assert_eq!(serde_json::to_value(patch).unwrap()[key], expected);
    }
}

#[gpui::test]
fn every_menu_control_emits_its_action(cx: &mut gpui::TestAppContext) {
    use std::{cell::RefCell, rc::Rc};
    let actions = Rc::new(RefCell::new(Vec::new()));
    let recorded = actions.clone();
    cx.update(lucent_ui::text_field::init);
    let (_bar, cx) = cx.add_window_view(|window, cx| {
        let mut bar = GameBar::new(cx.focus_handle(), window, cx);
        bar.state.settings.walk_speed = 30.;
        bar.state.settings.frame_rate = 60.;
        bar.state.settings.custom_name_configured = true;
        bar.state.settings.custom_guild_configured = true;
        bar.state.servers = vec!["Twilly".into(), "Artix".into()];
        cx.subscribe_self(move |_, action: &BarAction, _| {
            recorded.borrow_mut().push(action.clone())
        })
        .detach();
        bar.refresh(window, cx);
        bar
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let click = |cx: &mut gpui::VisualTestContext, label: &'static str| {
        let bounds = cx
            .debug_bounds(label)
            .unwrap_or_else(|| panic!("missing {label}"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    };
    click(cx, "Options ▾");
    for (label, expected) in [
        (
            "Infinite Range",
            BarAction::Toggle(Toggle::InfiniteRange, true),
        ),
        ("Provoke Cell", BarAction::Toggle(Toggle::ProvokeCell, true)),
        ("Enemy Magnet", BarAction::Toggle(Toggle::EnemyMagnet, true)),
        ("Hide Players", BarAction::Toggle(Toggle::HidePlayers, true)),
        (
            "Skip Cutscenes",
            BarAction::Toggle(Toggle::SkipCutscenes, true),
        ),
        (
            "Anti-Counter",
            BarAction::Toggle(Toggle::AntiCounter, false),
        ),
        ("Animations", BarAction::Toggle(Toggle::Animations, false)),
        ("Collisions", BarAction::Toggle(Toggle::Collisions, false)),
        ("Death Ads", BarAction::Toggle(Toggle::DeathAds, false)),
    ] {
        actions.borrow_mut().clear();
        click(cx, label);
        assert_eq!(*actions.borrow(), vec![expected], "{label}");
    }
    for (index, expected) in [
        BarAction::WalkSpeed(8.),
        BarAction::Fps(24.),
        BarAction::CustomName(None),
        BarAction::CustomGuild(None),
    ]
    .into_iter()
    .enumerate()
    {
        let bounds = cx
            .debug_bounds(["walk-speed", "fps", "custom-name", "custom-guild"][index])
            .unwrap();
        actions.borrow_mut().clear();
        let reset_x = cx.debug_bounds("Reset").unwrap().center().x;
        cx.simulate_click(point(reset_x, bounds.center().y), Default::default());
        cx.run_until_parked();
        assert_eq!(*actions.borrow(), vec![expected]);
    }
    for (id, value, expected) in [
        ("walk-speed", "31", BarAction::WalkSpeed(31.)),
        ("fps", "50", BarAction::Fps(50.)),
        (
            "custom-name",
            "Test",
            BarAction::CustomName(Some("Test".into())),
        ),
        (
            "custom-guild",
            "Guild",
            BarAction::CustomGuild(Some("Guild".into())),
        ),
    ] {
        actions.borrow_mut().clear();
        click(cx, id);
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input(value);
        cx.simulate_keystrokes("enter");
        assert_eq!(*actions.borrow(), vec![expected]);
    }
    for (label, expected) in [
        ("Full", BarAction::Rendering(RenderingMode::Full)),
        (
            "Interface Only",
            BarAction::Rendering(RenderingMode::InterfaceOnly),
        ),
        ("Minimal", BarAction::Rendering(RenderingMode::Minimal)),
    ] {
        actions.borrow_mut().clear();
        click(cx, label);
        assert_eq!(*actions.borrow(), vec![expected]);
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    click(cx, "Auto Zone ▾");
    for (label, expected) in [
        ("Auto Zone", BarAction::ZoneEnabled(true)),
        ("None", BarAction::ZoneMap(None)),
        (
            "Ledgermayne",
            BarAction::ZoneMap(Some(AutoZoneMap::Ledgermayne)),
        ),
        (
            "More Skulls",
            BarAction::ZoneMap(Some(AutoZoneMap::Moreskulls)),
        ),
        (
            "Ultra Dage",
            BarAction::ZoneMap(Some(AutoZoneMap::Ultradage)),
        ),
        (
            "Dark Carnax",
            BarAction::ZoneMap(Some(AutoZoneMap::Darkcarnax)),
        ),
        (
            "Astral Shrine",
            BarAction::ZoneMap(Some(AutoZoneMap::Astralshrine)),
        ),
        (
            "Queen Iona",
            BarAction::ZoneMap(Some(AutoZoneMap::Queeniona)),
        ),
        (
            "Magnum Opus",
            BarAction::ZoneMap(Some(AutoZoneMap::Magnumopus)),
        ),
    ] {
        actions.borrow_mut().clear();
        click(cx, label);
        assert_eq!(*actions.borrow(), vec![expected], "{label}");
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    actions.borrow_mut().clear();
    click(cx, "Auto Relogin ▾");
    assert_eq!(*actions.borrow(), vec![BarAction::RefreshServers]);
    for (label, expected) in [
        ("Auto Relogin", BarAction::ReloginEnabled(true)),
        ("None", BarAction::Server(None)),
        ("Twilly", BarAction::Server(Some("Twilly".into()))),
        ("Artix", BarAction::Server(Some("Artix".into()))),
    ] {
        actions.borrow_mut().clear();
        click(cx, label);
        assert_eq!(*actions.borrow(), vec![expected]);
    }
    actions.borrow_mut().clear();
    click(cx, "delay");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("5");
    cx.simulate_keystrokes("enter");
    assert_eq!(*actions.borrow(), vec![BarAction::Delay(5.)]);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    click(cx, "Auto Attack · Generic ▾");
    actions.borrow_mut().clear();
    click(cx, "Auto Attack");
    assert_eq!(*actions.borrow(), vec![BarAction::Attack(true)]);
    actions.borrow_mut().clear();
    click(cx, "Profile: Generic");
    assert!(actions.borrow().is_empty());
}

#[test]
fn script_phases_have_readable_labels() {
    for (phase, label) in [
        (Phase::Idle, "Idle"),
        (Phase::Waiting, "Waiting for login"),
        (Phase::Preparing, "Preparing"),
        (Phase::Starting, "Starting"),
        (Phase::Running, "Running"),
        (Phase::Stopping, "Stopping"),
        (Phase::Finishing, "Finishing"),
    ] {
        let state = BarState {
            phase,
            ..Default::default()
        };
        assert_eq!(state.phase_label(), label);
    }
}

#[test]
fn relogin_status_matches_activity_and_retry_wording() {
    use lucent_protocol::auto_relogin::{AutoReloginPhase, AutoReloginRecords};
    let mut records = AutoReloginRecords::default();
    records.status.last_error = Some("Connection failed".into());
    assert_eq!(relogin_status(&records), "");
    records.config.enabled = true;
    assert_eq!(relogin_status(&records), "Waiting for login");
    records.status.captured = true;
    assert_eq!(relogin_status(&records), "Connection failed");
    records.status.attempts_remaining = Some(3.);
    assert_eq!(relogin_status(&records), "Connection failed");
    records.status.last_error = None;
    assert_eq!(relogin_status(&records), "");
    records.status.last_error = Some("Previous connection failed".into());
    for (phase, text) in [
        (AutoReloginPhase::WaitingDelay, "Waiting to reconnect"),
        (AutoReloginPhase::WaitingKick, "Waiting to reconnect"),
        (AutoReloginPhase::AwaitingPlayerReady, "Reconnecting"),
        (AutoReloginPhase::AwaitingServerSelect, "Reconnecting"),
        (
            AutoReloginPhase::Backoff { delay_ms: 3000. },
            "Reconnecting",
        ),
        (AutoReloginPhase::LoggingIn, "Reconnecting"),
        (AutoReloginPhase::SelectingServer, "Reconnecting"),
    ] {
        records.status.phase = phase;
        for (attempts, suffix) in [
            (None, ""),
            (Some(3.), " (3 retries left)"),
            (Some(1.), " (1 retry left)"),
            (Some(0.), " (final attempt)"),
        ] {
            records.status.attempts_remaining = attempts;
            assert_eq!(relogin_status(&records), format!("{text}{suffix}"));
        }
    }
    records.status.phase = AutoReloginPhase::AwaitingSession;
    records.status.captured = false;
    records.status.attempts_remaining = Some(1.);
    assert_eq!(relogin_status(&records), "Waiting for login (1 retry left)");
    records.config.enabled = false;
    assert_eq!(relogin_status(&records), "");
}

#[test]
fn relogin_delay_preserves_fractional_seconds() {
    assert!(matches!(
        command(BarAction::Delay(3.5)),
        Some(Command::SetAutoReloginDelay { delay_ms: 3500. })
    ));
}

#[gpui::test]
fn waiting_and_preparing_can_stop_and_finishing_disables_controls(cx: &mut gpui::TestAppContext) {
    use std::{cell::RefCell, rc::Rc};
    let actions = Rc::new(RefCell::new(Vec::new()));
    let recorded = actions.clone();
    let (bar, cx) = cx.add_window_view(|window, cx| {
        let focus = cx.focus_handle();
        let mut bar = GameBar::new(focus, window, cx);
        bar.state.loaded = Some("test.js".into());
        cx.subscribe_self(move |_, action: &BarAction, _| {
            recorded.borrow_mut().push(action.clone())
        })
        .detach();
        bar
    });
    for phase in [Phase::Waiting, Phase::Preparing, Phase::Finishing] {
        bar.update(cx, |bar, cx| {
            bar.state.phase = phase;
            cx.notify();
        });
        cx.run_until_parked();
        actions.borrow_mut().clear();
        let stop = cx.debug_bounds("Stop").unwrap();
        cx.simulate_click(stop.center(), Default::default());
        let load = cx.debug_bounds("Load").unwrap();
        cx.simulate_click(load.center(), Default::default());
        assert_eq!(
            *actions.borrow(),
            if phase == Phase::Finishing {
                vec![]
            } else {
                vec![BarAction::Stop]
            }
        );
    }
}
