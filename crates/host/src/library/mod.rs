use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use lucent_protocol::script::{ModulePath, PathError, Script, ScriptError};

const SCRIPT_MAX_BYTES: u64 = 16 << 20;
const MANIFEST_MAX_BYTES: u64 = 1 << 20;
const PACKAGE_FILE_MAX_BYTES: u64 = 64 << 20;
const PACKAGE_MAX_BYTES: u64 = 256 << 20;
const PACKAGE_MAX_FILES: usize = 10_000;
pub(crate) const SNAPSHOT_MAX_BYTES: usize = 16 << 20;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{0}")]
    Limit(String),
    #[error("couldn't read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a Lucent package manifest: {reason}")]
    Manifest { path: PathBuf, reason: String },
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Script(#[from] ScriptError),
    #[error("duplicate package name {0}")]
    Duplicate(String),
    #[error("couldn't load script {spec:?}; searched {root}: {reason}")]
    Spec {
        spec: String,
        root: PathBuf,
        reason: String,
    },
}

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub main: ModulePath,
    pub modules: BTreeMap<ModulePath, String>,
}

impl Package {
    pub fn read(dir: &Path) -> Result<Self, LoadError> {
        let path = dir.join("package.json");
        let manifest = manifest(&path)?;
        let invalid = |reason: String| LoadError::Manifest {
            path: path.clone(),
            reason,
        };
        if !manifest
            .get("lucent")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(invalid("\"lucent\" must be an object".into()));
        }
        let field = |name| {
            manifest
                .get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid(format!("{name:?} is missing")))
        };
        let name = field("name")?.to_owned();
        let main = field("main")?;
        let main = main.strip_prefix("./").unwrap_or(main);
        validate_module_path(main)?;
        let main = ModulePath::in_package(&name, main)?;
        let mut inventory = Inventory::default();
        read_modules(dir, dir, &name, &mut inventory)?;
        let mut modules = BTreeMap::new();
        for (module, path) in inventory.modules {
            modules.insert(module, read_script_source(&path)?);
        }
        Ok(Self {
            name,
            main,
            modules,
        })
    }

    pub fn script(self, entry: &str) -> Result<Script, LoadError> {
        validate_module_path(entry)?;
        Ok(Script::new(
            ModulePath::in_package(&self.name, entry)?,
            self.modules,
            [(self.name, self.main)].into(),
        )?)
    }
}

pub fn load_script(spec: &str, cwd: &Path) -> Result<Script, LoadError> {
    let root = cwd.join("script-packages");
    load(spec, cwd, &root).map_err(|error| LoadError::Spec {
        spec: spec.into(),
        root,
        reason: error.to_string(),
    })
}

fn load(spec: &str, cwd: &Path, root: &Path) -> Result<Script, LoadError> {
    let mut packages = BTreeMap::new();
    match fs::read_dir(root) {
        Ok(entries) => {
            let mut dirs = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|source| read_error(root, source))?;
                let path = entry.path();
                if path.is_dir() && path.join("package.json").is_file() {
                    dirs.push(path);
                }
            }
            dirs.sort();
            for dir in dirs {
                let dir = canonicalize(&dir)?;
                let package = Package::read(&dir)?;
                insert_package(&mut packages, dir, package)?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(read_error(root, error)),
    }
    let file = cwd.join(spec);
    let mut modules = BTreeMap::new();
    let entry = if file.is_file() {
        let file = canonicalize(&file)?;
        let mut owner = None;
        for dir in file.parent().unwrap().ancestors() {
            let path = dir.join("package.json");
            if path.is_file() {
                if manifest(&path)?.get("lucent").is_some() {
                    owner = Some(dir);
                }
                break;
            }
        }
        if let Some(dir) = owner {
            let package = Package::read(dir)?;
            let entry = ModulePath::in_package(
                &package.name,
                module_name(file.strip_prefix(dir).unwrap())?,
            )?;
            insert_package(&mut packages, dir.to_owned(), package)?;
            entry
        } else {
            let entry = ModulePath::loose(module_name(Path::new(file.file_name().unwrap()))?)?;
            modules.insert(entry.clone(), read_script_source(&file)?);
            entry
        }
    } else if let Some((name, path)) = package_spec(spec) {
        validate_module_path(path)?;
        ModulePath::in_package(name, path)?
    } else {
        read_script_source(&file)?;
        return Err(PathError::InvalidPath(spec.into()).into());
    };
    let mut table = BTreeMap::new();
    for (_, (_, package)) in packages {
        table.insert(package.name, package.main);
        modules.extend(package.modules);
    }
    Ok(Script::new(entry, modules, table)?)
}

fn insert_package(
    packages: &mut BTreeMap<String, (PathBuf, Package)>,
    dir: PathBuf,
    package: Package,
) -> Result<(), LoadError> {
    if let Some((previous, _)) = packages.get(&package.name) {
        if previous != &dir {
            return Err(LoadError::Duplicate(package.name));
        }
    } else {
        packages.insert(package.name.clone(), (dir, package));
    }
    Ok(())
}

fn package_spec(spec: &str) -> Option<(&str, &str)> {
    let split = if spec.starts_with('@') {
        let scope = spec.find('/')?;
        scope + 1 + spec[scope + 1..].find('/')?
    } else {
        spec.find('/')?
    };
    let (name, path) = (&spec[..split], &spec[split + 1..]);
    ModulePath::in_package(name, path).ok()?;
    Some((name, path))
}

fn manifest(path: &Path) -> Result<serde_json::Value, LoadError> {
    if !fs::symlink_metadata(path)
        .map_err(|error| read_error(path, error))?
        .is_file()
    {
        return Err(LoadError::Manifest {
            path: path.to_owned(),
            reason: "package.json must be a file.".into(),
        });
    }
    serde_json::from_str(&read_source(
        path,
        MANIFEST_MAX_BYTES,
        "package.json exceeds the 1 MiB limit.".into(),
    )?)
    .map_err(|error| LoadError::Manifest {
        path: path.to_owned(),
        reason: error.to_string(),
    })
}

#[derive(Default)]
struct Inventory {
    files: usize,
    bytes: u64,
    modules: Vec<(ModulePath, PathBuf)>,
}

fn read_modules(
    root: &Path,
    dir: &Path,
    name: &str,
    inventory: &mut Inventory,
) -> Result<(), LoadError> {
    for entry in fs::read_dir(dir).map_err(|error| read_error(dir, error))? {
        let entry = entry.map_err(|error| read_error(dir, error))?;
        if entry.file_name().to_string_lossy().starts_with('.')
            || entry.file_name() == "node_modules"
        {
            continue;
        }
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| read_error(&path, error))?;
        if kind.is_dir() {
            module_name(path.strip_prefix(root).unwrap())?;
            read_modules(root, &path, name, inventory)?;
        } else if kind.is_file() {
            inventory.files += 1;
            if inventory.files > PACKAGE_MAX_FILES {
                return Err(LoadError::Limit(format!(
                    "Directory contains more than 10000 files: {}.",
                    root.display()
                )));
            }
            let bytes = fs::metadata(&path)
                .map_err(|error| read_error(&path, error))?
                .len();
            if bytes > PACKAGE_FILE_MAX_BYTES {
                return Err(LoadError::Limit(format!(
                    "File exceeds the 64 MiB limit: {}.",
                    path.display()
                )));
            }
            inventory.bytes += bytes;
            if inventory.bytes > PACKAGE_MAX_BYTES {
                return Err(LoadError::Limit(
                    "The package exceeds the 256 MiB limit.".into(),
                ));
            }
            let relative = module_name(path.strip_prefix(root).unwrap())?;
            if path
                .extension()
                .is_some_and(|ext| ext == "js" || ext == "mjs")
            {
                inventory
                    .modules
                    .push((ModulePath::in_package(name, relative)?, path));
            }
        }
    }
    Ok(())
}

fn validate_module_path(path: &str) -> Result<(), LoadError> {
    if path.len() > 1024 {
        return Err(LoadError::Limit(
            "Module path exceeds the 1 KiB limit.".into(),
        ));
    }
    if path.split('/').any(|component| component.len() > 255) {
        return Err(LoadError::Limit(
            "Module path component exceeds the 255 bytes limit.".into(),
        ));
    }
    Ok(())
}

fn module_name(path: &Path) -> Result<String, LoadError> {
    let name = path
        .to_str()
        .map(|s| s.replace('\\', "/"))
        .ok_or_else(|| PathError::InvalidPath(path.to_string_lossy().into_owned()))?;
    validate_module_path(&name)?;
    Ok(name)
}

fn canonicalize(path: &Path) -> Result<PathBuf, LoadError> {
    fs::canonicalize(path).map_err(|error| read_error(path, error))
}
fn read_script_source(path: &Path) -> Result<String, LoadError> {
    read_source(
        path,
        SCRIPT_MAX_BYTES,
        format!("Script file exceeds the 16 MiB limit: {}.", path.display()),
    )
}

fn read_source(path: &Path, max_bytes: u64, message: String) -> Result<String, LoadError> {
    let file = fs::File::open(path).map_err(|error| read_error(path, error))?;
    let bytes = file
        .metadata()
        .map_err(|error| read_error(path, error))?
        .len();
    if bytes > max_bytes {
        return Err(LoadError::Limit(message));
    }
    let mut source = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut source)
        .map_err(|error| read_error(path, error))?;
    if source.len() as u64 > max_bytes {
        return Err(LoadError::Limit(message));
    }
    String::from_utf8(source).map_err(|error| {
        read_error(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })
}
fn read_error(path: &Path, source: std::io::Error) -> LoadError {
    LoadError::Read {
        path: path.to_owned(),
        source,
    }
}
