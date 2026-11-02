mod common;
use common::{hold::hold, logs::Logs, *};
use lucent_host::{Session, Supervisor, testing};
use lucent_protocol::bridge::Value;
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
fn wait_status(session: &Session, ready: impl Fn(&Session) -> bool) {
    let (tx, rx) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    let deadline = Instant::now() + TIMEOUT;
    while !ready(session) {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}
fn emit_connection(
    observer: &lucent_host::ScriptLink,
    status: &str,
) -> lucent_host::bridge::PendingCall {
    observer.call_loader(
        "probe.emit",
        vec![
            Value::String("onConnection".into()),
            Value::String(status.into()),
        ],
    )
}

#[test]
fn a_reconnect_during_a_contended_projection_read_still_ends_the_run() {
    let logs = Logs::default();
    let (start_layer, started) = hold(
        "session 1: contend.js started",
        Some("lucent-supervisor"),
        true,
    );
    let (write_layer, write) = hold("projection connection changed", Some("player-"), false);
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(start_layer)
            .with(write_layer),
    )
    .expect("run this probe alone");
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    session
        .update_script_options(lucent_protocol::script_options::ScriptOptionsPatch {
            restart_after_reconnect: Some(false),
            safe_start_stop: Some(false),
            ..Default::default()
        })
        .unwrap();
    log_in_ready(&observer);
    let (request, controls) = lucent_host::ScriptRequest::new(loose(&[(
        "contend.js",
        "import * as script from 'lucent/script'; export default function* () { yield* script.sleep('1 hour'); }",
    )]));
    let _supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
    started.wait();
    emit_connection(&observer, "OnConnectionLost")
        .recv_timeout(TIMEOUT)
        .unwrap();
    log_in_ready(&observer);
    write.arm();
    let unrelated = emit_connection(&observer, "Unrelated");
    write.wait();
    started.open();
    controls.status();
    controls.status();
    logs.wait_for_count(INFO, "session 1: contend.js is running", 2);
    write.open();
    unrelated.recv_timeout(TIMEOUT).unwrap();
    wait_status(&session, |s| s.script_status().last_end.is_some());
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("contend.js was stopped: Connection lost")
    );
}
