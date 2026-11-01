use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModulePath {
    package: Option<String>,
    path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    InvalidPath(String),
    InvalidPackage(String),
    ReservedPackage(String),
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(path) => write!(f, "Invalid module path: {path}"),
            Self::InvalidPackage(name) => write!(f, "Invalid package name: {name}"),
            Self::ReservedPackage(name) => write!(f, "Reserved package name: {name}"),
        }
    }
}

impl Error for PathError {}

impl ModulePath {
    pub fn loose(path: impl Into<String>) -> Result<Self, PathError> {
        Self::new(None, path.into())
    }

    pub fn in_package(name: impl Into<String>, path: impl Into<String>) -> Result<Self, PathError> {
        let name = name.into();
        if name == "effect" || name == "lucent" || name.starts_with("lucent/") {
            return Err(PathError::ReservedPackage(name));
        }
        let part = |s: &str| {
            !s.is_empty()
                && !s.starts_with(['.', '_'])
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-._~".contains(&b))
        };
        let valid = if let Some(scoped) = name.strip_prefix('@') {
            scoped
                .split_once('/')
                .is_some_and(|(scope, name)| part(scope) && part(name))
        } else {
            part(&name)
        };
        if !valid || name.len() > 214 {
            return Err(PathError::InvalidPackage(name));
        }
        Self::new(Some(name), path.into())
    }

    fn new(package: Option<String>, path: String) -> Result<Self, PathError> {
        if path.contains('\\') || path.split('/').any(|s| matches!(s, "" | "." | "..")) {
            return Err(PathError::InvalidPath(path));
        }
        Ok(Self { package, path })
    }

    pub fn package(&self) -> Option<&str> {
        self.package.as_deref()
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn candidates(&self, specifier: &str) -> Vec<Self> {
        if !specifier.starts_with("./") && !specifier.starts_with("../") {
            return Vec::new();
        }
        let mut segments: Vec<_> = self.path.split('/').collect();
        segments.pop();
        for segment in specifier.split('/') {
            match segment {
                "." => {}
                ".." => {
                    if segments.pop().is_none() {
                        return Vec::new();
                    }
                }
                "" => return Vec::new(),
                s => segments.push(s),
            }
        }
        let joined = segments.join("/");
        [
            joined.clone(),
            format!("{joined}.js"),
            format!("{joined}/index.js"),
        ]
        .into_iter()
        .filter_map(|path| Self::new(self.package.clone(), path).ok())
        .collect()
    }
}

impl fmt::Display for ModulePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(package) = &self.package {
            write!(f, "{package}/")?;
        }
        f.write_str(&self.path)
    }
}

#[derive(Clone, Debug)]
pub struct Script {
    entry: ModulePath,
    modules: BTreeMap<ModulePath, String>,
    packages: BTreeMap<String, ModulePath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptError {
    MissingEntry(ModulePath),
    MissingPackageMain { name: String, path: ModulePath },
    PackageMainMismatch { name: String, path: ModulePath },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEntry(path) => write!(f, "Missing script entry: {path}"),
            Self::MissingPackageMain { name, path } => {
                write!(f, "Missing main module for {name}: {path}")
            }
            Self::PackageMainMismatch { name, path } => {
                write!(
                    f,
                    "Main module for {name} must belong to that package: {path}"
                )
            }
        }
    }
}

impl Error for ScriptError {}

impl Script {
    pub fn new(
        entry: ModulePath,
        modules: BTreeMap<ModulePath, String>,
        packages: BTreeMap<String, ModulePath>,
    ) -> Result<Self, ScriptError> {
        if !modules.contains_key(&entry) {
            return Err(ScriptError::MissingEntry(entry));
        }
        for (name, path) in &packages {
            if path.package() != Some(name.as_str()) {
                return Err(ScriptError::PackageMainMismatch {
                    name: name.clone(),
                    path: path.clone(),
                });
            }
            if !modules.contains_key(path) {
                return Err(ScriptError::MissingPackageMain {
                    name: name.clone(),
                    path: path.clone(),
                });
            }
        }
        Ok(Self {
            entry,
            modules,
            packages,
        })
    }

    pub fn entry(&self) -> &ModulePath {
        &self.entry
    }

    pub fn source(&self, path: &ModulePath) -> Option<&str> {
        self.modules.get(path).map(String::as_str)
    }

    pub fn resolve(&self, referrer: &ModulePath, specifier: &str) -> Option<&ModulePath> {
        referrer
            .candidates(specifier)
            .iter()
            .find_map(|path| self.modules.get_key_value(path).map(|(path, _)| path))
            .or_else(|| self.packages.get(specifier))
    }
}

pub type ScriptInputs = BTreeMap<String, InputValue>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Strings(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind")]
pub enum ScriptOutcome {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed {
        message: String,
        details: Option<String>,
    },
    #[serde(rename = "script-stopped")]
    StoppedByScript { reason: Option<String> },
    #[serde(rename = "script-exited")]
    Exited {
        #[serde(default)]
        logout: bool,
        #[serde(rename = "closeClient")]
        close_client: bool,
    },
    #[serde(rename = "externally-stopped")]
    Stopped { reason: Option<String> },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ScriptPhase {
    #[default]
    Idle,
    Waiting,
    Preparing,
    Starting,
    Running,
    Stopping,
    Finishing,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScriptStatus {
    pub phase: ScriptPhase,
    pub script: Option<String>,
    pub last_end: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ScriptDialogKind {
    Alert,
    Confirm,
    Prompt { placeholder: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScriptDialog {
    pub id: u64,
    pub kind: ScriptDialogKind,
    pub script: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ScriptDialogResponse {
    Dismissed,
    Confirmed(bool),
    Submitted(Option<String>),
}
