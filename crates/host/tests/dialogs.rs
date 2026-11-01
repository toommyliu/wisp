mod common;

use common::{logs::Logs, *};
use lucent_host::{
    Session,
    bridge::EventInterests,
    scripting::{Kill, RuntimeError, ScriptRun},
    testing,
};
use lucent_protocol::script::{
    ScriptDialog, ScriptDialogKind, ScriptDialogResponse, ScriptOutcome,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    task::{Wake, Waker},
    time::Instant,
};

struct Changed(mpsc::Sender<()>, Arc<AtomicUsize>);
impl Wake for Changed {
    fn wake(self: Arc<Self>) {
        self.1.fetch_add(1, Ordering::SeqCst);
        let _ = self.0.send(());
    }
}

struct DialogRun {
    session: Session,
    run: ScriptRun,
    changed: mpsc::Receiver<()>,
    notifications: Arc<AtomicUsize>,
}

impl DialogRun {
    fn start(body: &str) -> Self {
        let logs = Logs::default();
        let dispatch = logs.dispatch();
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (session, link) = testing::start_probe(&host, ready_movie());
        log_in(&link);
        let (sender, changed) = mpsc::channel();
        let notifications = Arc::new(AtomicUsize::new(0));
        session.watch(Waker::from(Arc::new(Changed(
            sender,
            notifications.clone(),
        ))));
        let source = format!(
            r#"
            import * as script from 'lucent/script';
            import {{ Effect }} from 'effect';
            export default function* () {{
                setTimeout(() => console.log('dialogs queued'), 0);
                const result = yield* Effect.gen(function* () {{ {body} }});
                yield* script.stop(JSON.stringify(result));
            }}
        "#
        );
        let run =
            ScriptRun::start(link, loose(&[("dialogs.js", &source)]), Default::default()).unwrap();
        logs.wait_for(tracing::Level::INFO, "dialogs queued");
        Self {
            session,
            run,
            changed,
            notifications,
        }
    }

    fn current(&self, message: &str) -> ScriptDialog {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(dialog) = self.session.script_dialog()
                && dialog.message == message
            {
                return dialog;
            }
            self.changed
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!(
                        "waiting for {message}: {error:?}, current {:?}",
                        self.session.script_dialog()
                    )
                });
        }
    }

    fn respond(&self, dialog: &ScriptDialog, response: ScriptDialogResponse) {
        self.session.respond_dialog(dialog.id, response);
    }

    fn result(&self) -> Value {
        let outcome = self.run.wait(TIMEOUT).unwrap();
        assert!(self.session.script_dialog().is_none());
        let ScriptOutcome::StoppedByScript {
            reason: Some(value),
        } = outcome
        else {
            panic!("{outcome:?}")
        };
        serde_json::from_str(&value).unwrap()
    }
}

#[test]
fn alert_waits_for_dismissal_before_the_script_continues() {
    let task = DialogRun::start(
        "const dismissed = yield* script.alert('message'); return [dismissed === undefined, 'answered'];",
    );
    let dialog = task.current("message");
    assert_eq!(dialog.kind, ScriptDialogKind::Alert);
    assert_eq!(dialog.script, "dialogs.js");
    assert!(matches!(
        task.run.wait(std::time::Duration::ZERO),
        Err(RuntimeError::Timeout)
    ));
    task.respond(&dialog, ScriptDialogResponse::Dismissed);
    assert_eq!(task.result(), json!([true, "answered"]));
}

#[test]
fn confirm_returns_true_for_ok_and_false_for_cancel() {
    let task =
        DialogRun::start("return [yield* script.confirm('ok'), yield* script.confirm('cancel')];");
    let first = task.current("ok");
    task.respond(&first, ScriptDialogResponse::Confirmed(true));
    let second = task.current("cancel");
    assert!(second.id > first.id);
    task.respond(&second, ScriptDialogResponse::Confirmed(false));
    assert_eq!(task.result(), json!([true, false]));
}

#[test]
fn prompt_returns_text_empty_text_and_null_without_using_its_placeholder() {
    let task = DialogRun::start(
        "return [yield* script.prompt('text', 'hint'), yield* script.prompt('empty', 'hint'), yield* script.prompt('cancel', 'hint')];",
    );
    let first = task.current("text");
    assert_eq!(
        first.kind,
        ScriptDialogKind::Prompt {
            placeholder: "hint".into()
        }
    );
    task.respond(
        &first,
        ScriptDialogResponse::Submitted(Some("answer".into())),
    );
    task.respond(
        &task.current("empty"),
        ScriptDialogResponse::Submitted(Some(String::new())),
    );
    task.respond(
        &task.current("cancel"),
        ScriptDialogResponse::Submitted(None),
    );
    assert_eq!(task.result(), json!(["answer", "", null]));
}

const CONCURRENT: &str = "return yield* Effect.raceFirst(Effect.flatMap(script.confirm('first'), () => Effect.tryPromise(() => new Promise(() => {}))), script.confirm('second'));";

#[test]
fn concurrent_dialogs_are_served_in_fifo_order() {
    let task = DialogRun::start(CONCURRENT);
    let first = task.current("first");
    task.respond(&first, ScriptDialogResponse::Confirmed(true));
    let second = task.current("second");
    assert!(second.id > first.id);
    task.respond(&second, ScriptDialogResponse::Confirmed(false));
    assert_eq!(task.result(), json!(false));
}

#[test]
fn stale_and_mismatched_responses_do_not_advance_the_queue() {
    let task = DialogRun::start(
        "return [yield* script.confirm('first'), yield* script.prompt('second')];",
    );
    let first = task.current("first");
    let notifications = task.notifications.load(Ordering::SeqCst);
    task.session
        .respond_dialog(first.id + 1, ScriptDialogResponse::Confirmed(true));
    task.respond(&first, ScriptDialogResponse::Dismissed);
    task.respond(&first, ScriptDialogResponse::Submitted(None));
    assert_eq!(task.session.script_dialog(), Some(first.clone()));
    assert_eq!(task.notifications.load(Ordering::SeqCst), notifications);
    task.respond(&first, ScriptDialogResponse::Confirmed(false));
    let second = task.current("second");
    task.respond(&first, ScriptDialogResponse::Confirmed(true));
    task.respond(&second, ScriptDialogResponse::Confirmed(true));
    assert_eq!(task.session.script_dialog(), Some(second.clone()));
    task.respond(
        &second,
        ScriptDialogResponse::Submitted(Some("new answer".into())),
    );
    assert_eq!(task.result(), json!([false, "new answer"]));
}

#[test]
fn stopping_clears_the_current_dialog_and_queue_with_one_notification() {
    let task = DialogRun::start(CONCURRENT);
    let first = task.current("first");
    let notifications = task.notifications.load(Ordering::SeqCst);
    task.run.stop_script();
    assert_eq!(
        task.run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    assert!(task.session.script_dialog().is_none());
    assert_eq!(task.notifications.load(Ordering::SeqCst) - notifications, 1);
    task.respond(&first, ScriptDialogResponse::Confirmed(true));
    assert!(task.session.script_dialog().is_none());
}

#[test]
fn killing_clears_the_current_dialog_and_queue_with_one_notification() {
    let task = DialogRun::start(CONCURRENT);
    let first = task.current("first");
    let notifications = task.notifications.load(Ordering::SeqCst);
    task.run.kill();
    assert!(matches!(
        task.run.wait(TIMEOUT),
        Err(RuntimeError::Killed(Kill::Requested))
    ));
    assert!(task.session.script_dialog().is_none());
    assert_eq!(task.notifications.load(Ordering::SeqCst) - notifications, 1);
    task.respond(&first, ScriptDialogResponse::Confirmed(true));
    assert_eq!(task.notifications.load(Ordering::SeqCst), notifications + 1);
}

#[test]
fn player_restart_ends_the_run_once_and_clears_dialogs() {
    let mut task = DialogRun::start(CONCURRENT);
    let first = task.current("first");
    let notifications = task.notifications.load(Ordering::SeqCst);
    task.session
        .restart_player(testing::headless_surface())
        .unwrap();
    assert!(matches!(task.run.wait(TIMEOUT), Err(RuntimeError::Stopped)));
    assert!(task.session.script_dialog().is_none());
    assert_eq!(task.notifications.load(Ordering::SeqCst) - notifications, 1);
    task.respond(&first, ScriptDialogResponse::Confirmed(true));
    assert!(task.session.script_dialog().is_none());
    assert_eq!(task.notifications.load(Ordering::SeqCst), notifications + 1);
    assert!(matches!(task.run.wait(TIMEOUT), Err(RuntimeError::Stopped)));
    let observer = task.session.attach(EventInterests::ALL).unwrap();
    task.session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100., 100., 1.).unwrap(),
    ));
    testing::wait_loaded(&observer);
}

#[test]
fn watchers_are_notified_for_publication_replacement_and_close() {
    let notifications = 0;
    let task = DialogRun::start(CONCURRENT);
    let first = task.current("first");
    assert_eq!(task.notifications.load(Ordering::SeqCst) - notifications, 1);
    task.respond(&first, ScriptDialogResponse::Confirmed(true));
    let second = task.current("second");
    assert_eq!(task.notifications.load(Ordering::SeqCst) - notifications, 2);
    task.respond(&second, ScriptDialogResponse::Confirmed(false));
    assert_eq!(task.result(), json!(false));
    assert_eq!(task.notifications.load(Ordering::SeqCst) - notifications, 3);
}

#[test]
fn fiber_interruption_removes_current_and_queued_dialogs() {
    let task = DialogRun::start(
        "yield* Effect.raceFirst(Effect.raceFirst(script.alert('current'), script.alert('queued')), Effect.tryPromise(() => Promise.resolve('winner'))); return yield* script.prompt('survivor', 'hint');",
    );
    let survivor = task.current("survivor");
    task.respond(
        &survivor,
        ScriptDialogResponse::Submitted(Some("still running".into())),
    );
    assert_eq!(task.result(), json!("still running"));
}
