use cap_std::fs::Dir;
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

use lucent_protocol::script_options::{ScriptOptions, ScriptOptionsPatch};

use super::documents::accounts::{self, AccountsDocument};
use super::documents::{Decoded, Issues, Loaded, Unreadable, account_edits, account_settings};
use super::fs::{self, Failure, WriteFailure};
use crate::accounts::{AccountEdit, AccountsError, AccountsSnapshot, Answer, FileError};
use crate::sessions::Changes;

#[cfg(feature = "testing")]
type Probe = Arc<dyn Fn() + Send + Sync>;

pub(crate) struct AppData {
    sender: Option<mpsc::Sender<Request>>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
    pub(crate) changes: Arc<Changes>,
    #[cfg(feature = "testing")]
    probe: Arc<Mutex<Option<Probe>>>,
}

pub(crate) enum Request {
    Snapshot(Answer<AccountsSnapshot>),
    Edit(AccountEdit, Answer<AccountsSnapshot>),
    Settings(String, Answer<ScriptOptions>),
    UpdateSettings(String, ScriptOptionsPatch, Answer<ScriptOptions>),
}

impl AppData {
    pub(crate) fn start(root: PathBuf, real: PathBuf) -> io::Result<Self> {
        let (sender, requests) = mpsc::channel::<Request>();
        let changes = Arc::new(Changes::default());
        #[cfg(feature = "testing")]
        let probe: Arc<Mutex<Option<Probe>>> = Arc::default();
        let mut worker = Worker {
            store: Store {
                root,
                real,
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
        let _ = self.sender.as_ref().unwrap().send(request);
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
            Request::Snapshot(answer) => {
                answer.send(self.accounts(true).map(AccountsDocument::snapshot))
            }
            Request::Edit(edit, answer) => answer.send(self.edit(edit)),
            Request::Settings(username, answer) => answer.send(self.settings(&username)),
            Request::UpdateSettings(username, patch, answer) => {
                answer.send(self.update_settings(&username, patch))
            }
        }
    }

    fn settings(&self, username: &str) -> Result<ScriptOptions, AccountsError> {
        let file = account_settings::file_name(username)?;
        let document = Document::Settings(&file);
        self.store.load(document, account_settings::decode, true)
    }

    fn update_settings(
        &self,
        username: &str,
        patch: ScriptOptionsPatch,
    ) -> Result<ScriptOptions, AccountsError> {
        let file = account_settings::file_name(username)?;
        let document = Document::Settings(&file);
        let current = self.store.load(document, account_settings::decode, false)?;
        let next = account_settings::apply(current, patch);
        self.store
            .write(document, &account_settings::encode(&next))?;
        Ok(next)
    }

    fn accounts(&mut self, diagnostics: bool) -> Result<&AccountsDocument, AccountsError> {
        Ok(match &mut self.accounts {
            Some(accounts) => accounts,
            cold @ None => cold.insert(self.store.load(
                Document::Accounts,
                accounts::decode,
                diagnostics,
            )?),
        })
    }

    fn edit(&mut self, edit: AccountEdit) -> Result<AccountsSnapshot, AccountsError> {
        let cold = self.accounts.is_none();
        let committed = self.commit(edit);
        if cold && committed.is_err() {
            self.accounts = None;
        }
        committed
    }

    fn commit(&mut self, edit: AccountEdit) -> Result<AccountsSnapshot, AccountsError> {
        let next = account_edits::apply(edit, || self.accounts(false))?;
        let snapshot = next.snapshot();
        if self.accounts.as_ref() != Some(&next) {
            self.store
                .write(Document::Accounts, &accounts::encode(&next))?;
            self.accounts = Some(next);
            self.changes.notify();
        }
        Ok(snapshot)
    }
}

#[derive(Clone, Copy)]
enum Document<'a> {
    Accounts,
    Settings(&'a str),
}

impl Document<'_> {
    fn directory(self) -> Option<&'static str> {
        match self {
            Document::Accounts => None,
            Document::Settings(_) => Some("account-settings"),
        }
    }

    fn name(&self) -> &str {
        match self {
            Document::Accounts => "accounts.json",
            Document::Settings(file) => file,
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Document::Accounts => "accounts",
            Document::Settings(_) => "account settings",
        }
    }

    fn component(self) -> &'static str {
        match self {
            Document::Accounts => "accounts",
            Document::Settings(_) => "account-settings",
        }
    }

    fn log_path(self) -> &'static str {
        match self {
            Document::Accounts => "accounts.json",
            Document::Settings(_) => "account-settings/[account].json",
        }
    }
}

struct Store {
    root: PathBuf,
    real: PathBuf,
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

    fn folder(&self, document: Document, create: bool) -> Result<Option<Dir>, Failure> {
        let Some(root) = fs::open_root(&self.real, create)? else {
            return Ok(None);
        };
        let Some(name) = document.directory() else {
            return Ok(Some(root));
        };
        if create {
            fs::create_directory(&root, name)?;
        }
        match Dir::open_ambient_dir(self.real.join(name), cap_std::ambient_authority()) {
            Ok(dir) => Ok(Some(dir)),
            Err(error) if error.kind() == io::ErrorKind::NotFound && !create => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn read(&self, document: Document) -> Result<Option<Vec<u8>>, AccountsError> {
        let failed = |failure| self.failure("read", document, failure);
        match self.folder(document, false).map_err(failed)? {
            Some(dir) => fs::read(&dir, document.name()).map_err(failed),
            None => Ok(None),
        }
    }

    fn write(&self, document: Document, contents: &[u8]) -> Result<(), AccountsError> {
        let failed = |operation| move |failure| self.failure(operation, document, failure);
        if contents.len() as u64 > fs::MAX_BYTES {
            return Err(failed("write")(Failure::TooLarge));
        }
        let missing = || failed("mkdir")(Failure::Io(io::ErrorKind::NotFound.into()));
        let dir = self
            .folder(document, true)
            .map_err(failed("mkdir"))?
            .ok_or_else(missing)?;
        let renamed = fs::write_atomic(&dir, document.name(), contents, true, || {
            #[cfg(feature = "testing")]
            {
                let probe = self.probe.lock().clone();
                if let Some(probe) = probe {
                    probe();
                }
            }
        });
        renamed.map_err(|failure| match failure {
            WriteFailure::Write(failure) => failed("write")(failure),
            WriteFailure::Rename(failure) => failed("rename")(failure),
        })?;
        if let Err(error) = fs::sync_directory(&dir) {
            tracing::warn!(
                path = document.log_path(),
                %error,
                "couldn't flush the app data folder after a save"
            );
        }
        Ok(())
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
        AccountsError::File(FileError {
            operation,
            path,
            too_large: matches!(failure, Failure::TooLarge),
        })
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
