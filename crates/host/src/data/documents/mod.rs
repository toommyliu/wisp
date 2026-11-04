pub(super) mod account_edits;
pub(super) mod account_settings;
pub(crate) mod accounts;
pub(super) mod app_settings;
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

fn integer(raw: &Value) -> Option<f64> {
    raw.as_f64().filter(|value| value.fract() == 0.0)
}

fn newer_version(document: &Map<String, Value>, current: u32, issues: &mut Issues) -> bool {
    match document.get("version").map(integer) {
        None => false,
        Some(Some(version)) if version >= 0.0 => version > f64::from(current),
        Some(_) => {
            issues.reset.push("version");
            false
        }
    }
}

pub(crate) fn is_space(c: char) -> bool {
    c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}')
}

pub(crate) fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}
