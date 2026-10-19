// SPDX-License-Identifier: GPL-3.0-or-later
//! Text runs of a monolithic KFX with their KFX positions, the coordinate system X-Ray and
//! Word Wise sidecars use (kfxlib's json-content: run position = pid of its element,
//! occurrence = pid + character offset).

use std::collections::HashMap;

use crate::container::{ContainerError, read_kfx};
use crate::ion::{Sym, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// KFX position of the run's first character.
    pub pid: i64,
    pub text: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TextError {
    #[error(transparent)]
    Container(#[from] ContainerError),
    #[error("KFX has no position map ($265)")]
    NoPositionMap,
}

/// Position of the first occurrence of `needle` inside a run (code-point offset).
pub fn position_of(run: &Run, needle: &str) -> Option<i64> {
    let byte = run.text.find(needle)?;
    Some(run.pid + run.text[..byte].chars().count() as i64)
}

fn resolve_text(v: &Value, contents: &HashMap<String, Vec<String>>) -> Option<String> {
    match v {
        Value::String(t) => Some(t.clone()),
        Value::Struct(r) => {
            let name = r.get("name").and_then(Value::as_sym)?;
            let idx = r.get("$403").and_then(Value::as_int)? as usize;
            contents.get(&name.0)?.get(idx).cloned()
        }
        _ => None,
    }
}

fn walk(v: &Value, contents: &HashMap<String, Vec<String>>, out: &mut HashMap<i64, String>) {
    match v {
        Value::Annotated { value, .. } => walk(value, contents, out),
        Value::List(items) | Value::SExp(items) => {
            items.iter().for_each(|i| walk(i, contents, out))
        }
        Value::Struct(s) => {
            if let (Some(Value::Int(eid)), Some(t)) = (s.get("$155"), s.get("$145"))
                && let Some(text) = resolve_text(t, contents)
            {
                out.entry(*eid).or_insert(text);
            }
            for (_, fv) in s.iter() {
                walk(fv, contents, out);
            }
        }
        _ => {}
    }
}

/// Text runs in reading order and the end position (`erl`) of the book.
pub fn content_runs(kfx: &[u8]) -> Result<(Vec<Run>, i64), TextError> {
    let doc = read_kfx(kfx)?;
    let mut contents: HashMap<String, Vec<String>> = HashMap::new();
    for f in doc.fragments.all(&Sym::new("$145")) {
        if let (Some(fid), Some(list)) = (
            &f.fid,
            f.value
                .as_struct()
                .and_then(|s| s.get("$146"))
                .and_then(Value::as_list),
        ) {
            contents.insert(
                fid.0.clone(),
                list.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
            );
        }
    }
    let mut texts: HashMap<i64, String> = HashMap::new();
    for f in doc.fragments.all(&Sym::new("$259")) {
        walk(&f.value, &contents, &mut texts);
    }
    let pim = doc
        .fragments
        .get_single(&Sym::new("$265"))
        .ok_or(TextError::NoPositionMap)?;
    let entries = pim.value.as_list().ok_or(TextError::NoPositionMap)?;
    let mut runs = Vec::new();
    let mut erl = 0;
    for e in entries {
        let Some(s) = e.as_struct() else { continue };
        let pid = s.get("$184").and_then(Value::as_int).unwrap_or(0);
        let eid = s.get("$185").and_then(Value::as_int).unwrap_or(0);
        erl = erl.max(pid);
        if eid == 0 {
            continue;
        }
        if let Some(text) = texts.get(&eid) {
            erl = erl.max(pid + text.chars().count() as i64);
            runs.push(Run {
                pid,
                text: text.clone(),
            });
        }
    }
    Ok((runs, erl))
}
