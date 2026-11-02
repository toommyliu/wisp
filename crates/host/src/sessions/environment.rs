use super::environment_order::{PRIMARY, TERTIARY};
use crate::projection::{Projection, wire::trim_js};
use lucent_protocol::environment::*;
use std::{cmp::Ordering, collections::HashSet};

pub(super) fn compare_names(left: &str, right: &str) -> Ordering {
    if !left.is_ascii() || !right.is_ascii() {
        return match (left.is_ascii(), right.is_ascii()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => left.cmp(right),
        };
    }
    fn weights<'a>(name: &'a str, table: &'a [u8; 128]) -> impl Iterator<Item = u8> + 'a {
        name.bytes()
            .filter(|b| PRIMARY[*b as usize] != 0)
            .map(|b| table[b as usize])
    }
    weights(left, &PRIMARY)
        .cmp(weights(right, &PRIMARY))
        .then_with(|| weights(left, &TERTIARY).cmp(weights(right, &TERTIARY)))
}
fn key(name: &str) -> String {
    trim_js(name).to_lowercase()
}
fn normalize_names(names: &mut Vec<String>) {
    let mut seen = HashSet::new();
    *names = names
        .iter()
        .map(|s| trim_js(s).to_owned())
        .filter(|s| !s.is_empty() && seen.insert(s.to_lowercase()))
        .collect();
    names.sort_by(|a, b| compare_names(a, b));
}
fn positive(value: f64) -> Option<f64> {
    (value.is_finite() && value.trunc() > 0.0 && value.trunc() <= 9_007_199_254_740_991.0)
        .then_some(value.trunc())
}
pub(super) fn normalize(state: &mut EnvironmentState) {
    state.quest_ids = state
        .quest_ids
        .iter()
        .filter_map(|v| positive(*v))
        .collect();
    state.quest_ids.sort_by(f64::total_cmp);
    state.quest_ids.dedup();
    let mut rewards: Vec<_> = state.quest_rewards.iter().collect();
    rewards.sort_by_key(|(key, _)| {
        let index = key
            .parse::<u32>()
            .ok()
            .filter(|index| *index != u32::MAX && index.to_string() == **key);
        (index.is_none(), index.unwrap_or(0))
    });
    let mut normalized = indexmap::IndexMap::new();
    for (id, reward) in rewards {
        if let Some((id, reward)) =
            crate::projection::wire::wire_number(&serde_json::Value::String(id.clone()))
                .and_then(positive)
                .zip(positive(*reward))
            && state.quest_ids.contains(&id)
        {
            normalized.insert(id.to_string(), reward);
        }
    }
    normalized.sort_by(|left, _, right, _| {
        left.parse::<f64>()
            .unwrap()
            .total_cmp(&right.parse::<f64>().unwrap())
    });
    state.quest_rewards = normalized;
    normalize_names(&mut state.item_names);
    normalize_names(&mut state.item_notification_names);
    let registered: HashSet<_> = state.item_names.iter().map(|s| key(s)).collect();
    state
        .item_notification_names
        .retain(|s| registered.contains(&key(s)));
    normalize_names(&mut state.boosts);
    state.item_rules.buckets.sort();
    state.item_rules.buckets.dedup();
}
fn remove(names: &mut Vec<String>, name: &str) {
    let key = key(name);
    names.retain(|s| s.to_lowercase() != key);
}
fn add_quest(state: &mut EnvironmentState, quest: f64, reward: Option<f64>) {
    if let Some(id) = positive(quest) {
        state.quest_ids.push(id);
        if let Some(reward) = reward.and_then(positive) {
            state.quest_rewards.insert(id.to_string(), reward);
        }
    }
}
pub(super) fn reduce(state: &mut EnvironmentState, edit: EnvironmentEdit) {
    use EnvironmentEdit::*;
    match edit {
        Clear {} => {
            state.quest_ids.clear();
            state.quest_rewards.clear();
            state.item_names.clear();
            state.item_notification_names.clear();
            state.boosts.clear();
        }
        AddQuest {
            quest_id,
            reward_item_id,
        } => add_quest(state, quest_id, reward_item_id),
        SetQuestReward {
            quest_id,
            reward_item_id,
        } => {
            if positive(reward_item_id).is_some() {
                add_quest(state, quest_id, Some(reward_item_id));
            }
        }
        RemoveQuest { quest_id } => {
            if let Some(id) = positive(quest_id) {
                state.quest_ids.retain(|q| *q != id);
                state.quest_rewards.shift_remove(&id.to_string());
            }
        }
        ClearQuestReward { quest_id } => {
            if let Some(id) = positive(quest_id) {
                state.quest_rewards.shift_remove(&id.to_string());
            }
        }
        ClearQuests {} => {
            state.quest_ids.clear();
            state.quest_rewards.clear();
        }
        SetQuestAutoRegister {
            requirements,
            rewards,
        } => {
            if let Some(requirements) = requirements {
                state.quest_auto_register.requirements = requirements;
            }
            if let Some(rewards) = rewards {
                state.quest_auto_register.rewards = rewards;
            }
        }
        SetAutomationEnabled {
            capability,
            enabled,
        } => match capability {
            EnvironmentAutomationCapability::Boosts => state.automation.boosts = enabled,
            EnvironmentAutomationCapability::Drops => state.automation.drops = enabled,
            EnvironmentAutomationCapability::Quests => state.automation.quests = enabled,
        },
        AddItem { name } => state.item_names.push(name),
        RemoveItem { name } => remove(&mut state.item_names, &name),
        SetItemNotification { name, enabled } => {
            if enabled {
                if let Some(registered) = state.item_names.iter().find(|s| key(s) == key(&name)) {
                    state.item_notification_names.push(registered.clone());
                }
            } else {
                remove(&mut state.item_notification_names, &name);
            }
        }
        SetItemRules { rules } => state.item_rules = rules,
        UpdateDropPolicy { patch } => {
            use EnvironmentItemBucket::*;
            for (bucket, enabled) in [
                (AcMember, patch.accept_ac_member_only_drops),
                (AcNonMember, patch.accept_ac_non_member_drops),
                (NonAcMember, patch.accept_non_ac_member_only_drops),
                (NonAcNonMember, patch.accept_non_ac_non_member_drops),
            ] {
                if let Some(enabled) = enabled {
                    state.item_rules.buckets.retain(|b| *b != bucket);
                    if enabled {
                        state.item_rules.buckets.push(bucket);
                    }
                }
            }
            if let Some(reject) = patch.reject_unregistered_drops {
                state.item_rules.reject_else = reject;
            }
        }
        ClearItems {} => {
            state.item_names.clear();
            state.item_notification_names.clear();
        }
        AddBoost { name } => state.boosts.push(name),
        RemoveBoost { name } => remove(&mut state.boosts, &name),
        ClearBoosts {} => state.boosts.clear(),
    }
    normalize(state);
}
pub(super) fn write(
    records: &super::Records,
    update: impl FnOnce(&mut EnvironmentState),
) -> EnvironmentState {
    let mut state = records.environment.write();
    let before = state.clone();
    update(&mut state);
    let changed = *state != before;
    let result = state.clone();
    drop(state);
    if changed {
        records.changes.notify();
    }
    result
}

pub(super) fn apply(state: &mut EnvironmentState, edit: EnvironmentEdit, projection: &Projection) {
    let before = (state.quest_ids.clone(), state.quest_auto_register.clone());
    reduce(state, edit);
    if state.quest_ids == before.0 && state.quest_auto_register == before.1 {
        return;
    }
    register_loaded(state, projection);
}

pub(super) fn register_loaded(state: &mut EnvironmentState, projection: &Projection) {
    for quest in projection.quests.all() {
        let quest = quest.data();
        if !state.quest_ids.contains(&quest.id) {
            continue;
        }
        if state.quest_auto_register.rewards {
            state
                .item_names
                .extend(quest.rewards.iter().map(|r| r.item.name.clone()));
        }
        if state.quest_auto_register.requirements {
            state.item_names.extend(
                quest
                    .requirements
                    .iter()
                    .filter(|r| r.temporary_item == Some(false))
                    .map(|r| r.name.clone()),
            );
        }
    }
    normalize(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_corpus_matches_the_schema() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../packages/schemas/test-fixtures/environment/normalization.json"
        ))
        .unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let mut state = serde_json::from_value(fixture["input"].clone()).unwrap();
            if fixture["clear"] == true {
                reduce(&mut state, EnvironmentEdit::Clear {});
            } else {
                normalize(&mut state);
            }
            assert_eq!(
                state.quest_rewards.keys().collect::<Vec<_>>(),
                fixture["expected"]["questRewards"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                state,
                serde_json::from_value::<EnvironmentState>(fixture["expected"].clone()).unwrap(),
                "{}",
                fixture["name"]
            );
        }
    }
    #[test]
    fn reducer_corpus_matches_the_schema() {
        let sequences: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../packages/schemas/test-fixtures/environment/reducers.json"
        ))
        .unwrap();
        for sequence in sequences.as_array().unwrap() {
            let mut state: EnvironmentState =
                serde_json::from_value(sequence["input"].clone()).unwrap();
            for step in sequence["steps"].as_array().unwrap() {
                reduce(
                    &mut state,
                    serde_json::from_value(step["edit"].clone()).unwrap(),
                );
                let expected: EnvironmentState =
                    serde_json::from_value(step["expected"].clone()).unwrap();
                assert_eq!(
                    state.quest_rewards.keys().collect::<Vec<_>>(),
                    expected.quest_rewards.keys().collect::<Vec<_>>(),
                    "{}",
                    step["edit"]
                );
                assert_eq!(state, expected, "{}", step["edit"]);
            }
        }
    }
    #[test]
    fn ascii_order_matches_node_pairs_and_item_names() {
        let pairs: Vec<(String, String, i32)> = serde_json::from_str(include_str!(
            "../../../../packages/schemas/test-fixtures/environment/order.json"
        ))
        .unwrap();
        for (left, right, expected) in pairs {
            assert_eq!(
                compare_names(&left, &right) as i32,
                expected,
                "{left:?} {right:?}"
            );
        }
    }
    #[test]
    fn non_ascii_names_deliberately_follow_ascii_then_code_point_order() {
        let mut names = vec![
            "éclair".into(),
            "Ωmega".into(),
            "Zorbak".into(),
            "apple".into(),
        ];
        normalize_names(&mut names);
        assert_eq!(names, ["apple", "Zorbak", "éclair", "Ωmega"]);
    }
}

#[cfg(test)]
mod target_tests {
    use super::*;
    #[test]
    fn loaded_targets_keep_reward_spelling_and_require_explicit_non_temporary_items() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../test-fixtures/environment/quest-targets.json"
        ))
        .unwrap();
        let mut projection = Projection::default();
        projection.packet(
            &lucent_protocol::Packet::Extension {
                command: "getQuests".into(),
                data: fixture["quest"].clone(),
                raw: String::new(),
                encoding: lucent_protocol::PacketEncoding::Json,
            },
            0.,
            false,
        );
        let mut state = EnvironmentState {
            quest_ids: vec![1.],
            ..Default::default()
        };
        state.quest_auto_register.requirements = true;
        state.quest_auto_register.rewards = true;
        register_loaded(&mut state, &projection);
        assert_eq!(serde_json::json!(state.item_names), fixture["expected"]);
        reduce(&mut state, EnvironmentEdit::ClearQuests {});
        assert_eq!(serde_json::json!(state.item_names), fixture["expected"]);
        state.item_names.clear();
        state.quest_ids = vec![1.];
        state.quest_auto_register.rewards = false;
        register_loaded(&mut state, &projection);
        assert_eq!(
            serde_json::json!(state.item_names),
            fixture["requirementsOnly"]
        );
    }
}
