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
        command(BarAction::ZoneMap(AutoZoneMap::Ultradage)).unwrap()
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
        Some(&BarAction::ZoneMap(AutoZoneMap::Ultradage))
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let bounds = cx.debug_bounds("Options ▾").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("space");
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
