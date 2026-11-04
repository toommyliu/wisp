#![allow(dead_code)]

use lucent_host::reply::Reply;
use lucent_host::{Host, testing};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::Instant;
use tracing::field::{Field, Visit};
use tracing_subscriber::{Layer, Registry, layer, prelude::*};

use super::TIMEOUT;

pub struct Workspace(pub PathBuf);

impl Workspace {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lucent-app-data-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("scripts")).unwrap();
        Self(path)
    }

    pub fn scripts(&self) -> PathBuf {
        self.0.join("scripts")
    }

    pub fn app_data(&self) -> PathBuf {
        self.0.join("app-data")
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.app_data(), |path, segment| path.join(segment))
    }

    pub fn host(&self) -> Host {
        testing::headless_host_with_roots(self.scripts(), self.app_data()).unwrap()
    }

    pub fn write(&self, relative: &str, contents: impl AsRef<[u8]>) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn wait<T, E>(reply: Reply<T, E>) -> Result<T, E> {
    struct Unpark(thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut reply = std::pin::pin!(reply);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Poll::Ready(result) = reply.as_mut().poll(&mut Context::from_waker(&waker)) {
            return result;
        }
        let left = deadline
            .checked_duration_since(Instant::now())
            .expect("the app data service did not answer");
        thread::park_timeout(left);
    }
}

#[derive(Default)]
pub struct Wakes(pub AtomicUsize);

impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn count_writes(host: &Host) -> Arc<AtomicUsize> {
    let writes = Arc::new(AtomicUsize::new(0));
    let counter = writes.clone();
    host.set_app_data_probe(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    writes
}

#[derive(Clone, Default)]
pub struct Capture(Arc<Mutex<Vec<BTreeMap<String, Value>>>>);

struct Fields(BTreeMap<String, Value>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().into(), format!("{value:?}").into());
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().into(), value.into());
    }
}

impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: layer::Context<'_, S>) {
        let mut fields = Fields(BTreeMap::new());
        event.record(&mut fields);
        fields
            .0
            .insert("level".into(), event.metadata().level().as_str().into());
        self.0.lock().unwrap().push(fields.0);
    }
}

impl Capture {
    pub fn start() -> (Self, tracing::dispatcher::DefaultGuard) {
        let capture = Self::default();
        let dispatch = tracing::Dispatch::new(Registry::default().with(capture.clone()));
        (capture, tracing::dispatcher::set_default(&dispatch))
    }

    pub fn take(&self) -> Vec<Value> {
        let events = std::mem::take(&mut *self.0.lock().unwrap());
        events
            .into_iter()
            .map(|event| Value::Object(event.into_iter().collect()))
            .collect()
    }
}

pub fn permissions_apply() -> bool {
    #[cfg(unix)]
    return unsafe { libc::geteuid() != 0 };
    #[cfg(not(unix))]
    return false;
}

pub fn set_mode(path: &std::path::Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}
