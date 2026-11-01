use lucent_protocol::{
    bridge::to_json,
    command::Command,
    item::ItemContainer,
    settings::{GameSettings, RenderingMode, SettingsPatch},
};
use serde::Serialize;
use serde_json::{Value as Json, json};

use super::Running;
use crate::bridge::{self, Error, Value};
use crate::projection::{
    Projection, commands,
    world::{MonsterLookup, monster_lookup},
};

impl Running {
    pub(super) fn invoke_command(&mut self, name: &str, args: Vec<Value>) -> Result<Value, Error> {
        let result = bridge::invoke(&mut self.player.lock().unwrap(), name, args);
        self.drain_events();
        result
    }

    pub(super) fn command_answer(&mut self, name: &str, args: Vec<Value>) -> Result<Json, Error> {
        to_json(self.invoke_command(name, args)?)
            .ok_or_else(|| commands::decode_error(name, "the callback returned undefined"))
    }

    fn commit_command<R: Serialize>(
        &mut self,
        commit: impl FnOnce(&mut Projection) -> Result<R, Error>,
    ) -> Result<String, Error> {
        self.drain_events();
        commit(&mut self.projection.write()).map(|output| serde_json::to_string(&output).unwrap())
    }

    fn send_settings(&mut self, patch: &SettingsPatch, state: &GameSettings) -> Result<(), Error> {
        let mut calls: Vec<(&'static str, Vec<Value>)> = Vec::new();
        macro_rules! setting {
            ($field:ident, $method:literal, $value:ident) => {
                if let Some(value) = &patch.$field {
                    calls.push(($method, vec![Value::$value(*value)]));
                }
            };
        }
        setting!(animations_enabled, "settings.setAnimationsEnabled", Bool);
        setting!(collisions_enabled, "settings.setCollisionsEnabled", Bool);
        if let Some(value) = &patch.custom_guild {
            calls.push(match value {
                Some(value) => (
                    "settings.setCustomGuild",
                    vec![Value::String(value.clone())],
                ),
                None => ("settings.resetCustomGuild", vec![]),
            });
        }
        if let Some(value) = &patch.custom_name {
            calls.push(match value {
                Some(value) => ("settings.setCustomName", vec![Value::String(value.clone())]),
                None => ("settings.resetCustomName", vec![]),
            });
        }
        setting!(death_ads_visible, "settings.setDeathAdsVisible", Bool);
        setting!(
            other_players_visible,
            "settings.setOtherPlayersVisible",
            Bool
        );
        setting!(walk_speed, "settings.setWalkSpeed", Number);
        if patch.rendering_mode.is_some() {
            calls.push((
                "settings.setLagKillerEnabled",
                vec![Value::Bool(state.rendering_mode != RenderingMode::Full)],
            ));
        }
        if patch.frame_rate.is_some() || patch.rendering_mode.is_some() {
            let rate = if state.rendering_mode == RenderingMode::Minimal {
                2.0
            } else {
                state.frame_rate
            };
            calls.push(("settings.setFrameRate", vec![Value::Number(rate)]));
        }
        for (enabled, method) in [
            (patch.enemy_magnet_enabled, "settings.enemyMagnet"),
            (patch.infinite_range_enabled, "settings.infiniteRange"),
            (patch.provoke_cell_enabled, "settings.provokeCell"),
            (patch.skip_cutscenes_enabled, "settings.skipCutscenes"),
        ] {
            if enabled == Some(true) {
                calls.push((method, vec![]));
            }
        }
        let mut first_failure = None;
        for (callback, args) in calls {
            match self.invoke_command(callback, args) {
                Err(error) => {
                    if self.rejected_settings.insert(callback) {
                        tracing::warn!(callback, %error, "couldn't apply game setting");
                    }
                    first_failure.get_or_insert(error);
                }
                Ok(_) => {
                    if self.rejected_settings.remove(callback) {
                        tracing::info!(callback, "game setting accepted again");
                    }
                }
            }
        }
        first_failure.map_or(Ok(()), Err)
    }

    pub(super) fn reapply_settings(&mut self, actions_only: bool) -> Result<(), Error> {
        let state = self.settings.read().clone();
        let patch = SettingsPatch {
            enemy_magnet_enabled: Some(state.enemy_magnet_enabled),
            infinite_range_enabled: Some(state.infinite_range_enabled),
            provoke_cell_enabled: Some(state.provoke_cell_enabled),
            skip_cutscenes_enabled: Some(state.skip_cutscenes_enabled),
            ..if actions_only {
                SettingsPatch::default()
            } else {
                SettingsPatch {
                    animations_enabled: Some(state.animations_enabled),
                    anti_counter_enabled: Some(state.anti_counter_enabled),
                    collisions_enabled: Some(state.collisions_enabled),
                    custom_guild: state
                        .custom_guild_configured
                        .then(|| Some(state.custom_guild.clone())),
                    custom_name: state
                        .custom_name_configured
                        .then(|| Some(state.custom_name.clone())),
                    death_ads_visible: Some(state.death_ads_visible),
                    frame_rate: Some(state.frame_rate),
                    other_players_visible: Some(state.other_players_visible),
                    rendering_mode: Some(state.rendering_mode),
                    walk_speed: Some(state.walk_speed),
                    ..SettingsPatch::default()
                }
            }
        };
        self.send_settings(&patch, &state)
    }

    pub(super) fn command(&mut self, command: Command) -> Result<String, Error> {
        match command {
            Command::RefreshCombatState {} => self
                .refresh_combat_state()
                .map(|snapshot| serde_json::to_string(&snapshot).unwrap()),
            Command::UpdateEnvironment { .. } => Err(Error::Unavailable(
                "environment requires a session record".into(),
            )),
            Command::SetAutoReloginEnabled { .. }
            | Command::SetAutoReloginDelay { .. }
            | Command::SetAutoReloginServer { .. } => Err(Error::Unavailable(
                "auto relogin requires a session controller".into(),
            )),
            Command::PatchAutoZone { patch } => Ok(super::patch_auto_zone(
                &self.auto_zone,
                patch,
                &self.script_events,
                &self.changes,
            )),
            Command::UpdateSettings { mut patch } => {
                let next = patched_settings(&self.settings.read(), &mut patch);
                let result = self.send_settings(&patch, &next);
                self.changes.set(&self.settings, next.clone());
                result?;
                Ok(serde_json::to_string(&next).unwrap())
            }
            Command::ReapplySettings { actions_only } => {
                self.reapply_settings(actions_only)?;
                Ok("null".into())
            }
            Command::RefreshContainer { container } => {
                let (_, callback) = commands::container_kind(container);
                let answer = self.command_answer(callback, vec![])?;
                self.commit_command(|projection| {
                    commands::refresh_container(projection, container, answer)
                })
            }
            Command::ConfirmShopClosed { shop_id } => {
                let args = shop_id
                    .into_iter()
                    .map(|id| Value::Number(id as f64))
                    .collect();
                let answer = self.command_answer("shops.isOpen", args)?;
                self.commit_command(|projection| {
                    commands::confirm_shop_closed(projection, shop_id, answer)
                })
            }
            Command::RejectDrop { selector } => {
                let id = self
                    .projection
                    .read()
                    .items
                    .resolve(ItemContainer::Drop, &selector)
                    .map(|item| item.data().item_id);
                let Some(id) = id else {
                    return Ok("false".into());
                };
                let answer = self.command_answer("drops.reject", vec![Value::Number(id as f64)])?;
                self.commit_command(|projection| commands::reject_drop(projection, id, answer))
            }
            Command::RefreshCells {} => {
                let loaded = commands::decode::<bool>(
                    "world.isLoaded",
                    self.command_answer("world.isLoaded", vec![])?,
                )?;
                if !loaded {
                    return self.commit_command(|projection| Ok(projection.world.cells()));
                }
                let answer = self.command_answer("world.getCells", vec![])?;
                self.commit_command(|projection| commands::refresh_cells(projection, answer))
            }
            Command::RefreshCellPads {} => {
                let loaded = commands::decode::<bool>(
                    "world.isLoaded",
                    self.command_answer("world.isLoaded", vec![])?,
                )?;
                if !loaded {
                    return self.commit_command(|projection| Ok(projection.world.cell_pads()));
                }
                let answer = self.command_answer("world.getCellPads", vec![])?;
                self.commit_command(|projection| commands::refresh_cell_pads(projection, answer))
            }
            Command::HydrateMonster { selector } => {
                if let Some(monster) = self.projection.read().world.resolve_monster(&selector) {
                    return Ok(serde_json::to_string(&monster.snapshot_data()).unwrap());
                }
                let selector = match monster_lookup(&selector) {
                    MonsterLookup::Id(id) => json!({"monsterMapId": id}),
                    MonsterLookup::Name(name) => json!({"name": name}),
                };
                let answer = self.command_answer(
                    "world.getMonster",
                    vec![lucent_protocol::bridge::from_json(selector)],
                )?;
                self.commit_command(|projection| commands::hydrate_monster(projection, answer))
            }
            Command::Login { username, password } => {
                let previous =
                    commands::install_login(&mut self.projection.write(), username, password);
                let Some(previous) = previous else {
                    return Ok("false".into());
                };
                let credentials = self.projection.read().auth.credentials();
                let result = self
                    .invoke_command(
                        "auth.login",
                        vec![
                            Value::String(credentials.username),
                            Value::String(credentials.password),
                        ],
                    )
                    .and_then(|_| self.commit_command(|_| Ok(true)));
                if result.is_err() {
                    self.projection.write().auth.set_credentials(previous);
                }
                result
            }
            Command::Logout {} => {
                let result = self.invoke_command("auth.logout", vec![]);
                let output = self.commit_command(|projection| {
                    projection.auth.clear();
                    Ok(())
                })?;
                result.map(|_| output)
            }
            Command::RefreshCredentials {} => {
                if !self
                    .projection
                    .read()
                    .current_readiness()
                    .missing
                    .is_empty()
                {
                    return Ok(
                        serde_json::to_string(&self.projection.read().auth.credentials()).unwrap(),
                    );
                }
                let username = self.command_answer(
                    "flash.getGameObjectS",
                    vec![Value::String("loginInfo.strUsername".into())],
                )?;
                let password = self.command_answer(
                    "flash.getGameObjectS",
                    vec![Value::String("loginInfo.strPassword".into())],
                )?;
                self.commit_command(|projection| {
                    commands::refresh_credentials(projection, username, password)
                })
            }
            Command::RefreshServers {} => {
                let answer = self.command_answer("auth.getServers", vec![])?;
                self.commit_command(|projection| commands::refresh_servers(projection, answer))
            }
            Command::RefreshLoggedIn {} => {
                let answer = self.command_answer("auth.isLoggedIn", vec![])?;
                self.commit_command(|projection| commands::refresh_logged_in(projection, answer))
            }
            Command::RefreshAcceptedQuests {} => {
                let answer = self.command_answer("quests.getAccepted", vec![])?;
                self.commit_command(|projection| {
                    commands::refresh_accepted_quests(projection, answer)
                })
            }
            Command::RefreshEquippedConsumable {} => {
                let answer = self.command_answer("combat.getConsumableSkillItem", vec![])?;
                self.commit_command(|projection| {
                    commands::refresh_equipped_consumable(projection, answer)
                })
            }
        }
    }
}

pub(super) fn patched_settings(state: &GameSettings, patch: &mut SettingsPatch) -> GameSettings {
    patch.frame_rate = patch.frame_rate.map(|value| value.clamp(1.0, 60.0));
    patch.walk_speed = patch.walk_speed.map(|value| value.max(1.0));
    let mut next = state.clone();
    macro_rules! update {
        ($($field:ident),*) => { $(if let Some(value) = patch.$field { next.$field = value; })* };
    }
    update!(
        animations_enabled,
        anti_counter_enabled,
        collisions_enabled,
        death_ads_visible,
        enemy_magnet_enabled,
        frame_rate,
        infinite_range_enabled,
        other_players_visible,
        provoke_cell_enabled,
        rendering_mode,
        skip_cutscenes_enabled,
        walk_speed
    );
    if let Some(value) = &patch.custom_guild {
        next.custom_guild = value.clone().unwrap_or_default();
        next.custom_guild_configured = value.is_some();
    }
    if let Some(value) = &patch.custom_name {
        next.custom_name = value.clone().unwrap_or_default();
        next.custom_name_configured = value.is_some();
    }
    next
}
