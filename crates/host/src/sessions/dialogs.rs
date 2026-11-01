use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use lucent_protocol::script::{ScriptDialog, ScriptDialogKind, ScriptDialogResponse};
use parking_lot::Mutex;
use serde_json::{Value, json};

use super::Changes;
use crate::scripting::io::Ticket;

struct Pending {
    owner: u64,
    request: u64,
    dialog: ScriptDialog,
    ticket: Ticket,
}

#[derive(Default)]
struct State {
    next: u64,
    queue: VecDeque<Pending>,
}

pub(crate) struct Dialogs {
    owners: AtomicU64,
    state: Mutex<State>,
    changes: Arc<Changes>,
}

impl Dialogs {
    pub(crate) fn new(changes: Arc<Changes>) -> Self {
        Self {
            owners: AtomicU64::new(1),
            state: Mutex::default(),
            changes,
        }
    }

    pub(crate) fn owner(&self) -> u64 {
        self.owners.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn enqueue(
        &self,
        owner: u64,
        request: u64,
        kind: ScriptDialogKind,
        script: String,
        message: String,
        ticket: Ticket,
    ) {
        let mut state = self.state.lock();
        if !ticket.issue() {
            return;
        }
        state.next = state.next.checked_add(1).expect("dialog IDs exhausted");
        let dialog = ScriptDialog {
            id: state.next,
            kind,
            script,
            message,
        };
        let changed = state.queue.is_empty();
        state.queue.push_back(Pending {
            owner,
            request,
            dialog,
            ticket,
        });
        drop(state);
        if changed {
            self.changes.notify();
        }
    }

    pub(crate) fn current(&self) -> Option<ScriptDialog> {
        self.state
            .lock()
            .queue
            .front()
            .map(|pending| pending.dialog.clone())
    }

    pub(crate) fn respond(&self, id: u64, response: ScriptDialogResponse) {
        let mut state = self.state.lock();
        let Some(current) = state.queue.front() else {
            return;
        };
        if current.dialog.id != id {
            return;
        }
        let value = match (&current.dialog.kind, response) {
            (ScriptDialogKind::Alert, ScriptDialogResponse::Dismissed) => Value::Null,
            (ScriptDialogKind::Confirm, ScriptDialogResponse::Confirmed(value)) => json!(value),
            (ScriptDialogKind::Prompt { .. }, ScriptDialogResponse::Submitted(value)) => {
                json!(value)
            }
            _ => return,
        };
        let pending = state.queue.pop_front().unwrap();
        drop(state);
        self.changes.notify();
        pending.ticket.complete(json!({"ok":true, "value":value}));
    }

    pub(crate) fn cancel(&self, owner: u64, request: Option<u64>) {
        let mut state = self.state.lock();
        let previous = state.queue.front().map(|pending| pending.dialog.id);
        state.queue.retain(|pending| {
            pending.owner != owner || request.is_some_and(|id| id != pending.request)
        });
        let changed = previous != state.queue.front().map(|pending| pending.dialog.id);
        drop(state);
        if changed {
            self.changes.notify();
        }
    }
}
