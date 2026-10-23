// SPDX-License-Identifier: GPL-3.0-or-later
//! The words looked up on a Kindle as a CSV Anki imports (File → Import): one card per word,
//! from its newest lookup, with the short definition, the sentence it was met in and the book.

use std::path::Path;

use chrono::{DateTime, Utc};
use deckle_core::vocab::VocabFiles;
use deckle_digest::shelf::store::WordSnap;

/// Anki's file headers (2.1.55+): the separator, plain text, and the column names it offers
/// when mapping columns to a note type's fields.
const HEADER: &str =
    "#separator:Comma\n#html:false\n#columns:Word,Definition,Sentence,Book,Looked up\n";

fn field(s: &str) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    format!("\"{}\"", flat.replace('"', "\"\""))
}

/// The CSV for `words` (newest first, as [`crate::words::read`] returns them): one row per stem,
/// the newest lookup's, so a word looked up twice is one card.
pub fn csv(words: &[WordSnap]) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out = String::from(HEADER);
    for w in words.iter().filter(|w| seen.insert(w.stem.to_lowercase())) {
        let row = [
            w.word.as_str(),
            w.definition.as_deref().unwrap_or_default(),
            w.usage.as_str(),
            w.book.as_str(),
            w.at.get(..10).unwrap_or_default(),
        ];
        out.push_str(&row.map(field).join(","));
        out.push('\n');
    }
    out
}

/// Every English lookup in `files`, as [`csv`], written over `to` (never appended to). Returns
/// how many words it holds.
pub fn export(
    files: &VocabFiles,
    to: &Path,
    define: &dyn Fn(&str) -> Option<String>,
) -> Result<usize, String> {
    let words = crate::words::read(files, DateTime::<Utc>::UNIX_EPOCH, define)?;
    let text = csv(&words);
    let rows = text.lines().count() - HEADER.lines().count();
    let io = |e: std::io::Error| format!("{}: {e}", to.display());
    if let Some(dir) = to.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let mut tmp = to.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, to))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
        .map_err(io)?;
    Ok(rows)
}
