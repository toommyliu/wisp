mod account_settings;
mod accounts;
mod json;
#[cfg(test)]
mod tests;

use json::{JsMap, Json, ParseError};

pub(super) enum Loaded<T> {
    Missing,
    Unreadable(Unreadable),
    Decoded(Decoded<T>),
}

pub(super) enum Unreadable {
    Malformed(ParseError),
    NotAnObject,
}

pub(super) struct Decoded<T> {
    pub(super) value: T,
    pub(super) issues: Issues,
}

#[derive(Debug, Default)]
pub(super) struct Issues {
    reset: Vec<&'static str>,
    ignored: Vec<(&'static str, usize)>,
}

fn decode_document<T>(
    file: Option<&[u8]>,
    decode: impl FnOnce(&JsMap<Json>) -> Decoded<T>,
) -> Loaded<T> {
    todo!()
}

fn trim(text: &str) -> &str {
    todo!()
}
