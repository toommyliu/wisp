use super::Running;
use crate::{
    bridge::{Error, Value},
    game::{
        jobs::{Dispatch, DispatchReply, JobClock, Observation, prepare::ConsumableInfo},
        profile::*,
    },
    projection::combat_state::{CombatHydration, now_ms, skill_cooldown},
};
use lucent_protocol::{
    aura::AuraValue,
    combat_state::{CombatSnapshot, CombatTarget, SkillCooldown},
    item::ItemContainer,
    query::MonsterSelector,
};
use std::{sync::Arc, time::Instant};

fn aura(data: lucent_protocol::aura::AuraData) -> Aura {
    Aura {
        name: data.name,
        stack: Some(data.stack),
        value: match data.value {
            Some(AuraValue::Number(value)) => Some(value),
            _ => None,
        },
    }
}
impl Running {
    pub(super) fn refresh_combat_state(&mut self) -> Result<CombatSnapshot, Error> {
        let mut read = |path: &str| -> Result<serde_json::Value, Error> {
            let value =
                self.invoke_command("flash.getGameObject", vec![Value::String(path.into())])?;
            if path == "world.GCDTS" && matches!(value, Value::Null | Value::Undefined) {
                return Ok(serde_json::Value::String("undefined".into()));
            }
            let raw: String = crate::projection::commands::decode(
                "flash.getGameObject",
                lucent_protocol::bridge::to_json(value).unwrap_or_default(),
            )?;
            serde_json::from_str(&raw).map_err(|error| {
                crate::projection::commands::decode_error("flash.getGameObject", error)
            })
        };
        let actions = read("world.actions.active")?;
        let haste = read("world.myAvatar.dataLeaf.sta.$tha")?;
        let global_cooldown = read("world.GCD")?;
        let global_used_at = read("world.GCDTS")?;
        let potions_locked = read("world.lockdownPots")?;
        let target = self.command_answer("combat.getTarget", vec![])?;
        let available_monster_ids =
            self.command_answer("world.getAvailableMonsterMapIds", vec![])?;
        CombatHydration {
            actions,
            haste,
            global_cooldown,
            global_used_at,
            potions_locked,
            target,
            available_monster_ids,
        }
        .snapshot(now_ms())
    }
    pub(super) fn observe_profile(
        &mut self,
        priorities: Arc<[MonsterSelector]>,
        clock: JobClock,
    ) -> Result<Observation, Error> {
        let area = self.profile_area;
        let binding = self.operation_packets.binding();
        if self.projection.read().world.get_me().is_none() {
            return Ok(Observation {
                binding,
                area,
                snapshot: ProfileSnapshot::default(),
            });
        }
        let hydration = self.refresh_combat_state()?;
        if area != self.profile_area || binding != self.operation_packets.binding() {
            return Err(Error::CommandFailed {
                operation: "combat observation".into(),
                reason: "area or connection changed".into(),
            });
        }
        let at = Instant::now();
        let state = self.projection.read();
        if state.world.get_me().is_none() {
            return Ok(Observation {
                binding,
                area,
                snapshot: ProfileSnapshot::default(),
            });
        }
        let monsters = state.world.monsters();
        let allies = state.world.players();
        if monsters.len() > MAX_MONSTERS || allies.len() > MAX_ALLIES {
            return Err(Error::Overflow);
        }
        let facts = state.combat_monsters(
            now_ms(),
            &priorities,
            self.settings.read().anti_counter_enabled,
        );
        let mut snapshot = ProfileSnapshot::default();
        if let Some(me) = state.world.get_me() {
            let data = me.data();
            snapshot.alive = data.entity.alive();
            snapshot.hp = Stat {
                current: data.entity.hp,
                maximum: data.entity.max_hp,
            };
            snapshot.mp = Stat {
                current: data.entity.mp,
                maximum: data.entity.max_mp,
            };
            snapshot.me = Some(Ally {
                entity_id: data.entity_id,
                username: data.username,
                hp: snapshot.hp,
            });
            let auras = me.auras();
            if auras.len() > MAX_AURAS {
                return Err(Error::Overflow);
            }
            snapshot.self_auras = auras.into_iter().map(|value| aura(value.data())).collect();
        }
        snapshot.allies = allies
            .into_iter()
            .map(|player| {
                let data = player.data();
                Ally {
                    entity_id: data.entity_id,
                    username: data.username,
                    hp: Stat {
                        current: data.entity.hp,
                        maximum: data.entity.max_hp,
                    },
                }
            })
            .collect();
        for monster in monsters {
            let data = monster.data();
            if !hydration
                .available_monster_ids
                .contains(&data.monster_map_id)
            {
                continue;
            }
            let fact = facts
                .iter()
                .find(|fact| fact.monster_map_id == data.monster_map_id)
                .unwrap();
            let auras = monster.auras();
            if auras.len() > MAX_AURAS {
                return Err(Error::Overflow);
            }
            snapshot.monsters.push(Monster {
                key: TargetKey {
                    monster_map_id: data.monster_map_id,
                    generation: fact.generation,
                },
                hp: data.entity.hp,
                state: if data.entity.alive() {
                    LifeState::Alive
                } else {
                    LifeState::Dead
                },
                rank: fact.priority_rank,
                attack: if fact.attack_blocked {
                    AttackEligibility::Blocked
                } else {
                    AttackEligibility::Allowed
                },
                auras: auras.into_iter().map(|value| aura(value.data())).collect(),
            });
        }
        snapshot.selected = match hydration.selected_target {
            CombatTarget::None => SelectedTarget::None,
            CombatTarget::Player { alive, .. } => SelectedTarget::Player { alive },
            CombatTarget::Monster { monster_map_id } => {
                let monster = state.world.get_monster(monster_map_id).ok_or_else(|| {
                    Error::CommandFailed {
                        operation: "combat observation".into(),
                        reason: "selected monster has no projected generation".into(),
                    }
                })?;
                SelectedTarget::Monster(TargetKey {
                    monster_map_id,
                    generation: monster.generation(),
                })
            }
        };
        snapshot.skills = hydration.skills.map(|skill| match skill {
            SkillCooldown::Ready => SkillReadiness::Ready,
            SkillCooldown::Cooldown { remaining_ms } => {
                SkillReadiness::CooldownUntil(clock.ms(at).saturating_add(remaining_ms as u64))
            }
        });
        snapshot.consumable = hydration.consumable.map(|slot| ConsumableSlot {
            item_id: slot.item_id,
            ready: slot.ready,
        });
        Ok(Observation {
            area,
            binding,
            snapshot,
        })
    }
    fn profile_target_valid(&self, request: &Dispatch) -> bool {
        if !request.gate.allows(request.cycle, request.recovery)
            || self.operation_packets.binding() != request.binding
            || self.profile_area != request.area
        {
            return false;
        }
        let state = self.projection.read();
        if !state
            .world
            .get_me()
            .is_some_and(|me| me.data().entity.alive())
        {
            return false;
        }
        request.target.is_none_or(|key| {
            state
                .world
                .get_monster(key.monster_map_id)
                .is_some_and(|monster| {
                    monster.generation() == key.generation && monster.data().entity.alive()
                })
                && state
                    .combat_monsters(now_ms(), &[], self.settings.read().anti_counter_enabled)
                    .iter()
                    .any(|monster| {
                        monster.monster_map_id == key.monster_map_id && !monster.attack_blocked
                    })
        })
    }
    pub(super) fn dispatch_profile(&mut self, request: Dispatch) -> Result<DispatchReply, Error> {
        if !self.profile_target_valid(&request) {
            return Ok(DispatchReply::StaleTarget);
        }
        if let Action::UseSkill { skill, .. } = &request.action {
            let remaining = self.command_answer(
                "combat.getSkillCooldownRemaining",
                vec![Value::Number(u8::from(*skill).into())],
            )?;
            let remaining = crate::projection::wire::wire_number(&remaining).ok_or_else(|| {
                crate::projection::commands::decode_error(
                    "combat.getSkillCooldownRemaining",
                    "expected a numeric cooldown",
                )
            })?;
            if skill_cooldown(remaining) != SkillCooldown::Ready {
                return Ok(DispatchReply::Accepted(false));
            }
            if *skill == lucent_protocol::combat_profile::SkillSlot::Consumable {
                let slot = self.command_answer("combat.getConsumableSkillItem", vec![])?;
                if let Some(item) = request.item
                    && (slot
                        .get("itemId")
                        .and_then(crate::projection::wire::wire_i64)
                        != Some(item)
                        || slot.get("ready").and_then(serde_json::Value::as_bool) != Some(true))
                {
                    return Ok(DispatchReply::Accepted(false));
                }
            }
        }
        if let Some(target) = request.target {
            let available = self.command_answer("world.getAvailableMonsterMapIds", vec![])?;
            if !available.as_array().is_some_and(|ids| {
                ids.iter()
                    .any(|id| crate::projection::wire::wire_i64(id) == Some(target.monster_map_id))
            }) {
                return Ok(DispatchReply::StaleTarget);
            }
        }
        if !self.profile_target_valid(&request) {
            return Ok(DispatchReply::StaleTarget);
        }
        let target = request
            .target
            .map(|key| {
                lucent_protocol::bridge::from_json(
                    serde_json::json!({"monsterMapId":key.monster_map_id}),
                )
            })
            .unwrap_or(Value::Null);
        let result = match request.action {
            Action::Attack(_) => self.invoke_command("combat.attackMonster", vec![target]),
            Action::UseSkill { skill, force, .. } => self.invoke_command(
                "combat.useSkill",
                vec![
                    Value::String(u8::from(skill).to_string()),
                    target,
                    Value::Bool(force == CastForce::Forced),
                ],
            ),
            _ => unreachable!(),
        };
        if let Err(error) = &result {
            tracing::warn!(%error, "combat profile action rejected");
        }
        result.map(|value| DispatchReply::Accepted(matches!(value, Value::Bool(true))))
    }
    pub(super) fn stop_profile(&mut self, binding: crate::sessions::claims::Binding) {
        if self.operation_packets.binding() == binding {
            let _ = self.invoke_command("combat.cancelAutoAttack", vec![]);
            let _ = self.invoke_command("combat.cancelTarget", vec![]);
        }
    }
    pub(super) fn prepare_profile(
        &mut self,
        name: Option<String>,
        class_names: &[String],
        ambient: bool,
    ) -> Result<Option<ConsumableInfo>, Error> {
        if self.projection.read().world.get_me().is_none() {
            return Ok(None);
        }
        let equipped = if class_names.is_empty() {
            None
        } else {
            let inventory_class = self
                .projection
                .read()
                .items
                .all(ItemContainer::Inventory)
                .into_iter()
                .find(|item| item.data().category == "Class" && item.data().equipped)
                .map(|item| item.data().name.clone());
            let class_name = match self.invoke_command("player.getClassName", vec![]) {
                Ok(Value::String(name)) => name,
                _ => String::new(),
            };
            if inventory_class.as_ref().is_some_and(|name| {
                class_names
                    .iter()
                    .any(|expected| expected.eq_ignore_ascii_case(name))
            }) || class_names
                .iter()
                .any(|expected| expected.eq_ignore_ascii_case(&class_name))
            {
                None
            } else {
                inventory_class
                    .filter(|name| !name.is_empty())
                    .or_else(|| (!class_name.is_empty()).then_some(class_name))
            }
        };
        let class_warning = equipped.map(|equipped| {
            let expected = match class_names {
                [one] => one.clone(),
                [one, two] => format!("{one} or {two}"),
                _ => format!(
                    "{}, or {}",
                    class_names[..class_names.len() - 1].join(", "),
                    class_names.last().unwrap()
                ),
            };
            format!("This profile is for {expected}, but {equipped} is equipped.")
        });
        let item = name
            .as_ref()
            .and_then(|name| {
                self.projection
                    .read()
                    .items
                    .by_name(ItemContainer::Inventory, name)
            })
            .map(|item| item.data().clone());
        let member = if item.as_ref().is_some_and(|item| item.member_only) {
            matches!(
                self.invoke_command("player.isMember", vec![]),
                Ok(Value::Bool(true))
            )
        } else {
            true
        };
        let current = if ambient {
            let slot = self.command_answer("combat.getConsumableSkillItem", vec![])?;
            slot.get("itemId")
                .or_else(|| slot.get("ItemID"))
                .and_then(crate::projection::wire::wire_i64)
        } else {
            None
        };
        Ok(Some(ConsumableInfo {
            current,
            item,
            member,
            class_warning,
        }))
    }
}
