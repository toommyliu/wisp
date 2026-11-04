use cap_std::fs::Dir;
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

use lucent_protocol::script_options::{ScriptOptions, ScriptOptionsPatch};

use super::documents::accounts::{self, AccountsDocument};
use super::documents::{
    Decoded, Issues, Loaded, Unreadable, account_edits, account_settings, app_settings,
};
use super::fs::{self, Failure, WriteFailure};
use super::saved_objects::{self, Disk, Load};
use crate::accounts::{AccountEdit, AccountsError, AccountsSnapshot, FileError};
use crate::app_settings::{AppSettings, SettingsError, Update};
use crate::reply::Answer;
use crate::sessions::Changes;
use crate::updates::{ReleaseCache, UpdatesError};

#[cfg(feature = "testing")]
type Probe = Arc<dyn Fn() + Send + Sync>;

pub(crate) struct AppData {
    sender: Option<mpsc::Sender<Request>>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
    pub(crate) account_changes: Arc<Changes>,
    pub(crate) settings_changes: Arc<Changes>,
    #[cfg(feature = "testing")]
    probe: Arc<Mutex<Option<Probe>>>,
}

pub(crate) enum Request {
    Snapshot(Answer<AccountsSnapshot>),
    Edit(AccountEdit, Answer<AccountsSnapshot>),
    Settings(String, Answer<ScriptOptions>),
    UpdateSettings(String, ScriptOptionsPatch, Answer<ScriptOptions>),
    AppSettings(Answer<AppSettings, SettingsError>),
    UpdateAppSettings(Update, Answer<AppSettings, SettingsError>),
    ReleaseCache(Answer<Option<ReleaseCache>, UpdatesError>),
    SaveReleaseCache(ReleaseCache, Answer<(), UpdatesError>),
    LoadSavedObjects {
        folder: Vec<String>,
        partition: String,
        load: Arc<Load>,
    },
    SaveSavedObject {
        folder: Vec<String>,
        key: String,
        bytes: Option<Vec<u8>>,
    },
}

impl AppData {
    pub(crate) fn start(
        root: PathBuf,
        real: PathBuf,
        release: Option<PathBuf>,
    ) -> io::Result<Self> {
        let (sender, requests) = mpsc::channel::<Request>();
        let account_changes = Arc::new(Changes::default());
        let settings_changes = Arc::new(Changes::default());
        #[cfg(feature = "testing")]
        let probe: Arc<Mutex<Option<Probe>>> = Arc::default();
        let mut worker = Worker {
            store: Store {
                root,
                real,
                release,
                #[cfg(feature = "testing")]
                probe: probe.clone(),
            },
            accounts: None,
            app_settings: None,
            account_changes: account_changes.clone(),
            settings_changes: settings_changes.clone(),
        };
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let thread = thread::Builder::new()
            .name("lucent-app-data".into())
            .spawn(move || {
                let _dispatch = tracing::dispatcher::set_default(&dispatch);
                worker.store.import_release();
                for request in requests {
                    worker.handle(request);
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            thread: Mutex::new(Some(thread)),
            account_changes,
            settings_changes,
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
    app_settings: Option<AppSettings>,
    account_changes: Arc<Changes>,
    settings_changes: Arc<Changes>,
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
            Request::AppSettings(answer) => {
                answer.send(self.app_settings(true).cloned().map_err(Into::into))
            }
            Request::UpdateAppSettings(update, answer) => {
                answer.send(self.update_app_settings(update))
            }
            Request::ReleaseCache(answer) => answer.send(Ok(self.release_cache())),
            Request::SaveReleaseCache(cache, answer) => {
                let mut contents =
                    serde_json::to_vec_pretty(&cache).expect("a release cache serializes");
                contents.push(b'\n');
                let saved = self.store.write(Document::ReleaseCache, &contents);
                if let Err(error) = saved {
                    tracing::warn!(component = "updates", %error, "Failed to save release cache");
                }
                answer.send(Ok(()))
            }
            Request::LoadSavedObjects {
                folder,
                partition,
                load,
            } => load.fill(self.store.saved_objects(&folder, &partition)),
            Request::SaveSavedObject { folder, key, bytes } => {
                self.store.save_object(&folder, &key, bytes.as_deref());
            }
        }
    }

    fn release_cache(&self) -> Option<ReleaseCache> {
        let read = self.store.read(Document::ReleaseCache);
        let decoded = read.map(|bytes| bytes.map(|bytes| serde_json::from_slice(&bytes)));
        let error = match decoded {
            Ok(None) => return None,
            Ok(Some(Ok(cache))) => return Some(cache),
            Ok(Some(Err(error))) => error.to_string(),
            Err(error) => error.to_string(),
        };
        tracing::warn!(component = "updates", %error, "Failed to load release cache");
        None
    }

    fn app_settings(&mut self, diagnostics: bool) -> Result<&AppSettings, FileError> {
        Ok(match &mut self.app_settings {
            Some(settings) => settings,
            cold @ None => cold.insert(self.store.load(
                Document::AppSettings,
                app_settings::decode,
                diagnostics,
            )?),
        })
    }

    fn update_app_settings(&mut self, update: Update) -> Result<AppSettings, SettingsError> {
        let cold = self.app_settings.is_none();
        let committed = self.commit_app_settings(update);
        if cold && committed.is_err() {
            self.app_settings = None;
        }
        committed
    }

    fn commit_app_settings(&mut self, update: Update) -> Result<AppSettings, SettingsError> {
        let current = self.app_settings(false)?;
        let next = current.apply(update)?;
        if *current != next {
            self.store
                .write(Document::AppSettings, &app_settings::encode(&next))?;
            self.app_settings = Some(next.clone());
            self.settings_changes.notify();
        }
        Ok(next)
    }

    fn settings(&self, username: &str) -> Result<ScriptOptions, AccountsError> {
        let file = account_settings::file_name(username)?;
        let document = Document::Settings(&file);
        Ok(self.store.load(document, account_settings::decode, true)?)
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
            self.account_changes.notify();
        }
        Ok(snapshot)
    }
}

#[derive(Clone, Copy)]
enum Document<'a> {
    Accounts,
    Settings(&'a str),
    AppSettings,
    ReleaseCache,
}

impl Document<'_> {
    fn directory(self) -> Option<&'static str> {
        match self {
            Document::Accounts | Document::AppSettings | Document::ReleaseCache => None,
            Document::Settings(_) => Some("account-settings"),
        }
    }

    fn name(&self) -> &str {
        match self {
            Document::Accounts => "accounts.json",
            Document::Settings(file) => file,
            Document::AppSettings => "settings.json",
            Document::ReleaseCache => "update-cache.json",
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Document::Accounts => "accounts",
            Document::Settings(_) => "account settings",
            Document::AppSettings => "settings",
            Document::ReleaseCache => "release cache",
        }
    }

    fn component(self) -> &'static str {
        match self {
            Document::Accounts => "accounts",
            Document::Settings(_) => "account-settings",
            Document::AppSettings => "settings",
            Document::ReleaseCache => "updates",
        }
    }

    fn log_path(self) -> &'static str {
        match self {
            Document::Accounts => "accounts.json",
            Document::Settings(_) => "account-settings/[account].json",
            Document::AppSettings => "settings.json",
            Document::ReleaseCache => "update-cache.json",
        }
    }
}

struct Store {
    root: PathBuf,
    real: PathBuf,
    release: Option<PathBuf>,
    #[cfg(feature = "testing")]
    probe: Arc<Mutex<Option<Probe>>>,
}

impl Store {
    fn load<T: Default>(
        &self,
        document: Document,
        decode: fn(Option<&[u8]>) -> Loaded<T>,
        diagnostics: bool,
    ) -> Result<T, FileError> {
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

    fn import_release(&self) {
        if self.release.is_none() {
            return;
        }
        self.import(Document::Accounts, accounts::decode, accounts::encode);
        self.import(
            Document::AppSettings,
            app_settings::decode,
            app_settings::encode,
        );
        for file in self.release_files(Document::Settings("").directory()) {
            self.import(
                Document::Settings(&file),
                account_settings::decode,
                account_settings::encode,
            );
        }
    }

    fn import<T>(
        &self,
        document: Document,
        decode: fn(Option<&[u8]>) -> Loaded<T>,
        encode: fn(&T) -> Vec<u8>,
    ) {
        if !matches!(self.read(document), Ok(None)) {
            return;
        }
        let Some(bytes) = self.read_release(document) else {
            return;
        };
        let Loaded::Decoded(Decoded { value, .. }) = decode(Some(&bytes)) else {
            return;
        };
        match self.write(document, &encode(&value)) {
            Ok(()) => tracing::info!(
                component = document.component(),
                path = document.log_path(),
                from = self.release_name(),
                "Imported {} from the old data folder",
                document.kind(),
            ),
            Err(error) => tracing::warn!(
                component = document.component(),
                path = document.log_path(),
                from = self.release_name(),
                %error,
                "Couldn't import {} from the old data folder",
                document.kind(),
            ),
        }
    }

    fn release_dir(&self, name: Option<&str>) -> Result<Option<Dir>, Failure> {
        let Some(release) = self.release.as_deref() else {
            return Ok(None);
        };
        let Some(root) = fs::open_root(release, false)? else {
            return Ok(None);
        };
        match name {
            Some(name) => fs::directory(&root, name, false),
            None => Ok(Some(root)),
        }
    }

    fn release_files(&self, name: Option<&str>) -> Vec<String> {
        let Ok(Some(dir)) = self.release_dir(name) else {
            return Vec::new();
        };
        let Ok(entries) = dir.entries() else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|file| file.ends_with(".json") && !file.starts_with(super::path::TEMP_PREFIX))
            .collect()
    }

    fn read_release(&self, document: Document) -> Option<Vec<u8>> {
        let read = self
            .release_dir(document.directory())
            .and_then(|dir| dir.map_or(Ok(None), |dir| fs::read(&dir, document.name())));
        read.unwrap_or_else(|_| {
            tracing::warn!(
                component = document.component(),
                path = document.log_path(),
                from = self.release_name(),
                "Couldn't read {} in the old data folder",
                document.kind(),
            );
            None
        })
    }

    fn saved_objects(&self, folder: &[String], partition: &str) -> Disk {
        if matches!(self.saved_object_dir(folder, &[], false), Ok(None)) {
            self.import_saved_objects(folder, partition);
        }
        let mut disk = Disk::default();
        match self.saved_object_dir(folder, &[], false) {
            Ok(Some(dir)) => read_saved_objects(&dir, "", 0, &mut disk),
            Ok(None) => {}
            Err(_) => disk.failed = true,
        }
        if disk.failed || !disk.unreadable.is_empty() {
            tracing::warn!(
                component = "saved-objects",
                unreadable = disk.unreadable.len(),
                "Some game saves couldn't be read; they won't be overwritten until the game restarts"
            );
        }
        disk
    }

    fn saved_object_dir(
        &self,
        folder: &[String],
        dirs: &[&str],
        create: bool,
    ) -> Result<Option<Dir>, Failure> {
        let Some(mut dir) = fs::open_root(&self.real, create)? else {
            return Ok(None);
        };
        let names = std::iter::once(SAVED_OBJECTS)
            .chain(folder.iter().map(String::as_str))
            .chain(dirs.iter().copied());
        for name in names {
            match fs::directory(&dir, name, create)? {
                Some(next) => dir = next,
                None => return Ok(None),
            }
        }
        Ok(Some(dir))
    }

    fn save_object(&self, folder: &[String], key: &str, bytes: Option<&[u8]>) -> bool {
        let segments: Vec<&str> = key.split('/').collect();
        let (name, dirs) = segments.split_last().expect("a key has a name");
        let file = format!("{name}.sol");
        let saved = match bytes {
            Some(bytes) => self
                .saved_object_dir(folder, dirs, true)
                .and_then(|dir| dir.ok_or(Failure::NotDirectory))
                .and_then(|dir| {
                    fs::write_atomic(&dir, &file, bytes, true, || {}).map_err(|failure| {
                        match failure {
                            WriteFailure::Write(failure) | WriteFailure::Rename(failure) => failure,
                        }
                    })
                }),
            None => self.saved_object_dir(folder, dirs, false).and_then(|dir| {
                match dir.map(|dir| dir.remove_file(&file)) {
                    Some(Err(error)) if error.kind() != io::ErrorKind::NotFound => {
                        Err(error.into())
                    }
                    _ => Ok(()),
                }
            }),
        };
        if saved.is_err() {
            tracing::warn!(
                component = "saved-objects",
                key,
                "Couldn't save a game save"
            );
        }
        saved.is_ok()
    }

    fn import_saved_objects(&self, folder: &[String], partition: &str) {
        let files = self.release_saved_objects(partition);
        if files.is_empty() {
            return;
        }
        let imported = files
            .iter()
            .filter(|(key, bytes)| self.save_object(folder, key, Some(bytes)))
            .count();
        tracing::info!(
            component = "saved-objects",
            from = self.release_name(),
            imported,
            "Imported game saves from the old data folder"
        );
    }

    fn release_saved_objects(&self, partition: &str) -> Vec<(String, Vec<u8>)> {
        let Some(release) = &self.release else {
            return Vec::new();
        };
        let database = release
            .join("Partitions")
            .join(partition)
            .join("Local Storage")
            .join("leveldb");
        if !database.is_dir() {
            return Vec::new();
        }
        let copy = self.real.join(format!(
            "{}local-storage-{}",
            super::path::TEMP_PREFIX,
            std::process::id()
        ));
        let saves = local_storage_saves(&database, &copy);
        let _ = std::fs::remove_dir_all(&copy);
        saves.unwrap_or_else(|_| {
            tracing::warn!(
                component = "saved-objects",
                from = self.release_name(),
                "Couldn't read game saves in the old data folder"
            );
            Vec::new()
        })
    }

    fn release_name(&self) -> &str {
        let name = self.release.as_deref().and_then(|path| path.file_name());
        name.and_then(|name| name.to_str()).unwrap_or_default()
    }

    fn folder(&self, document: Document, create: bool) -> Result<Option<Dir>, Failure> {
        let Some(root) = fs::open_root(&self.real, create)? else {
            return Ok(None);
        };
        if create {
            owner_only(document, &root);
        }
        let Some(name) = document.directory() else {
            return Ok(Some(root));
        };
        if create {
            fs::create_directory(&root, name)?;
        }
        match Dir::open_ambient_dir(self.real.join(name), cap_std::ambient_authority()) {
            Ok(dir) => {
                if create {
                    owner_only(document, &dir);
                }
                Ok(Some(dir))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !create => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn read(&self, document: Document) -> Result<Option<Vec<u8>>, FileError> {
        let failed = |failure| self.failure("read", document, failure);
        match self.folder(document, false).map_err(failed)? {
            Some(dir) => fs::read(&dir, document.name()).map_err(failed),
            None => Ok(None),
        }
    }

    fn write(&self, document: Document, contents: &[u8]) -> Result<(), FileError> {
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

    fn failure(&self, operation: &'static str, document: Document, failure: Failure) -> FileError {
        let mut path = self.root.clone();
        path.extend(document.directory());
        path.push(document.name());
        FileError {
            operation,
            path,
            too_large: matches!(failure, Failure::TooLarge),
        }
    }
}

fn owner_only(document: Document, dir: &Dir) {
    if let Err(error) = fs::owner_only(dir) {
        tracing::warn!(
            path = document.log_path(),
            %error,
            "couldn't make the app data folder private before a save"
        );
    }
}

fn warn_unreadable(document: Document, unreadable: &Unreadable) {
    let at = match *unreadable {
        Unreadable::Malformed { line, column } => Some((line, column)),
        Unreadable::NotAnObject => None,
    };
    tracing::warn!(
        component = document.component(),
        path = document.log_path(),
        ignored = %json!({"file": 1}),
        line = at.map(|(line, _)| line),
        column = at.map(|(_, column)| column),
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

const SAVED_OBJECTS: &str = "saved-objects";

fn read_saved_objects(dir: &Dir, prefix: &str, depth: usize, disk: &mut Disk) {
    let Ok(listing) = dir.entries() else {
        disk.failed = true;
        return;
    };
    for entry in listing {
        let Ok(entry) = entry else {
            disk.failed = true;
            continue;
        };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            disk.failed = true;
            continue;
        };
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if kind.is_dir() && depth < 31 {
            match fs::directory(dir, &name, false) {
                Ok(Some(child)) => read_saved_objects(&child, &path, depth + 1, disk),
                Ok(None) => {}
                Err(_) => disk.failed = true,
            }
        } else if kind.is_file()
            && let Some(key) = path.strip_suffix(".sol")
            && saved_objects::key(key).as_deref() == Some(key)
        {
            match fs::read(dir, &name) {
                Ok(Some(bytes)) => {
                    disk.entries.insert(key.to_owned(), bytes);
                }
                Ok(None) => {}
                Err(_) => {
                    disk.unreadable.insert(key.to_owned());
                }
            }
        }
    }
}

fn local_storage_saves(
    database: &std::path::Path,
    copy: &std::path::Path,
) -> io::Result<Vec<(String, Vec<u8>)>> {
    let _ = std::fs::remove_dir_all(copy);
    fs::create_root(copy)?;
    for entry in std::fs::read_dir(database)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.file_name() != "LOCK" {
            std::fs::copy(entry.path(), copy.join(entry.file_name()))?;
        }
    }
    let options = rusty_leveldb::Options {
        create_if_missing: false,
        ..rusty_leveldb::Options::default()
    };
    let mut db = rusty_leveldb::DB::open(copy, options).map_err(io::Error::other)?;
    let mut entries = db.new_iter().map_err(io::Error::other)?;
    let mut saves = Vec::new();
    while let Some((key, value)) = rusty_leveldb::LdbIterator::next(&mut entries) {
        saves.extend(ruffle_save(&key, &value));
    }
    Ok(saves)
}

fn ruffle_save(key: &[u8], value: &[u8]) -> Option<(String, Vec<u8>)> {
    use base64::Engine;
    let name = chromium_text(key.strip_prefix(b"_file://\0")?)?;
    let (host, name) = name.split_once("//")?;
    if host.is_empty() || host.contains('/') || name.contains('/') {
        return None;
    }
    let key = saved_objects::key(&format!("localhost/{name}"))?;
    let text = chromium_text(value)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .ok()?;
    Some((key, bytes))
}

fn chromium_text(bytes: &[u8]) -> Option<String> {
    match bytes.split_first()? {
        (1, latin1) => Some(latin1.iter().map(|&byte| char::from(byte)).collect()),
        (0, utf16) => {
            let (units, []) = utf16.as_chunks::<2>() else {
                return None;
            };
            String::from_utf16(
                &units
                    .iter()
                    .map(|unit| u16::from_le_bytes(*unit))
                    .collect::<Vec<_>>(),
            )
            .ok()
        }
        _ => None,
    }
}
