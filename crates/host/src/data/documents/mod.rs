pub(super) mod account_edits;
pub(super) mod account_settings;
pub(crate) mod accounts;
mod json;
#[cfg(test)]
mod tests;

use serde_json::{Map, Value};

pub(super) enum Loaded<T> {
    Missing,
    Unreadable(Unreadable),
    Decoded(Decoded<T>),
}

pub(super) enum Unreadable {
    Malformed { line: usize, column: usize },
    NotAnObject,
}

pub(super) struct Decoded<T> {
    pub(super) value: T,
    pub(super) issues: Issues,
}

#[derive(Default)]
pub(super) struct Issues {
    pub(super) reset: Vec<&'static str>,
    pub(super) ignored: Vec<(&'static str, usize)>,
}

fn decode_document<T>(
    file: Option<&[u8]>,
    decode: impl FnOnce(&Map<String, Value>) -> Decoded<T>,
) -> Loaded<T> {
    let Some(bytes) = file else {
        return Loaded::Missing;
    };
    match &json::parse(bytes) {
        Ok(Value::Object(document)) => Loaded::Decoded(decode(document)),
        Ok(_) => Loaded::Unreadable(Unreadable::NotAnObject),
        Err(error) => Loaded::Unreadable(Unreadable::Malformed {
            line: error.line(),
            column: error.column(),
        }),
    }
}

fn trim(text: &str) -> &str {
    text.trim_matches(|c: char| c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}'))
}
