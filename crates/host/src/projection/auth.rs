use super::wire::{wire_bool, wire_int};
use indexmap::IndexMap;
use lucent_protocol::auth::{AuthSnapshot, CredentialsSnapshot, ServerData, ServerSnapshot};
use serde_json::Value;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct LiveServer(Arc<RwLock<ServerData>>);
impl LiveServer {
    pub fn snapshot(&self) -> ServerSnapshot {
        let data = self.0.read().unwrap().clone();
        let full = data.max > 0.0 && data.count >= data.max;
        ServerSnapshot { data, full }
    }
    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Default)]
pub struct AuthState {
    pub username: String,
    pub password: String,
    pub logged_in: bool,
    pub(crate) credentials_epoch: Option<u64>,
    servers: IndexMap<String, LiveServer>,
}
impl std::fmt::Debug for AuthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthState")
            .field("logged_in", &self.logged_in)
            .finish_non_exhaustive()
    }
}
impl AuthState {
    pub fn credentials(&self) -> CredentialsSnapshot {
        CredentialsSnapshot {
            username: self.username.clone(),
            password: self.password.clone(),
        }
    }

    pub(crate) fn set_credentials(&mut self, credentials: CredentialsSnapshot) {
        self.username = credentials.username;
        self.password = credentials.password;
    }

    pub fn snapshot(&self) -> AuthSnapshot {
        AuthSnapshot {
            logged_in: self.logged_in,
            username: self.username.clone(),
            password: self.password.clone(),
            servers: self.servers.values().map(LiveServer::snapshot).collect(),
        }
    }

    pub fn servers(&self) -> Vec<LiveServer> {
        self.servers.values().cloned().collect()
    }
    pub fn set_servers(&mut self, servers: Vec<ServerData>) {
        self.servers
            .retain(|name, _| servers.iter().any(|server| server.name == *name));
        for server in servers {
            if let Some(current) = self.servers.get(&server.name) {
                *current.0.write().unwrap() = server;
            } else {
                self.servers.insert(
                    server.name.clone(),
                    LiveServer(Arc::new(RwLock::new(server))),
                );
            }
        }
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

pub fn decode_servers(value: &Value) -> Option<Vec<ServerData>> {
    value
        .as_array()?
        .iter()
        .map(|value| {
            let object = value.as_object()?;
            fn optional<T>(
                object: &serde_json::Map<String, Value>,
                key: &str,
                decode: impl FnOnce(&Value) -> Option<T>,
            ) -> Option<Option<T>> {
                object
                    .get(key)
                    .map_or(Some(None), |value| decode(value).map(Some))
            }
            Some(ServerData {
                chat: optional(object, "iChat", wire_int)?.unwrap_or(0.0),
                count: optional(object, "iCount", wire_int)?.unwrap_or(0.0),
                max: optional(object, "iMax", wire_int)?.unwrap_or(0.0),
                language: optional(object, "sLang", |value| value.as_str().map(str::to_owned))?
                    .unwrap_or_default(),
                member_only: optional(object, "bUpg", wire_bool)?.unwrap_or(false),
                online: optional(object, "bOnline", wire_bool)?.unwrap_or(true),
                name: object.get("sName")?.as_str()?.to_owned(),
            })
        })
        .collect()
}
