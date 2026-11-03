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

pub(crate) fn app_data_root(
    platform: Platform,
    dev: bool,
    var: impl Fn(&str) -> Option<OsString>,
    home: impl FnOnce() -> Option<PathBuf>,
) -> io::Result<PathBuf> {
    todo!()
}

pub(crate) fn overlap(first: &Path, second: &Path) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn roots_overlap_when_equal_or_nested() {
        let base = std::env::temp_dir().join(format!("lucent-roots-{}", std::process::id()));
        let data = base.join("data");
        assert!(overlap(&data, &data));
        assert!(overlap(&data, &data.join("app")));
        assert!(overlap(&data.join("app"), &data));
        assert!(overlap(&data, &base.join("other").join("..").join("data")));
        assert!(!overlap(&data, &base.join("database")));
        assert!(!overlap(&data, &base.join("app")));
    }

    #[cfg(unix)]
    #[test]
    fn roots_that_alias_through_a_symlink_overlap() {
        let base = std::env::temp_dir().join(format!("lucent-roots-link-{}", std::process::id()));
        std::fs::create_dir_all(base.join("real")).unwrap();
        let link = base.join("link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(base.join("real"), &link).unwrap();
        let aliased = overlap(
            &base.join("real").join("data"),
            &link.join("data").join("app"),
        );
        std::fs::remove_dir_all(&base).unwrap();
        assert!(aliased);
    }
}
