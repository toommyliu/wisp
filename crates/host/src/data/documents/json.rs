use indexmap::IndexMap;
use std::collections::BTreeMap;

pub(super) enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(JsMap<Json>),
}

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
        todo!()
    }

    pub(super) fn get(&self, key: &str) -> Option<&V> {
        todo!()
    }

    pub(super) fn len(&self) -> usize {
        todo!()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        todo!();
        std::iter::empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ParseError {
    pub(super) at: Option<Position>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Position {
    pub(super) line: usize,
    pub(super) column: usize,
}

pub(super) fn parse(bytes: &[u8]) -> Result<Json, ParseError> {
    todo!()
}

pub(super) enum Output<'a> {
    Bool(bool),
    Integer(u32),
    String(&'a str),
    Array(Vec<Output<'a>>),
    Object(Vec<(&'a str, Output<'a>)>),
}

pub(super) fn stringify(value: &Output) -> Vec<u8> {
    todo!()
}
