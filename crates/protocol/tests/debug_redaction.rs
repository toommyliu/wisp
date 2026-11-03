use lucent_protocol::{
    auth::{AuthSnapshot, CredentialsSnapshot},
    command::Command,
};
use std::fmt::Debug;

const CANARY: &str = "hunter2-canary";

fn assert_redacted(value: &impl Debug, kept: &[&str]) {
    for rendered in [format!("{value:?}"), format!("{value:#?}")] {
        assert!(!rendered.contains(CANARY), "password leaked: {rendered}");
        for field in kept {
            assert!(rendered.contains(field), "{field} missing: {rendered}");
        }
    }
}

#[test]
fn auth_snapshot_debug_hides_the_password() {
    let snapshot = AuthSnapshot {
        logged_in: true,
        username: "Hero".into(),
        password: CANARY.into(),
        servers: Vec::new(),
    };
    assert_redacted(&snapshot, &["logged_in", "Hero", "servers"]);
}

#[test]
fn credentials_snapshot_debug_hides_the_password() {
    let credentials = CredentialsSnapshot {
        username: "Hero".into(),
        password: CANARY.into(),
    };
    assert_redacted(&credentials, &["Hero"]);
}

#[test]
fn login_command_debug_hides_the_password() {
    let login = Command::Login {
        username: "Hero".into(),
        password: CANARY.into(),
    };
    assert_redacted(&login, &["Login", "Hero"]);
}

#[test]
fn other_commands_keep_their_fields_in_debug() {
    let command = Command::SetAutoReloginServer {
        server: Some("Twilly".into()),
    };
    assert_redacted(&command, &["SetAutoReloginServer", "server", "Twilly"]);
}
