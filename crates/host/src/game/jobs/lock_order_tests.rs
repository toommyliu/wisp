use super::tests::start_profile;
use super::*;
use crate::sessions::operations::LockProbe;
use lucent_protocol::operation::{Operation, OperationAnswer};
use serde_json::json;
use std::{io::Read, process::Command, sync::mpsc, thread};

const WAIT: Duration = Duration::from_secs(5);

struct Hold {
    entered: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

impl Hold {
    fn new() -> (Self, LockProbe) {
        let (entered, waiting) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let resume = Mutex::new(resume);
        let first = AtomicBool::new(true);
        let probe = Arc::new(move || {
            if first.swap(false, Ordering::AcqRel) {
                let _ = entered.send(());
                let _ = resume.lock().recv();
            }
        });
        (
            Self {
                entered: waiting,
                release,
            },
            probe,
        )
    }

    fn wait(&self) {
        self.entered
            .recv_timeout(WAIT)
            .expect("lock boundary not reached");
    }

    fn release(self) {
        drop(self.release);
    }
}

#[test]
fn cancellation_claim_and_dispatch_finish_without_deadlock() {
    const CHILD: &str = "LUCENT_JOB_LOCK_ORDER_CHILD";
    if std::env::var_os(CHILD).is_some() {
        run_lock_interleaving();
        return;
    }
    let name = concat!(
        module_path!(),
        "::cancellation_claim_and_dispatch_finish_without_deadlock"
    )
    .split_once("::")
    .unwrap()
    .1;
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(CHILD, "1")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (finished, result) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut output = String::new();
        stdout.read_to_string(&mut output).unwrap();
        let _ = finished.send(output);
    });
    let output = result.recv_timeout(Duration::from_secs(15));
    if output.is_err() {
        child.kill().unwrap();
    }
    let status = child.wait().unwrap();
    reader.join().unwrap();
    assert!(
        output.is_ok(),
        "job cancellation, claim acquisition, and dispatch deadlocked"
    );
    assert!(status.success(), "{}", output.unwrap());
}

fn run_lock_interleaving() {
    let host = crate::testing::headless_host();
    let (mut session, observer) = crate::testing::start_probe(
        &host,
        crate::testing::ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .constant("bank.loadItems", json!(null))
            .constant("bank.isLoaded", json!(true))
            .constant("bank.getItems", json!([])),
    );
    let jobs = ProfileJobs::new(
        session.attach(ProfileJobs::INTERESTS).unwrap(),
        Waker::noop().clone(),
    );
    let owner = session.attach(EventInterests::ALL).unwrap();
    let job = start_profile(&owner, json!({"steps":[{"skill":5}]}));
    let request = jobs.requests.handoff.lock().queue.pop_front().unwrap();
    let operation = owner.prepare_operation().unwrap();
    request.gate.state.lock().operation = Some(operation);
    let dispatch = observer.prepare_operation().unwrap();
    let (before_issue, issue_probe) = Hold::new();
    owner.operations.set_probe(move |id, issued| {
        if id == dispatch && !issued {
            issue_probe();
        }
    });
    observer
        .submit_operation(dispatch, Operation::LoadBank { force: true })
        .unwrap();
    before_issue.wait();

    let (cancelling, cancel_probe) = Hold::new();
    *owner.operations.cancel_probe.lock() = Some(cancel_probe);
    let (claiming, claim_probe) = Hold::new();
    *request.gate.open_probe.lock() = Some(claim_probe);
    let (issuing, dispatch_probe) = Hold::new();
    *owner.operations.dispatch_probe.lock() = Some(dispatch_probe);
    let (cancelled, cancellation) = mpsc::channel();
    let (claimed, claim) = mpsc::channel();
    thread::scope(|scope| {
        let cancel = scope.spawn(|| {
            owner.jobs.cancel(job);
            cancelled.send(()).unwrap();
        });
        cancelling.wait();
        let acquire = scope.spawn(|| {
            let result = request.player.packets.acquire_for_job(
                request.owner,
                Some(100),
                &request.gate,
                &request.detached,
            );
            claimed.send(result).unwrap();
        });
        claiming.wait();
        before_issue.release();
        issuing.wait();
        cancelling.release();
        claiming.release();
        issuing.release();
        cancellation
            .recv_timeout(WAIT)
            .expect("job cancellation blocked");
        assert!(matches!(
            claim.recv_timeout(WAIT).unwrap(),
            Err(Error::Stopped)
        ));
        cancel.join().unwrap();
        acquire.join().unwrap();
    });
    assert!(matches!(
        owner.submit_operation(operation, Operation::LoadBank { force: true }),
        Err(Error::Stopped)
    ));
    let sibling = observer
        .acquire_consumable_claim(Some(101))
        .unwrap()
        .unwrap();
    observer.release_claim(sibling.id);
    struct Wake(thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    observer.set_waker(Waker::from(Arc::new(Wake(thread::current()))));
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(ScriptEvent::Operation { id, answer }) = observer.try_event().unwrap() {
            assert_eq!(id, dispatch);
            assert!(matches!(answer, Ok(OperationAnswer::LoadBank(true))));
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "issued operation did not settle");
        thread::park_timeout(remaining);
    }
}
