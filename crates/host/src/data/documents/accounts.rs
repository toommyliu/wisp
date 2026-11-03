use super::json::{JsMap, Json};
use super::{Decoded, Loaded};

pub(super) struct AccountsDocument {
    pub(super) accounts: Vec<StoredAccount>,
    pub(super) groups: JsMap<Vec<String>>,
}

pub(super) struct StoredAccount {
    pub(super) label: String,
    pub(super) username: String,
    pub(super) password: String,
}

pub(super) fn decode(file: Option<&[u8]>) -> Loaded<AccountsDocument> {
    todo!()
}

pub(super) fn encode(document: &AccountsDocument) -> Vec<u8> {
    todo!()
}

fn from_object(raw: &JsMap<Json>) -> Decoded<AccountsDocument> {
    todo!()
}
