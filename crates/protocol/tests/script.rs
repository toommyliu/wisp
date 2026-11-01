use lucent_protocol::script::{
    InputValue, ModulePath, Script, ScriptError, ScriptInputs, ScriptOutcome,
};
use std::collections::BTreeMap;

#[test]
fn paths_validate_and_display() {
    assert_eq!(ModulePath::loose("farm.js").unwrap().to_string(), "farm.js");
    let p = ModulePath::in_package("@lucent/dailies", "scripts/farm.js").unwrap();
    assert_eq!(p.package(), Some("@lucent/dailies"));
    assert_eq!(p.path(), "scripts/farm.js");
    assert_eq!(p.to_string(), "@lucent/dailies/scripts/farm.js");
    for path in ["", "/a", "a/", "a//b", "./a", "a/../b", "a\\b"] {
        assert!(ModulePath::loose(path).is_err(), "{path}");
    }
    for name in [
        "",
        "effect",
        "lucent",
        "lucent/api",
        "@scope",
        "@/x",
        "@scope/",
        "a/b",
        "Upper",
        ".x",
        "a b",
    ] {
        assert!(ModulePath::in_package(name, "a.js").is_err(), "{name}");
    }
}

#[test]
fn relative_candidates_stay_in_the_package() {
    let p = ModulePath::in_package("@lucent/dailies", "scripts/run.js").unwrap();
    for (specifier, expected) in [
        (
            "./x",
            vec!["scripts/x", "scripts/x.js", "scripts/x/index.js"],
        ),
        ("../x", vec!["x", "x.js", "x/index.js"]),
        (
            "./x.js",
            vec!["scripts/x.js", "scripts/x.js.js", "scripts/x.js/index.js"],
        ),
        ("../../x", vec![]),
        ("lucent/api", vec![]),
    ] {
        let actual = p.candidates(specifier);
        assert_eq!(
            actual.iter().map(ModulePath::path).collect::<Vec<_>>(),
            expected
        );
        assert!(
            actual
                .iter()
                .all(|p| p.package() == Some("@lucent/dailies"))
        );
    }
}

#[test]
fn script_requires_sources_and_resolves_in_order() {
    let entry = ModulePath::loose("run.js").unwrap();
    assert_eq!(
        Script::new(entry.clone(), BTreeMap::new(), BTreeMap::new()).unwrap_err(),
        ScriptError::MissingEntry(entry.clone())
    );
    let main = ModulePath::in_package("@lucent/dailies", "index.js").unwrap();
    let modules = BTreeMap::from([(entry.clone(), "run".into())]);
    let packages = BTreeMap::from([("@lucent/dailies".into(), main.clone())]);
    assert_eq!(
        Script::new(entry.clone(), modules, packages.clone()).unwrap_err(),
        ScriptError::MissingPackageMain {
            name: "@lucent/dailies".into(),
            path: main.clone()
        }
    );
    let modules = ["run.js", "x", "x.js", "y.js", "z/index.js"]
        .into_iter()
        .map(|s| (ModulePath::loose(s).unwrap(), s.into()))
        .chain([(main.clone(), "package".into())])
        .collect();
    let script = Script::new(entry.clone(), modules, packages).unwrap();
    assert_eq!(script.entry(), &entry);
    assert_eq!(script.source(&entry), Some("run.js"));
    for (specifier, expected) in [
        ("./x", "x"),
        ("./y", "y.js"),
        ("./z", "z/index.js"),
        ("@lucent/dailies", "@lucent/dailies/index.js"),
    ] {
        assert_eq!(
            script.resolve(&entry, specifier).unwrap().to_string(),
            expected
        );
    }
    assert_eq!(script.resolve(&entry, "./missing"), None);
}

#[test]
fn package_mains_must_belong_to_the_package_table_key() {
    for (main, expected) in [
        (
            ModulePath::in_package("@lucent/other", "index.js").unwrap(),
            "Main module for @lucent/dailies must belong to that package: @lucent/other/index.js",
        ),
        (
            ModulePath::loose("index.js").unwrap(),
            "Main module for @lucent/dailies must belong to that package: index.js",
        ),
    ] {
        let entry = ModulePath::loose("run.js").unwrap();
        assert_eq!(
            Script::new(
                entry.clone(),
                BTreeMap::from([(entry, "".into()), (main.clone(), "".into())]),
                BTreeMap::from([("@lucent/dailies".into(), main)]),
            )
            .unwrap_err()
            .to_string(),
            expected
        );
    }
}

#[test]
fn outcomes_decode_the_wire_contract() {
    for (json, expected) in [
        (r#"{"kind":"completed"}"#, ScriptOutcome::Completed),
        (
            r#"{"kind":"failed","message":"broken","details":"stack"}"#,
            ScriptOutcome::Failed {
                message: "broken".into(),
                details: Some("stack".into()),
            },
        ),
        (
            r#"{"kind":"script-stopped","reason":"done"}"#,
            ScriptOutcome::StoppedByScript {
                reason: Some("done".into()),
            },
        ),
        (
            r#"{"kind":"script-exited","closeClient":true}"#,
            ScriptOutcome::Exited {
                logout: false,
                close_client: true,
            },
        ),
        (
            r#"{"kind":"externally-stopped"}"#,
            ScriptOutcome::Stopped { reason: None },
        ),
    ] {
        assert_eq!(
            serde_json::from_str::<ScriptOutcome>(json).unwrap(),
            expected
        );
    }
    let inputs: ScriptInputs =
        serde_json::from_str(r#"{"name":"farm","count":2.5,"enabled":true,"items":["a","b"]}"#)
            .unwrap();
    assert_eq!(
        inputs,
        BTreeMap::from([
            ("name".into(), InputValue::String("farm".into())),
            ("count".into(), InputValue::Number(2.5)),
            ("enabled".into(), InputValue::Boolean(true)),
            (
                "items".into(),
                InputValue::Strings(vec!["a".into(), "b".into()])
            )
        ])
    );
    assert!(serde_json::from_str::<InputValue>("null").is_err());
    assert!(serde_json::from_str::<InputValue>("[1]").is_err());
}
