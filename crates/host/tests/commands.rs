use lucent_host::bridge::EventInterests;
use std::time::Duration;

use lucent_host::ScriptLink;
use lucent_host::bridge::Value as LoaderValue;
use lucent_host::testing::{self, ProbeMovie};
use lucent_protocol::command::Command;
use lucent_protocol::query::Query;
use serde_json::{Value, json};

fn command(link: &ScriptLink, command: Value) -> Result<Value, String> {
    let command: Command = serde_json::from_value(command).unwrap();
    link.command(command)
        .recv_timeout(Duration::from_secs(5))
        .map(|json: String| serde_json::from_str(&json).unwrap())
        .map_err(|error| error.to_string())
}

fn query(link: &ScriptLink, query: Value) -> Value {
    let query: Query = serde_json::from_value(query).unwrap();
    let answer = link.query(query).unwrap();
    serde_json::from_str(&answer).unwrap()
}

fn emit(link: &ScriptLink, name: &str, payload: String) {
    assert_eq!(
        link.call_loader(
            "probe.emit",
            vec![
                LoaderValue::String(name.into()),
                LoaderValue::String(payload)
            ],
        )
        .recv_timeout(Duration::from_secs(5)),
        Ok(LoaderValue::Bool(true))
    );
}

fn packet(link: &ScriptLink, data: Value) {
    emit(
        link,
        "onExtensionResponse",
        json!({"type": "json", "dataObj": data}).to_string(),
    );
}

fn start(movie: ProbeMovie) -> (lucent_host::Host, lucent_host::Session, ScriptLink) {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(&host, movie);
    (host, session, link)
}

fn enter_area(link: &ScriptLink, area: &str) {
    packet(
        link,
        json!({
            "cmd": "moveToArea",
            "areaName": area,
            "uoBranch": [{"entID": 7, "uoName": "player-a", "strFrame": "Enter", "intHP": 100}],
            "mondef": [{"MonID": 1, "strMonName": "Frogzard"}, {"MonID": 2, "strMonName": "Sneevil"}],
            "monBranch": [
                {"MonID": 2, "MonMapID": 3, "intHP": 80},
                {"MonID": 1, "MonMapID": 2, "intHP": 50}
            ]
        }),
    );
}

fn become_ready(link: &ScriptLink) {
    enter_area(link, "testroom-1");
    packet(
        link,
        json!({"cmd": "initInventory", "items": [{"ItemID": 1, "sName": "Sword"}]}),
    );
    packet(link, json!({"cmd": "loadHouseInventory", "items": []}));
    assert_eq!(
        query(link, json!({"kind": "readiness"}))["missing"],
        json!([])
    );
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

fn container_items(link: &ScriptLink, container: &str) -> Value {
    fields(
        &query(link, json!({"kind": "items", "container": container})),
        &["itemId", "name"],
    )
}

fn hydration(link: &ScriptLink, container: &str) -> Value {
    query(
        link,
        json!({"kind": "itemHydration", "container": container}),
    )
}

fn auth(link: &ScriptLink) -> Value {
    query(link, json!({"kind": "auth"}))
}

fn logged_out() -> Value {
    json!({"loggedIn": false, "username": "", "password": "", "servers": []})
}

fn monster_fields(monster: &Value) -> Value {
    fields(
        monster,
        &["monsterMapId", "monsterId", "name", "race", "hp", "maxHp"],
    )
}

fn load_shop(link: &ScriptLink) {
    packet(
        link,
        json!({
            "cmd": "loadShop",
            "ShopID": 3,
            "sName": "Armory",
            "items": [{"ItemID": 1, "ShopItemID": 10, "sName": "Sword"}]
        }),
    );
}

fn drop_frog_leg(link: &ScriptLink) {
    packet(
        link,
        json!({"cmd": "dropItem", "items": {"5": {"ItemID": 5, "sName": "Frog Leg"}}}),
    );
}

#[test]
fn refresh_bank_stores_decodable_non_class_items_and_counts_one_hydration() {
    let (_host, _session, link) = start(ProbeMovie::new().constant(
        "bank.getItems",
        json!([
            {"ItemID": 1, "sName": "Sword", "sType": "Sword"},
            {"ItemID": 2, "sName": "Warrior", "sType": "Class"},
            {"sName": "Broken"}
        ]),
    ));
    assert_eq!(hydration(&link, "bank"), json!(0));
    assert_eq!(
        command(
            &link,
            json!({"kind": "refreshContainer", "container": "bank"})
        ),
        Ok(json!(1))
    );
    assert_eq!(
        container_items(&link, "bank"),
        json!([{"itemId": 1, "name": "Sword"}])
    );
    assert_eq!(hydration(&link, "bank"), json!(1));
}

#[test]
fn refresh_inventory_replaces_the_baseline_and_bumps_hydration_again() {
    let (_host, _session, link) = start(ProbeMovie::new().constant(
        "inventory.getItems",
        json!([{"ItemID": 2, "sName": "Shield"}]),
    ));
    packet(
        &link,
        json!({"cmd": "initInventory", "items": [{"ItemID": 1, "sName": "Sword"}]}),
    );
    assert_eq!(
        container_items(&link, "inventory"),
        json!([{"itemId": 1, "name": "Sword"}])
    );
    assert_eq!(hydration(&link, "inventory"), json!(1));
    assert_eq!(
        command(
            &link,
            json!({"kind": "refreshContainer", "container": "inventory"})
        ),
        Ok(json!(1))
    );
    assert_eq!(
        container_items(&link, "inventory"),
        json!([{"itemId": 2, "name": "Shield"}])
    );
    assert_eq!(hydration(&link, "inventory"), json!(2));
}

#[test]
fn refresh_house_without_a_loader_callback_fails_and_keeps_the_house() {
    let (_host, _session, link) = start(ProbeMovie::new());
    packet(
        &link,
        json!({"cmd": "loadHouseInventory", "items": [{"ItemID": 3, "sName": "Chair"}]}),
    );
    assert_eq!(
        command(
            &link,
            json!({"kind": "refreshContainer", "container": "house"})
        ),
        Err("the loader has not registered callback house.getItems".into())
    );
    assert_eq!(
        container_items(&link, "house"),
        json!([{"itemId": 3, "name": "Chair"}])
    );
    assert_eq!(hydration(&link, "house"), json!(1));
}

#[test]
fn refresh_container_rejects_the_drop_container() {
    let parsed =
        serde_json::from_value::<Command>(json!({"kind": "refreshContainer", "container": "drop"}));
    assert_eq!(
        parsed.err().map(|error| error.to_string()),
        Some("unknown variant `drop`, expected one of `bank`, `house`, `inventory`".into())
    );
}

#[test]
fn confirm_shop_closed_clears_the_shop_when_the_loader_reports_it_closed() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("shops.isOpen", json!(false)));
    load_shop(&link);
    assert_eq!(query(&link, json!({"kind": "shop"}))["id"], json!(3));
    assert_eq!(
        command(&link, json!({"kind": "confirmShopClosed", "shopId": 3})),
        Ok(json!(true))
    );
    assert_eq!(query(&link, json!({"kind": "shop"})), Value::Null);
    assert_eq!(container_items(&link, "shop"), json!([]));
}

#[test]
fn confirm_shop_closed_keeps_a_shop_the_loader_reports_open() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("shops.isOpen", json!(true)));
    load_shop(&link);
    assert_eq!(
        command(&link, json!({"kind": "confirmShopClosed"})),
        Ok(json!(false))
    );
    assert_eq!(query(&link, json!({"kind": "shop"}))["id"], json!(3));
    assert_eq!(
        container_items(&link, "shop"),
        json!([{"itemId": 1, "name": "Sword"}])
    );
}

#[test]
fn confirm_shop_closed_for_another_shop_keeps_the_open_shop() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("shops.isOpen", json!(false)));
    load_shop(&link);
    assert_eq!(
        command(&link, json!({"kind": "confirmShopClosed", "shopId": 4})),
        Ok(json!(true))
    );
    assert_eq!(query(&link, json!({"kind": "shop"}))["id"], json!(3));
    assert_eq!(
        container_items(&link, "shop"),
        json!([{"itemId": 1, "name": "Sword"}])
    );
}

#[test]
fn reject_drop_removes_the_drop_when_the_loader_accepts() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("drops.reject", json!(true)));
    drop_frog_leg(&link);
    assert_eq!(
        command(
            &link,
            json!({"kind": "rejectDrop", "selector": " frog LEG "})
        ),
        Ok(json!(true))
    );
    assert_eq!(container_items(&link, "drop"), json!([]));
}

#[test]
fn reject_drop_keeps_the_drop_when_the_loader_refuses() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("drops.reject", json!(false)));
    drop_frog_leg(&link);
    assert_eq!(
        command(&link, json!({"kind": "rejectDrop", "selector": 5})),
        Ok(json!(false))
    );
    assert_eq!(
        container_items(&link, "drop"),
        json!([{"itemId": 5, "name": "Frog Leg"}])
    );
}

#[test]
fn reject_drop_of_an_item_not_dropped_is_false_without_a_loader_call() {
    let (_host, _session, link) = start(ProbeMovie::new());
    drop_frog_leg(&link);
    assert_eq!(
        command(
            &link,
            json!({"kind": "rejectDrop", "selector": "Dragon Scale"})
        ),
        Ok(json!(false))
    );
    assert_eq!(
        container_items(&link, "drop"),
        json!([{"itemId": 5, "name": "Frog Leg"}])
    );
}

#[test]
fn refresh_cells_stores_the_loaded_cells_until_the_area_changes() {
    let (_host, _session, link) = start(
        ProbeMovie::new()
            .constant("world.isLoaded", json!(true))
            .constant("world.getCells", json!(["Enter", "r2"])),
    );
    enter_area(&link, "testroom-1");
    assert_eq!(query(&link, json!({"kind": "cells"})), json!([]));
    assert_eq!(
        command(&link, json!({"kind": "refreshCells"})),
        Ok(json!(["Enter", "r2"]))
    );
    assert_eq!(
        query(&link, json!({"kind": "cells"})),
        json!(["Enter", "r2"])
    );
    enter_area(&link, "otherroom-2");
    assert_eq!(query(&link, json!({"kind": "cells"})), json!([]));
}

#[test]
fn refresh_cells_while_the_world_is_not_loaded_returns_no_cells_without_reading_them() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("world.isLoaded", json!(false)));
    enter_area(&link, "testroom-1");
    assert_eq!(
        command(&link, json!({"kind": "refreshCells"})),
        Ok(json!([]))
    );
    assert_eq!(query(&link, json!({"kind": "cells"})), json!([]));
}

#[test]
fn refresh_cells_with_a_non_string_cell_fails_and_stores_nothing() {
    let (_host, _session, link) = start(
        ProbeMovie::new()
            .constant("world.isLoaded", json!(true))
            .constant("world.getCells", json!(["Enter", 3])),
    );
    enter_area(&link, "testroom-1");
    assert!(command(&link, json!({"kind": "refreshCells"})).is_err());
    assert_eq!(query(&link, json!({"kind": "cells"})), json!([]));
}

fn cell_pads_movie() -> ProbeMovie {
    ProbeMovie::new()
        .constant("world.isLoaded", json!(true))
        .constant("world.getCellPads", json!(["Left", "Right"]))
}

#[test]
fn refresh_cell_pads_stores_the_pads_until_self_moves_to_another_cell() {
    let (_host, _session, link) = start(cell_pads_movie());
    enter_area(&link, "testroom-1");
    assert_eq!(query(&link, json!({"kind": "cellPads"})), json!([]));
    assert_eq!(
        command(&link, json!({"kind": "refreshCellPads"})),
        Ok(json!(["Left", "Right"]))
    );
    assert_eq!(
        query(&link, json!({"kind": "cellPads"})),
        json!(["Left", "Right"])
    );
    packet(
        &link,
        json!({"cmd": "uotls", "unm": "player-a", "o": {"strFrame": "r2", "strPad": "Left"}}),
    );
    assert_eq!(query(&link, json!({"kind": "me"}))["cell"], json!("r2"));
    assert_eq!(query(&link, json!({"kind": "cellPads"})), json!([]));
}

#[test]
fn refresh_cell_pads_stores_the_pads_until_the_area_changes() {
    let (_host, _session, link) = start(cell_pads_movie());
    enter_area(&link, "testroom-1");
    assert_eq!(
        command(&link, json!({"kind": "refreshCellPads"})),
        Ok(json!(["Left", "Right"]))
    );
    assert_eq!(
        query(&link, json!({"kind": "cellPads"})),
        json!(["Left", "Right"])
    );
    enter_area(&link, "otherroom-2");
    assert_eq!(query(&link, json!({"kind": "cellPads"})), json!([]));
}

#[test]
fn hydrate_monster_returns_a_host_match_without_calling_the_loader() {
    let (_host, _session, link) = start(ProbeMovie::new());
    enter_area(&link, "testroom-1");
    assert_eq!(
        command(
            &link,
            json!({"kind": "hydrateMonster", "selector": "frogzard"})
        )
        .map(|monster| fields(&monster, &["monsterMapId", "name"])),
        Ok(json!({"monsterMapId": 2, "name": "Frogzard"}))
    );
}

#[test]
fn hydrate_monster_adds_a_flat_loader_payload_to_the_area() {
    let (_host, _session, link) = start(ProbeMovie::new().constant(
        "world.getMonster",
        json!({
            "MonMapID": 4, "MonID": 5, "strMonName": "Grizzly", "sRace": "Beast",
            "intHP": 10, "intHPMax": 10
        }),
    ));
    enter_area(&link, "testroom-1");
    assert_eq!(
        query(&link, json!({"kind": "monster", "selector": "grizzly"})),
        Value::Null
    );
    let grizzly = json!({
        "monsterMapId": 4, "monsterId": 5, "name": "Grizzly", "race": "Beast",
        "hp": 10.0, "maxHp": 10.0
    });
    assert_eq!(
        command(
            &link,
            json!({"kind": "hydrateMonster", "selector": "Grizzly"})
        )
        .map(|monster| monster_fields(&monster)),
        Ok(grizzly.clone())
    );
    assert_eq!(
        monster_fields(&query(
            &link,
            json!({"kind": "monster", "selector": "grizzly"})
        )),
        grizzly
    );
    assert_eq!(
        fields(
            &query(&link, json!({"kind": "monsters"})),
            &["monsterMapId"]
        ),
        json!([{"monsterMapId": 3}, {"monsterMapId": 2}, {"monsterMapId": 4}])
    );
}

#[test]
fn hydrate_monster_merges_a_nested_payload_with_data_leaf_winning() {
    let (_host, _session, link) = start(ProbeMovie::new().constant(
        "world.getMonster",
        json!({
            "dataLeaf": {"MonMapID": 4, "strMonName": "Grizzly", "intHP": 5},
            "objData": {
                "MonMapID": 4, "MonID": 5, "strMonName": "Bear", "sRace": "Beast",
                "intHP": 10, "intHPMax": 10
            }
        }),
    ));
    enter_area(&link, "testroom-1");
    let grizzly = json!({
        "monsterMapId": 4, "monsterId": 5, "name": "Grizzly", "race": "Beast",
        "hp": 5.0, "maxHp": 10.0
    });
    assert_eq!(
        command(
            &link,
            json!({"kind": "hydrateMonster", "selector": {"monsterMapId": 4}})
        )
        .map(|monster| monster_fields(&monster)),
        Ok(grizzly.clone())
    );
    assert_eq!(
        monster_fields(&query(&link, json!({"kind": "monster", "selector": 4}))),
        grizzly
    );
}

#[test]
fn hydrate_monster_stores_nothing_when_the_loader_returns_null() {
    let (_host, _session, link) =
        start(ProbeMovie::new().constant("world.getMonster", Value::Null));
    enter_area(&link, "testroom-1");
    assert_eq!(
        command(
            &link,
            json!({"kind": "hydrateMonster", "selector": "Grizzly"})
        ),
        Ok(Value::Null)
    );
    assert_eq!(
        fields(
            &query(&link, json!({"kind": "monsters"})),
            &["monsterMapId"]
        ),
        json!([{"monsterMapId": 3}, {"monsterMapId": 2}])
    );
}

#[test]
fn login_installs_trimmed_credentials_without_marking_logged_in() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("auth.login", Value::Null));
    assert_eq!(
        command(
            &link,
            json!({"kind": "login", "username": "  player-a ", "password": "secret"})
        ),
        Ok(json!(true))
    );
    assert_eq!(
        auth(&link),
        json!({"loggedIn": false, "username": "player-a", "password": "secret", "servers": []})
    );
}

#[test]
fn login_credentials_bind_self_by_username_instead_of_the_loader_user_id() {
    let (_host, _session, link) = start(
        ProbeMovie::new()
            .constant("auth.login", Value::Null)
            .constant("player.getUserId", json!(99)),
    );
    assert_eq!(
        command(
            &link,
            json!({"kind": "login", "username": "player-a", "password": "secret"})
        ),
        Ok(json!(true))
    );
    packet(
        &link,
        json!({
            "cmd": "moveToArea",
            "areaName": "testroom-1",
            "uoBranch": [
                {"entID": 99, "uoName": "player-b"},
                {"entID": 8, "uoName": "player-a"}
            ]
        }),
    );
    assert_eq!(
        fields(
            &query(&link, json!({"kind": "me"})),
            &["entityId", "username"]
        ),
        json!({"entityId": 8, "username": "player-a"})
    );
}

#[test]
fn login_without_a_loader_callback_fails_and_keeps_the_previous_credentials() {
    let (_host, _session, link) = start(ProbeMovie::new());
    assert_eq!(
        command(
            &link,
            json!({"kind": "login", "username": "player-a", "password": "secret"})
        ),
        Err("the loader has not registered callback auth.login".into())
    );
    assert_eq!(auth(&link), logged_out());
}

#[test]
fn login_with_an_empty_password_or_blank_username_is_false_without_a_loader_call() {
    let (_host, _session, link) = start(ProbeMovie::new());
    assert_eq!(
        command(
            &link,
            json!({"kind": "login", "username": "player-a", "password": ""})
        ),
        Ok(json!(false))
    );
    assert_eq!(
        command(
            &link,
            json!({"kind": "login", "username": "   ", "password": "secret"})
        ),
        Ok(json!(false))
    );
    assert_eq!(auth(&link), logged_out());
}

#[test]
fn logout_clears_credentials_and_servers_even_when_the_loader_call_fails() {
    let (_host, _session, link) = start(
        ProbeMovie::new()
            .constant("auth.login", Value::Null)
            .constant("auth.getServers", json!([{"sName": "Artix"}])),
    );
    command(
        &link,
        json!({"kind": "login", "username": "player-a", "password": "secret"}),
    )
    .unwrap();
    command(&link, json!({"kind": "refreshServers"})).unwrap();
    assert_eq!(auth(&link)["servers"][0]["name"], json!("Artix"));
    let _ = command(&link, json!({"kind": "logout"}));
    assert_eq!(auth(&link), logged_out());
}

#[test]
fn logout_returns_null_when_the_loader_call_succeeds() {
    let (_host, _session, link) = start(
        ProbeMovie::new()
            .constant("auth.login", Value::Null)
            .constant("auth.logout", Value::Null),
    );
    command(
        &link,
        json!({"kind": "login", "username": "player-a", "password": "secret"}),
    )
    .unwrap();
    assert_eq!(command(&link, json!({"kind": "logout"})), Ok(Value::Null));
    assert_eq!(auth(&link), logged_out());
}

#[test]
fn logout_clears_credentials_when_the_disconnect_it_causes_resets_the_connection() {
    let (_host, _session, link) = start(
        ProbeMovie::new()
            .constant("auth.login", Value::Null)
            .raising("auth.logout", "onConnection", "OnConnectionLost"),
    );
    command(
        &link,
        json!({"kind": "login", "username": "player-a", "password": "secret"}),
    )
    .unwrap();
    assert_eq!(command(&link, json!({"kind": "logout"})), Ok(Value::Null));
    assert_eq!(auth(&link), logged_out());
}

#[test]
fn refresh_credentials_before_readiness_returns_stored_values_without_a_loader_call() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("auth.login", Value::Null));
    command(
        &link,
        json!({"kind": "login", "username": "player-a", "password": "secret"}),
    )
    .unwrap();
    assert_eq!(
        command(&link, json!({"kind": "refreshCredentials"})),
        Ok(json!({"username": "player-a", "password": "secret"}))
    );
}

#[test]
fn refresh_credentials_when_ready_stores_the_trimmed_username_and_the_raw_password() {
    let (_host, _session, link) =
        start(ProbeMovie::new().constant("flash.getGameObjectS", json!("\"  player-c \"")));
    become_ready(&link);
    assert_eq!(
        command(&link, json!({"kind": "refreshCredentials"})),
        Ok(json!({"username": "player-c", "password": "  player-c "}))
    );
    assert_eq!(
        auth(&link),
        json!({"loggedIn": false, "username": "player-c", "password": "  player-c ", "servers": []})
    );
}

#[test]
fn refresh_servers_stores_the_list_with_full_derived_from_count_and_max() {
    let (_host, _session, link) = start(ProbeMovie::new().constant(
        "auth.getServers",
        json!([
            {"sName": "Artix", "iCount": 10, "iMax": 10},
            {"sName": "Yorumi", "iCount": 2, "iMax": 10, "sLang": "en", "bUpg": true}
        ]),
    ));
    let servers = json!([
        {
            "chat": 0.0, "count": 10.0, "language": "", "max": 10.0, "memberOnly": false,
            "name": "Artix", "online": true, "full": true
        },
        {
            "chat": 0.0, "count": 2.0, "language": "en", "max": 10.0, "memberOnly": true,
            "name": "Yorumi", "online": true, "full": false
        }
    ]);
    assert_eq!(
        command(&link, json!({"kind": "refreshServers"})),
        Ok(servers.clone())
    );
    assert_eq!(auth(&link)["servers"], servers);
}

#[test]
fn refresh_servers_answered_with_null_keeps_the_stored_servers() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("auth.getServers", Value::Null));
    assert_eq!(
        command(&link, json!({"kind": "refreshServers"})),
        Ok(json!([]))
    );
    assert_eq!(auth(&link)["servers"], json!([]));
}

#[test]
fn refresh_logged_in_stores_the_loader_value_until_the_connection_is_lost() {
    let (_host, _session, link) = start(ProbeMovie::new().constant("auth.isLoggedIn", json!(true)));
    assert_eq!(auth(&link)["loggedIn"], json!(false));
    assert_eq!(
        command(&link, json!({"kind": "refreshLoggedIn"})),
        Ok(json!(true))
    );
    assert_eq!(auth(&link)["loggedIn"], json!(true));
    emit(&link, "onConnection", "OnConnectionLost".into());
    assert_eq!(auth(&link)["loggedIn"], json!(false));
}

#[test]
fn refresh_accepted_quests_returns_the_accepted_quests_with_metadata() {
    let (_host, _session, link) =
        start(ProbeMovie::new().constant("quests.getAccepted", json!([10])));
    packet(
        &link,
        json!({
            "cmd": "getQuests",
            "quests": {"10": {"sName": "Frog hunt"}, "12": {"sName": "Sneevil hunt"}}
        }),
    );
    assert_eq!(query(&link, json!({"kind": "acceptedQuests"})), json!([]));
    let frog_hunt = json!({
        "cadence": "none", "id": 10.0, "name": "Frog hunt", "once": false,
        "requirements": [], "rewards": []
    });
    assert_eq!(
        command(&link, json!({"kind": "refreshAcceptedQuests"})),
        Ok(json!([frog_hunt]))
    );
    assert_eq!(
        query(&link, json!({"kind": "acceptedQuests"})),
        json!([frog_hunt])
    );
}

fn load_consumables(link: &ScriptLink) {
    packet(
        link,
        json!({
            "cmd": "initInventory",
            "items": [
                {"ItemID": 1, "sName": "Sword", "sType": "Sword", "sES": "Weapon", "bEquip": true},
                {"ItemID": 2, "sName": "Potion A", "sType": "Item", "sLink": "potion-a", "bEquip": false},
                {"ItemID": 3, "sName": "Potion B", "sType": "Item", "sLink": "potion-b", "bEquip": true},
                {"ItemID": 4, "sName": "Token", "sType": "Item", "sLink": " None ", "bEquip": true}
            ]
        }),
    );
}

fn equipped(link: &ScriptLink) -> Value {
    fields(
        &query(link, json!({"kind": "items", "container": "inventory"})),
        &["itemId", "equipped"],
    )
}

#[test]
fn refresh_equipped_consumable_equips_the_slot_item_and_unequips_other_consumables() {
    let (_host, _session, link) =
        start(ProbeMovie::new().constant("combat.getConsumableSkillItem", json!({"itemId": 2})));
    load_consumables(&link);
    assert_eq!(
        command(&link, json!({"kind": "refreshEquippedConsumable"})),
        Ok(json!(2))
    );
    assert_eq!(
        equipped(&link),
        json!([
            {"itemId": 1, "equipped": true},
            {"itemId": 2, "equipped": true},
            {"itemId": 3, "equipped": false},
            {"itemId": 4, "equipped": true}
        ])
    );
}

#[test]
fn refresh_equipped_consumable_with_an_empty_slot_unequips_every_consumable() {
    let (_host, _session, link) =
        start(ProbeMovie::new().constant("combat.getConsumableSkillItem", Value::Null));
    load_consumables(&link);
    assert_eq!(
        command(&link, json!({"kind": "refreshEquippedConsumable"})),
        Ok(Value::Null)
    );
    assert_eq!(
        equipped(&link),
        json!([
            {"itemId": 1, "equipped": true},
            {"itemId": 2, "equipped": false},
            {"itemId": 3, "equipped": false},
            {"itemId": 4, "equipped": true}
        ])
    );
}

#[test]
fn commands_before_the_viewport_are_not_ready_and_stop_with_the_session() {
    let host = testing::headless_host();
    let mut session = host.start_session(testing::headless_surface()).unwrap();
    let link = session.attach(EventInterests::ALL).unwrap();
    assert_eq!(
        link.command(Command::RefreshLoggedIn {})
            .recv_timeout(Duration::from_secs(5)),
        Err(lucent_host::bridge::Error::NotReady)
    );
    drop(session);
    assert_eq!(
        link.command(Command::RefreshLoggedIn {})
            .recv_timeout(Duration::from_secs(5)),
        Err(lucent_host::bridge::Error::Stopped)
    );
}
fn settings_movie() -> ProbeMovie {
    [
        "settings.setAnimationsEnabled",
        "settings.setCollisionsEnabled",
        "settings.setCustomGuild",
        "settings.resetCustomGuild",
        "settings.setCustomName",
        "settings.resetCustomName",
        "settings.setDeathAdsVisible",
        "settings.setOtherPlayersVisible",
        "settings.setWalkSpeed",
        "settings.setLagKillerEnabled",
        "settings.setFrameRate",
        "settings.enemyMagnet",
        "settings.infiniteRange",
        "settings.provokeCell",
        "settings.skipCutscenes",
    ]
    .into_iter()
    .fold(
        ProbeMovie::new().constant("player.isLoaded", json!(true)),
        |movie, name| movie.recording_when_ready(name),
    )
}

fn start_settings() -> (lucent_host::Host, lucent_host::Session, ScriptLink) {
    let (host, session, link) = start(settings_movie());
    link.call_loader("probe.echo", vec![])
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(loader_calls(&link), Vec::<Value>::new());
    link.call_loader("probe.ready", vec![])
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    emit(&link, "onConnection", "OnConnection".into());
    become_ready(&link);
    await_settings_ready(&link);
    loader_calls(&link);
    (host, session, link)
}

fn await_settings_ready(link: &ScriptLink) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match link.try_event().unwrap() {
            Some(lucent_host::bridge::ScriptEvent::Readiness { can_act: true, .. }) => break,
            Some(_) => {}
            None => {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                assert!(!remaining.is_zero(), "settings did not become ready");
                std::thread::park_timeout(remaining);
            }
        }
    }
    link.call_loader("probe.echo", vec![])
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
}

fn loader_calls(link: &ScriptLink) -> Vec<Value> {
    let mut calls = vec![];
    while let Some(event) = link.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert_ne!(
                event.name, "onProbeNotReady",
                "setter ran before readiness: {:?}",
                event.args
            );
            if event.name == "onProbeCall" {
                calls.push(json!(
                    event
                        .args
                        .into_iter()
                        .map(|arg| lucent_protocol::bridge::to_json(arg).unwrap_or(Value::Null))
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    calls
}

#[test]
fn update_settings_calls_the_loader_then_stores_the_record() {
    let (_host, _session, link) = start_settings();
    use std::{
        sync::{Arc, Mutex, Weak, mpsc},
        task::{Wake, Waker},
    };
    struct Observe {
        link: Weak<Mutex<ScriptLink>>,
        seen: mpsc::Sender<Value>,
    }
    impl Wake for Observe {
        fn wake(self: Arc<Self>) {
            let link = self.link.upgrade().unwrap();
            self.seen
                .send(query(&link.lock().unwrap(), json!({"kind": "settings"})))
                .unwrap();
        }
    }
    let link = Arc::new(Mutex::new(link));
    let (seen, snapshots) = mpsc::channel();
    link.lock()
        .unwrap()
        .set_waker(Waker::from(Arc::new(Observe {
            link: Arc::downgrade(&link),
            seen,
        })));
    let patch = json!({
        "animationsEnabled": false, "antiCounterEnabled": false, "collisionsEnabled": false,
        "customGuild": "Lucent", "customName": "Tommy", "deathAdsVisible": false,
        "otherPlayersVisible": false, "walkSpeed": 20, "renderingMode": "minimal", "frameRate": 12,
        "enemyMagnetEnabled": true, "infiniteRangeEnabled": true,
        "provokeCellEnabled": true, "skipCutscenesEnabled": true,
    });
    let pending = link.lock().unwrap().command(
        serde_json::from_value(json!({"kind": "updateSettings", "patch": patch})).unwrap(),
    );
    let updated: Value =
        serde_json::from_str(&pending.recv_timeout(Duration::from_secs(5)).unwrap()).unwrap();
    link.lock().unwrap().set_waker(Waker::noop().clone());
    let link = Arc::try_unwrap(link).ok().unwrap().into_inner().unwrap();
    let snapshots: Vec<Value> = snapshots.try_iter().collect();
    assert_eq!(snapshots.len(), 13);
    for snapshot in snapshots {
        assert_eq!(snapshot["walkSpeed"], 8.0);
        assert_eq!(snapshot["customNameConfigured"], false);
    }
    assert_eq!(
        updated,
        json!({
            "animationsEnabled": false, "antiCounterEnabled": false, "collisionsEnabled": false,
            "customGuild": "Lucent", "customGuildConfigured": true, "customName": "Tommy", "customNameConfigured": true,
            "deathAdsVisible": false, "otherPlayersVisible": false, "walkSpeed": 20.0,
            "renderingMode": "minimal", "frameRate": 12.0, "enemyMagnetEnabled": true,
            "infiniteRangeEnabled": true, "provokeCellEnabled": true, "skipCutscenesEnabled": true,
        })
    );
    assert_eq!(query(&link, json!({"kind": "settings"})), updated);
    let expected = vec![
        json!(["settings.setAnimationsEnabled", false]),
        json!(["settings.setCollisionsEnabled", false]),
        json!(["settings.setCustomGuild", "Lucent"]),
        json!(["settings.setCustomName", "Tommy"]),
        json!(["settings.setDeathAdsVisible", false]),
        json!(["settings.setOtherPlayersVisible", false]),
        json!(["settings.setWalkSpeed", 20.0]),
        json!(["settings.setLagKillerEnabled", true]),
        json!(["settings.setFrameRate", 2.0]),
        json!(["settings.enemyMagnet", null]),
        json!(["settings.infiniteRange", null]),
        json!(["settings.provokeCell", null]),
        json!(["settings.skipCutscenes", null]),
    ];
    assert_eq!(loader_calls(&link), expected);
    command(&link, json!({"kind": "updateSettings", "patch": patch})).unwrap();
    assert_eq!(loader_calls(&link), expected);
    command(&link, json!({"kind": "updateSettings", "patch": {
        "customGuild": null, "customName": null, "frameRate": 99, "walkSpeed": 0,
        "renderingMode": "interface-only", "enemyMagnetEnabled": false,
        "infiniteRangeEnabled": false, "provokeCellEnabled": false, "skipCutscenesEnabled": false,
    }})).unwrap();
    assert_eq!(
        loader_calls(&link),
        vec![
            json!(["settings.resetCustomGuild", null]),
            json!(["settings.resetCustomName", null]),
            json!(["settings.setWalkSpeed", 1.0]),
            json!(["settings.setLagKillerEnabled", true]),
            json!(["settings.setFrameRate", 60.0])
        ]
    );
    let reset = query(&link, json!({"kind": "settings"}));
    assert_eq!(reset["customNameConfigured"], false);
    assert_eq!(reset["customGuildConfigured"], false);
    assert_eq!(reset["customName"], "");
    assert_eq!(reset["customGuild"], "");
    command(
        &link,
        json!({"kind": "updateSettings", "patch": {"frameRate": 0, "renderingMode": "full"}}),
    )
    .unwrap();
    assert_eq!(
        loader_calls(&link),
        vec![
            json!(["settings.setLagKillerEnabled", false]),
            json!(["settings.setFrameRate", 1.0])
        ]
    );
    command(&link, json!({"kind": "updateSettings", "patch": {}})).unwrap();
    assert_eq!(loader_calls(&link), Vec::<Value>::new());
    assert_eq!(
        query(&link, json!({"kind": "settings"}))["customNameConfigured"],
        false
    );
}

#[test]
fn recurring_settings_and_user_changes_run_in_player_order() {
    let (_host, _session, link) = start_settings();
    let reapply = link.command(
        serde_json::from_value(json!({"kind": "reapplySettings", "actionsOnly": false})).unwrap(),
    );
    let update = link.command(
        serde_json::from_value(
            json!({"kind": "updateSettings", "patch": {"animationsEnabled": false}}),
        )
        .unwrap(),
    );
    reapply.recv_timeout(Duration::from_secs(5)).unwrap();
    update.recv_timeout(Duration::from_secs(5)).unwrap();
    let animations: Vec<Value> = loader_calls(&link)
        .into_iter()
        .filter(|call| call[0] == "settings.setAnimationsEnabled")
        .collect();
    assert_eq!(
        animations,
        vec![
            json!(["settings.setAnimationsEnabled", true]),
            json!(["settings.setAnimationsEnabled", false])
        ]
    );
    assert_eq!(
        query(&link, json!({"kind": "settings"}))["animationsEnabled"],
        false
    );
    command(&link, json!({"kind": "updateSettings", "patch": {"enemyMagnetEnabled": true, "provokeCellEnabled": true}})).unwrap();
    loader_calls(&link);
    command(
        &link,
        json!({"kind": "reapplySettings", "actionsOnly": true}),
    )
    .unwrap();
    assert_eq!(
        loader_calls(&link),
        vec![
            json!(["settings.enemyMagnet", null]),
            json!(["settings.provokeCell", null])
        ]
    );
}

#[test]
fn a_restarted_player_gets_the_stored_settings() {
    use std::{
        sync::Arc,
        task::{Wake, Waker},
    };
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let (_host, mut session, link) = start_settings();
    command(
        &link,
        json!({"kind": "updateSettings", "patch": {"walkSpeed": 20, "customGuild": "Lucent"}}),
    )
    .unwrap();
    session.restart(testing::headless_surface()).unwrap();
    assert!(
        link.query(serde_json::from_value(json!({"kind": "settings"})).unwrap())
            .is_err()
    );
    let current = session.attach(EventInterests::ALL).unwrap();
    current.set_waker(Waker::from(Arc::new(Unpark(std::thread::current()))));
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(!left.is_zero(), "restarted loader did not become ready");
        match current.try_event().unwrap() {
            Some(lucent_host::bridge::ScriptEvent::Loader { event, .. })
                if event.name == "onLoaded" =>
            {
                break;
            }
            Some(_) => {}
            None => std::thread::park_timeout(left),
        }
    }
    current
        .call_loader("probe.echo", vec![])
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(loader_calls(&current), Vec::<Value>::new());
    current
        .call_loader("probe.ready", vec![])
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    emit(&current, "onConnection", "OnConnection".into());
    enter_area(&current, "testroom-1");
    packet(&current, json!({"cmd": "initInventory", "items": []}));
    assert_eq!(
        query(&current, json!({"kind": "readiness"}))["missing"],
        json!(["houseInventory"])
    );
    assert_eq!(loader_calls(&current), Vec::<Value>::new());
    packet(&current, json!({"cmd": "loadHouseInventory", "items": []}));
    await_settings_ready(&current);
    let calls = loader_calls(&current);
    assert!(calls.contains(&json!(["settings.setWalkSpeed", 20.0])));
    assert!(calls.contains(&json!(["settings.setCustomGuild", "Lucent"])));
    assert_eq!(
        query(&current, json!({"kind": "settings"}))["walkSpeed"],
        20.0
    );
    assert_eq!(
        query(&current, json!({"kind": "settings"}))["customGuild"],
        "Lucent"
    );
}

#[test]
fn settings_updated_before_the_first_viewport_are_stored_and_applied_when_ready() {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe_without_viewport(&host, settings_movie());
    let updated = command(
        &link,
        json!({"kind": "updateSettings", "patch": {
            "walkSpeed": 20, "frameRate": 99, "customName": "Tommy", "antiCounterEnabled": false
        }}),
    )
    .unwrap();
    assert_eq!(updated["walkSpeed"], 20.0);
    assert_eq!(updated["frameRate"], 60.0);
    assert_eq!(updated["customName"], "Tommy");
    assert_eq!(updated["customNameConfigured"], true);
    assert_eq!(updated["antiCounterEnabled"], false);
    assert_eq!(query(&link, json!({"kind": "settings"})), updated);
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&link);
    link.call_loader("probe.echo", vec![])
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(loader_calls(&link), Vec::<Value>::new());
    link.call_loader("probe.ready", vec![])
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    emit(&link, "onConnection", "OnConnection".into());
    become_ready(&link);
    await_settings_ready(&link);
    let calls = loader_calls(&link);
    assert!(calls.contains(&json!(["settings.setWalkSpeed", 20.0])));
    assert!(calls.contains(&json!(["settings.setCustomName", "Tommy"])));
    assert!(calls.contains(&json!(["settings.setFrameRate", 60.0])));
    emit(&link, "onLoaded", String::new());
    become_ready(&link);
    assert_eq!(loader_calls(&link), Vec::<Value>::new());
}

#[test]
fn an_identical_auto_zone_patch_sends_no_event() {
    let (_host, mut session, link) = start(ProbeMovie::new());
    let other = session.attach(EventInterests::ALL).unwrap();
    command(&other, json!({"kind":"patchAutoZone","patch":{}})).unwrap();
    for link in [&link, &other] {
        while link.try_event().unwrap().is_some() {}
    }
    let patch = json!({"kind":"patchAutoZone","patch":{"enabled":true,"map":"ledgermayne"}});
    let parsed = serde_json::from_value::<Command>(patch.clone());
    assert!(parsed.is_ok(), "auto zone patch rejected: {parsed:?}");
    let expected = json!({"enabled":true,"map":"ledgermayne"});
    assert_eq!(command(&link, patch.clone()), Ok(expected.clone()));
    let first = link.try_event().unwrap().expect("missing change event");
    assert_eq!(
        first,
        lucent_host::bridge::ScriptEvent::AutoZoneChanged {
            config: lucent_protocol::auto_zone::AutoZoneConfig {
                enabled: true,
                map: Some(lucent_protocol::auto_zone::AutoZoneMap::Ledgermayne),
            },
        }
    );
    assert_eq!(other.try_event().unwrap(), Some(first));
    assert_eq!(command(&link, patch), Ok(expected.clone()));
    assert_eq!(query(&link, json!({"kind":"autoZone"})), expected);
    for link in [&link, &other] {
        assert_eq!(link.try_event().unwrap(), None);
    }
}

#[test]
fn auto_zone_patches_preserve_omitted_fields_clear_maps_and_reject_unknown_maps() {
    let (_host, _session, link) = start(ProbeMovie::new());
    command(
        &link,
        json!({"kind":"patchAutoZone","patch":{"enabled":true,"map":"queeniona"}}),
    )
    .unwrap();
    assert_eq!(
        command(
            &link,
            json!({"kind":"patchAutoZone","patch":{"enabled":false}})
        ),
        Ok(json!({"enabled":false,"map":"queeniona"}))
    );
    while link.try_event().unwrap().is_some() {}
    assert!(
        serde_json::from_value::<Command>(
            json!({"kind":"patchAutoZone","patch":{"enabled":true,"map":"battleon"}})
        )
        .is_err()
    );
    assert_eq!(
        query(&link, json!({"kind":"autoZone"})),
        json!({"enabled":false,"map":"queeniona"})
    );
    assert_eq!(link.try_event().unwrap(), None);
    assert_eq!(
        command(&link, json!({"kind":"patchAutoZone","patch":{"map":null}})),
        Ok(json!({"enabled":false,"map":null}))
    );
}

#[test]
fn auto_zone_config_survives_a_player_restart_before_the_viewport() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe_without_viewport(&host, ProbeMovie::new());
    let expected = json!({"enabled":true,"map":"astralshrine"});
    assert_eq!(
        command(
            &link,
            json!({"kind":"patchAutoZone","patch":{"enabled":true,"map":"astralshrine"}})
        ),
        Ok(expected.clone())
    );
    assert_eq!(query(&link, json!({"kind":"autoZone"})), expected);
    session.restart(testing::headless_surface()).unwrap();
    let current = session.attach(EventInterests::ALL).unwrap();
    assert_eq!(query(&current, json!({"kind":"autoZone"})), expected);
}

#[test]
fn throwing_callbacks_are_distinct_from_null_and_undefined_replies() {
    let (_host, _session, link) = start(
        ProbeMovie::new()
            .throwing("auth.logout", "logout boom")
            .constant("auth.connectTo", Value::Null),
    );
    let error = lucent_host::bridge::Error::LoaderThrew {
        callback: "auth.logout".into(),
        message: "logout boom".into(),
    };
    assert_eq!(
        link.call_loader("auth.logout", vec![])
            .recv_timeout(Duration::from_secs(5)),
        Err(error.clone())
    );
    assert_eq!(
        error.to_string(),
        "loader callback auth.logout threw: logout boom"
    );
    assert_eq!(
        link.call_loader("auth.connectTo", vec![])
            .recv_timeout(Duration::from_secs(5)),
        Ok(LoaderValue::Null)
    );
    assert_eq!(
        link.call_loader("probe.echo", vec![])
            .recv_timeout(Duration::from_secs(5)),
        Ok(LoaderValue::Undefined)
    );
}

#[test]
fn refresh_commands_propagate_callback_failures_without_changing_auth() {
    for (command, callback) in [
        (Command::RefreshCredentials {}, "flash.getGameObjectS"),
        (Command::RefreshServers {}, "auth.getServers"),
        (Command::RefreshLoggedIn {}, "auth.isLoggedIn"),
    ] {
        let (_host, _session, link) = start(ProbeMovie::new().throwing(callback, "refresh boom"));
        become_ready(&link);
        assert_eq!(
            link.command(command).recv_timeout(Duration::from_secs(5)),
            Err(lucent_host::bridge::Error::LoaderThrew {
                callback: callback.into(),
                message: "refresh boom".into(),
            })
        );
        assert_eq!(auth(&link), logged_out());
    }
}

#[test]
fn refresh_commands_propagate_missing_callbacks() {
    let (_host, _session, link) = start(ProbeMovie::new());
    become_ready(&link);
    for (command, callback) in [
        (Command::RefreshCredentials {}, "flash.getGameObjectS"),
        (Command::RefreshServers {}, "auth.getServers"),
        (Command::RefreshLoggedIn {}, "auth.isLoggedIn"),
    ] {
        assert_eq!(
            link.command(command).recv_timeout(Duration::from_secs(5)),
            Err(lucent_host::bridge::Error::Unavailable(callback.into()))
        );
    }
}

#[test]
fn settings_failures_preserve_desired_values_and_attempt_the_remaining_callbacks() {
    let (_host, _session, link) = start(
        ProbeMovie::new()
            .throwing("settings.setWalkSpeed", "walk boom")
            .recording("settings.setFrameRate"),
    );
    assert_eq!(
        link.command(Command::UpdateSettings {
            patch: lucent_protocol::settings::SettingsPatch {
                walk_speed: Some(30.0),
                frame_rate: Some(24.0),
                ..Default::default()
            }
        })
        .recv_timeout(Duration::from_secs(5)),
        Err(lucent_host::bridge::Error::LoaderThrew {
            callback: "settings.setWalkSpeed".into(),
            message: "walk boom".into(),
        })
    );
    assert_eq!(
        query(&link, json!({"kind":"settings"}))["walkSpeed"],
        json!(30.0)
    );
    assert_eq!(
        query(&link, json!({"kind":"settings"}))["frameRate"],
        json!(24.0)
    );
    assert_eq!(
        loader_calls(&link),
        vec![json!(["settings.setFrameRate", 24.0])]
    );
}
