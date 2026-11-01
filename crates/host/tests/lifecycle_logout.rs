mod common;
use common::{hold::hold, logs::Logs, *};
use lucent_host::{Session, Supervisor, testing};
use lucent_protocol::script::ScriptPhase;
use std::{
    sync::{Arc, mpsc},
    task::{Wake, Waker},
    time::Instant,
};
const INFO: tracing::Level = tracing::Level::INFO;
use tracing_subscriber::prelude::*;
struct Changed(mpsc::Sender<()>);
impl Wake for Changed {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }
}
fn wait_idle(session: &Session) {
    let (tx, rx) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    let deadline = Instant::now() + TIMEOUT;
    while session.script_status().phase != ScriptPhase::Idle {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}
#[test]
fn logout_finishes_before_idle() {
    let logs = Logs::default();
    let (logout_h, logout) = hold("projection connection changed", None, false);
    let layered = tracing_subscriber::registry()
        .with(logs.clone())
        .with(logout_h);
    tracing::subscriber::set_global_default(layered).expect("run this probe alone");
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        ready_movie().raising_sequence(
            "auth.logout",
            [
                ("onDebug".into(), "host logout".into()),
                ("onConnection".into(), "OnConnectionLost".into()),
            ],
        ),
    );
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    log_in(&observer);
    logout.arm();
    let controls = supervisor
        .start_script(
            &mut session,
            loose(&[(
                "x.js",
                "import * as script from 'lucent/script'; export default function* () { yield* script.exit({ logout: true }); }",
            )]),
        )
        .unwrap();
    logout.wait();
    assert!(session.wait_for_script_reaping(TIMEOUT));
    controls.status();
    let deadline = Instant::now() + TIMEOUT;
    let line = loop {
        let lines: Vec<_> = logs
            .messages(INFO)
            .into_iter()
            .filter(|l| {
                l == "session 1: x.js is finishing" || l == "session 1: no script is running"
            })
            .collect();
        if let Some(line) = lines.first() {
            break line.clone();
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    let phase = session.script_status().phase;
    logout.open();
    wait_idle(&session);
    println!("while logout held: status line {line:?}, phase {phase:?}");
    assert_eq!(line, "session 1: x.js is finishing");
    assert_eq!(phase, ScriptPhase::Finishing);
}
