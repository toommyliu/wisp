use super::super::controls::sync_script_dialog;
use super::super::game_bar::{BarAction, GameBar, Toggle};
use super::*;
use core::prelude::v1::test;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Responses = Rc<RefCell<Vec<(u64, ScriptDialogResponse)>>>;

struct Harness {
    modal: Entity<ScriptDialogModal>,
    bar: Entity<GameBar>,
    actions: Rc<RefCell<Vec<BarAction>>>,
    game: FocusHandle,
    clicks: Rc<Cell<usize>>,
    game_keys: Rc<Cell<usize>>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        let keys = self.game_keys.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.bar.clone())
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .relative()
                    .child(
                        div()
                            .id("player")
                            .size_full()
                            .track_focus(&self.game)
                            .on_key_down(move |_, _, _| keys.set(keys.get() + 1))
                            .child(
                                canvas(
                                    |bounds, window, _| {
                                        window.insert_hitbox(bounds, HitboxBehavior::Normal)
                                    },
                                    move |_, hitbox, window, _| {
                                        window.on_mouse_event(
                                            move |_: &MouseDownEvent, phase, window, _| {
                                                if phase == DispatchPhase::Bubble
                                                    && hitbox.is_hovered(window)
                                                {
                                                    clicks.set(clicks.get() + 1);
                                                }
                                            },
                                        );
                                    },
                                )
                                .size_full(),
                            ),
                    )
                    .child(self.modal.clone()),
            )
    }
}

fn setup(
    kind: ScriptDialogKind,
    cx: &mut TestAppContext,
) -> (Entity<Harness>, &mut VisualTestContext, Responses) {
    setup_current(Some(dialog(42, kind)), cx)
}

fn dialog(id: u64, kind: ScriptDialogKind) -> ScriptDialog {
    ScriptDialog {
        id,
        kind,
        script: "dialogs.js".into(),
        message: "A question for the player".into(),
    }
}

fn setup_current(
    current: Option<ScriptDialog>,
    cx: &mut TestAppContext,
) -> (Entity<Harness>, &mut VisualTestContext, Responses) {
    cx.update(lucent_ui::text_field::init);
    let responses: Responses = Default::default();
    let recorded = responses.clone();
    let (harness, cx) = cx.add_window_view(move |window, cx| {
        let game = cx.focus_handle();
        window.focus(&game, cx);
        let actions: Rc<RefCell<Vec<BarAction>>> = Default::default();
        let recorded_actions = actions.clone();
        let modal = cx.new(|cx| {
            let modal = ScriptDialogModal::new(game.clone(), cx);
            cx.subscribe_self(move |_, response: &DialogAnswered, _| {
                recorded
                    .borrow_mut()
                    .push((response.id, response.response.clone()))
            })
            .detach();
            modal
        });
        let bar = cx.new(|cx| GameBar::new(game.clone(), window, cx));
        sync_script_dialog(current, &modal, &bar, window, cx);
        cx.subscribe(&bar, move |_, _, action: &BarAction, _| {
            recorded_actions.borrow_mut().push(action.clone());
        })
        .detach();
        Harness {
            modal,
            bar,
            actions,
            game,
            clicks: Default::default(),
            game_keys: Default::default(),
        }
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (harness, cx, responses)
}

fn refresh(h: &Entity<Harness>, current: Option<ScriptDialog>, cx: &mut VisualTestContext) {
    h.update_in(cx, |h, window, cx| {
        sync_script_dialog(current, &h.modal, &h.bar, window, cx);
    });
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
}

#[gpui::test]
fn every_dialog_renders_its_controls_and_default_focus(cx: &mut TestAppContext) {
    for kind in [
        ScriptDialogKind::Alert,
        ScriptDialogKind::Confirm,
        ScriptDialogKind::Prompt {
            placeholder: "A hint".into(),
        },
    ] {
        let is_prompt = matches!(kind, ScriptDialogKind::Prompt { .. });
        let cancel = kind != ScriptDialogKind::Alert;
        let is_confirm = kind == ScriptDialogKind::Confirm;
        let (h, cx, _) = setup(kind, cx);
        assert!(cx.debug_bounds("OK").is_some());
        assert_eq!(cx.debug_bounds("Cancel").is_some(), cancel);
        assert_eq!(cx.debug_bounds("dialog-response").is_some(), is_prompt);
        let focus = h.read_with(cx, |h, cx| {
            let modal = h.modal.read(cx);
            modal
                .field
                .as_ref()
                .map(|field| field.focus_handle(cx))
                .unwrap_or_else(|| {
                    if is_confirm {
                        modal.cancel_focus.clone()
                    } else {
                        modal.ok_focus.clone()
                    }
                })
        });
        assert!(cx.update(|window, _| focus.is_focused(window)));
    }
}

#[gpui::test]
fn enter_and_escape_return_each_kinds_answer(cx: &mut TestAppContext) {
    for (kind, key, expected) in [
        (
            ScriptDialogKind::Alert,
            "enter",
            ScriptDialogResponse::Dismissed,
        ),
        (
            ScriptDialogKind::Alert,
            "escape",
            ScriptDialogResponse::Dismissed,
        ),
        (
            ScriptDialogKind::Confirm,
            "enter",
            ScriptDialogResponse::Confirmed(false),
        ),
        (
            ScriptDialogKind::Confirm,
            "escape",
            ScriptDialogResponse::Confirmed(false),
        ),
        (
            ScriptDialogKind::Prompt {
                placeholder: "hint".into(),
            },
            "enter",
            ScriptDialogResponse::Submitted(Some(String::new())),
        ),
        (
            ScriptDialogKind::Prompt {
                placeholder: "hint".into(),
            },
            "escape",
            ScriptDialogResponse::Submitted(None),
        ),
    ] {
        let (_, cx, responses) = setup(kind, cx);
        press(cx, key);
        assert_eq!(*responses.borrow(), [(42, expected)]);
    }
}

#[gpui::test]
fn prompt_submits_edited_text_and_ignores_repeat_answers(cx: &mut TestAppContext) {
    let (_, cx, responses) = setup(
        ScriptDialogKind::Prompt {
            placeholder: "hint".into(),
        },
        cx,
    );
    cx.simulate_input("answer");
    press(cx, "enter");
    press(cx, "escape");
    assert_eq!(
        *responses.borrow(),
        [(42, ScriptDialogResponse::Submitted(Some("answer".into())))]
    );
}

#[gpui::test]
fn modal_occludes_player_clicks_and_keeps_keyboard_focus(cx: &mut TestAppContext) {
    let (h, cx, _) = setup(ScriptDialogKind::Confirm, cx);
    cx.simulate_click(point(px(4.), px(200.)), Default::default());
    for key in ["tab", "tab", "shift-tab", "shift-tab"] {
        press(cx, key);
    }
    let (clicks, game) = h.read_with(cx, |h, _| (h.clicks.get(), h.game.clone()));
    assert_eq!(clicks, 0);
    assert!(!cx.update(|window, _| game.is_focused(window)));
}

#[gpui::test]
fn closing_restores_focus_to_the_player(cx: &mut TestAppContext) {
    let (h, cx, _) = setup(ScriptDialogKind::Alert, cx);
    refresh(&h, None, cx);
    let game = h.read_with(cx, |h, _| h.game.clone());
    assert!(cx.update(|window, _| game.is_focused(window)));
    assert!(cx.debug_bounds("OK").is_none());
}

#[gpui::test]
fn long_messages_scroll_without_hiding_the_actions(cx: &mut TestAppContext) {
    let (h, cx, _) = setup(
        ScriptDialogKind::Prompt {
            placeholder: "hint".into(),
        },
        cx,
    );
    refresh(
        &h,
        Some(ScriptDialog {
            id: 43,
            kind: ScriptDialogKind::Prompt {
                placeholder: "hint".into(),
            },
            script: "long-message.js".into(),
            message: "A long message with several words.\n".repeat(200),
        }),
        cx,
    );
    cx.simulate_resize(size(px(360.), px(360.)));
    cx.run_until_parked();
    for selector in ["OK", "Cancel", "dialog-response"] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= px(360.),
            "{selector}: {bounds:?}"
        );
        assert!(
            bounds.top() >= px(0.) && bounds.bottom() <= px(360.),
            "{selector}: {bounds:?}"
        );
    }
}

#[gpui::test]
fn buttons_send_the_same_answers_as_the_keyboard(cx: &mut TestAppContext) {
    for (kind, label, expected) in [
        (
            ScriptDialogKind::Alert,
            "OK",
            ScriptDialogResponse::Dismissed,
        ),
        (
            ScriptDialogKind::Confirm,
            "OK",
            ScriptDialogResponse::Confirmed(true),
        ),
        (
            ScriptDialogKind::Confirm,
            "Cancel",
            ScriptDialogResponse::Confirmed(false),
        ),
        (
            ScriptDialogKind::Prompt {
                placeholder: "hint".into(),
            },
            "OK",
            ScriptDialogResponse::Submitted(Some(String::new())),
        ),
        (
            ScriptDialogKind::Prompt {
                placeholder: "hint".into(),
            },
            "Cancel",
            ScriptDialogResponse::Submitted(None),
        ),
    ] {
        let (_, cx, responses) = setup(kind, cx);
        let button = cx.debug_bounds(label).unwrap();
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(*responses.borrow(), [(42, expected)]);
    }
}

#[gpui::test]
fn long_messages_can_be_read_with_page_keys(cx: &mut TestAppContext) {
    let (h, cx, _) = setup(ScriptDialogKind::Alert, cx);
    refresh(
        &h,
        Some(ScriptDialog {
            id: 43,
            kind: ScriptDialogKind::Alert,
            script: "long.js".into(),
            message: "A line of a long question.\n".repeat(200),
        }),
        cx,
    );
    let before = cx.debug_bounds("OK").unwrap();
    press(cx, "pagedown");
    let offset = h.read_with(cx, |h, cx| h.modal.read(cx).message_scroll.offset().y);
    assert!(offset < px(0.));
    press(cx, "pageup");
    assert_eq!(
        h.read_with(cx, |h, cx| h.modal.read(cx).message_scroll.offset().y),
        px(0.)
    );
    assert_eq!(cx.debug_bounds("OK").unwrap(), before);
}

fn assert_enter_cancels(
    kind: ScriptDialogKind,
    expected: ScriptDialogResponse,
    cx: &mut TestAppContext,
) {
    let (h, cx, responses) = setup(kind, cx);
    let cancel = h.read_with(cx, |h, cx| h.modal.read(cx).cancel_focus.clone());
    for _ in 0..3 {
        if cx.update(|window, _| cancel.is_focused(window)) {
            break;
        }
        press(cx, "tab");
    }
    assert!(cx.update(|window, _| cancel.is_focused(window)));
    press(cx, "enter");
    assert_eq!(*responses.borrow(), [(42, expected)]);
}

#[gpui::test]
fn enter_on_confirm_cancel_returns_false(cx: &mut TestAppContext) {
    assert_enter_cancels(
        ScriptDialogKind::Confirm,
        ScriptDialogResponse::Confirmed(false),
        cx,
    );
}

#[gpui::test]
fn enter_on_prompt_cancel_returns_null(cx: &mut TestAppContext) {
    assert_enter_cancels(
        ScriptDialogKind::Prompt {
            placeholder: "hint".into(),
        },
        ScriptDialogResponse::Submitted(None),
        cx,
    );
}

#[gpui::test]
fn enter_activates_ok_when_ok_has_focus(cx: &mut TestAppContext) {
    for (kind, expected) in [
        (
            ScriptDialogKind::Confirm,
            ScriptDialogResponse::Confirmed(true),
        ),
        (
            ScriptDialogKind::Prompt {
                placeholder: "hint".into(),
            },
            ScriptDialogResponse::Submitted(Some(String::new())),
        ),
    ] {
        let (h, cx, responses) = setup(kind, cx);
        let ok = h.read_with(cx, |h, cx| h.modal.read(cx).ok_focus.clone());
        for _ in 0..3 {
            if cx.update(|window, _| ok.is_focused(window)) {
                break;
            }
            press(cx, "tab");
        }
        assert!(cx.update(|window, _| ok.is_focused(window)));
        press(cx, "enter");
        assert_eq!(*responses.borrow(), [(42, expected)]);
    }
}

#[gpui::test]
fn bar_menus_act_above_dialogs_while_player_clicks_stay_blocked(cx: &mut TestAppContext) {
    for kind in [
        ScriptDialogKind::Confirm,
        ScriptDialogKind::Prompt {
            placeholder: "hint".into(),
        },
    ] {
        let (h, cx, responses) = setup(kind, cx);
        let options = cx.debug_bounds("Options ▾").unwrap();
        cx.simulate_click(options.center(), Default::default());
        cx.run_until_parked();
        let item = cx.debug_bounds("Infinite Range").unwrap();
        cx.simulate_click(item.center(), Default::default());
        cx.run_until_parked();
        h.read_with(cx, |h, cx| {
            assert_eq!(
                *h.actions.borrow(),
                [BarAction::Toggle(Toggle::InfiniteRange, true)]
            );
            assert_eq!(h.clicks.get(), 0);
            assert!(h.modal.read(cx).current.is_some());
        });
        assert!(responses.borrow().is_empty());
        cx.simulate_click(point(px(4.), px(700.)), Default::default());
        cx.run_until_parked();
        assert_eq!(h.read_with(cx, |h, _| h.clicks.get()), 0);
    }
}

#[gpui::test]
fn closing_a_bar_menu_keeps_player_keyboard_focus_blocked(cx: &mut TestAppContext) {
    for kind in [
        ScriptDialogKind::Confirm,
        ScriptDialogKind::Prompt {
            placeholder: "hint".into(),
        },
    ] {
        let (h, cx, responses) = setup(kind, cx);
        let options = cx.debug_bounds("Options ▾").unwrap();
        cx.simulate_click(options.center(), Default::default());
        cx.run_until_parked();
        press(cx, "escape");
        assert!(responses.borrow().is_empty());
        let game = h.read_with(cx, |h, _| h.game.clone());
        assert!(!cx.update(|window, _| game.is_focused(window)));
        press(cx, "enter");
        assert_eq!(responses.borrow().len(), 1);
    }
}

#[gpui::test]
fn arriving_dialog_closes_the_menu_and_updates_its_return_focus(cx: &mut TestAppContext) {
    for kind in [
        ScriptDialogKind::Alert,
        ScriptDialogKind::Confirm,
        ScriptDialogKind::Prompt {
            placeholder: "hint".into(),
        },
    ] {
        for previous in [
            None,
            Some(dialog(
                41,
                ScriptDialogKind::Prompt {
                    placeholder: "previous".into(),
                },
            )),
        ] {
            let (h, cx, responses) = setup_current(previous, cx);
            click(cx, "Options ▾");
            click(cx, "custom-name");
            refresh(&h, Some(dialog(42, kind.clone())), cx);
            assert!(cx.debug_bounds("Infinite Range").is_none());
            let focus = h.read_with(cx, |h, cx| h.modal.focus_handle(cx));
            assert!(cx.update(|window, _| focus.is_focused(window)));
            assert!(responses.borrow().is_empty());
            click(cx, "Options ▾");
            press(cx, "escape");
            assert!(cx.debug_bounds("Infinite Range").is_none());
            assert!(responses.borrow().is_empty());
            assert!(cx.update(|window, _| focus.is_focused(window)));
            if matches!(kind, ScriptDialogKind::Prompt { .. }) {
                cx.simulate_input("answer");
                press(cx, "enter");
                assert_eq!(
                    *responses.borrow(),
                    [(42, ScriptDialogResponse::Submitted(Some("answer".into())))]
                );
            } else {
                press(cx, "escape");
                let expected = if kind == ScriptDialogKind::Alert {
                    ScriptDialogResponse::Dismissed
                } else {
                    ScriptDialogResponse::Confirmed(false)
                };
                assert_eq!(*responses.borrow(), [(42, expected)]);
            }
            assert_eq!(h.read_with(cx, |h, _| h.game_keys.get()), 0);
        }
    }
}

#[gpui::test]
fn closing_dialog_preserves_menu_focus_until_the_menu_closes(cx: &mut TestAppContext) {
    for kind in [
        ScriptDialogKind::Alert,
        ScriptDialogKind::Confirm,
        ScriptDialogKind::Prompt {
            placeholder: "hint".into(),
        },
    ] {
        for field in [false, true] {
            let (h, cx, responses) = setup(kind.clone(), cx);
            click(cx, "Options ▾");
            if field {
                click(cx, "custom-name");
                cx.simulate_input("before");
            }
            let focus = cx.update(|window, cx| window.focused(cx).unwrap());
            refresh(&h, None, cx);
            assert!(cx.debug_bounds("Infinite Range").is_some());
            assert!(cx.update(|window, _| focus.is_focused(window)));
            refresh(&h, None, cx);
            assert!(cx.update(|window, _| focus.is_focused(window)));
            if field {
                cx.simulate_input("after");
                press(cx, "enter");
                assert_eq!(
                    *h.read_with(cx, |h, _| h.actions.clone()).borrow(),
                    [BarAction::CustomName(Some("beforeafter".into()))]
                );
            }
            press(cx, "escape");
            assert!(cx.debug_bounds("Infinite Range").is_none());
            assert!(responses.borrow().is_empty());
            let game = h.read_with(cx, |h, _| h.game.clone());
            assert!(cx.update(|window, _| game.is_focused(window)));
            assert_eq!(h.read_with(cx, |h, _| h.game_keys.get()), 0);
            press(cx, "a");
            assert_eq!(h.read_with(cx, |h, _| h.game_keys.get()), 1);
        }
    }
}
