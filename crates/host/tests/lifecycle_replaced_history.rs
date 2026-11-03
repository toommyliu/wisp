mod common;
use common::{hold::hold, logs::Logs, *};
use lucent_host::{Session, Supervisor, testing};
use lucent_protocol::{bridge::Value, script::ScriptPhase};
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
fn a_replacement_player_reconnected_before_the_controller_takes_it_starts_the_next_run() {
    let logs = Logs::default();
    let (status_layer, status) = hold(
        "session 1: no script is running",
        Some("lucent-supervisor"),
        true,
    );
    let (write_layer, write) = hold("projection connection changed", Some("player-"), false);
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(status_layer)
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
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    log_in_ready(&observer);
    let controls = supervisor
        .start_script(
            &mut session,
            loose(&[("first.js", "export default function* () {}")]),
        )
        .unwrap();
    logs.wait_for(INFO, "session 1: first.js completed");
    wait_status(&session, |s| s.script_status().phase == ScriptPhase::Idle);
    controls.status();
    status.wait();
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    let observer = session
        .attach(lucent_host::bridge::EventInterests::ALL)
        .unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    log_in_ready(&observer);
    emit_connection(&observer, "OnConnectionLost")
        .recv_timeout(TIMEOUT)
        .unwrap();
    log_in_ready(&observer);
    write.arm();
    let unrelated = emit_connection(&observer, "Unrelated");
    write.wait();
    std::thread::scope(|scope| {
        let starting = scope.spawn(|| {
            supervisor.start_script(
                &mut session,
                loose(&[(
                    "next.js",
                    "export default function* () { console.log('next body'); }",
                )]),
            )
        });
        status.open();
        starting.join().unwrap().unwrap();
    });
    write.open();
    unrelated.recv_timeout(TIMEOUT).unwrap();
    wait_status(&session, |s| {
        s.script_status()
            .last_end
            .is_some_and(|end| end.starts_with("next.js"))
    });
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("next.js completed")
    );
    assert_eq!(
        logs.messages(INFO)
            .iter()
            .filter(|line| *line == "next body")
            .count(),
        1
    );
}
