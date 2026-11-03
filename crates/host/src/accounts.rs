use lucent_protocol::script_options::{ScriptOptions, ScriptOptionsPatch};
use parking_lot::Mutex;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use crate::data::app_data::{AppData, Request};

pub use crate::data::Password;

#[derive(Clone)]
pub struct Accounts {
    pub(crate) app_data: Arc<AppData>,
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

    pub fn watch(&self, waker: Waker) {
        self.app_data.changes.watch(waker);
    }

    fn ask<T>(&self, request: impl FnOnce(Answer<T>) -> Request) -> Reply<T> {
        let slot = Arc::new(Mutex::new(Slot::Waiting(None)));
        self.app_data.submit(request(Answer(Some(slot.clone()))));
        Reply(slot)
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
    File(FileError),
    #[error("the account data service has stopped")]
    Stopped,
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

pub struct Reply<T>(Arc<Mutex<Slot<T>>>);

enum Slot<T> {
    Waiting(Option<Waker>),
    Ready(Result<T, AccountsError>),
    Taken,
}

pub(crate) struct Answer<T>(Option<Arc<Mutex<Slot<T>>>>);

impl<T> Future for Reply<T> {
    type Output = Result<T, AccountsError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut slot = self.0.lock();
        match std::mem::replace(&mut *slot, Slot::Taken) {
            Slot::Ready(result) => Poll::Ready(result),
            Slot::Waiting(_) => {
                *slot = Slot::Waiting(Some(cx.waker().clone()));
                Poll::Pending
            }
            Slot::Taken => panic!("a reply was polled after it completed"),
        }
    }
}

impl<T> Answer<T> {
    pub(crate) fn send(mut self, result: Result<T, AccountsError>) {
        if let Some(slot) = self.0.take() {
            fill(&slot, result);
        }
    }
}

impl<T> Drop for Answer<T> {
    fn drop(&mut self) {
        if let Some(slot) = self.0.take() {
            fill(&slot, Err(AccountsError::Stopped));
        }
    }
}

fn fill<T>(slot: &Mutex<Slot<T>>, result: Result<T, AccountsError>) {
    let previous = std::mem::replace(&mut *slot.lock(), Slot::Ready(result));
    if let Slot::Waiting(Some(waker)) = previous {
        waker.wake();
    }
}
