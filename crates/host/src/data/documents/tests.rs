use serde_json::{Map, Value, json};
use std::path::Path;

use super::accounts::AccountsDocument;
use super::json::JsMap;
use super::{Issues, Loaded, Unreadable, account_settings, accounts};
use crate::accounts::{Account, Password};

fn corpus(path: &str) -> Vec<u8> {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/schemas/test-fixtures/app-data");
    std::fs::read(root.join(path)).unwrap()
}

fn outcome<T>(loaded: Loaded<T>, encode: fn(&T) -> Vec<u8>) -> (Value, Option<Vec<u8>>) {
    match loaded {
        Loaded::Missing => (json!("missing"), None),
        Loaded::Unreadable(Unreadable::NotAnObject) => {
            (json!({"unreadable": "not-an-object"}), None)
        }
        Loaded::Unreadable(Unreadable::Malformed { .. }) => {
            (json!({"unreadable": "malformed"}), None)
        }
        Loaded::Decoded(decoded) => (
            json!({"issues": issues(&decoded.issues)}),
            Some(encode(&decoded.value)),
        ),
    }
}

fn issues(issues: &Issues) -> Value {
    let mut fields = Map::new();
    if !issues.reset.is_empty() {
        fields.insert("reset".into(), json!(issues.reset));
    }
    if !issues.ignored.is_empty() {
        let counts = issues.ignored.iter();
        fields.insert(
            "ignored".into(),
            counts
                .map(|(name, count)| (name.to_string(), json!(count)))
                .collect(),
        );
    }
    fields.into()
}

#[test]
fn documents_read_and_write_like_the_release() {
    let cases: Vec<Value> = serde_json::from_slice(&corpus("cases.json")).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let input = corpus(case["input"].as_str().unwrap());
        let (read, output) = match case["document"].as_str().unwrap() {
            "accounts" => outcome(accounts::decode(Some(&input)), accounts::encode),
            "account-settings" => outcome(
                account_settings::decode(Some(&input)),
                account_settings::encode,
            ),
            document => panic!("unknown document {document}"),
        };
        let host = &case["host"];
        let (mut expected_read, expected) = match host.get("read") {
            Some(read) => (read.clone(), host["output"].as_str()),
            None => (
                case["read"].clone(),
                host["output"].as_str().or(case["output"].as_str()),
            ),
        };
        let expected = expected.map(corpus);
        if let Some(position) = expected_read.as_object_mut() {
            position.remove("line");
            position.remove("column");
        }
        if read != expected_read || output != expected {
            failures.push(format!(
                "{}\n  read {read}, expected {}\n  wrote {:?}\n  expected {:?}",
                case["name"],
                expected_read,
                output.map(String::from_utf8),
                expected.map(String::from_utf8),
            ));
        }
    }
    assert!(cases.len() > 100, "the corpus has {} cases", cases.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn account_settings_file_names_match_the_release() {
    let cases: Vec<Value> =
        serde_json::from_slice(&corpus("account-settings-file-names.json")).unwrap();
    for case in &cases {
        let username = case["username"].as_str().unwrap();
        assert_eq!(
            account_settings::file_name(username).ok().as_deref(),
            case["file"].as_str(),
            "{}",
            case["name"]
        );
    }
    assert_eq!(
        account_settings::file_name("..").unwrap_err().to_string(),
        "Invalid account username."
    );
}

#[test]
fn a_missing_file_reads_as_missing() {
    assert!(matches!(accounts::decode(None), Loaded::Missing));
    assert!(matches!(account_settings::decode(None), Loaded::Missing));
}

#[test]
fn deep_nesting_reads_as_malformed_without_exhausting_the_stack() {
    let depth = 1_000_000;
    let file = format!(
        r#"{{"accounts":[],"groups":{{}},"arrays":{}{},"objects":{}1{}}}"#,
        "[".repeat(depth),
        "]".repeat(depth),
        r#"{"a":"#.repeat(depth),
        "}".repeat(depth),
    );
    let (read, output) = outcome(accounts::decode(Some(file.as_bytes())), accounts::encode);
    assert_eq!(read, json!({"unreadable": "malformed"}));
    assert_eq!(output, None);
}

#[test]
fn encoding_normalizes_like_the_release_serializer() {
    let account = |label: &str, username: &str, password: &str| Account {
        label: label.into(),
        username: username.into(),
        password: Password::new(password),
    };
    let mut groups = JsMap::default();
    let members = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
    groups.insert(
        " Raiders ".into(),
        members(&["Hero", "Missing", "hero", "Alt", "Alt"]),
    );
    groups.insert("raiders".into(), members(&["Alt"]));
    groups.insert("__proto__".into(), members(&[]));
    groups.insert("2".into(), members(&[]));
    groups.insert("".into(), members(&["Hero"]));
    let document = AccountsDocument {
        accounts: vec![
            account("Hero", "Hero", "secret"),
            account("Lower", "hero", "other"),
            account("Alt", "Alt", "alt"),
        ],
        groups,
    };
    assert_eq!(
        String::from_utf8(accounts::encode(&document)).unwrap(),
        "{\n  \"accounts\": [\n    {\n      \"label\": \"Hero\",\n      \"username\": \"Hero\",\n      \"password\": \"secret\"\n    },\n    {\n      \"label\": \"Alt\",\n      \"username\": \"Alt\",\n      \"password\": \"alt\"\n    }\n  ],\n  \"groups\": {\n    \"2\": [],\n    \"Raiders\": [\n      \"Hero\",\n      \"Alt\"\n    ]\n  }\n}\n"
    );
}
