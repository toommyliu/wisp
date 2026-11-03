use lucent_protocol::{
    auth::{CredentialsSnapshot, ServerSnapshot},
    command::RefreshContainer,
    item::ItemContainer,
    quest::QuestData,
    world::MonsterSnapshot,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{
    Projection,
    auth::decode_servers,
    decode_item,
    entity::LiveMonster,
    wire::{trim_js, wire_i64, wire_int},
    world::decode_monster,
};
use crate::bridge::Error;

pub(crate) fn decode_error(callback: &str, reason: impl ToString) -> Error {
    Error::CommandDecode {
        callback: callback.into(),
        reason: reason.to_string(),
    }
}

pub(crate) fn decode<T: DeserializeOwned>(callback: &str, answer: Value) -> Result<T, Error> {
    serde_json::from_value(answer).map_err(|error| decode_error(callback, error))
}

pub(crate) fn container_kind(container: RefreshContainer) -> (ItemContainer, &'static str) {
    match container {
        RefreshContainer::Bank => (ItemContainer::Bank, "bank.getItems"),
        RefreshContainer::House => (ItemContainer::House, "house.getItems"),
        RefreshContainer::Inventory => (ItemContainer::Inventory, "inventory.getItems"),
    }
}

pub(crate) fn refresh_container(
    projection: &mut Projection,
    container: RefreshContainer,
    answer: Value,
) -> Result<usize, Error> {
    let (kind, callback) = container_kind(container);
    let entries: Vec<Value> = decode(callback, answer)?;
    let mut items = Vec::new();
    for entry in entries {
        if kind == ItemContainer::Bank
            && entry.get("sType").and_then(Value::as_str) == Some("Class")
        {
            continue;
        }
        match decode_item(&entry, kind) {
            Some(item) => items.push(item),
            None => tracing::warn!(callback, "container refresh skipped a malformed item"),
        }
    }
    projection.items.replace(kind, items);
    Ok(projection.items.all(kind).len())
}

pub(crate) fn confirm_shop_closed(
    projection: &mut Projection,
    shop_id: Option<i64>,
    answer: Value,
) -> Result<bool, Error> {
    let open: bool = decode("shops.isOpen", answer)?;
    let stored = projection.shops.get().map(|shop| shop.data().id);
    if !open && (shop_id.is_none() || stored == shop_id) {
        projection.shops.clear();
        projection.items.replace(ItemContainer::Shop, vec![]);
    }
    Ok(!open)
}

pub(crate) fn reject_drop(
    projection: &mut Projection,
    id: i64,
    answer: Value,
) -> Result<bool, Error> {
    let rejected: bool = decode("drops.reject", answer)?;
    if rejected {
        projection.items.remove(ItemContainer::Drop, id);
    }
    Ok(rejected)
}

pub(crate) fn refresh_cells(
    projection: &mut Projection,
    answer: Value,
) -> Result<Vec<String>, Error> {
    let cells = decode("world.getCells", answer)?;
    projection.world.set_cells(cells);
    Ok(projection.world.cells())
}

pub(crate) fn refresh_cell_pads(
    projection: &mut Projection,
    answer: Value,
) -> Result<Vec<String>, Error> {
    let pads = decode("world.getCellPads", answer)?;
    projection.world.set_cell_pads(pads);
    Ok(projection.world.cell_pads())
}

pub(crate) fn hydrate_monster(
    projection: &mut Projection,
    answer: Value,
) -> Result<Option<MonsterSnapshot>, Error> {
    if answer.is_null() {
        return Ok(None);
    }
    let answer = if let Some(leaf) = answer.get("dataLeaf") {
        let mut merged = match answer.get("objData") {
            Some(data) => data
                .as_object()
                .cloned()
                .ok_or_else(|| decode_error("world.getMonster", "expected an objData object"))?,
            None => serde_json::Map::new(),
        };
        merged.extend(
            leaf.as_object()
                .ok_or_else(|| decode_error("world.getMonster", "expected a dataLeaf object"))?
                .clone(),
        );
        Value::Object(merged)
    } else {
        answer
    };
    let (data, _, explicit_state) = decode_monster(&answer)
        .ok_or_else(|| decode_error("world.getMonster", "expected a monster payload"))?;
    Ok(Some(
        projection
            .world
            .put_monster(LiveMonster::new(data, explicit_state))
            .snapshot_data(),
    ))
}

pub(crate) fn install_login(
    projection: &mut Projection,
    username: String,
    password: String,
) -> Option<CredentialsSnapshot> {
    let username = trim_js(&username).to_owned();
    if username.is_empty() || password.is_empty() {
        return None;
    }
    let previous = projection.auth.credentials();
    projection
        .auth
        .set_credentials(CredentialsSnapshot { username, password });
    Some(previous)
}

fn credential(answer: Value) -> Result<Option<String>, Error> {
    let raw: String = decode("flash.getGameObjectS", answer)?;
    serde_json::from_str(&raw).map_err(|error| decode_error("flash.getGameObjectS", error))
}

pub(crate) fn refresh_credentials(
    projection: &mut Projection,
    read_epoch: u64,
    username: Value,
    password: Value,
) -> Result<CredentialsSnapshot, Error> {
    let username = credential(username)?
        .map(|value| trim_js(&value).to_owned())
        .filter(|value| !value.is_empty());
    let password = credential(password)?.filter(|value| !value.is_empty());
    if let Some(username) = username {
        projection.auth.username = username;
        if read_epoch == projection.receipts.epoch() {
            projection.auth.credentials_epoch = Some(read_epoch);
        }
    }
    if let Some(password) = password {
        projection.auth.password = password;
    }
    Ok(projection.auth.credentials())
}

pub(crate) fn refresh_servers(
    projection: &mut Projection,
    answer: Value,
) -> Result<Vec<ServerSnapshot>, Error> {
    if answer.is_null() {
        return Ok(projection.auth.snapshot().servers);
    }
    let servers = decode_servers(&answer)
        .ok_or_else(|| decode_error("auth.getServers", "expected an array of server payloads"))?;
    let output = servers
        .iter()
        .map(|data| ServerSnapshot {
            data: data.clone(),
            full: data.max > 0.0 && data.count >= data.max,
        })
        .collect();
    projection.auth.set_servers(servers);
    Ok(output)
}

pub(crate) fn refresh_logged_in(projection: &mut Projection, answer: Value) -> Result<bool, Error> {
    projection.auth.logged_in = decode("auth.isLoggedIn", answer)?;
    Ok(projection.auth.logged_in)
}

pub(crate) fn refresh_accepted_quests(
    projection: &mut Projection,
    answer: Value,
) -> Result<Vec<QuestData>, Error> {
    let entries: Vec<Value> = decode("quests.getAccepted", answer)?;
    let ids: Vec<f64> = entries
        .iter()
        .map(|entry| {
            wire_int(entry).filter(|id| *id > 0.0).ok_or_else(|| {
                decode_error("quests.getAccepted", "expected positive integer quest ids")
            })
        })
        .collect::<Result<_, _>>()?;
    projection.quests.set_accepted(&ids);
    Ok(projection
        .quests
        .accepted()
        .iter()
        .map(|quest| quest.data().clone())
        .collect())
}

pub(crate) fn refresh_equipped_consumable(
    projection: &mut Projection,
    answer: Value,
) -> Result<Option<i64>, Error> {
    let id = if answer.is_null() {
        None
    } else {
        Some(
            answer
                .get("itemId")
                .and_then(wire_i64)
                .filter(|id| *id > 0)
                .ok_or_else(|| {
                    decode_error(
                        "combat.getConsumableSkillItem",
                        "expected a slot with a positive itemId",
                    )
                })?,
        )
    };
    for item in projection.items.all(ItemContainer::Inventory) {
        item.update(|data| {
            let link = trim_js(&data.link);
            if data.category == "Item" && !link.is_empty() && !link.eq_ignore_ascii_case("none") {
                data.equipped = id == Some(data.item_id);
            }
        });
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn malformed_answers_leave_previously_committed_state_intact() {
        let mut projection = Projection::default();
        refresh_servers(&mut projection, json!([{"sName": "Artix"}])).unwrap();
        assert!(refresh_servers(&mut projection, json!([{"sName": "Yorumi"}, {}])).is_err());
        assert_eq!(
            projection
                .auth
                .snapshot()
                .servers
                .iter()
                .map(|server| server.data.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Artix"]
        );
        assert!(refresh_accepted_quests(&mut projection, json!([10, "bad"])).is_err());
        assert_eq!(projection.quests.accepted_ids(), Vec::<f64>::new());
        assert!(
            refresh_credentials(&mut projection, 0, json!("\"new-name\""), json!("{}")).is_err()
        );
        assert_eq!(projection.auth.username, "");
        assert_eq!(projection.auth.password, "");
        assert!(
            hydrate_monster(
                &mut projection,
                json!({"dataLeaf": {"MonMapID": 4}, "objData": []})
            )
            .is_err()
        );
        assert!(projection.world.monsters().is_empty());
        assert!(refresh_equipped_consumable(&mut projection, json!({"itemId": 0})).is_err());
        assert_eq!(
            refresh_equipped_consumable(&mut projection, json!({"itemId": "2"})),
            Ok(Some(2))
        );
    }

    #[test]
    fn only_a_named_read_on_one_connection_marks_the_account_current() {
        let mut projection = Projection::default();
        projection.auth.username = "player-a".into();
        let epoch = projection.receipts.epoch();
        for missing in [json!("null"), json!("\"\""), json!("\" \\t\"")] {
            let credentials =
                refresh_credentials(&mut projection, epoch, missing, json!("null")).unwrap();
            assert_eq!(credentials.username, "player-a");
            assert_eq!(projection.auth.credentials_epoch, None);
        }
        projection.receipts.reset();
        refresh_credentials(&mut projection, epoch, json!("\"player-a\""), json!("null")).unwrap();
        assert_eq!(projection.auth.credentials_epoch, None);
        let epoch = projection.receipts.epoch();
        refresh_credentials(&mut projection, epoch, json!("\"player-b\""), json!("null")).unwrap();
        assert_eq!(projection.auth.username, "player-b");
        assert_eq!(projection.auth.credentials_epoch, Some(epoch));
    }
}
