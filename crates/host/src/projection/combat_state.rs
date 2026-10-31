use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use lucent_protocol::combat_state::{
    CombatMonsterState, CombatSnapshot, CombatTarget, ConsumableReadiness, SkillCooldown,
};
use lucent_protocol::event::ProjectionEvent;
use lucent_protocol::query::MonsterSelector;
use serde_json::Value;

use super::Projection;
use super::commands::decode_error;
use super::wire::{selector_id, trim_js, wire_number};
use super::world::{MonsterLookup, monster_lookup};
use crate::bridge::Error;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatHydration {
    pub actions: Value,
    pub haste: Value,
    pub global_cooldown: Value,
    pub global_used_at: Value,
    pub potions_locked: Value,
    pub target: Value,
    pub available_monster_ids: Value,
}

#[derive(Debug)]
struct Skill {
    cooldown: f64,
    used_at: f64,
}

#[derive(Debug, Default)]
pub(super) struct CounterWindows {
    counters: BTreeMap<i64, f64>,
}

impl CounterWindows {
    pub(super) fn observe(&mut self, events: &[ProjectionEvent], now_ms: f64) {
        for event in events {
            match event {
                ProjectionEvent::JoinMap { .. } => *self = Self::default(),
                ProjectionEvent::CounterAttackStart {
                    monster_map_id,
                    duration_ms,
                    ..
                } => {
                    let duration = duration_ms
                        .filter(|duration| duration.is_finite() && *duration > 0.0)
                        .unwrap_or(0.0);
                    let deadline = now_ms + duration + 7000.0;
                    self.counters
                        .entry(*monster_map_id)
                        .and_modify(|current| *current = current.max(deadline))
                        .or_insert(deadline);
                }
                ProjectionEvent::CounterAttackEnd { monster_map_id, .. }
                | ProjectionEvent::MonsterDeath { monster_map_id }
                | ProjectionEvent::MonsterRespawn { monster_map_id } => {
                    self.counters.remove(monster_map_id);
                }
                _ => {}
            }
        }
    }
}

fn invalid(reason: &str) -> Error {
    decode_error("refreshCombatState", reason)
}

fn number(value: &Value) -> Result<f64, Error> {
    wire_number(value)
        .filter(|value| value.abs() <= 9_007_199_254_740_991.0)
        .ok_or_else(|| invalid("expected a finite safe number"))
}

fn positive_id(value: &Value) -> Result<i64, Error> {
    selector_id(number(value)?)
        .filter(|id| *id > 0)
        .ok_or_else(|| invalid("expected a positive integral ID"))
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_f64().is_some_and(|value| value != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

fn timestamp(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Null) => 0.0,
        Some(Value::Bool(value)) => u8::from(*value).into(),
        Some(Value::String(value)) if trim_js(value).is_empty() => 0.0,
        Some(value) => wire_number(value).unwrap_or(f64::NAN),
        None => f64::NAN,
    }
}

impl Skill {
    fn remaining(&self, now_ms: f64) -> f64 {
        {
            let remaining = self.cooldown - (now_ms - self.used_at);
            if remaining < 0.0 { 0.0 } else { remaining }
        }
    }
}

pub(crate) fn skill_cooldown(remaining: f64) -> SkillCooldown {
    match remaining.trunc() as i64 {
        remaining_ms if remaining_ms > 0 => SkillCooldown::Cooldown { remaining_ms },
        _ => SkillCooldown::Ready,
    }
}

impl CombatHydration {
    pub fn snapshot(self, now_ms: f64) -> Result<CombatSnapshot, Error> {
        let now_ms = now_ms.floor();
        let observed_at_ms = number(&Value::from(now_ms))?;
        let haste = 1.0 - number(&self.haste)?.clamp(-1.0, 0.5);
        let actions = self
            .actions
            .as_array()
            .ok_or_else(|| invalid("expected an active action array"))?;
        let mut skills: [Option<Skill>; 6] = std::array::from_fn(|_| None);
        for (slot, skill) in skills.iter_mut().enumerate() {
            let Some(action) = actions.get(slot).filter(|action| !action.is_null()) else {
                continue;
            };
            let cooldown = action
                .get("OldCD")
                .filter(|value| !value.is_null())
                .or_else(|| action.get("cd"))
                .ok_or_else(|| invalid("missing action cooldown"))?;
            let cooldown = number(cooldown)?;
            if !(0.0..=f64::from(i32::MAX) / 2.0).contains(&cooldown) {
                return Err(invalid("action cooldown exceeds the client integer range"));
            }
            *skill = Some(Skill {
                cooldown: (cooldown * haste + 0.5).floor(),
                used_at: timestamp(action.get("ts")),
            });
        }
        let global = Skill {
            cooldown: number(&self.global_cooldown)?,
            used_at: timestamp(Some(&self.global_used_at)),
        };
        let locked = truthy(Some(&self.potions_locked));
        let consumable = actions
            .get(5)
            .filter(|action| action.get("ref").and_then(Value::as_str) == Some("i1"))
            .and_then(|action| {
                action
                    .get("sArg1")
                    .and_then(|id| positive_id(id).ok())
                    .map(|id| {
                        (
                            id,
                            !locked
                                && truthy(action.get("isOK"))
                                && !truthy(action.get("skillLock"))
                                && !truthy(action.get("lock")),
                        )
                    })
            });
        let target = if self.target.is_null() {
            CombatTarget::None
        } else {
            match self.target.get("type").and_then(Value::as_str) {
                Some("monster") => {
                    let id = positive_id(
                        self.target
                            .get("monsterMapId")
                            .ok_or_else(|| invalid("missing target ID"))?,
                    )?;
                    CombatTarget::Monster { monster_map_id: id }
                }
                Some("player") => {
                    let entity_id = positive_id(
                        self.target
                            .get("entityId")
                            .ok_or_else(|| invalid("missing target ID"))?,
                    )?;
                    let hp = number(
                        self.target
                            .get("hp")
                            .ok_or_else(|| invalid("missing target HP"))?,
                    )?;
                    let state = number(
                        self.target
                            .get("state")
                            .ok_or_else(|| invalid("missing target state"))?,
                    )?;
                    if ![0.0, 1.0, 2.0].contains(&state) {
                        return Err(invalid("invalid target state"));
                    }
                    CombatTarget::Player {
                        entity_id,
                        alive: hp > 0.0 && state > 0.0,
                    }
                }
                _ => return Err(invalid("expected a player or monster target")),
            }
        };
        let available = self
            .available_monster_ids
            .as_array()
            .ok_or_else(|| invalid("expected available monster IDs"))?
            .iter()
            .map(positive_id)
            .collect::<Result<Vec<_>, _>>()?;
        let remaining: [f64; 6] = std::array::from_fn(|slot| {
            skills[slot].as_ref().map_or(0.0, |skill| {
                let action = skill.remaining(now_ms);
                let global = global.remaining(now_ms);
                if action.is_nan() || global.is_nan() {
                    f64::NAN
                } else {
                    action.max(global)
                }
            })
        });
        Ok(CombatSnapshot {
            observed_at_ms,
            skills: remaining.map(skill_cooldown),
            selected_target: target,
            consumable: consumable.map(|(item_id, allowed)| ConsumableReadiness {
                item_id,
                ready: allowed && remaining[5] as i64 <= 0,
            }),
            available_monster_ids: available,
        })
    }
}

impl Projection {
    pub fn combat_monsters(
        &self,
        now_ms: f64,
        priorities: &[MonsterSelector],
        anti_counter_enabled: bool,
    ) -> Vec<CombatMonsterState> {
        self.world
            .monsters()
            .into_iter()
            .map(|monster| {
                let data = monster.data();
                CombatMonsterState {
                    monster_map_id: data.monster_map_id,
                    generation: monster.generation(),
                    priority_rank: priorities.iter().position(|selector| {
                        match monster_lookup(selector) {
                            MonsterLookup::Id(id) => selector_id(id) == Some(data.monster_map_id),
                            MonsterLookup::Name(name) => {
                                let name = trim_js(name).to_lowercase();
                                name == "*"
                                    || (!name.is_empty()
                                        && name == trim_js(&data.name).to_lowercase())
                            }
                        }
                    }),
                    attack_blocked: anti_counter_enabled
                        && self
                            .combat
                            .counters
                            .get(&data.monster_map_id)
                            .is_some_and(|until| *until > now_ms),
                }
            })
            .collect()
    }
}

pub(crate) fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_action_timestamp_does_not_prevent_combat_observation() {
        for timestamp in [None, Some(json!(null)), Some(json!("bad"))] {
            let mut action = json!({"cd":1000});
            if let Some(timestamp) = timestamp {
                action["ts"] = timestamp;
            }
            let observation = CombatHydration {
                actions: json!([action]),
                haste: json!(0),
                global_cooldown: json!(100),
                global_used_at: json!("bad"),
                potions_locked: json!(false),
                target: json!(null),
                available_monster_ids: json!([]),
            }
            .snapshot(1500.5)
            .unwrap();
            assert_eq!(observation.skills[0], SkillCooldown::Ready);
            assert_eq!(observation.skills[5], SkillCooldown::Ready);
        }
    }
}
