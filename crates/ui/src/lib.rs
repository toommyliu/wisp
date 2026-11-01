pub mod text_field;
pub mod theme;

use gpui::{prelude::*, *};

#[derive(Clone, Copy)]
pub enum ButtonStyle {
    Ghost,
    Success,
    Danger,
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    style: ButtonStyle,
    disabled: bool,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .debug_selector(|| label.to_string())
        .role(accesskit::Role::Button)
        .aria_label(label.clone())
        .focusable()
        .tab_stop(!disabled)
        .when(disabled, |d| d.aria_description("Disabled"))
        .flex()
        .items_center()
        .flex_none()
        .whitespace_nowrap()
        .justify_center()
        .h(px(27.))
        .px(px(8.))
        .rounded(px(6.))
        .text_size(px(13.))
        .text_color(if disabled {
            theme::disabled()
        } else {
            match style {
                ButtonStyle::Ghost => theme::foreground(),
                ButtonStyle::Success => theme::success(),
                ButtonStyle::Danger => theme::danger(),
            }
        })
        .when(!disabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::accent()))
                .focus(|s| {
                    s.bg(theme::accent())
                        .border_2()
                        .border_color(theme::foreground())
                })
        })
        .child(label)
}

pub fn switch_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
) -> Stateful<Div> {
    let label = label.into();
    button(id, label.clone(), ButtonStyle::Ghost, false)
        .role(accesskit::Role::Switch)
        .aria_toggled(if checked {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        })
        .aria_label(format!("{label}: {}", if checked { "On" } else { "Off" }))
        .w_full()
        .justify_between()
        .child(
            div()
                .rounded_full()
                .px_2()
                .text_color(if checked {
                    theme::success()
                } else {
                    theme::disabled()
                })
                .child(if checked { "On" } else { "Off" }),
        )
}

pub fn radio_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    let label = label.into();
    button(id, label.clone(), ButtonStyle::Ghost, false)
        .role(accesskit::Role::RadioButton)
        .aria_toggled(if selected {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        })
        .aria_selected(selected)
        .aria_label(format!(
            "{label}: {}",
            if selected { "Selected" } else { "Not selected" }
        ))
        .w_full()
        .justify_between()
        .child(if selected { "●" } else { "○" })
}

pub fn menu(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .role(accesskit::Role::Menu)
        .tab_group()
        .aria_label(label)
        .occlude()
        .flex()
        .flex_col()
        .w(px(270.))
        .max_h(px(600.))
        .overflow_y_scroll()
        .p(px(6.))
        .gap(px(2.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::border())
        .bg(theme::background())
        .text_color(theme::foreground())
        .text_size(px(13.))
        .shadow_lg()
        .on_key_down(|event, window, cx| match event.keystroke.key.as_str() {
            "up" => {
                window.focus_prev(cx);
                cx.stop_propagation();
            }
            "down" => {
                window.focus_next(cx);
                cx.stop_propagation();
            }
            _ => {}
        })
}

pub fn floating(panel: impl IntoElement) -> impl IntoElement {
    deferred(
        anchored()
            .anchor(Anchor::TopLeft)
            .snap_to_window_with_margin(px(8.))
            .child(panel),
    )
    .priority(1)
}

pub struct Tooltip(pub SharedString);
impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p_2()
            .max_w(px(420.))
            .rounded(px(6.))
            .bg(theme::accent())
            .text_color(theme::foreground())
            .text_size(px(13.))
            .child(self.0.clone())
    }
}

pub fn radio_list(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .role(accesskit::Role::RadioGroup)
        .aria_label(label)
        .flex()
        .flex_col()
}
