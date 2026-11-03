use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, DirBuilder, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::path::TEMP_PREFIX;

pub(super) const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub(super) enum Failure {
    Io(io::Error),
    InvalidPath,
    NotDirectory,
    NotFile,
    TooLarge,
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(super) enum WriteFailure {
    Write(Failure),
    Rename(Failure),
}

pub(super) fn metadata(dir: &Dir, name: &str) -> Result<Option<Metadata>, Failure> {
    match dir.symlink_metadata(name) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn create_directory(dir: &Dir, name: &str) -> io::Result<()> {
    let builder = DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use cap_std::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    match dir.create_dir_with(name, &builder) {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(error),
        _ => Ok(()),
    }
}

pub(super) fn directory(dir: &Dir, name: &str, create: bool) -> Result<Option<Dir>, Failure> {
    let mut info = metadata(dir, name)?;
    if info.is_none() && create {
        create_directory(dir, name)?;
        info = metadata(dir, name)?;
    }
    let Some(info) = info else {
        return Ok(None);
    };
    if info.is_symlink() {
        return Err(Failure::InvalidPath);
    }
    if !info.is_dir() {
        return Err(Failure::NotDirectory);
    }
    Ok(Some(dir.open_dir_nofollow(name)?))
}

pub(super) fn create_root(path: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

pub(super) fn open_root(path: &Path, create: bool) -> Result<Option<Dir>, Failure> {
    match std::fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_symlink() => return Err(Failure::InvalidPath),
        Ok(info) if !info.is_dir() => return Err(Failure::NotDirectory),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound && create => create_root(path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let parent = path.parent().ok_or(Failure::InvalidPath)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(Failure::InvalidPath)?;
    let parent = Dir::open_ambient_dir(parent, cap_std::ambient_authority())?;
    directory(&parent, name, false)
}

pub(super) fn require_file(info: &Metadata) -> Result<(), Failure> {
    if info.is_symlink() {
        Err(Failure::InvalidPath)
    } else if !info.is_file() {
        Err(Failure::NotFile)
    } else {
        Ok(())
    }
}

pub(super) fn read(dir: &Dir, name: &str) -> Result<Option<Vec<u8>>, Failure> {
    let Some(info) = metadata(dir, name)? else {
        return Ok(None);
    };
    require_file(&info)?;
    if info.len() > MAX_BYTES {
        return Err(Failure::TooLarge);
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = match dir.open_with(name, &options) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Failure::TooLarge);
    }
    Ok(Some(bytes))
}

pub(super) fn write_atomic(
    dir: &Dir,
    name: &str,
    contents: &[u8],
    durable: bool,
    before_rename: impl FnOnce(),
) -> Result<(), WriteFailure> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let temporary = format!(
        "{TEMP_PREFIX}{}-{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    );
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
    let write = |error: io::Error| WriteFailure::Write(error.into());
    let mut file = dir.open_with(&temporary, &options).map_err(write)?;
    let result = (|| {
        file.write_all(contents).map_err(write)?;
        if durable {
            file.sync_all().map_err(write)?;
        }
        drop(file);
        before_rename();
        dir.rename(&temporary, dir, name)
            .map_err(|error| WriteFailure::Rename(error.into()))
    })();
    if result.is_err() {
        let _ = dir.remove_file(&temporary);
    }
    result
}

pub(super) fn owner_only(dir: &Dir) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = dir.open(".")?.into_std();
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            file.set_permissions(std::fs::Permissions::from_mode(0o700))?;
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

pub(super) fn sync_directory(dir: &Dir) -> io::Result<()> {
    #[cfg(unix)]
    return dir.open(".")?.into_std().sync_all();
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}
