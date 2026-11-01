mod adapter;
pub(super) use adapter::apply;
#[cfg(test)]
mod adapter_tests;
mod model;
pub(super) use model::*;

#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod tests;

use gpui::{prelude::*, *};
use lucent_protocol::settings::RenderingMode;
use lucent_ui::{
    ButtonStyle, button, floating, menu, radio_list, radio_row, switch_row,
    text_field::{Committed, TextField},
    theme,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Options,
    Zone,
    Relogin,
    Attack,
}

pub struct GameBar {
    pub state: BarState,
    open: Option<Menu>,
    focus: FocusHandle,
    pub(super) return_focus: FocusHandle,
    fields: Vec<Entity<TextField>>,
    subscriptions: Vec<Subscription>,
}

impl EventEmitter<BarAction> for GameBar {}

impl GameBar {
    pub fn new(return_focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            state: BarState {
                delay: 3.,
                attack_profile: "Generic".into(),
                ..Default::default()
            },
            open: None,
            focus: cx.focus_handle(),
            return_focus,
            fields: vec![],
            subscriptions: vec![],
        };
        for (id, label, value, numeric) in [
            ("walk-speed", "Walk Speed", "8", true),
            ("fps", "FPS", "24", true),
            ("custom-name", "Custom Name", "", false),
            ("custom-guild", "Custom Guild", "", false),
            ("delay", "Delay in seconds", "3", true),
        ] {
            let index = this.fields.len();
            let field = cx.new(|cx| TextField::new(id, label, value.into(), numeric, window, cx));
            this.subscriptions
                .push(cx.subscribe(&field, move |this, _, event: &Committed, cx| {
                    let action = match index {
                        0 => BarAction::WalkSpeed(event.0.parse().unwrap()),
                        1 => BarAction::Fps(event.0.parse().unwrap()),
                        2 => BarAction::CustomName(Some(event.0.clone())),
                        3 => BarAction::CustomGuild(Some(event.0.clone())),
                        _ => BarAction::Delay(event.0.parse().unwrap()),
                    };
                    this.emit(action, cx);
                }));
            this.fields.push(field);
        }
        this
    }

    fn emit(&mut self, action: BarAction, cx: &mut Context<Self>) {
        cx.emit(action);
    }

    pub fn refresh(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let values = [
            self.state.settings.walk_speed.to_string(),
            self.state.settings.frame_rate.to_string(),
            self.state.settings.custom_name.clone(),
            self.state.settings.custom_guild.clone(),
            self.state.delay.to_string(),
        ];
        for (field, value) in self.fields.iter().zip(values) {
            field.update(cx, |field, cx| field.sync_value(value, cx));
        }
        cx.notify();
    }

    #[cfg(feature = "overlay-check")]
    pub(super) fn open_for_capture(&mut self, name: &str, cx: &mut Context<Self>) {
        self.open = match name {
            "options" => Some(Menu::Options),
            "zone" => Some(Menu::Zone),
            "relogin" => Some(Menu::Relogin),
            "attack" => Some(Menu::Attack),
            _ => None,
        };
        tracing::info!(menu = name, "game bar capture menu opened");
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = None;
        window.focus(&self.return_focus, cx);
        cx.notify();
    }

    pub(super) fn dialog_changed(
        &mut self,
        dialog_open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dialog_open {
            self.close(window, cx);
        } else if self.open.is_none() {
            window.focus(&self.return_focus, cx);
        }
    }

    fn toggle(
        &self,
        id: impl Into<ElementId>,
        label: &str,
        enabled: bool,
        action: BarAction,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        switch_row(id, label.to_owned(), enabled)
            .on_click(cx.listener(move |this, _, _, cx| this.emit(action.clone(), cx)))
    }

    fn radio(
        &self,
        id: impl Into<ElementId>,
        label: &str,
        selected: bool,
        action: BarAction,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        radio_row(id, label.to_owned(), selected)
            .on_click(cx.listener(move |this, _, _, cx| this.emit(action.clone(), cx)))
    }

    fn panel(
        &self,
        which: Menu,
        trigger_bounds: Rc<Cell<Bounds<Pixels>>>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = match which {
            Menu::Options => "Options",
            Menu::Zone => "Auto Zone",
            Menu::Relogin => "Auto Relogin",
            Menu::Attack => "Auto Attack",
        };
        let panel = menu("bar-menu", label)
            .track_focus(&self.focus)
            .on_mouse_down_out(
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.button != MouseButton::Left
                        || !trigger_bounds.get().contains(&event.position)
                    {
                        this.close(window, cx);
                    }
                }),
            );
        match which {
            Menu::Options => {
                let mut panel = panel;
                for (index, toggle) in Toggle::ALL.into_iter().enumerate() {
                    let value = toggle.value(&self.state.settings);
                    panel = panel.child(self.toggle(
                        ("setting", index),
                        toggle.label(),
                        value,
                        BarAction::Toggle(toggle, !value),
                        cx,
                    ));
                }
                for (index, label, reset) in [
                    (0, "Walk Speed", BarAction::WalkSpeed(8.)),
                    (1, "FPS", BarAction::Fps(24.)),
                    (2, "Custom Name", BarAction::CustomName(None)),
                    (3, "Custom Guild", BarAction::CustomGuild(None)),
                ] {
                    let disabled = match index {
                        2 => !self.state.settings.custom_name_configured,
                        3 => !self.state.settings.custom_guild_configured,
                        0 => self.state.settings.walk_speed == 8.,
                        _ => self.state.settings.frame_rate == 24.,
                    };
                    let id = match index {
                        0 => "reset-speed",
                        1 => "reset-fps",
                        2 => "reset-name",
                        _ => "reset-guild",
                    };
                    let reset_button = button(id, "Reset", ButtonStyle::Ghost, disabled)
                        .aria_label(format!("Reset {label}"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !disabled {
                                this.emit(reset.clone(), cx);
                            }
                        }));
                    panel = panel.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().w(px(90.)).child(label))
                            .child(div().flex_1().min_w_0().child(self.fields[index].clone()))
                            .child(reset_button),
                    );
                }
                let mut radios = radio_list("rendering-modes", "Rendering Mode");
                for (index, mode, label) in [
                    (0usize, RenderingMode::Full, "Full"),
                    (1, RenderingMode::InterfaceOnly, "Interface Only"),
                    (2, RenderingMode::Minimal, "Minimal"),
                ] {
                    radios = radios.child(self.radio(
                        ("rendering", index),
                        label,
                        self.state.settings.rendering_mode == mode,
                        BarAction::Rendering(mode),
                        cx,
                    ));
                }
                panel.child("Rendering Mode").child(radios)
            }
            Menu::Zone => {
                let panel = panel
                    .child(self.toggle(
                        "zone-enabled",
                        "Auto Zone",
                        self.state.zone_enabled,
                        BarAction::ZoneEnabled(!self.state.zone_enabled),
                        cx,
                    ))
                    .child("Map");
                let mut radios = radio_list("zone-maps", "Map").child(self.radio(
                    "zone-none",
                    "None",
                    self.state.zone_map.is_none(),
                    BarAction::ZoneMap(None),
                    cx,
                ));
                for (index, (map, label)) in MAPS.into_iter().enumerate() {
                    radios = radios.child(self.radio(
                        ("zone-map", index),
                        label,
                        self.state.zone_map == Some(map),
                        BarAction::ZoneMap(Some(map)),
                        cx,
                    ));
                }
                panel.child(radios)
            }
            Menu::Relogin => {
                let mut panel = panel
                    .when(!self.state.relogin_status.is_empty(), |panel| {
                        panel.child(self.state.relogin_status.clone())
                    })
                    .child(self.toggle(
                        "relogin-enabled",
                        "Auto Relogin",
                        self.state.relogin_enabled,
                        BarAction::ReloginEnabled(!self.state.relogin_enabled),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child("Delay (sec)")
                            .child(div().flex_1().min_w_0().child(self.fields[4].clone())),
                    )
                    .child("Server");
                let mut servers = radio_list("relogin-servers", "Server").child(self.radio(
                    "server-none",
                    "None",
                    self.state.server.is_none(),
                    BarAction::Server(None),
                    cx,
                ));
                if self.state.servers.is_empty() {
                    panel = panel.child("No servers found");
                }
                for (index, server) in self.state.servers.iter().enumerate() {
                    servers = servers.child(self.radio(
                        ("server", index),
                        server,
                        self.state.server.as_ref() == Some(server),
                        BarAction::Server(Some(server.clone())),
                        cx,
                    ));
                }
                panel.child(servers)
            }
            Menu::Attack => panel
                .when(!self.state.attack_issue.is_empty(), |panel| {
                    panel.child(self.state.attack_issue.clone())
                })
                .child(self.toggle(
                    "attack-enabled",
                    "Auto Attack",
                    self.state.attack_enabled,
                    BarAction::Attack(!self.state.attack_enabled),
                    cx,
                ))
                .child(button(
                    "attack-profile",
                    format!("Profile: {}", self.state.attack_profile),
                    ButtonStyle::Ghost,
                    true,
                )),
        }
    }

    fn trigger(&self, id: &'static str, label: String, which: Menu, cx: &mut Context<Self>) -> Div {
        let mut trigger = button(id, label, ButtonStyle::Ghost, false)
            .aria_expanded(self.open == Some(which))
            .relative()
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.open == Some(which) {
                    this.close(window, cx);
                } else {
                    this.open = Some(which);
                    window.focus(&this.focus, cx);
                    if which == Menu::Relogin {
                        this.emit(BarAction::RefreshServers, cx);
                    }
                    cx.notify();
                }
            }));
        if self.open == Some(which) {
            let trigger_bounds = Rc::new(Cell::new(Bounds::default()));
            trigger = trigger.child(
                div()
                    .absolute()
                    .top(px(29.))
                    .left_0()
                    .child(floating(self.panel(which, trigger_bounds.clone(), cx))),
            );
            div()
                .flex_none()
                .on_children_prepainted(move |bounds, _, _| trigger_bounds.set(bounds[0]))
                .child(trigger)
        } else {
            div().flex_none().child(trigger)
        }
    }
}

impl Render for GameBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let script_display = self.state.error.clone().unwrap_or_else(|| {
            self.state
                .script
                .clone()
                .or_else(|| {
                    self.state
                        .loaded
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .unwrap_or("No script".into())
        });
        let script_tooltip = self
            .state
            .error
            .clone()
            .or_else(|| self.state.loaded.as_ref().map(|p| p.display().to_string()));
        let action = self.state.script_action();
        let load_disabled = self.state.phase != Phase::Idle || self.state.loading;
        div()
            .id("game-bar")
            .overflow_x_scroll()
            .role(accesskit::Role::Toolbar)
            .aria_label("Game controls")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && this.open.is_some() {
                    this.close(window, cx);
                    cx.stop_propagation();
                }
            }))
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(6.))
            .h(px(35.))
            .w_full()
            .flex_none()
            .bg(theme::background())
            .text_color(theme::foreground())
            .text_size(px(13.))
            .child(
                button("bar-load", "Load", ButtonStyle::Ghost, load_disabled).on_click(
                    cx.listener(move |this, _, window, cx| {
                        if !load_disabled {
                            this.close(window, cx);
                            this.emit(BarAction::Load, cx);
                        }
                    }),
                ),
            )
            .child(
                div()
                    .id("bar-script")
                    .role(accesskit::Role::Label)
                    .aria_label(script_display.clone())
                    .max_w(px(145.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(if self.state.error.is_some() {
                        theme::danger()
                    } else {
                        theme::foreground()
                    })
                    .when_some(script_tooltip, |d, text| {
                        d.tooltip(move |_, cx| {
                            cx.new(|_| lucent_ui::Tooltip(text.clone().into())).into()
                        })
                    })
                    .child(script_display),
            )
            .child(
                button(
                    "bar-start",
                    self.state.script_label(),
                    if self.state.phase == Phase::Idle {
                        ButtonStyle::Success
                    } else {
                        ButtonStyle::Danger
                    },
                    action.is_none(),
                )
                .when_some(self.state.last_end.clone(), |d, end| {
                    d.aria_description(end.clone()).tooltip(move |_, cx| {
                        cx.new(|_| lucent_ui::Tooltip(end.clone().into())).into()
                    })
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(action) = action.clone() {
                        this.close(window, cx);
                        this.emit(action, cx);
                    }
                })),
            )
            .child(self.state.phase_label())
            .child(self.trigger("bar-options", "Options ▾".into(), Menu::Options, cx))
            .child(self.trigger(
                "bar-zone",
                format!(
                    "Auto Zone{} ▾",
                    if self.state.zone_enabled {
                        self.state
                            .zone_map
                            .and_then(|m| {
                                MAPS.iter()
                                    .find(|(map, _)| *map == m)
                                    .map(|(_, label)| format!(" · {label}"))
                            })
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                ),
                Menu::Zone,
                cx,
            ))
            .child(self.trigger(
                "bar-relogin",
                format!(
                    "Auto Relogin{} ▾",
                    if self.state.relogin_enabled {
                        self.state
                            .server
                            .as_ref()
                            .map(|s| format!(" · {s}"))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                ),
                Menu::Relogin,
                cx,
            ))
            .child(div().flex_1())
            .child(self.trigger(
                "bar-attack",
                format!("Auto Attack · {} ▾", self.state.attack_profile),
                Menu::Attack,
                cx,
            ))
    }
}
