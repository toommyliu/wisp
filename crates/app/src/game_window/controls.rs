use super::{
    GameWindow,
    game_bar::{self, BarAction},
};
use gpui::{prelude::*, *};
use std::{path::PathBuf, task::Poll};

impl GameWindow {
    pub(super) fn refresh_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let (Ok(player), Some(dialog)) = (&self.player, &self.dialog) {
            dialog.update(cx, |dialog, cx| {
                dialog.sync(
                    super::script_dialog::current(player.read(cx).session()),
                    window,
                    cx,
                )
            });
        }
        if let (Ok(player), Some(bar), Some(dialog)) = (&self.player, &self.bar, &self.dialog) {
            let return_focus = dialog.focus_handle(cx);
            bar.update(cx, |bar, cx| {
                bar.return_focus = return_focus;
                bar.state.read(player.read(cx).session());
                bar.refresh(window, cx);
            });
        }
    }

    pub(super) fn bar_action(
        &mut self,
        action: BarAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            BarAction::Load => {
                let picker = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Load Script".into()),
                });
                cx.spawn_in(window, async move |this, cx| match picker.await {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.update_in(cx, |this, window, cx| {
                                this.load_script(path, false, window, cx)
                            })
                            .ok();
                        }
                    }
                    Ok(Err(error)) => {
                        this.update_in(cx, |this, _, cx| {
                            if let Some(bar) = &this.bar {
                                bar.update(cx, |bar, cx| {
                                    bar.state.error = Some(error.to_string());
                                    cx.notify();
                                });
                            }
                        })
                        .ok();
                    }
                    _ => {}
                })
                .detach();
            }
            BarAction::Start => {
                let path = self
                    .bar
                    .as_ref()
                    .and_then(|bar| bar.read(cx).state.loaded.clone());
                if let Some(path) = path {
                    self.load_script(path, true, window, cx);
                }
            }
            action => {
                let refresh_servers = action == BarAction::RefreshServers;
                let Ok(player) = &self.player else { return };
                let pending = player.update(cx, |player, _| {
                    game_bar::apply(player.session_mut(), action)
                });
                if let Some(pending) = pending {
                    cx.spawn_in(window, async move |this, cx| {
                        let result = std::future::poll_fn(|cx| {
                            pending.set_waker(cx.waker().clone());
                            pending.try_recv().map_or(Poll::Pending, Poll::Ready)
                        })
                        .await;
                        if let Err(error) = result {
                            tracing::warn!(%error, "game bar command failed");
                        }
                        if refresh_servers {
                            let read = this
                                .update(cx, |this, cx| {
                                    this.player.as_ref().ok().map(|player| {
                                        player.read(cx).session().read_projection(|p| {
                                            p.auth
                                                .servers()
                                                .iter()
                                                .map(|server| server.snapshot().data.name)
                                                .collect::<Vec<_>>()
                                        })
                                    })
                                })
                                .ok()
                                .flatten();
                            if let Some(read) = read {
                                let servers = std::future::poll_fn(|cx| {
                                    read.set_waker(cx.waker().clone());
                                    read.try_recv().map_or(Poll::Pending, Poll::Ready)
                                })
                                .await;
                                match servers {
                                    Ok(servers) => {
                                        this.update(cx, |this, cx| {
                                            if let Some(bar) = &this.bar {
                                                bar.update(cx, |bar, _| {
                                                    bar.state.servers = servers
                                                });
                                            }
                                        })
                                        .ok();
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, "couldn't read server list")
                                    }
                                }
                            }
                        }
                        this.update_in(cx, |this, window, cx| this.refresh_bar(window, cx))
                            .ok();
                    })
                    .detach();
                }
            }
        }
    }

    fn load_script(
        &mut self,
        path: PathBuf,
        start: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bar) = &self.bar else { return };
        if bar.read(cx).state.loading {
            return;
        }
        bar.update(cx, |bar, cx| {
            bar.state.loading = true;
            bar.state.error = None;
            cx.notify();
        });
        let load = cx.background_spawn(async move {
            let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
            let spec = path
                .to_str()
                .ok_or_else(|| "The script path must be valid Unicode".to_string())?;
            let script =
                lucent_host::library::load_script(spec, &cwd).map_err(|error| error.to_string())?;
            Ok::<_, String>((path, script))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            this.update_in(cx, |this, window, cx| {
                if let Some(bar) = &this.bar {
                    let result = result.and_then(|(path, script)| {
                        if start {
                            let player = this.player.as_ref().map_err(|error| error.to_string())?;
                            player
                                .update(cx, |player, _| player.session_mut().start_script(script))
                                .map_err(|error| error.to_string())?;
                        }
                        Ok(path)
                    });
                    bar.update(cx, |bar, cx| {
                        bar.state.loading = false;
                        match result {
                            Ok(path) => {
                                bar.state.loaded = Some(path);
                                bar.state.error = None;
                            }
                            Err(error) => bar.state.error = Some(error),
                        }
                        cx.notify();
                    });
                }
                this.refresh_bar(window, cx);
            })
            .ok();
        })
        .detach();
    }
}

pub(super) struct BarWake(pub async_channel::Sender<()>);
impl std::task::Wake for BarWake {
    fn wake(self: std::sync::Arc<Self>) {
        let _ = self.0.try_send(());
    }
    fn wake_by_ref(self: &std::sync::Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
