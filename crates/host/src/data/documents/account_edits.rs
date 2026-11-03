use super::accounts::AccountsDocument;
use crate::accounts::{AccountEdit, AccountsError};

pub(in crate::data) fn apply<'a>(
    edit: AccountEdit,
    stored: impl FnOnce() -> Result<&'a AccountsDocument, AccountsError>,
) -> Result<AccountsDocument, AccountsError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::super::json::JsMap;
    use super::*;
    use crate::accounts::{Account, Password};

    type View = (Vec<[String; 3]>, Vec<(String, Vec<String>)>);

    fn document(usernames: &[&str], groups: &[(&str, &[&str])]) -> AccountsDocument {
        let accounts = usernames.iter().map(|username| Account {
            label: format!("{username} label"),
            username: (*username).into(),
            password: Password::new(format!("{username} password")),
        });
        let mut map = JsMap::default();
        for (name, members) in groups {
            map.insert(
                (*name).into(),
                members.iter().map(|member| (*member).into()).collect(),
            );
        }
        AccountsDocument {
            accounts: accounts.collect(),
            groups: map,
        }
    }

    fn view(document: &AccountsDocument) -> View {
        let accounts = document.accounts.iter().map(|account| {
            [
                account.label.clone(),
                account.username.clone(),
                account.password.reveal_for_editor().to_owned(),
            ]
        });
        let groups = document.groups.iter();
        (
            accounts.collect(),
            groups
                .map(|(name, members)| (name.to_owned(), members.clone()))
                .collect(),
        )
    }

    fn edit(stored: &AccountsDocument, edit: AccountEdit) -> Result<View, AccountsError> {
        apply(edit, || Ok(stored)).map(|next| view(&next))
    }

    fn unreadable() -> AccountsError {
        AccountsError::File {
            operation: "read",
            path: "accounts.json".into(),
            too_large: false,
        }
    }

    fn create(username: &str, password: &str) -> AccountEdit {
        AccountEdit::CreateAccount {
            label: None,
            username: username.into(),
            password: Password::new(password),
        }
    }

    fn update(username: &str) -> AccountEdit {
        AccountEdit::UpdateAccount {
            username: username.into(),
            label: None,
            new_username: None,
            password: None,
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn creating_trims_fields_and_defaults_the_label_to_the_username() {
        let stored = document(&[], &[]);
        let (accounts, _) = edit(
            &stored,
            AccountEdit::CreateAccount {
                label: Some("  ".into()),
                username: " Hero ".into(),
                password: Password::new(" secret "),
            },
        )
        .unwrap();
        assert_eq!(accounts, [["Hero", "Hero", "secret"].map(String::from)]);
        let (accounts, _) = edit(
            &stored,
            AccountEdit::CreateAccount {
                label: Some(" Main ".into()),
                username: "Hero".into(),
                password: Password::new("secret"),
            },
        )
        .unwrap();
        assert_eq!(accounts[0][0], "Main");
    }

    #[test]
    fn required_fields_use_the_release_messages() {
        let stored = document(&["Hero"], &[("Raiders", &[])]);
        let cases = [
            (create(" ", "secret"), "username is required"),
            (create("New", "\u{feff} \n"), "password is required"),
            (
                AccountEdit::UpdateAccount {
                    username: "Hero".into(),
                    label: Some(" ".into()),
                    new_username: None,
                    password: None,
                },
                "label is required",
            ),
            (
                AccountEdit::UpdateAccount {
                    username: "Hero".into(),
                    label: None,
                    new_username: Some("".into()),
                    password: None,
                },
                "username is required",
            ),
            (
                AccountEdit::UpdateAccount {
                    username: "Hero".into(),
                    label: None,
                    new_username: None,
                    password: Some(Password::new(" ")),
                },
                "password is required",
            ),
            (update(" "), "username is required"),
            (
                AccountEdit::DeleteAccounts {
                    usernames: strings(&["Hero", " "]),
                },
                "username is required",
            ),
            (
                AccountEdit::CreateGroup {
                    name: " ".into(),
                    usernames: vec![],
                },
                "group name is required",
            ),
            (
                AccountEdit::CreateGroup {
                    name: "Alts".into(),
                    usernames: strings(&[" "]),
                },
                "group username is required",
            ),
            (
                AccountEdit::UpdateGroup {
                    name: "Raiders".into(),
                    new_name: Some(" ".into()),
                    usernames: None,
                },
                "group name is required",
            ),
            (
                AccountEdit::DeleteGroup { name: "".into() },
                "group name is required",
            ),
        ];
        for (edit_case, message) in cases {
            let label = format!("{edit_case:?}");
            assert_eq!(
                edit(&stored, edit_case).unwrap_err().to_string(),
                message,
                "{label}"
            );
        }
    }

    #[test]
    fn usernames_conflict_case_insensitively_and_match_exactly() {
        let stored = document(&["Hero", "Alt"], &[]);
        assert_eq!(
            edit(&stored, create("hero", "secret")),
            Err(AccountsError::UsernameTaken)
        );
        assert_eq!(
            edit(&stored, update("hero")),
            Err(AccountsError::AccountNotFound)
        );
        assert_eq!(
            edit(
                &stored,
                AccountEdit::UpdateAccount {
                    username: "Missing".into(),
                    label: None,
                    new_username: Some("ALT".into()),
                    password: None,
                }
            ),
            Err(AccountsError::UsernameTaken)
        );
        assert_eq!(
            edit(
                &stored,
                AccountEdit::UpdateAccount {
                    username: "Hero".into(),
                    label: None,
                    new_username: Some("alt".into()),
                    password: None,
                }
            ),
            Err(AccountsError::UsernameTaken)
        );
    }

    #[test]
    fn updating_renames_group_members_and_allows_a_case_only_rename() {
        let stored = document(
            &["Hero", "Alt"],
            &[("Raiders", &["Hero", "Alt"]), ("Solo", &["Hero"])],
        );
        let (accounts, groups) = edit(
            &stored,
            AccountEdit::UpdateAccount {
                username: " Hero ".into(),
                label: None,
                new_username: Some(" HERO ".into()),
                password: Some(Password::new(" new ")),
            },
        )
        .unwrap();
        assert_eq!(accounts[0], ["Hero label", "HERO", "new"].map(String::from));
        assert_eq!(
            groups,
            [
                ("Raiders".into(), strings(&["HERO", "Alt"])),
                ("Solo".into(), strings(&["HERO"]))
            ]
        );
        let (accounts, groups) = edit(
            &stored,
            AccountEdit::UpdateAccount {
                username: "Alt".into(),
                label: Some(" Second ".into()),
                new_username: Some("Second".into()),
                password: None,
            },
        )
        .unwrap();
        assert_eq!(
            accounts[1],
            ["Second", "Second", "Alt password"].map(String::from)
        );
        assert_eq!(groups[0].1, strings(&["Hero", "Second"]));
    }

    #[test]
    fn deleting_is_all_or_nothing_and_removes_memberships() {
        let stored = document(
            &["Alice", "Bob", "Cara"],
            &[("Party", &["Alice", "Bob", "Cara"])],
        );
        assert_eq!(
            edit(
                &stored,
                AccountEdit::DeleteAccounts {
                    usernames: strings(&["Alice", "Missing"])
                }
            ),
            Err(AccountsError::AccountsNotFound)
        );
        assert_eq!(
            edit(
                &stored,
                AccountEdit::DeleteAccounts {
                    usernames: strings(&["alice"])
                }
            ),
            Err(AccountsError::AccountsNotFound)
        );
        let (accounts, groups) = edit(
            &stored,
            AccountEdit::DeleteAccounts {
                usernames: strings(&[" Alice ", "Bob", "Alice"]),
            },
        )
        .unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0][1], "Cara");
        assert_eq!(groups, [("Party".into(), strings(&["Cara"]))]);
        assert_eq!(
            edit(&stored, AccountEdit::DeleteAccounts { usernames: vec![] }).unwrap(),
            view(&stored)
        );
    }

    #[test]
    fn group_members_are_checked_before_the_group_name() {
        let stored = document(&["Alice", "Bob"], &[("Party", &[])]);
        let cases = [
            (strings(&["Missing"]), "Account not found: Missing"),
            (strings(&["alice"]), "Account not found: alice"),
            (
                strings(&["Alice", " Alice "]),
                "Duplicate group member: Alice",
            ),
        ];
        for (usernames, message) in cases {
            assert_eq!(
                edit(
                    &stored,
                    AccountEdit::CreateGroup {
                        name: "party".into(),
                        usernames: usernames.clone(),
                    }
                )
                .unwrap_err()
                .to_string(),
                message
            );
            assert_eq!(
                edit(
                    &stored,
                    AccountEdit::UpdateGroup {
                        name: "PARTY".into(),
                        new_name: None,
                        usernames: Some(usernames),
                    }
                )
                .unwrap_err()
                .to_string(),
                message
            );
        }
        assert_eq!(
            edit(
                &stored,
                AccountEdit::CreateGroup {
                    name: " party ".into(),
                    usernames: strings(&["Alice"]),
                }
            ),
            Err(AccountsError::GroupNameTaken)
        );
    }

    #[test]
    fn groups_keep_their_place_and_javascript_key_order() {
        let stored = document(
            &["Alice"],
            &[("Party", &["Alice"]), ("Solo", &[]), ("7", &[])],
        );
        let names = |result: Result<View, AccountsError>| {
            let names = result.unwrap().1.into_iter().map(|(name, _)| name);
            names.collect::<Vec<_>>()
        };
        assert_eq!(
            names(edit(
                &stored,
                AccountEdit::CreateGroup {
                    name: " Raid ".into(),
                    usernames: vec![],
                }
            )),
            ["7", "Party", "Solo", "Raid"]
        );
        assert_eq!(
            names(edit(
                &stored,
                AccountEdit::CreateGroup {
                    name: "3".into(),
                    usernames: vec![],
                }
            )),
            ["3", "7", "Party", "Solo"]
        );
        assert_eq!(
            names(edit(
                &stored,
                AccountEdit::UpdateGroup {
                    name: "party".into(),
                    new_name: Some("Crew".into()),
                    usernames: None,
                }
            )),
            ["7", "Crew", "Solo"]
        );
        assert_eq!(
            names(edit(
                &stored,
                AccountEdit::UpdateGroup {
                    name: "Solo".into(),
                    new_name: Some("1".into()),
                    usernames: None,
                }
            )),
            ["1", "7", "Party"]
        );
        let (_, groups) = edit(
            &stored,
            AccountEdit::UpdateGroup {
                name: "PARTY".into(),
                new_name: Some("party".into()),
                usernames: Some(vec![]),
            },
        )
        .unwrap();
        assert_eq!(groups[1], ("party".into(), vec![]));
        assert_eq!(
            names(edit(
                &stored,
                AccountEdit::DeleteGroup {
                    name: " solo ".into()
                }
            )),
            ["7", "Party"]
        );
    }

    #[test]
    fn group_lookups_and_conflicts_follow_the_release_order() {
        let stored = document(&["Alice"], &[("Party", &[]), ("Solo", &[])]);
        assert_eq!(
            edit(
                &stored,
                AccountEdit::UpdateGroup {
                    name: "Missing".into(),
                    new_name: Some(" ".into()),
                    usernames: Some(strings(&["Missing"])),
                }
            ),
            Err(AccountsError::GroupNotFound)
        );
        assert_eq!(
            edit(
                &stored,
                AccountEdit::UpdateGroup {
                    name: "Party".into(),
                    new_name: Some("SOLO".into()),
                    usernames: None,
                }
            ),
            Err(AccountsError::GroupNameTaken)
        );
        assert_eq!(
            edit(
                &stored,
                AccountEdit::DeleteGroup {
                    name: "Missing".into()
                }
            ),
            Err(AccountsError::GroupNotFound)
        );
    }

    #[test]
    fn account_updates_and_deletes_check_their_input_before_reading() {
        let read = || Err(unreadable());
        assert_eq!(
            apply(update(" "), read).map(|_| ()),
            Err(AccountsError::Required("username"))
        );
        assert_eq!(
            apply(
                AccountEdit::DeleteAccounts {
                    usernames: strings(&[""])
                },
                read
            )
            .map(|_| ()),
            Err(AccountsError::Required("username"))
        );
        assert_eq!(apply(update("Hero"), read).map(|_| ()), Err(unreadable()));
        assert_eq!(
            apply(create(" ", "secret"), read).map(|_| ()),
            Err(unreadable())
        );
        assert_eq!(
            apply(
                AccountEdit::CreateGroup {
                    name: " ".into(),
                    usernames: vec![]
                },
                read
            )
            .map(|_| ()),
            Err(unreadable())
        );
    }

    #[test]
    fn edits_return_normalized_accounts() {
        let mut stored = document(
            &["Hero", "hero"],
            &[(" Raiders ", &["Hero", "hero"]), (" ", &[])],
        );
        stored.groups.insert("__proto__".into(), vec![]);
        let (accounts, groups) =
            edit(&stored, AccountEdit::DeleteAccounts { usernames: vec![] }).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(groups, [("Raiders".into(), strings(&["Hero"]))]);
    }
}
