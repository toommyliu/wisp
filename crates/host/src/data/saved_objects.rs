use parking_lot::{Condvar, Mutex};
use ruffle_core::backend::storage::StorageBackend;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::time::Duration;

use super::app_data::{AppData, Request};
use super::documents::trim;
use super::path;

const LOAD_WAIT: Duration = Duration::from_secs(5);

pub(crate) type Entries = HashMap<String, Vec<u8>>;

#[derive(Default)]
pub(crate) struct Disk {
    pub(crate) entries: Entries,
    pub(crate) unreadable: HashSet<String>,
    pub(crate) failed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Owner {
    Default,
    Account(String),
}

impl Owner {
    pub(crate) fn account(username: &str) -> Self {
        Owner::Account(trim(username).to_lowercase())
    }

    pub(crate) fn folder(&self) -> Option<Vec<String>> {
        match self {
            Owner::Default => Some(vec!["default".into()]),
            Owner::Account(name) => path::segments(name)
                .is_ok_and(|segments| segments.len() == 1)
                .then(|| vec!["accounts".into(), name.clone()]),
        }
    }

    pub(crate) fn release_partition(&self) -> String {
        match self {
            Owner::Default => "lucent-game-default".into(),
            Owner::Account(name) => {
                let digest = Sha256::digest(name.as_bytes());
                let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
                format!("lucent-game-account-{hex}")
            }
        }
    }
}

pub(crate) struct SavedObjects {
    app_data: Arc<AppData>,
    profiles: Mutex<HashMap<Owner, Weak<Profile>>>,
}

impl SavedObjects {
    pub(crate) fn new(app_data: Arc<AppData>) -> Self {
        Self {
            app_data,
            profiles: Mutex::default(),
        }
    }

    pub(crate) fn profile(&self, owner: Owner) -> Arc<Profile> {
        let mut profiles = self.profiles.lock();
        if let Some(profile) = profiles.get(&owner).and_then(Weak::upgrade) {
            return profile;
        }
        let profile = Arc::new(Profile {
            folder: owner.folder(),
            load: Arc::default(),
            app_data: self.app_data.clone(),
        });
        profiles.retain(|_, profile| profile.strong_count() > 0);
        match &profile.folder {
            Some(folder) => self.app_data.submit(Request::LoadSavedObjects {
                folder: folder.clone(),
                partition: owner.release_partition(),
                load: profile.load.clone(),
            }),
            None => {
                tracing::warn!(
                    component = "saved-objects",
                    "This account's name can't be a folder; its game saves last only until the game closes"
                );
                profile.load.fill(Disk::default());
            }
        }
        profiles.insert(owner, Arc::downgrade(&profile));
        profile
    }
}

#[derive(Default)]
pub(crate) struct Load {
    state: Mutex<State>,
    loaded: Condvar,
}

#[derive(Default)]
struct State {
    entries: Option<Entries>,
    degraded: bool,
    unreadable: HashSet<String>,
}

impl Load {
    pub(crate) fn fill(&self, disk: Disk) {
        let mut state = self.state.lock();
        state.degraded |= disk.failed;
        state.unreadable.extend(disk.unreadable);
        match &mut state.entries {
            None => state.entries = Some(disk.entries),
            Some(entries) => {
                for (key, bytes) in disk.entries {
                    entries.entry(key).or_insert(bytes);
                }
            }
        }
        self.loaded.notify_all();
    }

    fn ready(&self) -> parking_lot::MutexGuard<'_, State> {
        let mut state = self.state.lock();
        if state.entries.is_none() {
            let _ =
                self.loaded
                    .wait_while_for(&mut state, |state| state.entries.is_none(), LOAD_WAIT);
        }
        if state.entries.is_none() {
            tracing::warn!(
                component = "saved-objects",
                "Game saves did not load in time; changes this session will not be saved"
            );
            state.entries = Some(Entries::new());
            state.degraded = true;
        }
        state
    }
}

pub(crate) struct Profile {
    folder: Option<Vec<String>>,
    load: Arc<Load>,
    app_data: Arc<AppData>,
}

impl Profile {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        let state = self.load.ready();
        state.entries.as_ref()?.get(key).cloned()
    }

    fn write(&self, key: String, bytes: Option<Vec<u8>>) {
        let mut state = self.load.ready();
        let Some(entries) = state.entries.as_mut() else {
            return;
        };
        let changed = match &bytes {
            Some(bytes) => entries.insert(key.clone(), bytes.clone()).as_ref() != Some(bytes),
            None => entries.remove(&key).is_some(),
        };
        if changed
            && !state.degraded
            && !state.unreadable.contains(&key)
            && let Some(folder) = &self.folder
        {
            self.app_data.submit(Request::SaveSavedObject {
                folder: folder.clone(),
                key,
                bytes,
            });
        }
    }
}

pub(crate) struct Storage {
    profile: Arc<Profile>,
    seen: Mutex<HashMap<String, Vec<u8>>>,
}

impl Storage {
    pub(crate) fn new(profile: Arc<Profile>) -> Self {
        Self {
            profile,
            seen: Mutex::default(),
        }
    }
}

impl StorageBackend for Storage {
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        let key = key(name)?;
        let bytes = self.profile.read(&key);
        let mut seen = self.seen.lock();
        match &bytes {
            Some(bytes) => seen.insert(key, bytes.clone()),
            None => seen.remove(&key),
        };
        bytes
    }

    fn put(&mut self, name: &str, value: &[u8]) -> bool {
        let Some(key) = key(name) else {
            return false;
        };
        let seen = self.seen.get_mut();
        if seen.get(&key).is_some_and(|bytes| bytes == value) {
            return true;
        }
        seen.insert(key.clone(), value.to_vec());
        self.profile.write(key, Some(value.to_vec()));
        true
    }

    fn remove_key(&mut self, name: &str) {
        let Some(key) = key(name) else {
            return;
        };
        self.seen.get_mut().remove(&key);
        self.profile.write(key, None);
    }
}

pub(crate) fn key(name: &str) -> Option<String> {
    let key = name
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    let valid = path::segments(&key).is_ok();
    valid.then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_drop_empty_segments_and_refuse_what_a_folder_cannot_hold() {
        assert_eq!(
            key("localhost//AQWUserPref").as_deref(),
            Some("localhost/AQWUserPref")
        );
        assert_eq!(
            key("localhost/lucent/loader.swf/#a/b").as_deref(),
            Some("localhost/lucent/loader.swf/#a/b")
        );
        for refused in ["", "localhost/con", "localhost/a:b", "localhost/trailing."] {
            assert_eq!(key(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn owners_name_their_folder_and_the_release_partition() {
        let hero = Owner::account("  Hero ");
        assert_eq!(hero, Owner::Account("hero".into()));
        assert_eq!(hero.folder(), Some(vec!["accounts".into(), "hero".into()]));
        assert_eq!(
            hero.release_partition(),
            "lucent-game-account-ae6c79d10f1fd410650790e63186ec108fa106325b52fbd88de21a43540e6f2c"
        );
        assert_eq!(Owner::Default.folder(), Some(vec!["default".into()]));
        assert_eq!(Owner::Default.release_partition(), "lucent-game-default");
        for unstorable in ["con", "a/b", "dots.", "."] {
            assert_eq!(Owner::account(unstorable).folder(), None, "{unstorable:?}");
        }
    }
}
