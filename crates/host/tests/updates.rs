mod common;

use common::app_data::*;
use lucent_host::app_settings::PreferencesPatch;
use lucent_host::updates::UpdateState;
use std::sync::Arc;
use std::task::Waker;

#[test]
fn update_checks_start_off_and_stay_off_the_network_until_enabled() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let wakes = Arc::new(Wakes::default());
    host.updates().watch(Waker::from(wakes.clone()));

    assert_eq!(
        host.updates().current_version().to_string(),
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(wait(host.updates().state()), Ok(UpdateState::Disabled));
    assert_eq!(wait(host.updates().check(false)), Ok(UpdateState::Disabled));
    assert!(wakes.0.load(std::sync::atomic::Ordering::SeqCst) > 0);
    assert!(!workspace.path("update-cache.json").exists());

    wait(host.app_settings().update_preferences(PreferencesPatch {
        check_for_updates: Some(true),
        ..PreferencesPatch::default()
    }))
    .unwrap();
    drop(host);
    let fresh = workspace.host();
    assert_eq!(wait(fresh.updates().state()), Ok(UpdateState::Idle));
}
