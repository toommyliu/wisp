use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Platform {
    Windows,
    MacOs,
    Other,
}

impl Platform {
    pub(crate) const CURRENT: Self = if cfg!(windows) {
        Self::Windows
    } else if cfg!(target_os = "macos") {
        Self::MacOs
    } else {
        Self::Other
    };
}

pub(crate) fn scripts_root(
    documents: impl FnOnce() -> Option<PathBuf>,
    home: impl FnOnce() -> Option<PathBuf>,
) -> io::Result<PathBuf> {
    documents()
        .or_else(|| home().map(|path| path.join("Documents")))
        .map(|path| path.join("Lucent/data"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Documents directory is unavailable",
            )
        })
}

pub(crate) fn app_data_root(
    platform: Platform,
    dev: bool,
    var: impl Fn(&str) -> Option<OsString>,
    home: impl FnOnce() -> Option<PathBuf>,
) -> io::Result<PathBuf> {
    let set = |name| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let home = || {
        home()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))
    };
    let base = match platform {
        Platform::Windows => match set("APPDATA") {
            Some(base) => base,
            None => home()?.join("AppData").join("Roaming"),
        },
        Platform::MacOs => home()?.join("Library").join("Application Support"),
        Platform::Other => match set("XDG_DATA_HOME") {
            Some(base) => base,
            None => home()?.join(".local").join("share"),
        },
    };
    Ok(base.join(if dev { "lucent-dev" } else { "lucent" }))
}

pub(crate) fn create(path: &Path) -> io::Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => super::fs::create_root(path)?,
        Err(error) => return Err(error),
        Ok(_) => {}
    }
    path.canonicalize()
}

pub(crate) struct Ancestry(Vec<Identity>);

pub(crate) fn ancestry(path: &Path) -> io::Result<Ancestry> {
    path.ancestors()
        .map(identity)
        .collect::<io::Result<_>>()
        .map(Ancestry)
}

pub(crate) fn overlap(first: &Ancestry, second: &Ancestry) -> bool {
    second.0.contains(&first.0[0]) || first.0.contains(&second.0[0])
}

#[cfg(unix)]
type Identity = (u64, u64);

#[cfg(unix)]
fn identity(path: &Path) -> io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|meta| (meta.dev(), meta.ino()))
}

#[cfg(windows)]
type Identity = same_file::Handle;

#[cfg(windows)]
fn identity(path: &Path) -> io::Result<Identity> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
    let folder = std::fs::OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    same_file::Handle::from_file(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_root_prefers_documents_and_falls_back_to_home() {
        assert_eq!(
            scripts_root(
                || Some(PathBuf::from("configured")),
                || panic!("home lookup is unnecessary")
            )
            .unwrap(),
            PathBuf::from("configured/Lucent/data")
        );
        assert_eq!(
            scripts_root(|| None, || Some(PathBuf::from("home"))).unwrap(),
            PathBuf::from("home/Documents/Lucent/data")
        );
        assert_eq!(
            scripts_root(|| None, || None).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    fn resolve(
        platform: Platform,
        dev: bool,
        vars: &[(&str, &str)],
        home: Option<&str>,
    ) -> io::Result<PathBuf> {
        app_data_root(
            platform,
            dev,
            |name| {
                vars.iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| value.into())
            },
            || home.map(PathBuf::from),
        )
    }

    #[test]
    fn the_app_data_folder_matches_the_release_per_platform_and_channel() {
        let home = Path::new("home");
        let roaming = Path::new("roaming");
        let xdg = Path::new("xdg");
        type Vars = &'static [(&'static str, &'static str)];
        let cases: [(Platform, Vars, PathBuf); 9] = [
            (Platform::Windows, &[("APPDATA", "roaming")], roaming.into()),
            (Platform::Windows, &[], home.join("AppData").join("Roaming")),
            (
                Platform::Windows,
                &[("APPDATA", "")],
                home.join("AppData").join("Roaming"),
            ),
            (
                Platform::MacOs,
                &[("APPDATA", "roaming"), ("XDG_DATA_HOME", "xdg")],
                home.join("Library").join("Application Support"),
            ),
            (Platform::Other, &[("XDG_DATA_HOME", "xdg")], xdg.into()),
            (Platform::Other, &[], home.join(".local").join("share")),
            (
                Platform::Other,
                &[("XDG_DATA_HOME", "")],
                home.join(".local").join("share"),
            ),
            (
                Platform::Other,
                &[("APPDATA", "roaming")],
                home.join(".local").join("share"),
            ),
            (
                Platform::Windows,
                &[("XDG_DATA_HOME", "xdg")],
                home.join("AppData").join("Roaming"),
            ),
        ];
        for (platform, vars, base) in cases {
            for (dev, folder) in [(false, "lucent"), (true, "lucent-dev")] {
                assert_eq!(
                    resolve(platform, dev, vars, Some("home")).unwrap(),
                    base.join(folder),
                    "{platform:?} {vars:?} dev={dev}"
                );
            }
        }
    }

    #[test]
    fn a_missing_home_fails_only_when_the_folder_needs_it() {
        assert_eq!(
            resolve(Platform::Windows, false, &[("APPDATA", "roaming")], None).unwrap(),
            Path::new("roaming").join("lucent")
        );
        assert_eq!(
            resolve(Platform::Other, false, &[("XDG_DATA_HOME", "xdg")], None).unwrap(),
            Path::new("xdg").join("lucent")
        );
        for platform in [Platform::Windows, Platform::MacOs, Platform::Other] {
            assert_eq!(
                resolve(platform, false, &[], None).unwrap_err().kind(),
                io::ErrorKind::NotFound
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn created_roots_that_alias_through_a_symlink_overlap() {
        let base = std::env::temp_dir().join(format!("lucent-roots-link-{}", std::process::id()));
        std::fs::create_dir_all(base.join("real")).unwrap();
        let link = base.join("link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(base.join("real"), &link).unwrap();
        let aliased = overlap(
            &ancestry(&create(&base.join("real").join("data")).unwrap()).unwrap(),
            &ancestry(&create(&link.join("data").join("app")).unwrap()).unwrap(),
        );
        std::fs::remove_dir_all(&base).unwrap();
        assert!(aliased);
    }
}
