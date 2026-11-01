use super::*;
use core::prelude::v1::test;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Responses = Rc<RefCell<Vec<(u64, ScriptDialogResponse)>>>;

struct Harness {
    modal: Entity<ScriptDialogModal>,
    game: FocusHandle,
    clicks: Rc<Cell<usize>>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        div()
            .size_full()
            .relative()
            .child(
                div()
                    .id("player")
                    .size_full()
                    .track_focus(&self.game)
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
            .child(self.modal.clone())
    }
}

fn setup(
    kind: ScriptDialogKind,
    cx: &mut TestAppContext,
) -> (Entity<Harness>, &mut VisualTestContext, Responses) {
    cx.update(lucent_ui::text_field::init);
    let responses: Responses = Default::default();
    let recorded = responses.clone();
    let (harness, cx) = cx.add_window_view(move |window, cx| {
        let game = cx.focus_handle();
        let modal = cx.new(|cx| {
            let mut modal = ScriptDialogModal::new(game.clone(), cx);
            cx.subscribe_self(move |_, response: &DialogAnswered, _| {
                recorded
                    .borrow_mut()
                    .push((response.id, response.response.clone()))
            })
            .detach();
            modal.sync(
                Some(ScriptDialog {
                    id: 42,
                    kind,
                    script: "dialogs.js".into(),
                    message: "A question for the player".into(),
                }),
                window,
                cx,
            );
            modal
        });
        Harness {
            modal,
            game,
            clicks: Default::default(),
        }
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (harness, cx, responses)
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
                .unwrap_or_else(|| modal.ok_focus.clone())
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
            ScriptDialogResponse::Confirmed(true),
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
    cx.simulate_click(point(px(4.), px(4.)), Default::default());
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
    h.update_in(cx, |h, window, cx| {
        h.modal.update(cx, |modal, cx| modal.sync(None, window, cx))
    });
    cx.run_until_parked();
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
    h.update_in(cx, |h, window, cx| {
        h.modal.update(cx, |modal, cx| {
            modal.sync(
                Some(ScriptDialog {
                    id: 43,
                    kind: ScriptDialogKind::Prompt {
                        placeholder: "hint".into(),
                    },
                    script: "long-message.js".into(),
                    message: "A long message with several words.\n".repeat(200),
                }),
                window,
                cx,
            );
        })
    });
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
    h.update_in(cx, |h, window, cx| {
        h.modal.update(cx, |modal, cx| {
            modal.sync(
                Some(ScriptDialog {
                    id: 43,
                    kind: ScriptDialogKind::Alert,
                    script: "long.js".into(),
                    message: "A line of a long question.\n".repeat(200),
                }),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
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
