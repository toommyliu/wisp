use indexmap::IndexMap;
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn parse(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    serde_json::from_str(&String::from_utf8_lossy(bytes))
}

pub(super) fn stringify(value: &Value) -> Vec<u8> {
    let mut text = serde_json::to_vec_pretty(value).expect("a JSON value always serializes");
    text.push(b'\n');
    text
}

pub(super) fn same(raw: &Value, kept: &Value) -> bool {
    match (raw, kept) {
        (Value::Number(raw), Value::Number(kept)) => {
            raw.as_f64().map(f64::to_bits) == kept.as_f64().map(f64::to_bits)
        }
        (Value::Array(raw), Value::Array(kept)) => {
            raw.len() == kept.len() && raw.iter().zip(kept).all(|(raw, kept)| same(raw, kept))
        }
        (Value::Object(raw), Value::Object(kept)) => {
            raw.len() == kept.len()
                && kept
                    .iter()
                    .all(|(key, kept)| raw.get(key).is_some_and(|raw| same(raw, kept)))
        }
        _ => raw == kept,
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct JsMap<V> {
    indices: BTreeMap<u32, (String, V)>,
    names: IndexMap<String, V>,
}

impl<V> Default for JsMap<V> {
    fn default() -> Self {
        Self {
            indices: BTreeMap::new(),
            names: IndexMap::new(),
        }
    }
}

impl<V> JsMap<V> {
    pub(super) fn insert(&mut self, key: String, value: V) {
        match array_index(&key) {
            Some(index) => {
                self.indices.insert(index, (key, value));
            }
            None => {
                self.names.insert(key, value);
            }
        }
    }

    pub(super) fn len(&self) -> usize {
        self.indices.len() + self.names.len()
    }

    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        let indices = self.indices.values_mut().map(|(_, value)| value);
        indices.chain(self.names.values_mut())
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        let indices = self
            .indices
            .values()
            .map(|(key, value)| (key.as_str(), value));
        indices.chain(self.names.iter().map(|(key, value)| (key.as_str(), value)))
    }
}

impl<V> FromIterator<(String, V)> for JsMap<V> {
    fn from_iter<I: IntoIterator<Item = (String, V)>>(entries: I) -> Self {
        let mut map = Self::default();
        for (key, value) in entries {
            map.insert(key, value);
        }
        map
    }
}

fn array_index(key: &str) -> Option<u32> {
    let canonical =
        key == "0" || (!key.starts_with('0') && key.bytes().all(|byte| byte.is_ascii_digit()));
    canonical
        .then(|| key.parse().ok())
        .flatten()
        .filter(|&index| index != u32::MAX)
}
