use std::time::Duration;

use lucent_host::ScriptLink;
use lucent_host::bridge::Value as LoaderValue;
use lucent_host::testing::{self, ProbeMovie};
use lucent_protocol::query::Query;
use serde_json::{Value, json};

fn query(link: &ScriptLink, query: Value) -> Value {
    let query: Query = serde_json::from_value(query).unwrap();
    let answer = link.query(query).unwrap();
    serde_json::from_str(&answer).unwrap()
}

fn packet(link: &ScriptLink, data: Value) {
    let raw = json!({"type": "json", "dataObj": data}).to_string();
    assert_eq!(
        link.call_loader(
            "probe.emit",
            vec![
                LoaderValue::String("onExtensionResponse".into()),
                LoaderValue::String(raw),
            ],
        )
        .recv_timeout(Duration::from_secs(5)),
        Ok(LoaderValue::Bool(true))
    );
}

fn start() -> (lucent_host::Host, lucent_host::Session, ScriptLink) {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(&host, ProbeMovie::new());
    (host, session, link)
}

fn enter_area(link: &ScriptLink) {
    packet(
        link,
        json!({
            "cmd": "moveToArea",
            "areaId": 5,
            "areaName": "testroom-1",
            "uoBranch": [
                {"entID": 8, "uoName": "player-b", "strFrame": "Enter", "intHP": 100, "intHPMax": 100},
                {"entID": 7, "uoName": "player-a", "strFrame": "Enter", "intHP": 100, "intHPMax": 100}
            ],
            "mondef": [
                {"MonID": 1, "strMonName": "Frogzard", "sRace": "Amphibian", "iLvl": 3},
                {"MonID": 2, "strMonName": "Sneevil", "sRace": "Goblin", "iLvl": 5}
            ],
            "monBranch": [
                {"MonID": 2, "MonMapID": 3, "intHP": 80, "intHPMax": 80},
                {"MonID": 1, "MonMapID": 2, "intHP": 50, "intHPMax": 50}
            ],
            "monmap": [
                {"MonMapID": 3, "strFrame": "r2"},
                {"MonMapID": 2, "strFrame": "Enter"}
            ]
        }),
    );
}

fn area_player(entity_id: i64, username: &str) -> Value {
    json!({
        "cell": "Enter", "hp": 100.0, "maxHp": 100.0, "maxMp": 0.0, "mp": 0.0, "state": 1,
        "afk": false, "entityId": entity_id, "entityType": "player", "level": 0.0, "pad": "",
        "position": {"x": 0.0, "y": 0.0}, "username": username,
        "alive": true, "auras": [], "dead": false, "hpPercent": 100.0, "idle": true,
        "inCombat": false, "mpPercent": 0.0
    })
}

fn fields(value: &Value, keys: &[&str]) -> Value {
    match value {
        Value::Null => Value::Null,
        Value::Array(values) => values.iter().map(|value| fields(value, keys)).collect(),
        Value::Object(object) => Value::Object(
            keys.iter()
                .map(|key| {
                    let field = object
                        .get(*key)
                        .unwrap_or_else(|| panic!("{key} is missing from {value}"));
                    ((*key).to_owned(), field.clone())
                })
                .collect(),
        ),
        other => panic!("expected an object, an array or null, got {other}"),
    }
}

#[test]
fn map_reports_the_area_a_move_to_area_packet_entered() {
    let (_host, _session, link) = start();
    assert_eq!(
        query(&link, json!({"kind": "map"})),
        json!({"id": 0, "name": "", "roomNumber": 0.0})
    );
    enter_area(&link);
    assert_eq!(
        query(&link, json!({"kind": "map"})),
        json!({"id": 5, "name": "testroom", "roomNumber": 1.0})
    );
}

#[test]
fn me_is_null_before_an_area_and_the_player_the_loader_user_id_names_after() {
    let (_host, _session, link) = start();
    assert_eq!(query(&link, json!({"kind": "me"})), Value::Null);
    enter_area(&link);
    assert_eq!(
        query(&link, json!({"kind": "me"})),
        area_player(7, "player-a")
    );
}

#[test]
fn self_entity_id_is_null_before_an_area_and_the_bound_entity_after() {
    let (_host, _session, link) = start();
    assert_eq!(query(&link, json!({"kind": "selfEntityId"})), Value::Null);
    enter_area(&link);
    assert_eq!(query(&link, json!({"kind": "selfEntityId"})), json!(7));
}

#[test]
fn player_finds_a_trimmed_case_insensitive_username_or_an_entity_id() {
    let (_host, _session, link) = start();
    enter_area(&link);
    assert_eq!(
        query(&link, json!({"kind": "player", "selector": "  PLAYER-B "})),
        area_player(8, "player-b")
    );
    assert_eq!(
        query(&link, json!({"kind": "player", "selector": 8})),
        area_player(8, "player-b")
    );
    assert_eq!(
        query(&link, json!({"kind": "player", "selector": "player-z"})),
        Value::Null
    );
    assert_eq!(
        query(&link, json!({"kind": "player", "selector": 99})),
        Value::Null
    );
}

#[test]
fn players_lists_the_area_in_packet_order() {
    let (_host, _session, link) = start();
    enter_area(&link);
    assert_eq!(
        query(&link, json!({"kind": "players"})),
        json!([area_player(8, "player-b"), area_player(7, "player-a")])
    );
}

#[test]
fn player_auras_filter_by_kind_and_an_unknown_entity_has_none() {
    let (_host, _session, link) = start();
    enter_area(&link);
    packet(
        &link,
        json!({"cmd": "aura+", "tInf": "p:7", "aura": {"nam": "Focus"}}),
    );
    packet(
        &link,
        json!({"cmd": "aura+p", "tInf": "p:7", "auras": [{"nam": "Calm"}]}),
    );
    let focus = json!({"duration": 0.0, "kind": "active", "name": "Focus", "persistent": false, "stack": 1.0});
    let calm = json!({"duration": 0.0, "kind": "passive", "name": "Calm", "persistent": false, "stack": 1.0});
    assert_eq!(
        query(&link, json!({"kind": "playerAuras", "entityId": 7})),
        json!([focus, calm])
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "playerAuras", "entityId": 7, "auraKind": "active"})
        ),
        json!([focus])
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "playerAuras", "entityId": 7, "auraKind": "passive"})
        ),
        json!([calm])
    );
    assert_eq!(
        query(&link, json!({"kind": "playerAuras", "entityId": 99})),
        json!([])
    );
}

#[test]
fn monster_by_map_id_number_returns_its_snapshot() {
    let (_host, _session, link) = start();
    enter_area(&link);
    assert_eq!(
        query(&link, json!({"kind": "monster", "selector": 2})),
        json!({
            "cell": "Enter", "hp": 50.0, "maxHp": 50.0, "maxMp": 0.0, "mp": 0.0, "state": 1,
            "aggressive": false, "level": 3.0, "monsterId": 1, "monsterMapId": 2,
            "name": "Frogzard", "race": "Amphibian",
            "alive": true, "auras": [], "dead": false, "hpPercent": 100.0, "idle": true,
            "inCombat": false, "mpPercent": 0.0, "drops": []
        })
    );
}

#[test]
fn monster_selector_forms_resolve_to_the_expected_map_id() {
    let (_host, _session, link) = start();
    enter_area(&link);
    let selectors = [
        json!(2),
        json!("id:2"),
        json!("ID.2"),
        json!("id'2"),
        json!("id-2"),
        json!(" Frogzard "),
        json!("*"),
        json!(""),
        json!({"monsterMapId": 2}),
        json!({"name": "sneevil"}),
        json!("Dragon"),
        json!("id:9"),
        json!(99),
    ];
    let found: Vec<(Value, Value)> = selectors
        .into_iter()
        .map(|selector| {
            let monster = query(&link, json!({"kind": "monster", "selector": selector}));
            (selector, monster["monsterMapId"].clone())
        })
        .collect();
    assert_eq!(
        found,
        vec![
            (json!(2), json!(2)),
            (json!("id:2"), json!(2)),
            (json!("ID.2"), json!(2)),
            (json!("id'2"), json!(2)),
            (json!("id-2"), json!(2)),
            (json!(" Frogzard "), json!(2)),
            (json!("*"), json!(3)),
            (json!(""), Value::Null),
            (json!({"monsterMapId": 2}), json!(2)),
            (json!({"name": "sneevil"}), json!(3)),
            (json!("Dragon"), Value::Null),
            (json!("id:9"), Value::Null),
            (json!(99), Value::Null),
        ]
    );
}

#[test]
fn monsters_lists_map_ids_and_names_in_packet_order() {
    let (_host, _session, link) = start();
    enter_area(&link);
    assert_eq!(
        fields(
            &query(&link, json!({"kind": "monsters"})),
            &["monsterMapId", "name"]
        ),
        json!([
            {"monsterMapId": 3, "name": "Sneevil"},
            {"monsterMapId": 2, "name": "Frogzard"}
        ])
    );
}

#[test]
fn monster_auras_filter_by_kind_and_an_unknown_monster_has_none() {
    let (_host, _session, link) = start();
    enter_area(&link);
    packet(
        &link,
        json!({"cmd": "aura+", "tInf": "m:2", "aura": {"nam": "Stun"}}),
    );
    packet(
        &link,
        json!({"cmd": "aura+p", "tInf": "m:2", "auras": [{"nam": "Thick Skin"}]}),
    );
    let stun = json!({"duration": 0.0, "kind": "active", "name": "Stun", "persistent": false, "stack": 1.0});
    let skin = json!({"duration": 0.0, "kind": "passive", "name": "Thick Skin", "persistent": false, "stack": 1.0});
    assert_eq!(
        query(&link, json!({"kind": "monsterAuras", "monsterMapId": 2})),
        json!([stun, skin])
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "monsterAuras", "monsterMapId": 2, "auraKind": "active"})
        ),
        json!([stun])
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "monsterAuras", "monsterMapId": 2, "auraKind": "passive"})
        ),
        json!([skin])
    );
    assert_eq!(
        query(&link, json!({"kind": "monsterAuras", "monsterMapId": 99})),
        json!([])
    );
}

#[test]
fn monster_drops_is_null_until_a_drop_packet_then_lists_item_ids_and_names() {
    let (_host, _session, link) = start();
    enter_area(&link);
    assert_eq!(
        query(&link, json!({"kind": "monsterDrops", "monsterMapId": 2})),
        Value::Null
    );
    packet(
        &link,
        json!({
            "cmd": "monsterDrops",
            "MonMapID": 2,
            "items": {
                "5": {"ItemID": 5, "sName": "Frog Leg", "iRty": 11},
                "6": {"ItemID": 6, "sName": "Frogzard Tooth"}
            }
        }),
    );
    let drops = query(&link, json!({"kind": "monsterDrops", "monsterMapId": 2}));
    let drops: Vec<Value> = drops
        .as_array()
        .unwrap()
        .iter()
        .map(|drop| {
            json!({
                "itemId": drop["item"]["itemId"],
                "name": drop["item"]["name"],
                "rarityName": drop["rarityName"]
            })
        })
        .collect();
    assert_eq!(
        drops,
        vec![
            json!({"itemId": 5, "name": "Frog Leg", "rarityName": "Common"}),
            json!({"itemId": 6, "name": "Frogzard Tooth", "rarityName": "Unknown"}),
        ]
    );
}

#[test]
fn item_resolves_each_inventory_selector_form_by_item_id() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({
            "cmd": "initInventory",
            "items": [
                {"ItemID": 1, "sName": "Sword", "iQty": 1},
                {"ItemID": 2, "sName": "Health Potion", "iQty": 5}
            ]
        }),
    );
    assert_eq!(
        fields(
            &query(
                &link,
                json!({"kind": "item", "container": "inventory", "selector": 2})
            ),
            &["itemId", "name", "quantity"]
        ),
        json!({"itemId": 2, "name": "Health Potion", "quantity": 5.0})
    );
    let selectors = [
        json!("  health POTION "),
        json!({"itemId": 1}),
        json!({"name": "SWORD"}),
        json!(1.5),
        json!(99),
        json!("Shield"),
    ];
    let found: Vec<(Value, Value)> = selectors
        .into_iter()
        .map(|selector| {
            let item = query(
                &link,
                json!({"kind": "item", "container": "inventory", "selector": selector}),
            );
            (selector, item["itemId"].clone())
        })
        .collect();
    assert_eq!(
        found,
        vec![
            (json!("  health POTION "), json!(2)),
            (json!({"itemId": 1}), json!(1)),
            (json!({"name": "SWORD"}), json!(1)),
            (json!(1.5), Value::Null),
            (json!(99), Value::Null),
            (json!("Shield"), Value::Null),
        ]
    );
}

#[test]
fn item_on_the_shop_finds_shop_item_ids_and_the_first_item_with_an_item_id() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({
            "cmd": "loadShop",
            "ShopID": 3,
            "sName": "Armory",
            "items": [
                {"ItemID": 1, "ShopItemID": 10, "sName": "Sword"},
                {"ItemID": 1, "ShopItemID": 11, "sName": "Sword+"}
            ]
        }),
    );
    let item = |selector: Value| {
        fields(
            &query(
                &link,
                json!({"kind": "item", "container": "shop", "selector": selector}),
            ),
            &["itemId", "shopItemId", "name"],
        )
    };
    assert_eq!(
        item(json!(1)),
        json!({"itemId": 1, "shopItemId": 10, "name": "Sword"})
    );
    assert_eq!(
        item(json!({"itemId": 1})),
        json!({"itemId": 1, "shopItemId": 10, "name": "Sword"})
    );
    assert_eq!(
        item(json!({"shopItemId": 11})),
        json!({"itemId": 1, "shopItemId": 11, "name": "Sword+"})
    );
    assert_eq!(item(json!({"shopItemId": 12})), Value::Null);
}

#[test]
fn items_lists_a_container_by_item_id_in_insertion_order() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({
            "cmd": "initInventory",
            "items": [
                {"ItemID": 3, "sName": "Cape"},
                {"ItemID": 1, "sName": "Sword"},
                {"ItemID": 2, "sName": "Helm"}
            ]
        }),
    );
    assert_eq!(
        fields(
            &query(&link, json!({"kind": "items", "container": "inventory"})),
            &["itemId", "name"]
        ),
        json!([
            {"itemId": 3, "name": "Cape"},
            {"itemId": 1, "name": "Sword"},
            {"itemId": 2, "name": "Helm"}
        ])
    );
}

#[test]
fn item_hydration_counts_inventory_baselines() {
    let (_host, _session, link) = start();
    assert_eq!(
        query(
            &link,
            json!({"kind": "itemHydration", "container": "inventory"})
        ),
        json!(0)
    );
    packet(
        &link,
        json!({"cmd": "initInventory", "items": [{"ItemID": 1, "sName": "Sword"}]}),
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "itemHydration", "container": "inventory"})
        ),
        json!(1)
    );
}

#[test]
fn readiness_reports_every_key_missing_until_area_and_baselines_arrive() {
    let (_host, _session, link) = start();
    assert_eq!(
        query(&link, json!({"kind": "readiness"})),
        json!({
            "epoch": 0,
            "canAct": false,
            "failures": {},
            "missing": ["houseInventory", "inventory", "map", "player"],
            "state": {"houseInventory": false, "inventory": false, "map": false, "player": false}
        })
    );
    enter_area(&link);
    packet(
        &link,
        json!({"cmd": "initInventory", "items": [{"ItemID": 1, "sName": "Sword"}]}),
    );
    packet(&link, json!({"cmd": "loadHouseInventory", "items": []}));
    assert_eq!(
        query(&link, json!({"kind": "readiness"})),
        json!({
            "epoch": 0,
            "canAct": false,
            "failures": {},
            "missing": [],
            "state": {"houseInventory": true, "inventory": true, "map": true, "player": true}
        })
    );
}

#[test]
fn quest_returns_loaded_metadata_and_null_for_an_unloaded_id() {
    let (_host, _session, link) = start();
    assert_eq!(
        query(&link, json!({"kind": "quest", "id": 10})),
        Value::Null
    );
    packet(
        &link,
        json!({"cmd": "getQuests", "quests": {"10": {"sName": "Frog hunt", "sField": "id0"}}}),
    );
    assert_eq!(
        query(&link, json!({"kind": "quest", "id": 10})),
        json!({
            "cadence": "daily", "id": 10.0, "name": "Frog hunt", "once": false,
            "requirements": [], "rewards": []
        })
    );
    assert_eq!(
        query(&link, json!({"kind": "quest", "id": 11})),
        Value::Null
    );
}

#[test]
fn quests_lists_ids_and_names_in_load_order() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({"cmd": "getQuests", "quests": {"12": {"sName": "Sneevil hunt"}}}),
    );
    packet(
        &link,
        json!({"cmd": "getQuests", "quests": {"10": {"sName": "Frog hunt"}}}),
    );
    assert_eq!(
        fields(&query(&link, json!({"kind": "quests"})), &["id", "name"]),
        json!([
            {"id": 12.0, "name": "Sneevil hunt"},
            {"id": 10.0, "name": "Frog hunt"}
        ])
    );
}

#[test]
fn shop_is_null_until_a_load_shop_packet_then_reports_its_fields_and_item_ids() {
    let (_host, _session, link) = start();
    assert_eq!(query(&link, json!({"kind": "shop"})), Value::Null);
    packet(
        &link,
        json!({
            "cmd": "loadShop",
            "ShopID": 3,
            "sName": "Armory",
            "bLimited": true,
            "items": [
                {"ItemID": 1, "ShopItemID": 10, "sName": "Sword"},
                {"ItemID": 2, "ShopItemID": 11, "sName": "Shield"}
            ]
        }),
    );
    let shop = query(&link, json!({"kind": "shop"}));
    assert_eq!(
        fields(&shop, &["house", "id", "limited", "merge", "name"]),
        json!({"house": false, "id": 3, "limited": true, "merge": false, "name": "Armory"})
    );
    assert_eq!(
        fields(&shop["items"], &["itemId", "shopItemId", "name"]),
        json!([
            {"itemId": 1, "shopItemId": 10, "name": "Sword"},
            {"itemId": 2, "shopItemId": 11, "name": "Shield"}
        ])
    );
}

#[test]
fn auth_starts_logged_out_with_empty_credentials_and_no_servers() {
    let (_host, _session, link) = start();
    assert_eq!(
        query(&link, json!({"kind": "auth"})),
        json!({"loggedIn": false, "username": "", "password": "", "servers": []})
    );
}

#[test]
fn fractional_entity_selectors_miss_without_failing_to_parse() {
    let (_host, _session, link) = start();
    enter_area(&link);
    for input in [
        json!({"kind": "player", "selector": 7.5}),
        json!({"kind": "monster", "selector": 2.5}),
        json!({"kind": "monster", "selector": {"monsterMapId": 2.5}}),
    ] {
        assert_eq!(query(&link, input), Value::Null);
    }
    assert_eq!(
        query(&link, json!({"kind": "monster", "selector": " id:02 "})),
        Value::Null
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "monster", "selector": "\u{feff}ID-2\u{00a0}"})
        )["monsterMapId"],
        json!(2)
    );
}

#[test]
fn item_primary_key_selectors_take_precedence_and_names_keep_the_first_match() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({
            "cmd": "initInventory",
            "items": [
                {"ItemID": 2, "sName": " Sword "},
                {"ItemID": 1, "sName": "Sword"},
                {"ItemID": 3, "sName": ""}
            ]
        }),
    );
    let item = |selector| {
        query(
            &link,
            json!({"kind": "item", "container": "inventory", "selector": selector}),
        )["itemId"]
            .clone()
    };
    assert_eq!(
        item(json!({"shopItemId": 1, "itemId": 2, "name": "Sword"})),
        json!(1)
    );
    assert_eq!(item(json!("\u{feff}SWORD\u{00a0}")), json!(2));
    assert_eq!(item(json!("")), json!(3));
    assert_eq!(item(json!("*")), Value::Null);
    assert_eq!(item(json!("id:2")), Value::Null);
    assert_eq!(item(json!({"shopItemId": 1.5})), Value::Null);
}

#[test]
fn null_item_arrays_complete_empty_inventory_baselines() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({"cmd": "loadInventoryBig", "hitems": null, "items": null}),
    );
    let readiness = query(&link, json!({"kind": "readiness"}));
    assert_eq!(readiness["state"]["houseInventory"], true);
    assert_eq!(readiness["state"]["inventory"], true);
    assert_eq!(
        query(&link, json!({"kind": "items", "container": "inventory"})),
        json!([])
    );
    assert_eq!(
        query(&link, json!({"kind": "items", "container": "house"})),
        json!([])
    );
}

#[test]
fn malformed_and_sparse_baselines_remain_unready_and_explain_the_failure() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({"cmd": "loadInventoryBig", "hitems": null, "items": [
            {"ItemID": "1", "sName": "Valid Item"}, {"sName": "Missing item id"}
        ]}),
    );
    let readiness = query(&link, json!({"kind": "readiness"}));
    assert_eq!(
        readiness["failures"],
        json!({"inventory": "items:loadInventoryBig:entries contained 1 malformed item entries"})
    );
    assert_eq!(readiness["missing"], json!(["inventory", "map", "player"]));
    assert_eq!(
        readiness["state"],
        json!({"houseInventory": true, "inventory": false, "map": false, "player": false})
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "item", "container": "inventory", "selector": 1})
        )["name"],
        "Valid Item"
    );
    packet(&link, json!({"cmd": "loadInventoryBig", "items": []}));
    let readiness = query(&link, json!({"kind": "readiness"}));
    assert_eq!(
        readiness["failures"],
        json!({"houseInventory": "loadInventoryBig omitted hitems"})
    );
    assert_eq!(
        readiness["state"],
        json!({"houseInventory": false, "inventory": true, "map": false, "player": false})
    );
}

#[test]
fn guild_item_additions_go_to_inventory() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({"cmd": "addItems", "items": {"8": {"sName": "Guild Item", "sType": "Guild", "iQty": 1}}}),
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "item", "container": "inventory", "selector": 8})
        )["name"],
        "Guild Item"
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "item", "container": "house", "selector": 8})
        ),
        Value::Null
    );
}

#[test]
fn class_point_gains_use_base_points_and_allow_negative_adjustments() {
    let (_host, _session, link) = start();
    packet(
        &link,
        json!({"cmd": "loadInventoryBig", "items": [{
            "CharItemID": 101, "ItemID": 1, "bEquip": true, "iQty": 99400,
            "sES": "ar", "sName": "Barber", "sType": "Class"
        }]}),
    );
    packet(
        &link,
        json!({"cmd": "addGoldExp", "bonusCP": 18, "bonusGold": 6,
        "iCP": 36, "id": 1, "intExp": 20, "intGold": 12, "typ": "m"}),
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "item", "container": "inventory", "selector": "Barber"})
        )["quantity"],
        99436.0
    );
    packet(
        &link,
        json!({"cmd": "addGoldExp", "iCP": -36, "id": 0, "typ": "q"}),
    );
    assert_eq!(
        query(
            &link,
            json!({"kind": "item", "container": "inventory", "selector": "Barber"})
        )["quantity"],
        99400.0
    );
}
