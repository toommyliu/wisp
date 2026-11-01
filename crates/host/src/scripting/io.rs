use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::task::Waker;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;

use super::http::{Http, Response};
use crate::data::{Files, Request};
use crate::sessions::dialogs::Dialogs;
use lucent_protocol::script::ScriptDialogKind;
use tokio::task::AbortHandle;

const MAX_REQUESTS: usize = 16;

#[derive(Deserialize)]
#[serde(tag = "service", rename_all = "camelCase")]
enum RequestKind {
    Http {
        #[serde(flatten)]
        request: super::http::Request,
    },
    Dialog {
        kind: ScriptDialogKind,
        message: String,
    },
    Filesystem {
        #[serde(flatten)]
        request: Request,
    },
}

impl RequestKind {
    fn failure(&self, reason: &str) -> Value {
        match self {
            Self::Filesystem { request } => request.failure(reason),
            Self::Http { request } => request.failure(reason),
            Self::Dialog { .. } => {
                serde_json::json!({"ok":false,"error":format!("Script dialog unavailable: {reason}")})
            }
        }
    }
}

enum Phase {
    Queued,
    Issued,
    Completed(u64, Box<serde_json::value::RawValue>),
}

#[derive(Clone, Copy, PartialEq)]
enum Service {
    Filesystem,
    Dialog,
    Http,
}

struct Pending {
    phase: Phase,
    closed: Value,
    service: Service,
    abort: Option<AbortHandle>,
    body: Option<Vec<u8>>,
    #[cfg(feature = "testing")]
    path: String,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
    }
}

#[derive(Default)]
struct State {
    closed: bool,
    stopping: bool,
    script: String,
    next: u64,
    completed: u64,
    notified: bool,
    requests: HashMap<u64, Pending>,
    bodies: HashMap<u64, Vec<u8>>,
}

pub(crate) struct Io {
    files: Weak<Files>,
    http: Weak<Http>,
    dialogs: Weak<Dialogs>,
    owner: u64,
    wake: Waker,
    state: Mutex<State>,
}

pub(crate) struct Ticket {
    io: Arc<Io>,
    id: u64,
}

impl Io {
    pub(crate) fn new(
        files: Arc<Files>,
        dialogs: Arc<Dialogs>,
        http: Arc<Http>,
        wake: Waker,
    ) -> Arc<Self> {
        Arc::new(Self {
            files: Arc::downgrade(&files),
            http: Arc::downgrade(&http),
            owner: dialogs.owner(),
            dialogs: Arc::downgrade(&dialogs),
            wake,
            state: Mutex::default(),
        })
    }

    #[cfg(test)]
    pub(crate) fn submit(self: &Arc<Self>, json: &str) -> Result<String, String> {
        self.submit_bytes(json, None)
    }

    pub(crate) fn submit_bytes(
        self: &Arc<Self>,
        json: &str,
        body: Option<Vec<u8>>,
    ) -> Result<String, String> {
        let request: RequestKind =
            serde_json::from_str(json).map_err(|error| format!("invalid I/O request: {error}"))?;
        let service = match request {
            RequestKind::Filesystem { .. } => Service::Filesystem,
            RequestKind::Dialog { .. } => Service::Dialog,
            RequestKind::Http { .. } => Service::Http,
        };
        let stopping = |state: &State| state.stopping && service != Service::Filesystem;
        let mut state = self.state.lock();
        if state.closed
            || stopping(&state)
            || state.requests.len() + state.bodies.len() >= MAX_REQUESTS
        {
            let reason = if state.closed || stopping(&state) {
                "session-closed"
            } else {
                "busy"
            };
            return Ok(serde_json::json!({"result":request.failure(reason)}).to_string());
        }
        state.next = state.next.checked_add(1).expect("I/O IDs exhausted");
        let id = state.next;
        state.requests.insert(
            id,
            Pending {
                phase: Phase::Queued,
                closed: request.failure("session-closed"),
                service,
                abort: None,
                body: None,
                #[cfg(feature = "testing")]
                path: match &request {
                    RequestKind::Filesystem { request } => request.path().unwrap_or("").into(),
                    RequestKind::Dialog { .. } | RequestKind::Http { .. } => String::new(),
                },
            },
        );
        let script = state.script.clone();
        drop(state);
        let ticket = Ticket {
            io: self.clone(),
            id,
        };
        match request {
            RequestKind::Http { request } => {
                let abort = self.http.upgrade().unwrap().submit(request, body, ticket);
                let mut state = self.state.lock();
                if let Some(pending) = state.requests.get_mut(&id)
                    && !matches!(pending.phase, Phase::Completed(_, _))
                {
                    pending.abort = Some(abort);
                } else {
                    abort.abort();
                }
            }
            RequestKind::Filesystem { request } => {
                self.files.upgrade().unwrap().submit(request, ticket)
            }
            RequestKind::Dialog { kind, message } => {
                if let Some(dialogs) = self.dialogs.upgrade() {
                    dialogs.enqueue(self.owner, id, kind, script, message, ticket);
                } else {
                    ticket
                        .complete(serde_json::json!({"ok":false,"error":"Script session closed"}));
                }
            }
        }
        Ok(serde_json::json!({"id":id.to_string()}).to_string())
    }

    pub(crate) fn set_script(&self, script: String) {
        self.state.lock().script = script;
    }

    pub(crate) fn begin_stop(&self) {
        let mut state = self.state.lock();
        state.stopping = true;
        state
            .requests
            .retain(|_, pending| pending.service == Service::Filesystem);
        state.bodies.clear();
        drop(state);
        if let Some(dialogs) = self.dialogs.upgrade() {
            dialogs.cancel(self.owner, None);
        }
    }

    pub(crate) fn cancel(&self, id: u64) {
        let mut state = self.state.lock();
        let pending = state.requests.remove(&id);
        state.bodies.remove(&id);
        drop(state);
        if pending
            .as_ref()
            .is_some_and(|pending| pending.service == Service::Dialog)
            && let Some(dialogs) = self.dialogs.upgrade()
        {
            dialogs.cancel(self.owner, Some(id));
        }
        #[cfg(feature = "testing")]
        if let Some(pending) = pending
            && let Some(files) = self.files.upgrade()
        {
            files.cancelled(&pending.path);
        }
        #[cfg(not(feature = "testing"))]
        drop(pending);
    }

    pub(crate) fn close(&self) {
        let mut state = self.state.lock();
        state.closed = true;
        state.bodies.clear();
        for pending in state.requests.values_mut() {
            if let Some(abort) = pending.abort.take() {
                abort.abort();
            }
            pending.body = None;
            pending.phase =
                Phase::Completed(0, serde_json::value::to_raw_value(&pending.closed).unwrap());
        }
        if !state.notified && !state.requests.is_empty() {
            state.notified = true;
            self.wake.wake_by_ref();
        }
        drop(state);
        if let Some(dialogs) = self.dialogs.upgrade() {
            dialogs.cancel(self.owner, None);
        }
    }

    pub(crate) fn take_body(&self, id: u64) -> Option<Vec<u8>> {
        self.state.lock().bodies.remove(&id)
    }

    pub(crate) fn take(&self) -> Vec<super::host::Item> {
        let mut state = self.state.lock();
        state.notified = false;
        let mut ids: Vec<_> = state
            .requests
            .iter()
            .filter_map(|(&id, pending)| match pending.phase {
                Phase::Completed(order, _) => Some((order, id)),
                _ => None,
            })
            .collect();
        ids.sort_unstable();
        ids.into_iter()
            .map(|(_, id)| {
                let mut pending = state.requests.remove(&id).unwrap();
                if let Some(body) = pending.body.take() {
                    state.bodies.insert(id, body);
                }
                let Phase::Completed(_, result) =
                    std::mem::replace(&mut pending.phase, Phase::Queued)
                else {
                    unreachable!()
                };
                super::host::Item::Io {
                    id: id.to_string(),
                    result,
                }
            })
            .collect()
    }
}

impl Ticket {
    pub(crate) fn issue(&self) -> bool {
        let io = &self.io;
        let mut state = io.state.lock();
        let Some(pending) = state.requests.get_mut(&self.id) else {
            return false;
        };
        if !matches!(pending.phase, Phase::Queued) {
            return false;
        }
        pending.phase = Phase::Issued;
        true
    }

    pub(crate) fn complete_http(self, mut response: Response) {
        let body = std::mem::take(&mut response.body);
        let mut value = serde_json::to_value(response).unwrap();
        value["body"] = self.id.to_string().into();
        self.finish(serde_json::json!({"ok":true,"value":value}), Some(body));
    }

    pub(crate) fn complete(self, result: Value) {
        self.finish(result, None);
    }

    fn finish(self, result: Value, body: Option<Vec<u8>>) {
        let result = serde_json::value::to_raw_value(&result).unwrap();
        let io = &self.io;
        let mut state = io.state.lock();
        state.completed += 1;
        let order = state.completed;
        if let Some(pending) = state.requests.get_mut(&self.id) {
            if matches!(pending.phase, Phase::Completed(_, _)) {
                return;
            }
            pending.phase = Phase::Completed(order, result);
            pending.body = body;
            if !state.notified {
                state.notified = true;
                io.wake.wake_by_ref();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc;
    use std::time::Duration;

    fn http() -> Arc<Http> {
        static HTTP: std::sync::OnceLock<Arc<Http>> = std::sync::OnceLock::new();
        HTTP.get_or_init(|| {
            static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> =
                std::sync::OnceLock::new();
            Http::new(
                RUNTIME
                    .get_or_init(|| tokio::runtime::Runtime::new().unwrap())
                    .handle()
                    .clone(),
            )
            .unwrap()
        })
        .clone()
    }

    #[test]
    fn a_run_at_capacity_leaves_room_for_another_session() {
        let root = std::env::temp_dir().join(format!("lucent-io-fair-{}", std::process::id()));
        let files = Arc::new(Files::start(root.clone()).unwrap());
        let (entered, waiting) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let released = Mutex::new(released);
        files.set_probe(Arc::new(move |stage, path| {
            if stage == crate::data::FileStage::Issued && path == "blocker" {
                entered.send(()).unwrap();
                let _ = released.lock().recv();
            }
        }));
        let blocker = Io::new(
            files.clone(),
            Arc::new(Dialogs::new(Arc::default())),
            http(),
            Waker::noop().clone(),
        );
        blocker
            .submit(r#"{"service":"filesystem","operation":"exists","path":"blocker"}"#)
            .unwrap();
        waiting.recv_timeout(Duration::from_secs(10)).unwrap();
        let first = Io::new(
            files.clone(),
            Arc::new(Dialogs::new(Arc::default())),
            http(),
            Waker::noop().clone(),
        );
        let second = Io::new(
            files.clone(),
            Arc::new(Dialogs::new(Arc::default())),
            http(),
            Waker::noop().clone(),
        );
        let request = r#"{"service":"filesystem","operation":"exists","path":"missing"}"#;
        for _ in 0..16 {
            let reply: Value = serde_json::from_str(&first.submit(request).unwrap()).unwrap();
            assert!(reply["id"].is_string());
        }
        let reply: Value = serde_json::from_str(&first.submit(request).unwrap()).unwrap();
        assert_eq!(reply["result"]["error"]["reason"], "busy");
        let reply: Value = serde_json::from_str(&second.submit(request).unwrap()).unwrap();
        assert!(reply["id"].is_string());
        release.send(()).unwrap();
        drop(files);
        let items = serde_json::to_value(second.take()).unwrap();
        assert_eq!(items[0]["result"], json!({"ok":true,"value":false}));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn closing_a_run_completes_pending_io_and_rejects_new_requests() {
        let root = std::env::temp_dir().join(format!("lucent-io-close-{}", std::process::id()));
        let files = Arc::new(Files::start(root.clone()).unwrap());
        let (entered, waiting) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let released = Mutex::new(released);
        files.set_probe(Arc::new(move |stage, path| {
            if stage == crate::data::FileStage::Issued && path == "issued" {
                entered.send(()).unwrap();
                released
                    .lock()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
            }
        }));
        let io = Io::new(
            files.clone(),
            Arc::new(Dialogs::new(Arc::default())),
            http(),
            Waker::noop().clone(),
        );
        io.submit(r#"{"service":"filesystem","operation":"write-text","path":"issued","contents":"whole"}"#).unwrap();
        waiting.recv_timeout(Duration::from_secs(10)).unwrap();
        io.submit(
            r#"{"service":"filesystem","operation":"write-text","path":"queued","contents":"bad"}"#,
        )
        .unwrap();
        io.close();
        let items = serde_json::to_value(io.take()).unwrap();
        let mut reasons: Vec<_> = items
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["result"]["error"]["reason"].as_str().unwrap())
            .collect();
        reasons.sort();
        assert_eq!(reasons, ["session-closed", "session-closed"]);
        assert_eq!(
            serde_json::from_str::<Value>(
                &io.submit(r#"{"service":"filesystem","operation":"exists","path":"queued"}"#)
                    .unwrap()
            )
            .unwrap(),
            json!({"result":{"ok":false,"error":{"operation":"exists","path":"queued","reason":"session-closed"}}})
        );
        release.send(()).unwrap();
        drop(io);
        drop(files);
        assert_eq!(
            std::fs::read_to_string(root.join("issued")).unwrap(),
            "whole"
        );
        assert!(!root.join("queued").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_io_is_bounded_until_the_run_consumes_completions() {
        let root = std::env::temp_dir().join(format!("lucent-io-cap-{}", std::process::id()));
        let files = Arc::new(Files::start(root.clone()).unwrap());
        let io = Io::new(
            files.clone(),
            Arc::new(Dialogs::new(Arc::default())),
            http(),
            Waker::noop().clone(),
        );
        for _ in 0..MAX_REQUESTS {
            let reply: Value = serde_json::from_str(
                &io.submit(r#"{"service":"filesystem","operation":"exists","path":"missing"}"#)
                    .unwrap(),
            )
            .unwrap();
            assert!(reply["id"].is_string());
        }
        let reply: Value = serde_json::from_str(
            &io.submit(r#"{"service":"filesystem","operation":"exists","path":"missing"}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(reply["result"]["error"]["reason"], "busy");
        drop(files);
        let items = serde_json::to_value(io.take()).unwrap();
        let ids: Vec<_> = items
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap().parse::<u64>().unwrap())
            .collect();
        assert_eq!(ids, (1..=16).collect::<Vec<_>>());
        for item in items.as_array().unwrap() {
            assert_eq!(item["result"], json!({"ok":true,"value":false}));
        }
        io.close();
        drop(io);
        if root.exists() {
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
