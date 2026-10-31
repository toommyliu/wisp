use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::task::Waker;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;

use crate::data::{Files, Request};

const MAX_REQUESTS: usize = 64;

#[derive(Deserialize)]
#[serde(tag = "service", rename_all = "camelCase")]
enum RequestKind {
    Filesystem {
        #[serde(flatten)]
        request: Request,
    },
}

enum Phase {
    Queued,
    Issued,
    Completed(u64, Box<serde_json::value::RawValue>),
}

struct Pending {
    phase: Phase,
    closed: Value,
    #[cfg(feature = "testing")]
    path: String,
}

#[derive(Default)]
struct State {
    closed: bool,
    next: u64,
    completed: u64,
    notified: bool,
    requests: HashMap<u64, Pending>,
}

pub(crate) struct Io {
    files: Weak<Files>,
    wake: Waker,
    state: Mutex<State>,
}

pub(crate) struct Ticket {
    io: Arc<Io>,
    id: u64,
}

impl Io {
    pub(crate) fn new(files: Arc<Files>, wake: Waker) -> Arc<Self> {
        Arc::new(Self {
            files: Arc::downgrade(&files),
            wake,
            state: Mutex::default(),
        })
    }

    pub(crate) fn submit(self: &Arc<Self>, json: &str) -> Result<String, String> {
        let RequestKind::Filesystem { request } =
            serde_json::from_str(json).map_err(|error| format!("invalid I/O request: {error}"))?;
        let mut state = self.state.lock();
        if state.closed || state.requests.len() >= MAX_REQUESTS {
            let reason = if state.closed {
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
                #[cfg(feature = "testing")]
                path: request.path().unwrap_or("").into(),
            },
        );
        drop(state);
        self.files.upgrade().unwrap().submit(
            request,
            Ticket {
                io: self.clone(),
                id,
            },
        );
        Ok(serde_json::json!({"id":id.to_string()}).to_string())
    }

    pub(crate) fn cancel(&self, id: u64) {
        let pending = self.state.lock().requests.remove(&id);
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
        for pending in state.requests.values_mut() {
            pending.phase =
                Phase::Completed(0, serde_json::value::to_raw_value(&pending.closed).unwrap());
        }
        if !state.notified && !state.requests.is_empty() {
            state.notified = true;
            self.wake.wake_by_ref();
        }
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
                let pending = state.requests.remove(&id).unwrap();
                let Phase::Completed(_, result) = pending.phase else {
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

    pub(crate) fn complete(self, result: Value) {
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
        let io = Io::new(files.clone(), Waker::noop().clone());
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
        let io = Io::new(files.clone(), Waker::noop().clone());
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
        assert_eq!(ids, (1..=64).collect::<Vec<_>>());
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
