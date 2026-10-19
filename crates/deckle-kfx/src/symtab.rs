// SPDX-License-Identifier: GPL-3.0-or-later
//! Ion symbol table: the `$ion` system symbols, the shared `YJ_symbols` table, and local
//! symbols. Mirrors kfxlib's `ion_symbol_table.py`.
//!
//! `YJ_symbols` is purely positional: entry *i* is the literal text `$(10+i)`, so no catalog
//! file is needed; a symbol written `$N` always means id N, even beyond the table.

use std::collections::HashMap;

use crate::ion::{IonError, Struct, Sym, Value};

/// The nine Ion system symbols (ids 1..=9).
pub const SYSTEM_SYMBOLS: [&str; 9] = [
    "$ion",
    "$ion_1_0",
    "$ion_symbol_table",
    "name",
    "version",
    "imports",
    "symbols",
    "max_id",
    "$ion_shared_symbol_table",
];

/// Version of the shared `YJ_symbols` table we emit.
pub const YJ_SYMBOLS_VERSION: u32 = 10;
/// Number of entries in `YJ_symbols` version 10 (`$10` .. `$859`).
pub const YJ_SYMBOLS_LEN: u32 = 850;
/// First symbol id of the shared table.
pub const YJ_FIRST_ID: u32 = 10;
/// Number of system symbols that a KFX container adds to the stored import `max_id`.
pub const SYSTEM_SYMBOL_COUNT: u32 = SYSTEM_SYMBOLS.len() as u32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub name: String,
    pub version: u32,
    pub max_id: u32,
}

#[derive(Clone, Debug)]
pub struct SymbolTable {
    symbols: Vec<Option<String>>, // index 0 unused; id == index
    ids: HashMap<String, u32>,
    imports: Vec<Import>,
    local_min_id: u32,
}

/// Parses `$N` into N.
fn dollar_id(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('$')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

impl SymbolTable {
    /// System symbols plus the full `YJ_symbols` v10 table; locals start at id 860.
    pub fn yj_default() -> Self {
        Self::with_yj_max_id(YJ_SYMBOLS_LEN)
    }

    /// Like [`yj_default`](Self::yj_default) but importing `max_id` entries of `YJ_symbols`;
    /// ids beyond the known 850 are unnamed placeholders, as in kfxlib.
    pub fn with_yj_max_id(max_id: u32) -> Self {
        let mut t = SymbolTable {
            symbols: vec![None],
            ids: HashMap::new(),
            imports: Vec::new(),
            local_min_id: 1,
        };
        for s in SYSTEM_SYMBOLS {
            t.push(Some(s.to_owned()));
        }
        t.import_yj(max_id);
        t
    }

    /// Builds a table from a decoded `$ion_symbol_table` struct as stored in a KDF, where the
    /// import `max_id` is the raw YJ count.
    pub fn from_kdf_table(table: &Value) -> Result<Self, IonError> {
        Self::from_table(table, 0)
    }

    /// Builds a table from a decoded `$ion_symbol_table` struct as stored in a KFX container,
    /// where the import `max_id` was inflated by the system symbol count.
    pub fn from_container_table(table: &Value) -> Result<Self, IonError> {
        Self::from_table(table, SYSTEM_SYMBOL_COUNT)
    }

    fn from_table(table: &Value, import_adjust: u32) -> Result<Self, IonError> {
        let s = table
            .unannotated()
            .as_struct()
            .ok_or_else(|| IonError::Malformed("symbol table is not a struct".into()))?;
        let mut max_id = YJ_SYMBOLS_LEN;
        if let Some(imports) = s.get("imports").and_then(Value::as_list) {
            for imp in imports {
                let imp = imp
                    .as_struct()
                    .ok_or_else(|| IonError::Malformed("import is not a struct".into()))?;
                let name = imp.get("name").and_then(Value::as_str).unwrap_or_default();
                if name != "YJ_symbols" {
                    return Err(IonError::Malformed(format!("unknown shared table {name}")));
                }
                if let Some(m) = imp.get("max_id").and_then(Value::as_int) {
                    max_id = (m as u32).saturating_sub(import_adjust);
                }
            }
        }
        let mut t = Self::with_yj_max_id(max_id);
        if let Some(symbols) = s.get("symbols").and_then(Value::as_list) {
            for sym in symbols {
                match sym {
                    Value::String(text) => {
                        t.push(Some(text.clone()));
                    }
                    Value::Null => {
                        t.push(None);
                    }
                    other => {
                        return Err(IonError::Malformed(format!("bad local symbol {other:?}")));
                    }
                }
            }
        }
        // kfxlib only logs a mismatch between the stored max_id and the symbol count.
        Ok(t)
    }

    fn push(&mut self, sym: Option<String>) -> u32 {
        let id = self.symbols.len() as u32;
        if let Some(s) = &sym {
            self.ids.entry(s.clone()).or_insert(id);
        }
        self.symbols.push(sym);
        id
    }

    fn import_yj(&mut self, max_id: u32) {
        for i in 0..max_id {
            let text = if i < YJ_SYMBOLS_LEN {
                Some(format!("${}", YJ_FIRST_ID + i))
            } else {
                None
            };
            self.push(text);
        }
        self.imports.push(Import {
            name: "YJ_symbols".into(),
            version: YJ_SYMBOLS_VERSION,
            max_id,
        });
        self.local_min_id = self.symbols.len() as u32;
    }

    /// Text of a defined symbol id. Unknown ids yield `None`; callers that mirror kfxlib's
    /// leniency fall back to the `$N` placeholder.
    pub fn symbol_of(&self, id: u32) -> Option<&str> {
        self.symbols.get(id as usize).and_then(|s| s.as_deref())
    }

    /// Id of a symbol. `$N` always resolves to N (kfxlib `get_id`).
    pub fn id_of(&self, sym: &Sym) -> Option<u32> {
        if let Some(id) = dollar_id(&sym.0) {
            return Some(id);
        }
        self.ids.get(&sym.0).copied()
    }

    pub fn local_min_id(&self) -> u32 {
        self.local_min_id
    }

    /// Total number of symbols (the value `max_id` takes).
    pub fn len(&self) -> usize {
        self.symbols.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn imports(&self) -> &[Import] {
        &self.imports
    }

    /// Adds a local symbol (idempotent) and returns it.
    pub fn create_local(&mut self, text: &str) -> Sym {
        if !self.ids.contains_key(text) && dollar_id(text).is_none() {
            self.push(Some(text.to_owned()));
        }
        Sym::new(text)
    }

    pub fn is_local(&self, sym: &Sym) -> bool {
        self.id_of(sym).is_some_and(|id| id >= self.local_min_id)
    }

    pub fn local_symbols(&self) -> Vec<String> {
        self.symbols[self.local_min_id as usize..]
            .iter()
            .map(|s| s.clone().unwrap_or_default())
            .collect()
    }

    /// Drops all local symbols and defines `locals` in order from `local_min_id`.
    pub fn replace_locals(&mut self, locals: Vec<String>) {
        for s in self.symbols.drain(self.local_min_id as usize..).flatten() {
            self.ids.remove(&s);
        }
        for s in locals {
            self.push(Some(s));
        }
    }

    /// The `$ion_symbol_table::{max_id, imports, symbols}` value in raw (KDF) form.
    pub fn import_fragment(&self) -> Value {
        let mut s = Struct::new();
        s.insert("max_id", Value::Int(self.len() as i64));
        s.insert(
            "imports",
            Value::List(
                self.imports
                    .iter()
                    .map(|i| {
                        let mut imp = Struct::new();
                        imp.insert("name", Value::String(i.name.clone()));
                        imp.insert("version", Value::Int(i64::from(i.version)));
                        imp.insert("max_id", Value::Int(i64::from(i.max_id)));
                        Value::Struct(imp)
                    })
                    .collect(),
            ),
        );
        s.insert(
            "symbols",
            Value::List(
                self.local_symbols()
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
        Value::Struct(s)
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::yj_default()
    }
}
