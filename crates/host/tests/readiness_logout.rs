mod common;

use common::TIMEOUT;
use lucent_host::{Supervisor, testing};
use std::time::Instant;

#[test]
fn logout_keeps_a_script_running_without_marking_the_avatar_ready() {
    use lucent_protocol::script::{ScriptDialogResponse, ScriptPhase};
    use std::{
        sync::{Arc, mpsc},
        task::{Wake, Waker},
    };
    struct Changed(mpsc::Sender<()>);
    impl Wake for Changed {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, common::ready_movie());
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    common::log_in(&link);
    let (tx, changed) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    let controls = supervisor
        .start_script(
            &mut session,
            common::loose(&[(
                "logout.js",
                r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* api.auth.logout();
            yield* script.confirm("continue after logout");
            yield* script.stop("survived logout");
        }
    "#,
            )]),
        )
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(dialog) = session.script_dialog() {
            assert!(!link.read(|p| p.readiness.can_act()).unwrap());
            controls.status();
            logs.wait_for(tracing::Level::INFO, "session 1: logout.js is running");
            assert_eq!(session.script_status().phase, ScriptPhase::Running);
            session.respond_dialog(dialog.id, ScriptDialogResponse::Confirmed(true));
            break;
        }
        assert!(
            session.script_status().last_end.is_none(),
            "{:?}",
            session.script_status()
        );
        changed
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    while session.script_status().phase != ScriptPhase::Idle {
        changed
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("logout.js stopped itself: survived logout")
    );
    assert!(!link.read(|p| p.readiness.can_act()).unwrap());
}
