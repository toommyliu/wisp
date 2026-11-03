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

impl Json {
    pub(super) fn equals(&self, value: &Output) -> bool {
        match (self, value) {
            (Json::Bool(left), Output::Bool(right)) => left == right,
            (Json::Number(left), Output::Integer(right)) => *left == f64::from(*right),
            (Json::String(left), Output::String(right)) => left == right,
            (Json::Array(left), Output::Array(right)) => {
                left.len() == right.len() && left.iter().zip(right).all(|(l, r)| l.equals(r))
            }
            (Json::Object(left), Output::Object(right)) => {
                left.len() == right.len()
                    && right
                        .iter()
                        .all(|(key, r)| left.get(key).is_some_and(|l| l.equals(r)))
            }
            _ => false,
        }
    }

    fn take_children(&mut self, into: &mut Vec<Json>) {
        match self {
            Json::Array(items) => into.append(items),
            Json::Object(members) => {
                let members = std::mem::take(members);
                into.extend(members.indices.into_values().map(|(_, value)| value));
                into.extend(members.names.into_values());
            }
            Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_) => {}
        }
    }
}

impl Drop for Json {
    fn drop(&mut self) {
        let mut nested = Vec::new();
        self.take_children(&mut nested);
        while let Some(mut value) = nested.pop() {
            value.take_children(&mut nested);
        }
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

    pub(super) fn get(&self, key: &str) -> Option<&V> {
        match array_index(key) {
            Some(index) => self.indices.get(&index).map(|(_, value)| value),
            None => self.names.get(key),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::data) struct ParseError {
    pub(in crate::data) at: Option<Position>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::data) struct Position {
    pub(in crate::data) line: usize,
    pub(in crate::data) column: usize,
}

pub(super) fn parse(bytes: &[u8]) -> Result<Json, ParseError> {
    let text = String::from_utf8_lossy(bytes);
    let mut parser = Parser { text: &text, at: 0 };
    parser.document().map_err(|failure| ParseError {
        at: match failure {
            Failure::At(at) => Some(position(&text[..at])),
            Failure::Unplaced => None,
        },
    })
}

const LAST_LATIN_1_LEAD_BYTE: u8 = 0xC3;

#[derive(Clone, Copy)]
enum Failure {
    At(usize),
    Unplaced,
}

enum Frame {
    Array(Vec<Json>),
    Object(JsMap<Json>, String),
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn document(&mut self) -> Result<Json, Failure> {
        let mut open = Vec::new();
        'value: loop {
            self.skip_whitespace();
            let mut value = match self.peek() {
                Some(b'[') => {
                    self.at += 1;
                    self.skip_whitespace();
                    if !self.eat(b']') {
                        open.push(Frame::Array(Vec::new()));
                        continue;
                    }
                    Json::Array(Vec::new())
                }
                Some(b'{') => {
                    self.at += 1;
                    self.skip_whitespace();
                    if !self.eat(b'}') {
                        let key = self.key()?;
                        open.push(Frame::Object(JsMap::default(), key));
                        continue;
                    }
                    Json::Object(JsMap::default())
                }
                _ => self.scalar()?,
            };
            loop {
                self.skip_whitespace();
                match open.pop() {
                    None if self.at == self.text.len() => return Ok(value),
                    None => return Err(Failure::At(self.at)),
                    Some(Frame::Array(mut items)) => {
                        items.push(value);
                        if self.eat(b',') {
                            open.push(Frame::Array(items));
                            continue 'value;
                        }
                        self.expect(b']')?;
                        value = Json::Array(items);
                    }
                    Some(Frame::Object(mut members, key)) => {
                        members.insert(key, value);
                        if self.eat(b',') {
                            self.skip_whitespace();
                            let key = self.key()?;
                            open.push(Frame::Object(members, key));
                            continue 'value;
                        }
                        self.expect(b'}')?;
                        value = Json::Object(members);
                    }
                }
            }
        }
    }

    fn key(&mut self) -> Result<String, Failure> {
        if self.peek() != Some(b'"') {
            return Err(Failure::At(self.at));
        }
        let key = self.string()?;
        self.skip_whitespace();
        self.expect(b':')?;
        Ok(key)
    }

    fn scalar(&mut self) -> Result<Json, Failure> {
        match self.peek() {
            Some(b'"') => self.string().map(Json::String),
            Some(b'-' | b'0'..=b'9') => self.number().map(Json::Number),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            _ => Err(Failure::Unplaced),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, Failure> {
        for &expected in word.as_bytes() {
            match self.peek() {
                Some(byte) if byte == expected => self.at += 1,
                Some(b'"' | b'-' | b'0'..=b'9') => return Err(Failure::At(self.at)),
                _ => return Err(Failure::Unplaced),
            }
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<f64, Failure> {
        let start = self.at;
        self.eat(b'-');
        if self.eat(b'0') {
            if self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return Err(Failure::At(self.at));
            }
        } else {
            self.digits()?;
        }
        if self.eat(b'.') {
            self.digits()?;
        }
        if self.eat(b'e') || self.eat(b'E') {
            let _ = self.eat(b'+') || self.eat(b'-');
            self.digits()?;
        }
        Ok(self.text[start..self.at]
            .parse()
            .expect("JSON number syntax is a subset of Rust's float syntax"))
    }

    fn digits(&mut self) -> Result<(), Failure> {
        let start = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        if self.at == start {
            return Err(Failure::At(self.at));
        }
        Ok(())
    }

    fn string(&mut self) -> Result<String, Failure> {
        self.at += 1;
        let mut text = String::new();
        loop {
            let rest = &self.text.as_bytes()[self.at..];
            let plain = rest
                .iter()
                .position(|&byte| byte == b'"' || byte == b'\\' || byte < 0x20)
                .unwrap_or(rest.len());
            text.push_str(&self.text[self.at..self.at + plain]);
            self.at += plain;
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(text);
                }
                Some(b'\\') => {
                    self.at += 1;
                    self.escape(&mut text)?;
                }
                _ => return Err(Failure::At(self.at)),
            }
        }
    }

    fn escape(&mut self, text: &mut String) -> Result<(), Failure> {
        let Some(byte) = self.peek() else {
            return Err(Failure::Unplaced);
        };
        self.at += 1;
        let unescaped = match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let unit = self.code_unit()?;
                let low = (0xD800..0xDC00)
                    .contains(&unit)
                    .then(|| self.low_surrogate())
                    .flatten();
                let decoded = char::decode_utf16(std::iter::once(unit).chain(low));
                text.extend(decoded.map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)));
                return Ok(());
            }
            ..=LAST_LATIN_1_LEAD_BYTE => return Err(Failure::At(self.at - 1)),
            _ => return Err(Failure::Unplaced),
        };
        text.push(unescaped);
        Ok(())
    }

    fn code_unit(&mut self) -> Result<u16, Failure> {
        let mut unit = 0;
        for _ in 0..4 {
            let digit = self.peek().and_then(|byte| char::from(byte).to_digit(16));
            let digit = digit.ok_or(Failure::At(self.at))?;
            unit = unit * 16 + digit as u16;
            self.at += 1;
        }
        Ok(unit)
    }

    fn low_surrogate(&mut self) -> Option<u16> {
        let start = self.at;
        if self.eat(b'\\')
            && self.eat(b'u')
            && let Ok(unit @ 0xDC00..=0xDFFF) = self.code_unit()
        {
            return Some(unit);
        }
        self.at = start;
        None
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        self.at += usize::from(found);
        found
    }

    fn expect(&mut self, byte: u8) -> Result<(), Failure> {
        if self.eat(byte) {
            Ok(())
        } else {
            Err(Failure::At(self.at))
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }
}

fn position(before: &str) -> Position {
    let mut position = Position { line: 1, column: 1 };
    let mut previous = None;
    for c in before.chars() {
        match c {
            '\n' if previous == Some('\r') => {}
            '\n' | '\r' => {
                position = Position {
                    line: position.line + 1,
                    column: 1,
                }
            }
            _ => position.column += c.len_utf16(),
        }
        previous = Some(c);
    }
    position
}

pub(super) enum Output<'a> {
    Bool(bool),
    Integer(u32),
    String(&'a str),
    Array(Vec<Output<'a>>),
    Object(Vec<(&'a str, Output<'a>)>),
}

pub(super) fn stringify(value: &Output) -> Vec<u8> {
    let mut text = String::new();
    write(value, 0, &mut text);
    text.push('\n');
    text.into_bytes()
}

fn write(value: &Output, depth: usize, text: &mut String) {
    match value {
        Output::Bool(value) => text.push_str(if *value { "true" } else { "false" }),
        Output::Integer(value) => text.push_str(&value.to_string()),
        Output::String(value) => quote(value, text),
        Output::Array(items) => {
            let items = items.iter().map(|item| (None, item));
            container(items, depth, ['[', ']'], text);
        }
        Output::Object(fields) => {
            let fields = fields.iter().map(|(key, value)| (Some(*key), value));
            container(fields, depth, ['{', '}'], text);
        }
    }
}

fn container<'a>(
    members: impl ExactSizeIterator<Item = (Option<&'a str>, &'a Output<'a>)>,
    depth: usize,
    [open, close]: [char; 2],
    text: &mut String,
) {
    text.push(open);
    if members.len() == 0 {
        text.push(close);
        return;
    }
    for (index, (key, value)) in members.enumerate() {
        text.push_str(if index == 0 { "\n" } else { ",\n" });
        indent(depth + 1, text);
        if let Some(key) = key {
            quote(key, text);
            text.push_str(": ");
        }
        write(value, depth + 1, text);
    }
    text.push('\n');
    indent(depth, text);
    text.push(close);
}

fn indent(depth: usize, text: &mut String) {
    text.extend(std::iter::repeat_n(' ', depth * 2));
}

fn quote(value: &str, text: &mut String) {
    text.push('"');
    for c in value.chars() {
        match c {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\u{8}' => text.push_str("\\b"),
            '\u{c}' => text.push_str("\\f"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            '\0'..='\u{1f}' => text.push_str(&format!("\\u{:04x}", u32::from(c))),
            _ => text.push(c),
        }
    }
    text.push('"');
}
