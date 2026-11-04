use gpui::{prelude::*, *};
use lucent_host::accounts::{Account, AccountEdit, Accounts, Password};
use lucent_ui::{ButtonStyle, button, text_field::TextField, theme};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Field {
    Username,
    Password,
    Label,
}

enum Mode {
    Add,
    Edit { username: String },
}

pub(super) enum DialogEvent {
    Closed,
    Remove { username: String, label: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop {
    Field(Field),
    ShowPassword,
    Remove,
    Cancel,
    AddAnother,
    Submit,
}

pub(super) struct AccountDialog {
    accounts: Accounts,
    mode: Mode,
    username: Entity<TextField>,
    password: Entity<TextField>,
    label: Entity<TextField>,
    show_password: bool,
    username_missing: bool,
    password_missing: bool,
    error: Option<String>,
    saving: bool,
    refocus: bool,
    show_password_focus: FocusHandle,
    remove_focus: FocusHandle,
    cancel_focus: FocusHandle,
    add_another_focus: FocusHandle,
    submit_focus: FocusHandle,
    _edits: [Subscription; 2],
}

impl EventEmitter<DialogEvent> for AccountDialog {}

impl AccountDialog {
    pub fn add(accounts: Accounts, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(accounts, Mode::Add, ["", "", ""], window, cx)
    }

    pub fn edit(
        accounts: Accounts,
        account: &Account,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let label = if account.label() == account.username() {
            ""
        } else {
            account.label()
        };
        Self::new(
            accounts,
            Mode::Edit {
                username: account.username().to_owned(),
            },
            [
                account.username(),
                account.password().reveal_for_editor(),
                label,
            ],
            window,
            cx,
        )
    }

    fn new(
        accounts: Accounts,
        mode: Mode,
        [username, password, label]: [&str; 3],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut field = |id: &str, name: &str, value: &str, cx: &mut Context<Self>| {
            cx.new(|cx| TextField::new(id, name, value.to_owned(), false, window, cx))
        };
        let username = field("account-username", "Username", username, cx);
        let password = field("account-password", "Password", password, cx);
        password.update(cx, |field, cx| field.set_masked(true, cx));
        let label = field("account-label", "Label", label, cx);
        let edits = [
            cx.observe(&username, |this: &mut Self, field, cx| {
                this.username_missing &= field.read(cx).value().trim().is_empty();
                cx.notify();
            }),
            cx.observe(&password, |this: &mut Self, field, cx| {
                this.password_missing &= field.read(cx).value().trim().is_empty();
                cx.notify();
            }),
        ];
        Self {
            accounts,
            mode,
            username,
            password,
            label,
            show_password: false,
            username_missing: false,
            password_missing: false,
            error: None,
            saving: false,
            refocus: true,
            show_password_focus: cx.focus_handle(),
            remove_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            add_another_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
            _edits: edits,
        }
    }

    pub fn field(&self, field: Field) -> &Entity<TextField> {
        match field {
            Field::Username => &self.username,
            Field::Password => &self.password,
            Field::Label => &self.label,
        }
    }

    fn value(&self, field: Field, cx: &App) -> String {
        self.field(field).read(cx).value().to_owned()
    }

    fn stops(&self) -> Vec<Stop> {
        let mut stops = vec![
            Stop::Field(Field::Username),
            Stop::Field(Field::Password),
            Stop::ShowPassword,
            Stop::Field(Field::Label),
        ];
        match self.mode {
            Mode::Add => stops.extend([Stop::Cancel, Stop::AddAnother, Stop::Submit]),
            Mode::Edit { .. } => stops.extend([Stop::Remove, Stop::Cancel, Stop::Submit]),
        }
        stops
    }

    fn focus_of(&self, stop: Stop, cx: &App) -> FocusHandle {
        match stop {
            Stop::Field(field) => self.field(field).focus_handle(cx),
            Stop::ShowPassword => self.show_password_focus.clone(),
            Stop::Remove => self.remove_focus.clone(),
            Stop::Cancel => self.cancel_focus.clone(),
            Stop::AddAnother => self.add_another_focus.clone(),
            Stop::Submit => self.submit_focus.clone(),
        }
    }

    fn focused(&self, window: &Window, cx: &App) -> Option<Stop> {
        self.stops()
            .into_iter()
            .find(|stop| self.focus_of(*stop, cx).is_focused(window))
    }

    fn tab(&self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        let stops = self.stops();
        let index = self
            .focused(window, cx)
            .and_then(|stop| stops.iter().position(|other| *other == stop))
            .unwrap_or(0);
        let next = if backwards {
            (index + stops.len() - 1) % stops.len()
        } else {
            (index + 1) % stops.len()
        };
        window.focus(&self.focus_of(stops[next], cx), cx);
    }

    fn activate(&mut self, stop: Stop, window: &mut Window, cx: &mut Context<Self>) {
        match stop {
            Stop::Field(_) | Stop::Submit => self.submit(false, window, cx),
            Stop::AddAnother => self.submit(true, window, cx),
            Stop::ShowPassword => {
                self.show_password = !self.show_password;
                let masked = !self.show_password;
                self.password
                    .update(cx, |field, cx| field.set_masked(masked, cx));
                cx.notify();
            }
            Stop::Cancel => cx.emit(DialogEvent::Closed),
            Stop::Remove => {
                if let Mode::Edit { username } = &self.mode {
                    let label = self.value(Field::Label, cx);
                    let label = if label.trim().is_empty() {
                        self.value(Field::Username, cx)
                    } else {
                        label
                    };
                    cx.emit(DialogEvent::Remove {
                        username: username.clone(),
                        label,
                    });
                }
            }
        }
    }

    fn submit(&mut self, add_another: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let username = self.value(Field::Username, cx);
        let password = self.value(Field::Password, cx);
        let label = self.value(Field::Label, cx);
        self.username_missing = username.trim().is_empty();
        self.password_missing = password.trim().is_empty();
        let first_missing = if self.username_missing {
            Some(Field::Username)
        } else if self.password_missing {
            Some(Field::Password)
        } else {
            None
        };
        if let Some(field) = first_missing {
            window.focus(&self.field(field).focus_handle(cx), cx);
            cx.notify();
            return;
        }
        let edit = match &self.mode {
            Mode::Add => AccountEdit::CreateAccount {
                label: Some(label),
                username,
                password: Password::new(password),
            },
            Mode::Edit { username: current } => AccountEdit::UpdateAccount {
                username: current.clone(),
                label: Some(if label.trim().is_empty() {
                    username.clone()
                } else {
                    label
                }),
                new_username: Some(username),
                password: Some(Password::new(password)),
            },
        };
        self.saving = true;
        self.error = None;
        cx.notify();
        let saved = self.accounts.edit(edit);
        cx.spawn(async move |this, cx| {
            let result = saved.await;
            this.update(cx, |this, cx| {
                this.saving = false;
                match result {
                    Ok(_) if add_another => this.clear(cx),
                    Ok(_) => cx.emit(DialogEvent::Closed),
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        for field in [&self.username, &self.password, &self.label] {
            field.update(cx, |field, cx| field.set_value(String::new(), cx));
        }
        self.username_missing = false;
        self.password_missing = false;
        self.refocus = true;
    }

    fn labeled(
        &self,
        name: &'static str,
        input: impl IntoElement,
        note: Option<(&'static str, &'static str, Hsla)>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(name)
            .child(input)
            .when_some(note, |d, (id, text, color)| {
                d.child(
                    div()
                        .id(id)
                        .debug_selector(|| id.to_owned())
                        .text_color(color)
                        .child(text),
                )
            })
    }

    fn stop_button(
        &self,
        stop: Stop,
        id: &'static str,
        label: &'static str,
        style: ButtonStyle,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        button(id, label, style, self.saving)
            .track_focus(&self.focus_of(stop, cx))
            .on_click(cx.listener(move |this, _, window, cx| {
                if !this.saving {
                    this.activate(stop, window, cx)
                }
            }))
    }
}

impl Focusable for AccountDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.username.focus_handle(cx)
    }
}

impl Render for AccountDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.refocus) {
            window.focus(&self.username.focus_handle(cx), cx);
        }
        let (title, submit) = match self.mode {
            Mode::Add => ("Add account", "Add account"),
            Mode::Edit { .. } => ("Edit account", "Save changes"),
        };
        let reveal = if self.show_password {
            "Hide password"
        } else {
            "Show password"
        };
        let mut footer = div().flex().items_center().gap_2();
        footer = match self.mode {
            Mode::Add => footer
                .justify_end()
                .child(self.stop_button(Stop::Cancel, "Cancel", "Cancel", ButtonStyle::Ghost, cx))
                .child(self.stop_button(
                    Stop::AddAnother,
                    "Add another",
                    "Add another",
                    ButtonStyle::Ghost,
                    cx,
                ))
                .child(self.stop_button(Stop::Submit, "Submit", submit, ButtonStyle::Success, cx)),
            Mode::Edit { .. } => footer
                .child(self.stop_button(Stop::Remove, "Remove", "Remove", ButtonStyle::Danger, cx))
                .child(div().flex_1())
                .child(self.stop_button(Stop::Cancel, "Cancel", "Cancel", ButtonStyle::Ghost, cx))
                .child(self.stop_button(Stop::Submit, "Submit", submit, ButtonStyle::Success, cx)),
        };
        div()
            .id("account-dialog")
            .debug_selector(|| "account-dialog".into())
            .role(accesskit::Role::Dialog)
            .aria_label(title)
            .tab_group()
            .w(px(544.))
            .max_w_full()
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
                        cx.emit(DialogEvent::Closed);
                        cx.stop_propagation();
                    }
                    "tab" => {
                        this.tab(event.keystroke.modifiers.shift, window, cx);
                        cx.stop_propagation();
                    }
                    "enter" => {
                        let composing = [&this.username, &this.password, &this.label]
                            .iter()
                            .any(|field| field.read(cx).is_composing());
                        if !composing && !this.saving {
                            let stop = this
                                .focused(window, cx)
                                .unwrap_or(Stop::Field(Field::Username));
                            this.activate(stop, window, cx);
                            cx.stop_propagation();
                        }
                    }
                    "space" => {
                        if let Some(stop) = this
                            .focused(window, cx)
                            .filter(|stop| !matches!(stop, Stop::Field(_)))
                        {
                            if !this.saving {
                                this.activate(stop, window, cx);
                            }
                            cx.stop_propagation();
                        }
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
            .when_some(self.error.clone(), |d, error| {
                d.child(
                    div()
                        .id("account-error")
                        .debug_selector(|| "account-error".into())
                        .role(accesskit::Role::Alert)
                        .p_2()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme::danger())
                        .text_color(theme::danger())
                        .child(error),
                )
            })
            .child(self.labeled(
                "Username",
                self.username.clone(),
                self.username_missing.then_some((
                    "username-error",
                    "Enter a username.",
                    theme::danger(),
                )),
            ))
            .child(
                self.labeled(
                    "Password",
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(self.password.clone()))
                        .child(
                            self.stop_button(
                                Stop::ShowPassword,
                                "show-password",
                                if self.show_password { "Hide" } else { "Show" },
                                ButtonStyle::Ghost,
                                cx,
                            )
                            .debug_selector(|| reveal.into())
                            .aria_label(reveal),
                        ),
                    self.password_missing.then_some((
                        "password-error",
                        "Enter a password.",
                        theme::danger(),
                    )),
                ),
            )
            .child(self.labeled(
                "Label (optional)",
                self.label.clone(),
                Some((
                    "label-help",
                    "Uses the username if left blank.",
                    theme::disabled(),
                )),
            ))
            .child(footer)
    }
}
