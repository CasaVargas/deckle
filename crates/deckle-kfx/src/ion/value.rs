// SPDX-License-Identifier: GPL-3.0-or-later
use std::fmt;

/// An Ion symbol, held as its text (e.g. `$490` or `content_1`).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sym(pub String);

impl Sym {
    pub fn new(text: &str) -> Self {
        Sym(text.to_owned())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Sym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Sym {
    fn from(s: &str) -> Self {
        Sym::new(s)
    }
}

/// Arbitrary-precision decimal as coefficient × 10^exponent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decimal {
    pub coefficient: i128,
    pub exponent: i32,
}

/// Ion timestamp with the precision information needed to re-encode it exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timestamp {
    /// `None` encodes the unknown offset (`-00:00`).
    pub offset_minutes: Option<i32>,
    pub year: u32,
    pub month: Option<u32>,
    pub day: Option<u32>,
    pub hour: Option<u32>,
    pub minute: Option<u32>,
    pub second: Option<u32>,
    /// Fractional seconds as (exponent, coefficient), e.g. (-3, 250) for .250.
    pub fraction: Option<(i32, i128)>,
}

/// Insertion-ordered struct. Assigning an existing key keeps its position (like Python's
/// `OrderedDict`), which matters for byte-exact re-encoding.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Struct(pub Vec<(Sym, Value)>);

impl Struct {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k.0 == key).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.0.iter_mut().find(|(k, _)| k.0 == key).map(|(_, v)| v)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    /// Replaces the value of an existing key in place, or appends a new field.
    pub fn insert(&mut self, key: impl Into<Sym>, value: Value) {
        let key = key.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let idx = self.0.iter().position(|(k, _)| k.0 == key)?;
        Some(self.0.remove(idx).1)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&Sym, &Value)> {
        self.0.iter().map(|(k, v)| (k, v))
    }
}

/// An Ion value. Annotations wrap a value; an annotated value is never itself annotated.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Decimal(Decimal),
    Timestamp(Timestamp),
    Symbol(Sym),
    String(String),
    Clob(Vec<u8>),
    Blob(Vec<u8>),
    List(Vec<Value>),
    SExp(Vec<Value>),
    Struct(Struct),
    Annotated { annots: Vec<Sym>, value: Box<Value> },
}

impl Value {
    pub fn as_struct(&self) -> Option<&Struct> {
        match self {
            Value::Struct(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_struct_mut(&mut self) -> Option<&mut Struct> {
        match self {
            Value::Struct(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&Vec<Value>> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }
    pub fn as_list_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_sym(&self) -> Option<&Sym> {
        match self {
            Value::Symbol(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }
    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Value::Blob(b) => Some(b),
            _ => None,
        }
    }
    /// The value with any annotation wrapper removed.
    pub fn unannotated(&self) -> &Value {
        match self {
            Value::Annotated { value, .. } => value,
            v => v,
        }
    }
    pub fn sym(text: &str) -> Value {
        Value::Symbol(Sym::new(text))
    }
    pub fn string(text: &str) -> Value {
        Value::String(text.to_owned())
    }
}
