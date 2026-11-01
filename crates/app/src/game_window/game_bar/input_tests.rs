use super::*;
use core::prelude::v1::test;
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
struct Counts {
    down: Cell<usize>,
    moves: Cell<usize>,
    wheel: Cell<usize>,
    keys: Cell<usize>,
    bounds: Cell<Option<Bounds<Pixels>>>,
}

struct Harness {
    bar: Entity<GameBar>,
    game: FocusHandle,
    counts: Rc<Counts>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let counts = self.counts.clone();
        let keys = self.counts.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.bar.clone())
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .track_focus(&self.game)
                    .on_key_down(move |_, _, _| keys.keys.set(keys.keys.get() + 1))
                    .child(
                        canvas(
                            |bounds, window, _| {
                                window.insert_hitbox(bounds, HitboxBehavior::Normal)
                            },
                            move |bounds, hitbox, window, _| {
                                counts.bounds.set(Some(bounds));
                                let c = counts.clone();
                                let h = hitbox.clone();
                                window.on_mouse_event(
                                    move |_: &MouseDownEvent, phase, window, _| {
                                        if phase == DispatchPhase::Bubble && h.is_hovered(window) {
                                            c.down.set(c.down.get() + 1);
                                        }
                                    },
                                );
                                let c = counts.clone();
                                let h = hitbox.clone();
                                window.on_mouse_event(
                                    move |_: &MouseMoveEvent, phase, window, _| {
                                        if phase == DispatchPhase::Bubble && h.is_hovered(window) {
                                            c.moves.set(c.moves.get() + 1);
                                        }
                                    },
                                );
                                let c = counts.clone();
                                window.on_mouse_event(
                                    move |_: &ScrollWheelEvent, phase, window, _| {
                                        if phase == DispatchPhase::Bubble
                                            && hitbox.should_handle_scroll(window)
                                        {
                                            c.wheel.set(c.wheel.get() + 1);
                                        }
                                    },
                                );
                            },
                        )
                        .size_full(),
                    ),
            )
    }
}

type Recorded = Rc<std::cell::RefCell<Vec<BarAction>>>;

fn setup(
    cx: &mut gpui::TestAppContext,
) -> (
    Entity<Harness>,
    &mut gpui::VisualTestContext,
    Rc<Counts>,
    Recorded,
) {
    cx.update(lucent_ui::text_field::init);
    let counts = Rc::new(Counts::default());
    let c = counts.clone();
    let actions: Recorded = Default::default();
    let a = actions.clone();
    let (harness, vcx) = cx.add_window_view(move |window, cx| {
        let game = cx.focus_handle();
        let bar = cx.new(|cx| {
            let bar = GameBar::new(game.clone(), window, cx);
            cx.subscribe_self(move |_, action: &BarAction, _| a.borrow_mut().push(action.clone()))
                .detach();
            bar
        });
        window.focus(&game, cx);
        Harness {
            bar,
            game,
            counts: c,
        }
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    (harness, vcx, counts, actions)
}

fn game_focused(h: &Entity<Harness>, cx: &mut gpui::VisualTestContext) -> bool {
    let game = h.read_with(cx, |h, _| h.game.clone());
    cx.update(|window, _| game.is_focused(window))
}

fn far_point(counts: &Counts) -> Point<Pixels> {
    let b = counts.bounds.get().unwrap();
    point(b.right() - px(20.), b.bottom() - px(20.))
}

fn open(cx: &mut gpui::VisualTestContext, trigger: &'static str) {
    let bounds = cx.debug_bounds(trigger).unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

fn is_open(h: &Entity<Harness>, cx: &mut gpui::VisualTestContext) -> bool {
    h.read_with(cx, |h, cx| h.bar.read(cx).open.is_some())
}

#[gpui::test]
fn wheel_and_move_inside_menu_do_not_reach_game(cx: &mut gpui::TestAppContext) {
    let (h, cx, counts, _) = setup(cx);
    open(cx, "Options ▾");
    let row = cx.debug_bounds("Death Ads").unwrap().center();
    assert!(
        counts.bounds.get().unwrap().contains(&row),
        "row is over the game"
    );
    cx.simulate_mouse_move(row, None, Default::default());
    cx.simulate_event(ScrollWheelEvent {
        position: row,
        delta: ScrollDelta::Lines(point(0., 1.)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    cx.simulate_mouse_down(row, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(row, MouseButton::Left, Default::default());
    assert_eq!(counts.moves.get(), 0, "move leaked");
    assert_eq!(counts.wheel.get(), 0, "wheel leaked");
    assert_eq!(counts.down.get(), 0, "down leaked");
    assert!(is_open(&h, cx));
    let far = far_point(&counts);
    cx.simulate_event(ScrollWheelEvent {
        position: far,
        delta: ScrollDelta::Lines(point(0., 1.)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    assert_eq!(counts.wheel.get(), 1, "wheel outside menu reaches game");
}

#[gpui::test]
fn outside_click_closes_and_returns_focus(cx: &mut gpui::TestAppContext) {
    let (h, cx, counts, _) = setup(cx);
    for button in MouseButton::all() {
        open(cx, "Options ▾");
        assert!(is_open(&h, cx));
        assert!(!game_focused(&h, cx));
        let before = counts.keys.get();
        cx.simulate_keystrokes("a");
        assert_eq!(
            counts.keys.get(),
            before,
            "key reached game while menu focused"
        );
        let far = far_point(&counts);
        cx.simulate_mouse_down(far, button, Default::default());
        cx.simulate_mouse_up(far, button, Default::default());
        cx.run_until_parked();
        assert!(!is_open(&h, cx), "{button:?}: outside click closes");
        assert!(game_focused(&h, cx), "{button:?}: focus returned to game");
        cx.simulate_keystrokes("a");
        assert_eq!(counts.keys.get(), before + 1, "keys reach game after close");
    }
}

#[gpui::test]
fn dragging_from_a_menu_keeps_it_open(cx: &mut gpui::TestAppContext) {
    let (h, cx, counts, _) = setup(cx);
    open(cx, "Options ▾");
    let field = cx.debug_bounds("walk-speed").unwrap();
    cx.simulate_mouse_down(field.center(), MouseButton::Left, Default::default());
    let far = far_point(&counts);
    cx.simulate_mouse_move(far, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(far, MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(
        is_open(&h, cx),
        "finishing a field drag outside keeps the menu open"
    );
    assert!(!game_focused(&h, cx));
}

#[gpui::test]
fn escape_returns_focus_and_keys(cx: &mut gpui::TestAppContext) {
    let (h, cx, counts, _) = setup(cx);
    for trigger in [
        "Options ▾",
        "Auto Zone ▾",
        "Auto Relogin ▾",
        "Auto Attack · Generic ▾",
    ] {
        open(cx, trigger);
        assert!(is_open(&h, cx), "{trigger} opens");
        let before = counts.keys.get();
        cx.simulate_keystrokes("escape");
        assert!(!is_open(&h, cx), "{trigger} escape closes");
        assert!(game_focused(&h, cx), "{trigger} focus back");
        assert_eq!(
            counts.keys.get(),
            before,
            "{trigger}: escape leaked to game"
        );
        cx.simulate_keystrokes("x");
        assert_eq!(counts.keys.get(), before + 1, "{trigger}: keys reach game");
    }
}

#[gpui::test]
fn typing_in_field_does_not_reach_game(cx: &mut gpui::TestAppContext) {
    let (h, cx, counts, actions) = setup(cx);
    open(cx, "Options ▾");
    let field = cx.debug_bounds("custom-name").unwrap();
    cx.simulate_click(field.center(), Default::default());
    cx.simulate_input("Hello");
    cx.simulate_keystrokes("a b space backspace left right 1 2 3");
    assert_eq!(counts.keys.get(), 0, "typing leaked");
    cx.simulate_keystrokes("enter");
    assert!(
        matches!(actions.borrow().last(), Some(BarAction::CustomName(Some(s))) if s.starts_with("Hello")),
        "{:?}",
        actions.borrow()
    );
    assert!(is_open(&h, cx), "enter keeps menu open");
    cx.simulate_keystrokes("escape");
    assert!(!is_open(&h, cx), "escape from field closes menu");
    assert!(game_focused(&h, cx));
    cx.simulate_keystrokes("z");
    assert_eq!(counts.keys.get(), 1);
}

#[gpui::test]
fn trigger_click_toggles_closed(cx: &mut gpui::TestAppContext) {
    let (h, cx, _, _) = setup(cx);
    for trigger in [
        "Options ▾",
        "Auto Zone ▾",
        "Auto Relogin ▾",
        "Auto Attack · Generic ▾",
    ] {
        for redraw_while_pressed in [false, true] {
            open(cx, trigger);
            assert!(is_open(&h, cx));
            let bounds = cx.debug_bounds(trigger).unwrap();
            cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Default::default());
            if redraw_while_pressed {
                cx.update(|window, cx| window.draw(cx).clear());
                cx.run_until_parked();
            }
            cx.simulate_mouse_up(bounds.center(), MouseButton::Left, Default::default());
            cx.run_until_parked();
            assert!(!is_open(&h, cx), "clicking {trigger} closes its menu");
            assert!(game_focused(&h, cx));
        }
    }
}

#[gpui::test]
fn load_disabled_unless_idle(cx: &mut gpui::TestAppContext) {
    let (h, cx, _, actions) = setup(cx);
    for phase in [
        Phase::Waiting,
        Phase::Starting,
        Phase::Running,
        Phase::Stopping,
    ] {
        h.update(cx, |h, cx| {
            h.bar.update(cx, |bar, cx| {
                bar.state.loaded = Some("test.js".into());
                bar.state.phase = phase;
                cx.notify()
            })
        });
        cx.run_until_parked();
        let load = cx.debug_bounds("Load").unwrap();
        cx.simulate_click(load.center(), Default::default());
        let label = if phase == Phase::Stopping {
            "Kill"
        } else {
            "Stop"
        };
        let button = cx.debug_bounds(label).unwrap();
        cx.simulate_click(button.center(), Default::default());
    }
    assert_eq!(
        *actions.borrow(),
        vec![
            BarAction::Stop,
            BarAction::Stop,
            BarAction::Stop,
            BarAction::Kill
        ]
    );
}

fn press(cx: &mut gpui::VisualTestContext, key: &str) {
    cx.update(|window, cx| window.draw(cx).clear());
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    cx.run_until_parked();
}

#[gpui::test]
fn bar_actions_return_keyboard_input_to_game(cx: &mut gpui::TestAppContext) {
    let (h, cx, counts, actions) = setup(cx);
    for (phase, label, expected, next_phase) in [
        (Phase::Idle, "Start", BarAction::Start, Phase::Running),
        (Phase::Running, "Stop", BarAction::Stop, Phase::Stopping),
        (Phase::Stopping, "Kill", BarAction::Kill, Phase::Idle),
        (Phase::Idle, "Load", BarAction::Load, Phase::Idle),
    ] {
        h.update(cx, |h, cx| {
            h.bar.update(cx, |bar, cx| {
                bar.state.loaded = Some("test.js".into());
                bar.state.phase = phase;
                cx.notify();
            })
        });
        cx.run_until_parked();
        actions.borrow_mut().clear();
        open(cx, label);
        assert_eq!(*actions.borrow(), vec![expected.clone()]);
        h.update(cx, |h, cx| {
            h.bar.update(cx, |bar, cx| {
                bar.state.phase = next_phase;
                cx.notify();
            })
        });
        cx.run_until_parked();
        let before = counts.keys.get();
        press(cx, "enter");
        press(cx, "space");
        press(cx, "1");
        assert_eq!(counts.keys.get() - before, 3, "{label} keeps game input");
        assert_eq!(
            *actions.borrow(),
            vec![expected],
            "keys never repeat {label}"
        );
        assert!(game_focused(&h, cx));
    }
}

#[gpui::test]
fn relogin_delay_has_room_to_edit(cx: &mut gpui::TestAppContext) {
    let (_, cx, _, _) = setup(cx);
    open(cx, "Auto Relogin ▾");
    let field = cx.debug_bounds("delay").unwrap();
    assert!(
        field.size.width >= px(100.),
        "delay field width: {:?}",
        field.size.width
    );
}

#[gpui::test]
fn attack_without_an_issue_starts_with_its_switch(cx: &mut gpui::TestAppContext) {
    let (_, cx, _, _) = setup(cx);
    open(cx, "Auto Zone ▾");
    let top = cx.debug_bounds("Auto Zone").unwrap().top();
    press(cx, "escape");
    open(cx, "Auto Attack · Generic ▾");
    assert_eq!(cx.debug_bounds("Auto Attack").unwrap().top(), top);
}

#[gpui::test]
fn numeric_resets_are_disabled_at_the_default(cx: &mut gpui::TestAppContext) {
    let (_, cx, _, actions) = setup(cx);
    open(cx, "Options ▾");
    for field in ["walk-speed", "fps"] {
        let reset = cx.debug_bounds("Reset").unwrap();
        let field = cx.debug_bounds(field).unwrap();
        cx.simulate_click(
            point(reset.center().x, field.center().y),
            Default::default(),
        );
        cx.run_until_parked();
        assert!(
            actions.borrow().is_empty(),
            "default reset sends no command"
        );
    }
}

#[gpui::test]
fn zone_map_can_be_cleared(cx: &mut gpui::TestAppContext) {
    let (h, cx, _, actions) = setup(cx);
    h.update(cx, |h, cx| {
        h.bar.update(cx, |bar, cx| {
            bar.state.zone_map = Some(lucent_protocol::auto_zone::AutoZoneMap::Ultradage);
            cx.notify();
        })
    });
    cx.run_until_parked();
    open(cx, "Auto Zone ▾");
    let none = cx
        .debug_bounds("None")
        .expect("None clears the selected map");
    cx.simulate_click(none.center(), Default::default());
    assert_eq!(actions.borrow().len(), 1);
    let lucent_protocol::command::Command::PatchAutoZone { patch } =
        super::command(actions.borrow()[0].clone()).unwrap()
    else {
        panic!()
    };
    assert_eq!(patch.map, Some(None));
    assert_eq!(patch.enabled, None);
}
