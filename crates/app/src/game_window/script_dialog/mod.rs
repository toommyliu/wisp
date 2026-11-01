#[cfg(test)]
mod tests;

use gpui::{prelude::*, *};
use lucent_protocol::script::{ScriptDialog, ScriptDialogKind, ScriptDialogResponse};
use lucent_ui::{ButtonStyle, button, text_field::TextField, theme};

#[derive(Clone)]
pub(super) struct DialogAnswered {
    pub id: u64,
    pub response: ScriptDialogResponse,
}

pub(super) struct ScriptDialogModal {
    current: Option<ScriptDialog>,
    answered: bool,
    field: Option<Entity<TextField>>,
    message_scroll: ScrollHandle,
    focus: FocusHandle,
    ok_focus: FocusHandle,
    cancel_focus: FocusHandle,
    return_focus: FocusHandle,
}

impl EventEmitter<DialogAnswered> for ScriptDialogModal {}

impl ScriptDialogModal {
    pub fn new(return_focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        Self {
            current: None,
            answered: false,
            field: None,
            message_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            ok_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            return_focus,
        }
    }

    pub fn sync(
        &mut self,
        current: Option<ScriptDialog>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current == current {
            return;
        }
        self.field = current.as_ref().and_then(|dialog| match &dialog.kind {
            ScriptDialogKind::Prompt { placeholder } => Some(cx.new(|cx| {
                let mut field = TextField::new(
                    "dialog-response",
                    "Response",
                    String::new(),
                    false,
                    window,
                    cx,
                );
                field.set_placeholder(placeholder.clone(), cx);
                field
            })),
            _ => None,
        });
        self.message_scroll.set_offset(point(px(0.), px(0.)));
        self.current = current;
        self.answered = false;
        window.focus(&self.focus_handle(cx), cx);
        cx.notify();
    }

    fn answer(&mut self, accept: bool, cx: &mut Context<Self>) {
        let Some(dialog) = &self.current else { return };
        if self.answered {
            return;
        }
        let response = match &dialog.kind {
            ScriptDialogKind::Alert => ScriptDialogResponse::Dismissed,
            ScriptDialogKind::Confirm => ScriptDialogResponse::Confirmed(accept),
            ScriptDialogKind::Prompt { .. } => ScriptDialogResponse::Submitted(
                accept.then(|| self.field.as_ref().unwrap().read(cx).value().to_owned()),
            ),
        };
        self.answered = true;
        cx.emit(DialogAnswered {
            id: dialog.id,
            response,
        });
        cx.notify();
    }

    fn tab(&self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut stops = vec![];
        if let Some(field) = &self.field {
            stops.push(field.focus_handle(cx));
        }
        if self
            .current
            .as_ref()
            .is_some_and(|dialog| dialog.kind != ScriptDialogKind::Alert)
        {
            stops.push(self.cancel_focus.clone());
        }
        stops.push(self.ok_focus.clone());
        let index = stops
            .iter()
            .position(|focus| focus.is_focused(window))
            .unwrap_or(stops.len() - 1);
        let next = if backwards {
            (index + stops.len() - 1) % stops.len()
        } else {
            (index + 1) % stops.len()
        };
        window.focus(&stops[next], cx);
    }
}

impl Focusable for ScriptDialogModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.current.as_ref().map(|dialog| &dialog.kind) {
            Some(ScriptDialogKind::Alert) => self.ok_focus.clone(),
            Some(ScriptDialogKind::Confirm) => self.cancel_focus.clone(),
            Some(ScriptDialogKind::Prompt { .. }) => self.field.as_ref().unwrap().focus_handle(cx),
            None => self.return_focus.clone(),
        }
    }
}

impl Render for ScriptDialogModal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(dialog) = &self.current else {
            return div().into_any_element();
        };
        let title = match dialog.kind {
            ScriptDialogKind::Alert => "Script message",
            ScriptDialogKind::Confirm => "Script needs confirmation",
            ScriptDialogKind::Prompt { .. } => "Script needs input",
        };
        let mut panel = div()
            .id("script-dialog")
            .role(match dialog.kind {
                ScriptDialogKind::Prompt { .. } => accesskit::Role::Dialog,
                _ => accesskit::Role::AlertDialog,
            })
            .aria_label(title)
            .tab_group()
            .track_focus(&self.focus)
            .w(px(440.))
            .max_w_full()
            .max_h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .rounded(px(8.))
            .bg(theme::background())
            .border_1()
            .border_color(theme::border())
            .text_color(theme::foreground())
            .text_size(px(13.))
            .shadow_lg()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        this.answer(false, cx);
                        cx.stop_propagation();
                    }
                    "enter" => {
                        if !this
                            .field
                            .as_ref()
                            .is_some_and(|field| field.read(cx).is_composing())
                        {
                            this.answer(!this.cancel_focus.is_focused(window), cx);
                            cx.stop_propagation();
                        }
                    }
                    "pageup" | "pagedown" => {
                        let scroll = &this.message_scroll;
                        let direction = if event.keystroke.key == "pageup" {
                            1.
                        } else {
                            -1.
                        };
                        let mut offset = scroll.offset();
                        offset.y = (offset.y + scroll.bounds().size.height * direction)
                            .clamp(-scroll.max_offset().y, px(0.));
                        scroll.set_offset(offset);
                        cx.notify();
                        cx.stop_propagation();
                    }
                    "tab" => {
                        this.tab(event.keystroke.modifiers.shift, window, cx);
                        cx.stop_propagation();
                    }
                    "space" if this.cancel_focus.is_focused(window) => {
                        this.answer(false, cx);
                        cx.stop_propagation();
                    }
                    "space" if this.ok_focus.is_focused(window) => {
                        this.answer(true, cx);
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .id("dialog-script")
                    .role(accesskit::Role::Label)
                    .aria_label(dialog.script.clone())
                    .text_color(theme::disabled())
                    .child(dialog.script.clone()),
            )
            .child(
                div()
                    .id("dialog-message")
                    .role(accesskit::Role::Label)
                    .aria_label(dialog.message.clone())
                    .min_h_0()
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .track_scroll(&self.message_scroll)
                    .whitespace_normal()
                    .line_height(relative(1.5))
                    .child(dialog.message.clone()),
            );
        if let Some(field) = &self.field {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Response")
                    .child(field.clone()),
            );
        }
        let mut buttons = div().flex().justify_end().gap_2();
        if dialog.kind != ScriptDialogKind::Alert {
            buttons = buttons.child(
                button("dialog-cancel", "Cancel", ButtonStyle::Ghost, self.answered)
                    .track_focus(&self.cancel_focus)
                    .on_click(cx.listener(|this, _, _, cx| this.answer(false, cx))),
            );
        }
        buttons = buttons.child(
            button("dialog-ok", "OK", ButtonStyle::Ghost, self.answered)
                .track_focus(&self.ok_focus)
                .on_click(cx.listener(|this, _, _, cx| this.answer(true, cx))),
        );
        deferred(
            div()
                .absolute()
                .inset_0()
                .size_full()
                .occlude()
                .bg(rgba(0x00000080))
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(panel.child(buttons)),
        )
        .priority(2)
        .into_any_element()
    }
}

#[cfg(feature = "overlay-check")]
pub(super) fn capture_record() -> Option<ScriptDialog> {
    let (kind, message) = match std::env::var("LUCENT_SCRIPT_DIALOG").ok()?.as_str() {
        "alert" => (ScriptDialogKind::Alert, "Your farming run has finished."),
        "confirm" => (ScriptDialogKind::Confirm, "Continue with the next quest?"),
        "prompt" => (
            ScriptDialogKind::Prompt {
                placeholder: "battleon-1".into(),
            },
            "Which map should the script visit next?",
        ),
        _ => return None,
    };
    Some(ScriptDialog {
        id: 1,
        kind,
        script: "daily-quests.js".into(),
        message: message.into(),
    })
}

pub(super) fn current(session: &lucent_host::Session) -> Option<ScriptDialog> {
    #[cfg(feature = "overlay-check")]
    if let Some(record) = capture_record() {
        return Some(record);
    }
    session.script_dialog()
}
