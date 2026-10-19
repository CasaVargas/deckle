// SPDX-License-Identifier: GPL-3.0-or-later
//! Moves inline storyline text into `$145` content fragments (kfxlib `kpf_collect_content_strings`).

use super::Book;
use crate::consts::MAX_CONTENT_FRAGMENT_SIZE;
use crate::fragment::Fragment;
use crate::ion::{Struct, Sym, Value};

/// Story names (`$176`) referenced by a section, in first-seen order
/// (kfxlib `extract_section_story_names`).
pub fn section_story_names(book: &Book, section: &Sym) -> Vec<Sym> {
    let mut names = Vec::new();
    if let Some(f) = book.fragments.get(&Sym::new("$260"), Some(section)) {
        collect_story_names(&f.value, &mut names);
    }
    names
}

fn collect_story_names(v: &Value, out: &mut Vec<Sym>) {
    match v {
        Value::Annotated { value, .. } => collect_story_names(value, out),
        Value::List(items) | Value::SExp(items) => {
            items.iter().for_each(|i| collect_story_names(i, out))
        }
        Value::Struct(s) => {
            for (k, fv) in s.iter() {
                if k.as_str() == "$176" {
                    if let Value::Symbol(name) = fv
                        && !out.contains(name)
                    {
                        out.push(name.clone());
                    }
                } else {
                    collect_story_names(fv, out);
                }
            }
        }
        _ => {}
    }
}

struct Collector {
    chunks: Vec<(Sym, Vec<Value>)>,
    current_size: usize,
}

impl Collector {
    fn take_string(&mut self, book: &mut Book, text: String) -> Value {
        if self.chunks.is_empty() || self.current_size >= MAX_CONTENT_FRAGMENT_SIZE {
            let name = book
                .symtab
                .create_local(&format!("content_{}", self.chunks.len() + 1));
            self.chunks.push((name, Vec::new()));
            self.current_size = 0;
        }
        let (name, list) = self.chunks.last_mut().unwrap();
        self.current_size += text.len();
        list.push(Value::String(text));
        let mut s = Struct::new();
        s.insert("name", Value::Symbol(name.clone()));
        s.insert("$403", Value::Int((list.len() - 1) as i64));
        Value::Struct(s)
    }

    fn walk(&mut self, book: &mut Book, v: &mut Value) {
        match v {
            Value::Annotated { value, .. } => self.walk(book, value),
            Value::List(items) | Value::SExp(items) => {
                items.iter_mut().for_each(|i| self.walk(book, i))
            }
            Value::Struct(s) => {
                for (k, fv) in s.0.iter_mut() {
                    if k.as_str() == "$145"
                        && let Value::String(text) = fv
                    {
                        let text = std::mem::take(text);
                        *fv = self.take_string(book, text);
                    } else {
                        self.walk(book, fv);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Replaces every inline `$145` string in the storylines (in reading order) with a reference
/// into a `$145` content fragment of at most 8192 UTF-8 bytes. Returns whether any text was found.
pub fn create_content_fragments(book: &mut Book) -> bool {
    let mut collector = Collector {
        chunks: Vec::new(),
        current_size: 0,
    };
    let story_type = Sym::new("$259");
    for section in book.ordered_section_names() {
        for story in section_story_names(book, &section) {
            let Some(idx) = book
                .fragments
                .0
                .iter()
                .position(|f| f.ftype == story_type && f.fid.as_ref() == Some(&story))
            else {
                continue;
            };
            let mut value = std::mem::replace(&mut book.fragments.0[idx].value, Value::Null);
            collector.walk(book, &mut value);
            book.fragments.0[idx].value = value;
        }
    }
    let has_text = !collector.chunks.is_empty();
    for (name, list) in collector.chunks {
        let mut s = Struct::new();
        s.insert("name", Value::Symbol(name.clone()));
        s.insert("$146", Value::List(list));
        book.push_fragment(Fragment {
            ftype: Sym::new("$145"),
            fid: Some(name),
            value: Value::Struct(s),
        });
    }
    has_text
}
