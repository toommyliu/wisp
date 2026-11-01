use super::TIMEOUT;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
pub struct Hold {
    message: String,
    thread: Option<&'static str>,
    armed: Arc<AtomicBool>,
    entered: Mutex<mpsc::Sender<()>>,
    release: Mutex<mpsc::Receiver<()>>,
    remaining: AtomicUsize,
}
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Hold {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        if !self.armed.load(Ordering::Acquire) {
            return;
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        let name = std::thread::current().name().unwrap_or("").to_string();
        if message.0 == self.message
            && !name.starts_with("lucent-js-watchdog")
            && self.thread.is_none_or(|prefix| name.starts_with(prefix))
            && self
                .remaining
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1))
                .is_ok()
        {
            let _ = self.entered.lock().unwrap().send(());
            let _ = self.release.lock().unwrap().recv_timeout(TIMEOUT);
        }
    }
}
pub struct Gate {
    entered: mpsc::Receiver<()>,
    release: Mutex<Option<mpsc::Sender<()>>>,
    armed: Arc<AtomicBool>,
}
impl Gate {
    pub fn wait(&self) {
        self.entered
            .recv_timeout(TIMEOUT)
            .expect("hold was not reached");
    }
    pub fn open(&self) {
        if let Some(release) = self.release.lock().unwrap().take() {
            let _ = release.send(());
        }
    }
    pub fn arm(&self) {
        self.armed.store(true, Ordering::Release);
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        self.open();
    }
}
pub fn hold(message: &str, thread: Option<&'static str>, armed: bool) -> (Hold, Gate) {
    let (entered_tx, entered) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let flag = Arc::new(AtomicBool::new(armed));
    (
        Hold {
            message: message.into(),
            thread,
            armed: flag.clone(),
            entered: Mutex::new(entered_tx),
            release: Mutex::new(release),
            remaining: AtomicUsize::new(1),
        },
        Gate {
            entered,
            release: Mutex::new(Some(release_tx)),
            armed: flag,
        },
    )
}
