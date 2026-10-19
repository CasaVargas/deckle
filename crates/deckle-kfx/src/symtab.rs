// SPDX-License-Identifier: GPL-3.0-or-later
//! Ion symbol table: the `$ion` system symbols, the shared `YJ_symbols` table, and local
//! symbols. Mirrors kfxlib's `ion_symbol_table.py`.

use std::collections::HashMap;

use crate::ion::Sym;

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

    pub fn symbol_of(&self, id: u32) -> Option<&str> {
        self.symbols.get(id as usize).and_then(|s| s.as_deref())
    }

    pub fn id_of(&self, sym: &Sym) -> Option<u32> {
        self.ids.get(&sym.0).copied()
    }

    pub fn local_min_id(&self) -> u32 {
        self.local_min_id
    }

    /// Total number of symbols (the value `max_id` would take).
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
        if !self.ids.contains_key(text) {
            self.push(Some(text.to_owned()));
        }
        Sym::new(text)
    }

    pub fn local_symbols(&self) -> Vec<String> {
        self.symbols[self.local_min_id as usize..]
            .iter()
            .map(|s| s.clone().unwrap_or_default())
            .collect()
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::yj_default()
    }
}
