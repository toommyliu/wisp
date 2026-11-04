mod dialog;
#[cfg(test)]
mod tests;

use gpui::{prelude::*, *};
use lucent_host::Host;
use lucent_host::accounts::{Account, AccountEdit, Accounts, AccountsError, AccountsSnapshot};
use lucent_ui::{ButtonStyle, button, floating, menu, theme};
use std::{cell::Cell, rc::Rc};

#[cfg(test)]
use dialog::Field;
use dialog::{AccountDialog, DialogEvent};

const WIDTH: f32 = 980.;
const HEIGHT: f32 = 720.;

pub struct Launch(pub String);

struct ManagerWindow {
    manager: Entity<AccountManager>,
    window: Option<WindowHandle<AccountManager>>,
    bounds: Option<WindowBounds>,
}

impl Global for ManagerWindow {}

pub fn show(host: &Host, cx: &mut App) -> anyhow::Result<WindowHandle<AccountManager>> {
    if !cx.has_global::<ManagerWindow>() {
        let manager = cx.new(|cx| AccountManager::new(host.accounts().clone(), cx));
        let host = host.clone();
        cx.subscribe(&manager, move |manager, Launch(username): &Launch, cx| {
            if let Err(error) = crate::game_window::open(&host, None, Some(username.clone()), cx) {
                tracing::error!(%error, "couldn't launch an account");
                manager.update(cx, |manager, cx| {
                    manager.notice = Some(format!("Failed to launch {username}: {error}"));
                    cx.notify();
                });
            }
        })
        .detach();
        cx.set_global(ManagerWindow {
            manager,
            window: None,
            bounds: None,
        });
    }
    if let Some(window) = cx.global::<ManagerWindow>().window
        && window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Ok(window);
    }
    let state = cx.global::<ManagerWindow>();
    let manager = state.manager.clone();
    let bounds = state.bounds.unwrap_or_else(|| {
        WindowBounds::Windowed(Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx))
    });
    let options = WindowOptions {
        window_bounds: Some(bounds),
        titlebar: Some(TitlebarOptions {
            title: Some("Account Manager".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let window = cx.open_window(options, |window, cx| {
        window.on_window_should_close(cx, |window, cx| {
            cx.global_mut::<ManagerWindow>().bounds = Some(window.window_bounds());
            true
        });
        window.focus(&manager.focus_handle(cx), cx);
        manager
    })?;
    cx.global_mut::<ManagerWindow>().window = Some(window);
    Ok(window)
}

struct Removal {
    username: String,
    label: String,
    removing: bool,
    error: Option<String>,
}

pub struct AccountManager {
    accounts: Accounts,
    list: Option<Result<Vec<Account>, AccountsError>>,
    menu: Option<String>,
    dialog: Option<Entity<AccountDialog>>,
    removal: Option<Removal>,
    notice: Option<String>,
    focus: FocusHandle,
    menu_focus: FocusHandle,
    confirm_focus: FocusHandle,
    _dialog_events: Option<Subscription>,
    _watch: Task<()>,
}

impl EventEmitter<Launch> for AccountManager {}

impl AccountManager {
    pub fn new(accounts: Accounts, cx: &mut Context<Self>) -> Self {
        let (waker, changed) = crate::wake::channel();
        accounts.watch(waker);
        let reader = accounts.clone();
        let watch = cx.spawn(async move |this, cx| {
            loop {
                let snapshot = reader.snapshot().await;
                if this
                    .update(cx, |this, cx| this.show_snapshot(snapshot, cx))
                    .is_err()
                    || changed.recv().await.is_err()
                {
                    break;
                }
            }
        });
        Self {
            accounts,
            list: None,
            menu: None,
            dialog: None,
            removal: None,
            notice: None,
            focus: cx.focus_handle(),
            menu_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            _dialog_events: None,
            _watch: watch,
        }
    }

    fn show_snapshot(
        &mut self,
        snapshot: Result<AccountsSnapshot, AccountsError>,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = &snapshot {
            tracing::warn!(%error, "couldn't read accounts");
        }
        let list = snapshot.map(|snapshot| snapshot.accounts().to_vec());
        if let (Ok(accounts), Some(open)) = (&list, &self.menu)
            && !accounts.iter().any(|account| account.username() == open)
        {
            self.menu = None;
        }
        self.list = Some(list);
        cx.notify();
    }

    fn account(&self, username: &str) -> Option<&Account> {
        self.list
            .as_ref()?
            .as_ref()
            .ok()?
            .iter()
            .find(|account| account.username() == username)
    }

    fn open_dialog(
        &mut self,
        build: impl FnOnce(&mut Window, &mut Context<AccountDialog>) -> AccountDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        let dialog = cx.new(|cx| build(window, cx));
        self._dialog_events = Some(cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &DialogEvent, window, cx| match event {
                DialogEvent::Closed => this.close_dialog(window, cx),
                DialogEvent::Remove { username, label } => {
                    this.ask_removal(username.clone(), label.clone(), window, cx)
                }
            },
        ));
        self.dialog = Some(dialog);
        cx.notify();
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let accounts = self.accounts.clone();
        self.open_dialog(
            |window, cx| AccountDialog::add(accounts, window, cx),
            window,
            cx,
        );
    }

    fn edit(&mut self, username: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(account) = self.account(username).cloned() else {
            return;
        };
        let accounts = self.accounts.clone();
        self.open_dialog(
            |window, cx| AccountDialog::edit(accounts, &account, window, cx),
            window,
            cx,
        );
    }

    fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        self._dialog_events = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn ask_removal(
        &mut self,
        username: String,
        label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.removal = Some(Removal {
            username,
            label,
            removing: false,
            error: None,
        });
        window.focus(&self.confirm_focus, cx);
        cx.notify();
    }

    fn cancel_removal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .removal
            .as_ref()
            .is_some_and(|removal| removal.removing)
        {
            return;
        }
        self.removal = None;
        window.focus(&self.focus_handle(cx), cx);
        cx.notify();
    }

    fn remove(&mut self, cx: &mut Context<Self>) {
        let Some(removal) = self.removal.as_mut().filter(|removal| !removal.removing) else {
            return;
        };
        removal.removing = true;
        removal.error = None;
        let removed = self.accounts.edit(AccountEdit::DeleteAccounts {
            usernames: vec![removal.username.clone()],
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = removed.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(_) => {
                        this.removal = None;
                        this.dialog = None;
                        this._dialog_events = None;
                    }
                    Err(error) => {
                        if let Some(removal) = &mut this.removal {
                            removal.removing = false;
                            removal.error = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn launch(&mut self, username: String, cx: &mut Context<Self>) {
        self.menu = None;
        self.notice = None;
        cx.emit(Launch(username));
        cx.notify();
    }

    fn row(&self, account: &Account, cx: &mut Context<Self>) -> Stateful<Div> {
        let username = account.username().to_owned();
        let open = self.menu.as_deref() == Some(account.username());
        let row_selector = format!("account-{username}");
        let actions_selector = format!("actions-{username}");
        let mut trigger = button(
            SharedString::from(actions_selector.clone()),
            "…",
            ButtonStyle::Ghost,
            false,
        )
        .debug_selector(|| actions_selector)
        .aria_label(format!("Account actions for {}", account.label()))
        .aria_expanded(open)
        .relative()
        .tooltip(|_, cx| {
            cx.new(|_| lucent_ui::Tooltip("Account actions".into()))
                .into()
        })
        .on_click(cx.listener({
            let username = username.clone();
            move |this, _, window, cx| {
                if this.menu.as_deref() == Some(username.as_str()) {
                    this.menu = None;
                    window.focus(&this.focus, cx);
                } else {
                    this.menu = Some(username.clone());
                    window.focus(&this.menu_focus, cx);
                }
                cx.notify();
            }
        }));
        let trigger_bounds = Rc::new(Cell::new(Bounds::default()));
        if open {
            trigger = trigger.child(div().absolute().top(px(29.)).right_0().child(floating(
                self.row_menu(&username, trigger_bounds.clone(), cx),
            )));
        }
        div()
            .id(SharedString::from(row_selector.clone()))
            .debug_selector(|| row_selector)
            .flex()
            .items_center()
            .gap_2()
            .min_h(px(64.))
            .px(px(12.))
            .py(px(11.))
            .rounded(px(8.))
            .border_1()
            .border_color(if open {
                theme::foreground()
            } else {
                theme::border()
            })
            .bg(theme::accent())
            .hover(|s| s.border_color(theme::disabled()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .max_w(relative(0.5))
                            .flex_none()
                            .truncate()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(account.label().to_owned()),
                    )
                    .when(account.label() != account.username(), |d| {
                        d.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(theme::disabled())
                                .child(username.clone()),
                        )
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .on_children_prepainted(move |bounds, _, _| trigger_bounds.set(bounds[0]))
                    .child(trigger),
            )
    }

    fn row_menu(
        &self,
        username: &str,
        trigger_bounds: Rc<Cell<Bounds<Pixels>>>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let item = |id: &'static str, label: &'static str, style: ButtonStyle| {
            button(id, label, style, false).w_full().justify_start()
        };
        let launch = username.to_owned();
        let edit = username.to_owned();
        let remove = username.to_owned();
        menu("account-menu", "Account actions")
            .w(px(180.))
            .track_focus(&self.menu_focus)
            .on_mouse_down_out(
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.button != MouseButton::Left
                        || !trigger_bounds.get().contains(&event.position)
                    {
                        this.menu = None;
                        window.focus(&this.focus, cx);
                        cx.notify();
                    }
                }),
            )
            .child(
                item("menu-launch", "Launch", ButtonStyle::Ghost).on_click(cx.listener(
                    move |this, _, window, cx| {
                        window.focus(&this.focus, cx);
                        this.launch(launch.clone(), cx)
                    },
                )),
            )
            .child(div().h(px(1.)).my_1().bg(theme::border()))
            .child(
                item("menu-edit", "Edit", ButtonStyle::Ghost)
                    .on_click(cx.listener(move |this, _, window, cx| this.edit(&edit, window, cx))),
            )
            .child(div().h(px(1.)).my_1().bg(theme::border()))
            .child(
                item("menu-remove", "Remove", ButtonStyle::Danger).on_click(cx.listener(
                    move |this, _, window, cx| {
                        let label = this
                            .account(&remove)
                            .map_or_else(|| remove.clone(), |account| account.label().to_owned());
                        this.ask_removal(remove.clone(), label, window, cx)
                    },
                )),
            )
    }

    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.list {
            None => div().into_any_element(),
            Some(Err(error)) => div()
                .id("accounts-error")
                .text_color(theme::danger())
                .child(format!("Couldn't read accounts: {error}"))
                .into_any_element(),
            Some(Ok(accounts)) if accounts.is_empty() => div()
                .id("accounts-empty")
                .debug_selector(|| "accounts-empty".into())
                .min_h(px(192.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1()
                .rounded(px(8.))
                .border_1()
                .border_dashed()
                .border_color(theme::border())
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("No accounts yet"),
                )
                .child(
                    div()
                        .text_color(theme::disabled())
                        .child("Add an account to get started."),
                )
                .into_any_element(),
            Some(Ok(accounts)) => {
                let rows: Vec<_> = accounts
                    .iter()
                    .map(|account| self.row(account, cx))
                    .collect();
                div()
                    .id("account-list")
                    .role(accesskit::Role::List)
                    .aria_label("Accounts")
                    .size_full()
                    .overflow_y_scroll()
                    .child(div().grid().grid_cols(2).gap(px(10.)).children(rows))
                    .into_any_element()
            }
        }
    }

    fn confirm(&self, removal: &Removal, cx: &mut Context<Self>) -> Deferred {
        let removing = removal.removing;
        overlay(
            div()
                .id("remove-confirm")
                .debug_selector(|| "remove-confirm".into())
                .role(accesskit::Role::AlertDialog)
                .aria_label("Remove account")
                .track_focus(&self.confirm_focus)
                .w(px(420.))
                .max_w_full()
                .flex()
                .flex_col()
                .gap_4()
                .p_4()
                .rounded(px(8.))
                .bg(theme::background())
                .border_1()
                .border_color(theme::border())
                .shadow_lg()
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.cancel_removal(window, cx);
                        cx.stop_propagation();
                    }
                }))
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Remove account"),
                )
                .child(format!(
                    "Remove “{}” and its saved login details?",
                    removal.label
                ))
                .when_some(removal.error.clone(), |d, error| {
                    d.child(div().text_color(theme::danger()).child(error))
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            button("confirm-cancel", "Cancel", ButtonStyle::Ghost, removing)
                                .debug_selector(|| "confirm-cancel".into())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.cancel_removal(window, cx)
                                })),
                        )
                        .child(
                            button(
                                "confirm-remove",
                                "Remove account",
                                ButtonStyle::Danger,
                                removing,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.remove(cx))),
                        ),
                ),
            4,
        )
    }
}

fn overlay(panel: impl IntoElement, priority: usize) -> Deferred {
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
            .child(panel),
    )
    .priority(priority)
}

impl Focusable for AccountManager {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.removal.is_some() {
            self.confirm_focus.clone()
        } else if let Some(dialog) = &self.dialog {
            dialog.focus_handle(cx)
        } else {
            self.focus.clone()
        }
    }
}

impl Render for AccountManager {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("account-manager")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .px(px(16.))
            .pt(px(8.))
            .pb(px(16.))
            .bg(theme::background())
            .text_color(theme::foreground())
            .text_size(px(13.))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke;
                if key.key == "escape" && this.menu.is_some() {
                    this.menu = None;
                    window.focus(&this.focus, cx);
                    cx.notify();
                    cx.stop_propagation();
                } else if key.key == "n"
                    && key.modifiers.secondary()
                    && this.dialog.is_none()
                    && this.removal.is_none()
                {
                    this.add(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .py_1()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Accounts"),
                    )
                    .child(
                        button("add-account", "Add account", ButtonStyle::Ghost, false)
                            .debug_selector(|| "add-account".into())
                            .border_1()
                            .border_color(theme::border())
                            .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
                    ),
            )
            .when_some(self.notice.clone(), |d, notice| {
                d.child(
                    div()
                        .id("account-notice")
                        .role(accesskit::Role::Alert)
                        .text_color(theme::danger())
                        .child(notice),
                )
            })
            .child(div().flex_1().min_h_0().child(self.list(cx)))
            .when_some(self.dialog.clone(), |d, dialog| d.child(overlay(dialog, 2)))
            .when_some(self.removal.as_ref(), |d, removal| {
                d.child(self.confirm(removal, cx))
            })
    }
}
