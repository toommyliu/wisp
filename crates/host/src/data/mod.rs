mod path;

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, DirBuilder, Metadata, OpenOptions};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
#[cfg(feature = "testing")]
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use crate::scripting::io::Ticket;
use path::{TEMP_PREFIX, segments};

const MAX_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    Exists { path: String },
    List { path: Option<String> },
    ReadText { path: String },
    ReadJson { path: String },
    WriteText { path: String, contents: String },
    WriteJson { path: String, contents: String },
    Remove { path: String },
}

impl Request {
    pub(crate) fn operation(&self) -> &'static str {
        match self {
            Self::Exists { .. } => "exists",
            Self::List { .. } => "list",
            Self::ReadText { .. } => "read-text",
            Self::ReadJson { .. } => "read-json",
            Self::WriteText { .. } => "write-text",
            Self::WriteJson { .. } => "write-json",
            Self::Remove { .. } => "remove",
        }
    }
    pub(crate) fn path(&self) -> Option<&str> {
        match self {
            Self::List { path } => path.as_deref(),
            Self::Exists { path }
            | Self::ReadText { path }
            | Self::ReadJson { path }
            | Self::WriteText { path, .. }
            | Self::WriteJson { path, .. }
            | Self::Remove { path } => Some(path),
        }
    }
    pub(crate) fn failure(&self, reason: &str) -> Value {
        let mut error = json!({"operation":self.operation(),"reason":reason});
        if let Some(path) = self.path() {
            error["path"] = path.into();
        }
        json!({"ok":false,"error":error})
    }
}

#[cfg(feature = "testing")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStage {
    Queued,
    Issued,
    BeforeRename,
    Finished,
    Cancelled,
}

#[cfg(feature = "testing")]
type Probe = Arc<dyn Fn(FileStage, &str) + Send + Sync>;

pub(crate) struct Files {
    root: PathBuf,
    sender: Option<mpsc::SyncSender<(Request, Ticket)>>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
    #[cfg(feature = "testing")]
    probe: Arc<Mutex<Option<Probe>>>,
}

impl Files {
    pub(crate) fn start(root: PathBuf) -> io::Result<Self> {
        let (sender, incoming) = mpsc::sync_channel::<(Request, Ticket)>(64);
        #[cfg(feature = "testing")]
        let probe: Arc<Mutex<Option<Probe>>> = Arc::default();
        #[cfg(feature = "testing")]
        let worker_probe = probe.clone();
        let worker_root = root.clone();
        let thread = thread::Builder::new()
            .name("lucent-files".into())
            .spawn(move || {
                let mut serial = 0_u64;
                for (request, ticket) in incoming {
                    if !ticket.issue() {
                        continue;
                    }
                    #[cfg(feature = "testing")]
                    notify(&worker_probe, FileStage::Issued, &request);
                    serial += 1;
                    let result = execute(
                        &worker_root,
                        &request,
                        serial,
                        #[cfg(feature = "testing")]
                        &worker_probe,
                    );
                    let result = match result {
                        Ok(Some(value)) => json!({"ok":true,"value":value}),
                        Ok(None) => json!({"ok":true}),
                        Err(reason) => request.failure(reason),
                    };
                    ticket.complete(result);
                    #[cfg(feature = "testing")]
                    notify(&worker_probe, FileStage::Finished, &request);
                }
            })?;
        Ok(Self {
            root,
            sender: Some(sender),
            thread: Mutex::new(Some(thread)),
            #[cfg(feature = "testing")]
            probe,
        })
    }

    pub(crate) fn submit(&self, request: Request, ticket: Ticket) {
        #[cfg(feature = "testing")]
        let path = request.path().unwrap_or("").to_string();
        if let Err(error) = self.sender.as_ref().unwrap().try_send((request, ticket)) {
            let ((request, ticket), reason) = match error {
                mpsc::TrySendError::Full(task) => (task, "busy"),
                mpsc::TrySendError::Disconnected(task) => (task, "unavailable"),
            };
            ticket.complete(request.failure(reason));
        } else {
            #[cfg(feature = "testing")]
            {
                let probe = self.probe.lock().clone();
                if let Some(probe) = probe {
                    probe(FileStage::Queued, &path);
                }
            }
        }
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    #[cfg(feature = "testing")]
    pub(crate) fn set_probe(&self, probe: Probe) {
        *self.probe.lock() = Some(probe);
    }
    #[cfg(feature = "testing")]
    pub(crate) fn cancelled(&self, path: &str) {
        let probe = self.probe.lock().clone();
        if let Some(probe) = probe {
            probe(FileStage::Cancelled, path);
        }
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.get_mut().take() {
            let _ = thread.join();
        }
    }
}

#[cfg(feature = "testing")]
fn notify(probe: &Mutex<Option<Probe>>, stage: FileStage, request: &Request) {
    let probe = probe.lock().clone();
    if let Some(probe) = probe {
        probe(stage, request.path().unwrap_or(""));
    }
}

type Result<T> = std::result::Result<T, &'static str>;

fn reason(error: io::Error) -> &'static str {
    #[cfg(unix)]
    if matches!(
        error.raw_os_error(),
        Some(libc::ELOOP | libc::ENAMETOOLONG | libc::EINVAL)
    ) {
        return "invalid-path";
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            ERROR_BUFFER_OVERFLOW, ERROR_CANT_RESOLVE_FILENAME, ERROR_FILENAME_EXCED_RANGE,
            ERROR_LOCK_VIOLATION, ERROR_PIPE_BUSY, ERROR_SHARING_VIOLATION,
            ERROR_TOO_MANY_OPEN_FILES,
        };
        match error.raw_os_error().map(|code| code as u32) {
            Some(
                ERROR_TOO_MANY_OPEN_FILES
                | ERROR_SHARING_VIOLATION
                | ERROR_LOCK_VIOLATION
                | ERROR_PIPE_BUSY,
            ) => return "busy",
            Some(
                ERROR_BUFFER_OVERFLOW | ERROR_FILENAME_EXCED_RANGE | ERROR_CANT_RESOLVE_FILENAME,
            ) => return "invalid-path",
            _ => {}
        }
    }
    use io::ErrorKind::*;
    match error.kind() {
        NotFound => "not-found",
        PermissionDenied | ReadOnlyFilesystem => "permission-denied",
        ResourceBusy | WouldBlock => "busy",
        DirectoryNotEmpty => "directory-not-empty",
        IsADirectory => "not-file",
        NotADirectory => "not-directory",
        InvalidInput | InvalidFilename => "invalid-path",
        FileTooLarge => "too-large",
        _ => {
            #[cfg(unix)]
            if matches!(error.raw_os_error(), Some(libc::EMFILE | libc::ENFILE)) {
                return "busy";
            }
            "unavailable"
        }
    }
}

fn metadata(dir: &Dir, name: &str) -> Result<Option<Metadata>> {
    match dir.symlink_metadata(name) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(reason(error)),
    }
}

fn directory(dir: &Dir, name: &str, create: bool) -> Result<Option<Dir>> {
    let mut info = metadata(dir, name)?;
    if info.is_none() && create {
        let builder = DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use cap_std::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        match dir.create_dir_with(name, &builder) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(reason(error)),
        }
        info = metadata(dir, name)?;
    }
    let Some(info) = info else {
        return Ok(None);
    };
    if info.is_symlink() {
        return Err("invalid-path");
    }
    if !info.is_dir() {
        return Err("not-directory");
    }
    dir.open_dir_nofollow(name).map(Some).map_err(reason)
}

fn root(path: &Path) -> Result<Dir> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match std::fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_symlink() => return Err("invalid-path"),
        Ok(info) if !info.is_dir() => return Err("not-directory"),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            builder.create(path).map_err(reason)?
        }
        Err(error) => return Err(reason(error)),
    }
    let parent = path.parent().ok_or("invalid-path")?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid-path")?;
    let parent = Dir::open_ambient_dir(parent, cap_std::ambient_authority()).map_err(reason)?;
    directory(&parent, name, false)?.ok_or("not-found")
}

fn execute(
    root_path: &Path,
    request: &Request,
    serial: u64,
    #[cfg(feature = "testing")] probe: &Mutex<Option<Probe>>,
) -> Result<Option<Value>> {
    let parts = request.path().map(segments).transpose()?;
    if let Request::WriteText { contents, .. } | Request::WriteJson { contents, .. } = request
        && contents.len() as u64 > MAX_BYTES
    {
        return Err("too-large");
    }
    let mut parent = root(root_path)?;
    let create = matches!(
        request,
        Request::WriteText { .. } | Request::WriteJson { .. }
    );
    if let Some(parts) = &parts {
        for name in &parts[..parts.len() - 1] {
            match directory(&parent, name, create)? {
                Some(dir) => parent = dir,
                None => {
                    return match request {
                        Request::Exists { .. } => Ok(Some(false.into())),
                        Request::ReadText { .. }
                        | Request::ReadJson { .. }
                        | Request::Remove { .. } => Ok(None),
                        _ => Err("not-found"),
                    };
                }
            }
        }
    }
    let name = parts.as_ref().map(|parts| *parts.last().unwrap());
    let info = name
        .map(|name| metadata(&parent, name))
        .transpose()?
        .flatten();
    match request {
        Request::Exists { .. } => Ok(Some(info.is_some().into())),
        Request::List { .. } => {
            let dir = match name {
                None => parent,
                Some(name) => directory(&parent, name, false)?.ok_or("not-found")?,
            };
            let mut entries = Vec::new();
            for entry in dir.entries().map_err(reason)? {
                let entry = entry.map_err(reason)?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "invalid-utf8")?;
                if name.starts_with(TEMP_PREFIX) {
                    continue;
                }
                if entries.len() == 1000 {
                    return Err("too-many-entries");
                }
                let kind = entry.file_type().map_err(reason)?;
                let kind = if kind.is_symlink() {
                    "symbolic-link"
                } else if kind.is_dir() {
                    "directory"
                } else if kind.is_file() {
                    "file"
                } else {
                    "other"
                };
                entries.push((name, kind));
            }
            entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            Ok(Some(
                entries
                    .into_iter()
                    .map(|(name, kind)| json!({"name":name,"kind":kind}))
                    .collect(),
            ))
        }
        Request::ReadText { .. } | Request::ReadJson { .. } => {
            let Some(info) = info else {
                return Ok(None);
            };
            require_file(&info)?;
            if info.len() > MAX_BYTES {
                return Err("too-large");
            }
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let file = match parent.open_with(name.unwrap(), &options) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(reason(error)),
            };
            let mut bytes = Vec::new();
            file.take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(reason)?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err("too-large");
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| "invalid-utf8")?;
            let text = text.strip_prefix('\u{feff}').unwrap_or(text);
            Ok(Some(text.into()))
        }
        Request::WriteText { contents, .. } | Request::WriteJson { contents, .. } => {
            if let Some(info) = info {
                require_file(&info)?;
            }
            let temporary = format!("{TEMP_PREFIX}{}-{serial}.tmp", std::process::id());
            let mut options = OpenOptions::new();
            options
                .write(true)
                .create_new(true)
                .follow(FollowSymlinks::No);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = parent.open_with(&temporary, &options).map_err(reason)?;
            let result = (|| {
                file.write_all(contents.as_bytes()).map_err(reason)?;
                drop(file);
                #[cfg(feature = "testing")]
                notify(probe, FileStage::BeforeRename, request);
                parent
                    .rename(&temporary, &parent, name.unwrap())
                    .map_err(reason)
            })();
            if result.is_err() {
                let _ = parent.remove_file(&temporary);
            }
            result?;
            Ok(None)
        }
        Request::Remove { .. } => {
            if let Some(info) = info {
                if info.is_dir() {
                    parent.remove_dir(name.unwrap()).map_err(reason)?;
                } else if info.is_file() || info.is_symlink() {
                    parent
                        .remove_file_or_symlink(name.unwrap())
                        .map_err(reason)?;
                } else {
                    return Err("unavailable");
                }
            }
            Ok(None)
        }
    }
}

fn require_file(info: &Metadata) -> Result<()> {
    if info.is_symlink() {
        Err("invalid-path")
    } else if !info.is_file() {
        Err("not-file")
    } else {
        Ok(())
    }
}
