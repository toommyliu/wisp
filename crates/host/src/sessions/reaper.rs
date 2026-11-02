use parking_lot::Mutex;
use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub(super) const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
struct Task {
    run: Box<dyn FnOnce() + Send>,
    _pending: Pending,
}

impl Task {
    fn run(self) {
        let _pending = self._pending;
        (self.run)();
    }
}

enum Message {
    Reap(Task),
    Shutdown,
}

#[derive(Clone)]
pub(super) struct Sender {
    tasks: mpsc::Sender<Message>,
    pending: Arc<AtomicUsize>,
}

struct Pending(Arc<AtomicUsize>);

impl Drop for Pending {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Sender {
    pub(super) fn reap(&self, task: impl FnOnce() + Send + 'static) {
        self.pending.fetch_add(1, Ordering::AcqRel);
        let _ = self.tasks.send(Message::Reap(Task {
            run: Box::new(task),
            _pending: Pending(self.pending.clone()),
        }));
    }
}

pub(super) struct Join {
    thread: thread::JoinHandle<()>,
    finished: mpsc::Receiver<()>,
}

impl Join {
    pub(super) fn spawn(
        name: String,
        run: impl FnOnce() + Send + 'static,
    ) -> std::io::Result<Self> {
        let (finish, finished) = mpsc::channel::<()>();
        let thread = thread::Builder::new().name(name).spawn(move || {
            run();
            drop(finish);
        })?;
        Ok(Self { thread, finished })
    }
}

#[derive(Default)]
pub(super) struct Reapers(Mutex<HashMap<u64, Reaper>>);

struct Reaper {
    sender: Sender,
    thread: Join,
    dispatch: tracing::Dispatch,
}

impl Reapers {
    pub(super) fn start(&self, session: u64) -> std::io::Result<Sender> {
        let mut reapers = self.0.lock();
        let Entry::Vacant(entry) = reapers.entry(session) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "session already has a reaper",
            ));
        };
        let (sender, tasks) = mpsc::channel();
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let worker_dispatch = dispatch.clone();
        let thread = Join::spawn(format!("lucent-reaper-{session}"), move || {
            let _guard = tracing::dispatcher::set_default(&worker_dispatch);
            while let Ok(Message::Reap(task)) = tasks.recv() {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| task.run())).is_err() {
                    tracing::error!("a worker join panicked");
                }
            }
        })?;
        let sender = Sender {
            tasks: sender,
            pending: Arc::default(),
        };
        entry.insert(Reaper {
            sender: sender.clone(),
            thread,
            dispatch,
        });
        Ok(sender)
    }

    pub(super) fn stop_session(&self, session: u64, deadline: Instant) {
        let reaper = self.0.lock().remove(&session);
        if let Some(reaper) = reaper {
            reaper.close();
            reaper.join(session, deadline);
        }
    }

    #[cfg(feature = "testing")]
    pub(super) fn barrier(&self, session: u64, timeout: Duration) -> bool {
        let sender = self.0.lock().get(&session).map(|r| r.sender.clone());
        let Some(sender) = sender else { return true };
        let (reply, answer) = mpsc::channel();
        sender.reap(move || {
            let _ = reply.send(());
        });
        answer.recv_timeout(timeout).is_ok()
    }
}

impl Reaper {
    fn close(&self) {
        let _ = self.sender.tasks.send(Message::Shutdown);
    }

    fn join(self, session: u64, deadline: Instant) {
        let _guard = tracing::dispatcher::set_default(&self.dispatch);
        if !join_until(self.thread, deadline) && self.sender.pending.load(Ordering::Acquire) != 0 {
            tracing::warn!(
                "session {session}: abandoning script worker teardown at shutdown deadline"
            );
        }
    }
}

pub(super) fn join_until(thread: Join, deadline: Instant) -> bool {
    while !thread.thread.is_finished() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let _ = thread.finished.recv_timeout(remaining);
        thread::yield_now();
    }
    let _ = thread.thread.join();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_a_session_joins_its_remaining_reaps() {
        let reapers = Reapers::default();
        let sender = reapers.start(1).unwrap();
        let (entered, ready) = mpsc::channel();
        let (release, resume) = mpsc::channel::<()>();
        sender.reap(move || {
            let _ = entered.send(());
            let _ = resume.recv();
        });
        ready.recv_timeout(Duration::from_secs(15)).unwrap();
        let (dropped, answer) = mpsc::channel();
        let dropper = thread::spawn(move || {
            reapers.stop_session(1, Instant::now() + SHUTDOWN_TIMEOUT);
            let _ = dropped.send(());
        });
        let early = answer.recv_timeout(Duration::from_millis(100));
        drop(release);
        if early.is_err() {
            answer.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        dropper.join().unwrap();
        assert!(matches!(early, Err(mpsc::RecvTimeoutError::Timeout)));
        assert!(sender.tasks.send(Message::Shutdown).is_err());
    }

    #[test]
    fn an_idle_reaper_at_the_deadline_does_not_report_abandoned_work() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use tracing_subscriber::prelude::*;
        struct Warnings(Arc<AtomicUsize>);
        impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Warnings {
            fn on_event(
                &self,
                event: &tracing::Event<'_>,
                _: tracing_subscriber::layer::Context<'_, S>,
            ) {
                if *event.metadata().level() == tracing::Level::WARN {
                    self.0.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        let warnings = Arc::new(AtomicUsize::new(0));
        let dispatch =
            tracing::Dispatch::new(tracing_subscriber::registry().with(Warnings(warnings.clone())));
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let reapers = Reapers::default();
        let sender = reapers.start(1).unwrap();
        let reaper = reapers.0.lock().remove(&1).unwrap();
        reaper.join(1, Instant::now());
        drop(sender);
        assert_eq!(
            warnings.load(Ordering::Relaxed),
            0,
            "an idle reaper has no abandoned work"
        );
    }

    #[test]
    fn a_late_reap_discards_its_task_without_panicking() {
        let reapers = Reapers::default();
        let sender = reapers.start(1).unwrap();
        reapers.stop_session(1, Instant::now() + SHUTDOWN_TIMEOUT);
        let (released, answer) = mpsc::channel::<()>();
        let result = std::panic::catch_unwind(|| {
            sender.reap(move || drop(released));
        });
        assert!(result.is_ok(), "late reap panicked after shutdown");
        assert!(matches!(
            answer.recv_timeout(Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn a_panicking_join_does_not_stop_the_sessions_reaper() {
        let reapers = Reapers::default();
        let sender = reapers.start(1).unwrap();
        sender.reap(|| panic!("teardown failed"));
        let (reply, answer) = mpsc::channel();
        sender.reap(move || reply.send(()).unwrap());
        answer.recv_timeout(Duration::from_secs(2)).unwrap();
        reapers.stop_session(1, Instant::now() + SHUTDOWN_TIMEOUT);
    }
}
