use lucent_protocol::script_options::{ScriptOptions, ScriptOptionsPatch};
use std::path::PathBuf;
use std::sync::Arc;
use std::task::Waker;
use std::time::SystemTime;

use crate::data::app_data::{AppData, Request};
use crate::reply::{Answer, Reply, Stopped, reply};

pub use crate::data::Password;

mod servers;
pub use servers::{LoadError, LoginServers, Ping, PingStatus, Server, ServerList};

#[derive(Clone)]
pub struct Accounts {
    pub(crate) app_data: Arc<AppData>,
    pub(crate) servers: LoginServers,
}

impl Accounts {
    pub fn snapshot(&self) -> Reply<AccountsSnapshot> {
        self.ask(Request::Snapshot)
    }

    pub fn edit(&self, edit: AccountEdit) -> Reply<AccountsSnapshot> {
        self.ask(|answer| Request::Edit(edit, answer))
    }

    pub fn settings(&self, username: &str) -> Reply<ScriptOptions> {
        self.ask(|answer| Request::Settings(username.to_owned(), answer))
    }

    pub fn update_settings(
        &self,
        username: &str,
        patch: ScriptOptionsPatch,
    ) -> Reply<ScriptOptions> {
        self.ask(|answer| Request::UpdateSettings(username.to_owned(), patch, answer))
    }

    pub fn servers(&self) -> &LoginServers {
        &self.servers
    }

    pub fn watch(&self, waker: Waker) {
        self.app_data.account_changes.watch(waker);
    }

    fn ask<T>(&self, request: impl FnOnce(Answer<T>) -> Request) -> Reply<T> {
        let (answer, reply) = reply();
        self.app_data.submit(request(answer));
        reply
    }
}

#[derive(Debug)]
pub enum AccountEdit {
    CreateAccount {
        label: Option<String>,
        username: String,
        password: Password,
    },
    UpdateAccount {
        username: String,
        label: Option<String>,
        new_username: Option<String>,
        password: Option<Password>,
    },
    DeleteAccounts {
        usernames: Vec<String>,
    },
    CreateGroup {
        name: String,
        usernames: Vec<String>,
    },
    UpdateGroup {
        name: String,
        new_name: Option<String>,
        usernames: Option<Vec<String>>,
    },
    DeleteGroup {
        name: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountsSnapshot {
    pub(crate) accounts: Vec<Account>,
    pub(crate) groups: Vec<Group>,
}

impl AccountsSnapshot {
    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub(crate) label: String,
    pub(crate) username: String,
    pub(crate) password: Password,
}

impl Account {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &Password {
        &self.password
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub(crate) name: String,
    pub(crate) usernames: Vec<String>,
}

impl Group {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn usernames(&self) -> &[String] {
        &self.usernames
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AccountsError {
    #[error("{0} is required")]
    Required(&'static str),
    #[error("An account with this username already exists")]
    UsernameTaken,
    #[error("Account not found")]
    AccountNotFound,
    #[error("One or more accounts were not found")]
    AccountsNotFound,
    #[error("Account not found: {0}")]
    MemberNotFound(String),
    #[error("Duplicate group member: {0}")]
    DuplicateMember(String),
    #[error("Group not found")]
    GroupNotFound,
    #[error("A group with this name already exists")]
    GroupNameTaken,
    #[error("Invalid account username.")]
    InvalidUsername,
    #[error("{0}")]
    File(#[from] FileError),
    #[error("{error}")]
    Servers {
        error: LoadError,
        refresh_available_at: Option<SystemTime>,
    },
    #[error("the account data service has stopped")]
    Stopped,
}

impl Stopped for AccountsError {
    const STOPPED: Self = AccountsError::Stopped;
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "JSON file {operation} failed at {}.{}",
    .path.display(),
    if *.too_large { " The document exceeds the limit of 8388608 bytes." } else { "" }
)]
pub struct FileError {
    pub(crate) operation: &'static str,
    pub(crate) path: PathBuf,
    pub(crate) too_large: bool,
}
