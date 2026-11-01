mod controls;
mod game_bar;
#[cfg(feature = "overlay-check")]
mod overlay_check;
mod player_view;
mod script_dialog;

use gpui::{
    App, AppContext as _, Bounds, Context, Entity, Focusable as _, IntoElement, ParentElement as _,
    Render, Styled as _, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowOptions, div, px, rgb, size,
};
use lucent_host::Host;
use lucent_host::ScriptRequest;

use player_view::{PlayerView, PlayerViewError};

const WINDOW_WIDTH: f32 = 1024.;
const WINDOW_HEIGHT: f32 = 798.;
const TAB_STRIP_HEIGHT: f32 = 30.;

pub fn open(
    host: &Host,
    script: Option<ScriptRequest>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<GameWindow>> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Lucent".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        cx.new(|cx| GameWindow::new(host, script, window, cx))
    })
}

pub struct GameWindow {
    player: Result<Entity<PlayerView>, PlayerViewError>,
    bar: Option<Entity<game_bar::GameBar>>,
    dialog: Option<Entity<script_dialog::ScriptDialogModal>>,
    _dialog_actions: Option<gpui::Subscription>,
    _actions: Option<gpui::Subscription>,
    _watch: Option<gpui::Task<()>>,
}

impl GameWindow {
    fn new(
        host: &Host,
        script: Option<ScriptRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> GameWindow {
        let player = PlayerView::new(host, script, window, cx);
        match &player {
            Ok(view) => window.focus(&view.focus_handle(cx), cx),
            Err(error) => tracing::error!(%error, "no player view"),
        }
        let mut dialog = None;
        let mut dialog_actions = None;
        let mut bar = None;
        let mut actions = None;
        let mut watch = None;
        if let Ok(view) = &player {
            let focus = view.focus_handle(cx);
            let modal = cx.new(|cx| script_dialog::ScriptDialogModal::new(focus.clone(), cx));
            dialog_actions = Some(cx.subscribe_in(
                &modal,
                window,
                |this, _, answer: &script_dialog::DialogAnswered, _, cx| {
                    if let Ok(view) = &this.player {
                        view.read(cx)
                            .session()
                            .respond_dialog(answer.id, answer.response.clone());
                    }
                },
            ));
            modal.update(cx, |modal, cx| {
                modal.sync(script_dialog::current(view.read(cx).session()), window, cx)
            });
            let return_focus = modal.focus_handle(cx);
            dialog = Some(modal);
            let game_bar = cx.new(|cx| game_bar::GameBar::new(return_focus, window, cx));
            actions = Some(cx.subscribe_in(
                &game_bar,
                window,
                |this, _, action: &game_bar::BarAction, window, cx| {
                    this.bar_action(action.clone(), window, cx)
                },
            ));
            let (sender, receiver) = async_channel::bounded(1);
            view.read(cx)
                .session()
                .watch(std::task::Waker::from(std::sync::Arc::new(
                    controls::BarWake(sender),
                )));
            game_bar.update(cx, |bar, cx| {
                bar.state.read(view.read(cx).session());
                bar.refresh(window, cx);
            });
            watch = Some(cx.spawn_in(window, async move |this, cx| {
                while receiver.recv().await.is_ok() {
                    if this
                        .update_in(cx, |this, window, cx| this.refresh_bar(window, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
            #[cfg(feature = "overlay-check")]
            if let Ok(name) = std::env::var("LUCENT_OPEN_MENU") {
                let capture_bar = game_bar.clone();
                window.on_next_frame(move |_, cx| {
                    capture_bar.update(cx, |bar, cx| bar.open_for_capture(&name, cx))
                });
            }
            bar = Some(game_bar);
        }
        GameWindow {
            player,
            bar,
            dialog,
            _dialog_actions: dialog_actions,
            _actions: actions,
            _watch: watch,
        }
    }
}

impl Render for GameWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let game_area = match &self.player {
            Ok(view) => div().flex_1().min_h_0().child(view.clone()),
            Err(error) => div()
                .flex_1()
                .bg(rgb(0x000000))
                .text_color(rgb(0xffffff))
                .child(error.to_string()),
        };
        #[cfg(feature = "overlay-check")]
        let game_area = if std::env::var_os("LUCENT_OPEN_MENU").is_some()
            || std::env::var_os("LUCENT_SCRIPT_DIALOG").is_some()
        {
            game_area
        } else {
            game_area.relative().children(overlay_check::overlay())
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(TAB_STRIP_HEIGHT))
                    .w_full()
                    .bg(rgb(0x1f1f1f)),
            )
            .children(self.bar.clone())
            .child(game_area.relative().children(self.dialog.clone()))
    }
}
