use gpui::{prelude::*, *};
use lucent_ui::{
    ButtonStyle, button, menu, radio_row, switch_row,
    text_field::{self, TextField},
    theme,
};

struct Gallery {
    field: Entity<TextField>,
    enabled: bool,
}
impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(theme::background())
            .text_color(theme::foreground())
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child("Lucent widgets")
            .child(button("ghost", "Ghost", ButtonStyle::Ghost, false))
            .child(button("success", "Start", ButtonStyle::Success, false))
            .child(button("danger", "Kill", ButtonStyle::Danger, false))
            .child(button("disabled", "Disabled", ButtonStyle::Ghost, true))
            .child(
                menu("gallery-menu", "Options")
                    .child(
                        switch_row("switch", "Auto Zone", self.enabled).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.enabled = !this.enabled;
                                cx.notify();
                            },
                        )),
                    )
                    .child(radio_row("radio", "Full", true))
                    .child(self.field.clone()),
            )
    }
}
fn main() {
    gpui_platform::application().run(|cx| {
        text_field::init(cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(480.), px(520.)),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| Gallery {
                    field: cx.new(|cx| {
                        TextField::new("number", "Walk Speed", "8".into(), true, window, cx)
                    }),
                    enabled: false,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
