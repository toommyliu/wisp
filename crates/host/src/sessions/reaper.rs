use parking_lot::Mutex;
use std::{
    collections::{HashMap, hash_map::Entry},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub(super) const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
type Task = Box<dyn FnOnce() + Send>;

enum Message {
    Reap(Task),
    Shutdown,
}

#[derive(Clone)]
pub(super) struct Sender(mpsc::Sender<Message>);

impl Sender {
    pub(super) fn reap(&self, task: impl FnOnce() + Send + 'static) {
        self.0
            .send(Message::Reap(Box::new(task)))
            .expect("session reaper stopped before its controller");
    }
}

#[derive(Default)]
pub(super) struct Reapers(Mutex<HashMap<u64, Reaper>>);

struct Reaper {
    sender: Sender,
    thread: thread::JoinHandle<()>,
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
        let thread = thread::Builder::new()
            .name(format!("lucent-reaper-{session}"))
            .spawn(move || {
                let _guard = tracing::dispatcher::set_default(&worker_dispatch);
                while let Ok(Message::Reap(task)) = tasks.recv() {
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(task)).is_err() {
                        tracing::error!("a worker join panicked");
                    }
                }
            })?;
        let sender = Sender(sender);
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

    pub(super) fn shutdown(&self) {
        let reapers = std::mem::take(&mut *self.0.lock());
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        for reaper in reapers.values() {
            reaper.close();
        }
        for (session, reaper) in reapers {
            reaper.join(session, deadline);
        }
    }

    #[cfg(feature = "testing")]
    pub(super) fn barrier(&self, session: u64, timeout: Duration) -> bool {
        let sender = self.0.lock().get(&session).map(|r| r.sender.clone());
        let Some(sender) = sender else { return true };
        let (reply, answer) = mpsc::channel();
        if sender
            .0
            .send(Message::Reap(Box::new(move || {
                let _ = reply.send(());
            })))
            .is_err()
        {
            return false;
        }
        answer.recv_timeout(timeout).is_ok()
    }
}

impl Reaper {
    fn close(&self) {
        let _ = self.sender.0.send(Message::Shutdown);
    }

    fn join(self, session: u64, deadline: Instant) {
        let _guard = tracing::dispatcher::set_default(&self.dispatch);
        if !join_until(self.thread, deadline) {
            tracing::warn!(
                "session {session}: abandoning script worker teardown at shutdown deadline"
            );
        }
    }
}

pub(super) fn join_until(thread: thread::JoinHandle<()>, deadline: Instant) -> bool {
    while !thread.is_finished() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        thread::park_timeout(remaining.min(Duration::from_millis(5)));
    }
    let _ = thread.join();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_the_host_joins_its_remaining_reapers() {
        let host = crate::testing::headless_host();
        let sender = host.shared.reapers.start(1).unwrap();
        let (entered, ready) = mpsc::channel();
        let (release, resume) = mpsc::channel::<()>();
        sender.reap(move || {
            let _ = entered.send(());
            let _ = resume.recv();
        });
        ready.recv_timeout(Duration::from_secs(15)).unwrap();
        let (dropped, answer) = mpsc::channel();
        let dropper = thread::spawn(move || {
            drop(host);
            let _ = dropped.send(());
        });
        let early = answer.recv_timeout(Duration::from_millis(100));
        drop(release);
        if early.is_err() {
            answer.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        dropper.join().unwrap();
        assert!(matches!(early, Err(mpsc::RecvTimeoutError::Timeout)));
        assert!(sender.0.send(Message::Reap(Box::new(|| {}))).is_err());
    }

    #[test]
    fn a_panicking_join_does_not_stop_the_sessions_reaper() {
        let reapers = Reapers::default();
        let sender = reapers.start(1).unwrap();
        sender.reap(|| panic!("teardown failed"));
        let (reply, answer) = mpsc::channel();
        sender.reap(move || reply.send(()).unwrap());
        answer.recv_timeout(Duration::from_secs(2)).unwrap();
        reapers.shutdown();
    }
}
