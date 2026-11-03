use std::borrow::Borrow;
use std::collections::HashSet;
use std::fmt;

use super::json::{self, JsMap, Json, Output};
use super::{Decoded, Issues, Loaded, decode_document, trim};
use crate::accounts::{Account, AccountsSnapshot, Group};

#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn reveal_for_editor(&self) -> &str {
        &self.0
    }

    pub(super) fn trimmed(&self) -> Option<Password> {
        let text = trim(&self.0);
        (!text.is_empty()).then(|| Password(text.to_owned()))
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(..)")
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(in crate::data) struct AccountsDocument {
    pub(super) accounts: Vec<Account>,
    pub(super) groups: JsMap<Vec<String>>,
}

impl AccountsDocument {
    pub(in crate::data) fn snapshot(&self) -> AccountsSnapshot {
        let groups = self.groups.iter().map(|(name, usernames)| Group {
            name: name.to_owned(),
            usernames: usernames.clone(),
        });
        AccountsSnapshot {
            accounts: self.accounts.clone(),
            groups: groups.collect(),
        }
    }
}

pub(in crate::data) fn decode(file: Option<&[u8]>) -> Loaded<AccountsDocument> {
    decode_document(file, from_object)
}

pub(super) fn normalize(document: AccountsDocument) -> AccountsDocument {
    let AccountsDocument { accounts, groups } = document;
    let accounts = first_per_username(accounts);
    let groups = groups.iter();
    let groups = normalize_groups(
        groups.map(|(name, members)| (name, members.iter().map(String::as_str).collect())),
        &accounts,
    );
    AccountsDocument { accounts, groups }
}

pub(in crate::data) fn encode(document: &AccountsDocument) -> Vec<u8> {
    let accounts = first_per_username(&document.accounts);
    let groups = document.groups.iter();
    let groups = normalize_groups(
        groups.map(|(name, members)| (name, members.iter().map(String::as_str).collect())),
        &accounts,
    );
    let accounts = accounts.iter().map(|account| {
        Output::Object(vec![
            ("label", Output::String(&account.label)),
            ("username", Output::String(&account.username)),
            ("password", Output::String(&account.password.0)),
        ])
    });
    let groups = groups.iter().map(|(name, members)| {
        let members = members.iter().map(|member| Output::String(member));
        (name, Output::Array(members.collect()))
    });
    json::stringify(&Output::Object(vec![
        ("accounts", Output::Array(accounts.collect())),
        ("groups", Output::Object(groups.collect())),
    ]))
}

fn from_object(raw: &JsMap<Json>) -> Decoded<AccountsDocument> {
    let no_groups = JsMap::default();
    let raw_accounts = match raw.get("accounts") {
        Some(Json::Array(accounts)) => accounts.as_slice(),
        _ => &[],
    };
    let raw_groups = match raw.get("groups") {
        Some(Json::Object(groups)) => groups,
        _ => &no_groups,
    };
    let mut issues = Issues::default();
    if raw
        .get("accounts")
        .is_some_and(|accounts| !matches!(accounts, Json::Array(_)))
    {
        issues.reset.push("accounts");
    }
    if raw
        .get("groups")
        .is_some_and(|groups| !matches!(groups, Json::Object(_)))
        || raw_groups
            .iter()
            .any(|(_, members)| !matches!(members, Json::Array(_)))
    {
        issues.reset.push("groups");
    }

    let accounts = first_per_username(raw_accounts.iter().filter_map(account));
    let groups = normalize_groups(
        raw_groups
            .iter()
            .map(|(name, members)| (name, strings(members))),
        &accounts,
    );

    let raw_members = raw_groups.iter().map(|(_, members)| match members {
        Json::Array(members) => members.len(),
        _ => 0,
    });
    let kept_members = groups.iter().map(|(_, members)| members.len());
    let ignored = [
        ("accounts", raw_accounts.len() - accounts.len()),
        ("groups", raw_groups.len() - groups.len()),
        (
            "groupMembers",
            raw_members.sum::<usize>() - kept_members.sum::<usize>(),
        ),
    ];
    if ignored.iter().any(|&(_, count)| count > 0) {
        issues.ignored = ignored.to_vec();
    }
    Decoded {
        value: AccountsDocument { accounts, groups },
        issues,
    }
}

fn account(entry: &Json) -> Option<Account> {
    let Json::Object(fields) = entry else {
        return None;
    };
    let text = |name| match fields.get(name) {
        Some(Json::String(text)) => Some(text.clone()),
        _ => None,
    };
    Some(Account {
        label: text("label")?,
        username: text("username")?,
        password: Password(text("password")?),
    })
}

fn strings(members: &Json) -> Vec<&str> {
    let Json::Array(members) = members else {
        return Vec::new();
    };
    let strings = members.iter().filter_map(|member| match member {
        Json::String(member) => Some(member.as_str()),
        _ => None,
    });
    strings.collect()
}

fn first_per_username<A: Borrow<Account>>(accounts: impl IntoIterator<Item = A>) -> Vec<A> {
    let mut seen = HashSet::new();
    let accounts = accounts.into_iter();
    accounts
        .filter(|account| seen.insert(account.borrow().username.to_lowercase()))
        .collect()
}

fn normalize_groups<'a>(
    groups: impl IntoIterator<Item = (&'a str, Vec<&'a str>)>,
    accounts: &[impl Borrow<Account>],
) -> JsMap<Vec<String>> {
    let usernames: HashSet<&str> = accounts
        .iter()
        .map(|account| account.borrow().username.as_str())
        .collect();
    let mut names = HashSet::new();
    let mut kept = JsMap::default();
    for (name, members) in groups {
        let name = trim(name);
        if name.is_empty() || !names.insert(name.to_lowercase()) {
            continue;
        }
        // Assigning `__proto__` to a JavaScript object sets its prototype, so a group
        // with this name never reached a file written by JavaScript.
        if name == "__proto__" {
            continue;
        }
        let mut seen = HashSet::new();
        let members = members
            .into_iter()
            .filter(|member| usernames.contains(member) && seen.insert(member.to_lowercase()));
        kept.insert(name.to_owned(), members.map(str::to_owned).collect());
    }
    kept
}
