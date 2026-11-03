use parking_lot::Mutex;
use serde_json::{Value, json};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

use super::documents::accounts::{self, AccountsDocument};
use super::documents::{Decoded, Issues, Loaded, Unreadable};
use super::fs::{self, Failure};
use crate::accounts::{AccountsError, AccountsSnapshot, Answer};
use crate::sessions::Changes;

#[cfg(feature = "testing")]
type Probe = Arc<dyn Fn() + Send + Sync>;

pub(crate) struct AppData {
    sender: Option<mpsc::SyncSender<Request>>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
    pub(crate) changes: Arc<Changes>,
    #[cfg(feature = "testing")]
    probe: Arc<Mutex<Option<Probe>>>,
}

pub(crate) enum Request {
    Snapshot(Answer<AccountsSnapshot>),
}

impl Request {
    fn reject(self, error: AccountsError) {
        match self {
            Request::Snapshot(answer) => answer.send(Err(error)),
        }
    }
}

impl AppData {
    pub(crate) fn start(root: PathBuf) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<Request>(64);
        let changes = Arc::new(Changes::default());
        #[cfg(feature = "testing")]
        let probe: Arc<Mutex<Option<Probe>>> = Arc::default();
        let mut worker = Worker {
            store: Store {
                root,
                #[cfg(feature = "testing")]
                probe: probe.clone(),
            },
            accounts: None,
            changes: changes.clone(),
        };
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let thread = thread::Builder::new()
            .name("lucent-app-data".into())
            .spawn(move || {
                let _dispatch = tracing::dispatcher::set_default(&dispatch);
                for request in requests {
                    worker.handle(request);
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            thread: Mutex::new(Some(thread)),
            changes,
            #[cfg(feature = "testing")]
            probe,
        })
    }

    pub(crate) fn submit(&self, request: Request) {
        if let Err(error) = self.sender.as_ref().unwrap().try_send(request) {
            match error {
                mpsc::TrySendError::Full(request) => request.reject(AccountsError::Busy),
                mpsc::TrySendError::Disconnected(request) => request.reject(AccountsError::Stopped),
            }
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn set_probe(&self, probe: Probe) {
        *self.probe.lock() = Some(probe);
    }
}

impl Drop for AppData {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.get_mut().take() {
            let _ = thread.join();
        }
    }
}

struct Worker {
    store: Store,
    accounts: Option<AccountsDocument>,
    changes: Arc<Changes>,
}

impl Worker {
    fn handle(&mut self, request: Request) {
        match request {
            Request::Snapshot(answer) => answer.send(self.snapshot()),
        }
    }

    fn snapshot(&mut self) -> Result<AccountsSnapshot, AccountsError> {
        let accounts = match &mut self.accounts {
            Some(accounts) => accounts,
            cold @ None => cold.insert(self.store.load(
                Document::Accounts,
                accounts::decode,
                true,
            )?),
        };
        Ok(accounts.snapshot())
    }
}

#[derive(Clone, Copy)]
enum Document {
    Accounts,
}

impl Document {
    fn directory(self) -> Option<&'static str> {
        match self {
            Document::Accounts => None,
        }
    }

    fn name(&self) -> &str {
        match self {
            Document::Accounts => "accounts.json",
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Document::Accounts => "accounts",
        }
    }

    fn component(self) -> &'static str {
        match self {
            Document::Accounts => "accounts",
        }
    }

    fn log_path(self) -> &'static str {
        match self {
            Document::Accounts => "accounts.json",
        }
    }
}

struct Store {
    root: PathBuf,
    #[cfg(feature = "testing")]
    probe: Arc<Mutex<Option<Probe>>>,
}

impl Store {
    fn load<T: Default>(
        &self,
        document: Document,
        decode: fn(Option<&[u8]>) -> Loaded<T>,
        diagnostics: bool,
    ) -> Result<T, AccountsError> {
        let bytes = self.read(document)?;
        Ok(match decode(bytes.as_deref()) {
            Loaded::Missing => T::default(),
            Loaded::Unreadable(unreadable) => {
                if diagnostics {
                    warn_unreadable(document, &unreadable);
                }
                T::default()
            }
            Loaded::Decoded(Decoded { value, issues }) => {
                if diagnostics && !(issues.reset.is_empty() && issues.ignored.is_empty()) {
                    warn_invalid(document, &issues);
                }
                value
            }
        })
    }

    fn read(&self, document: Document) -> Result<Option<Vec<u8>>, AccountsError> {
        let failed = |failure| self.failure("read", document, failure);
        let Some(mut dir) = fs::open_root(&self.root, false).map_err(failed)? else {
            return Ok(None);
        };
        if let Some(name) = document.directory() {
            match fs::directory(&dir, name, false).map_err(failed)? {
                Some(child) => dir = child,
                None => return Ok(None),
            }
        }
        fs::read(&dir, document.name()).map_err(failed)
    }

    fn failure(
        &self,
        operation: &'static str,
        document: Document,
        failure: Failure,
    ) -> AccountsError {
        let mut path = self.root.clone();
        path.extend(document.directory());
        path.push(document.name());
        AccountsError::File {
            operation,
            path,
            too_large: matches!(failure, Failure::TooLarge),
        }
    }
}

fn warn_unreadable(document: Document, unreadable: &Unreadable) {
    let at = match unreadable {
        Unreadable::Malformed(error) => error.at,
        Unreadable::NotAnObject => None,
    };
    tracing::warn!(
        component = document.component(),
        path = document.log_path(),
        ignored = %json!({"file": 1}),
        line = at.map(|at| at.line),
        column = at.map(|at| at.column),
        "Saved {} could not be read; using defaults until the next save",
        document.kind(),
    );
}

fn warn_invalid(document: Document, issues: &Issues) {
    let ignored = issues.ignored.iter();
    let ignored: serde_json::Map<String, Value> = ignored
        .map(|&(name, count)| (name.to_owned(), count.into()))
        .collect();
    let ignored = (!ignored.is_empty()).then(|| Value::from(ignored));
    tracing::warn!(
        component = document.component(),
        path = document.log_path(),
        reset = (!issues.reset.is_empty()).then(|| tracing::field::debug(&issues.reset)),
        ignored = ignored.as_ref().map(tracing::field::display),
        "Saved {} had invalid entries; ignoring them until the next save",
        document.kind(),
    );
}
