// SPDX-License-Identifier: GPL-3.0-or-later
//! Writing books into the folder, append only (highlights export design §2.4, §5): a new book
//! gets a new file; a known one gets only the entries it hasn't had, at the end.

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::Serialize;

use crate::book::{Book, Item, entry_id};
use crate::render::{render_items, render_new_file};
use crate::state::{BookState, State};

/// The longest file stem Deckle writes, in bytes.
const MAX_STEM: usize = 120;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pending {
    pub key: String,
    pub title: String,
    pub author: String,
    pub file: String,
    pub total: usize,
    pub new: usize,
    /// The whole new file, or the block that would be appended (empty when nothing is new).
    pub markdown: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Written {
    pub file: String,
    pub new: usize,
    pub created: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ExportReport {
    pub written: Vec<Written>,
    /// `(file, error)`; the error names the full path.
    pub failed: Vec<(String, String)>,
}

/// `title` as a file name that works on macOS and Windows.
pub fn file_name(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let mut stem = clean.trim_matches([' ', '.']).to_owned();
    if stem.len() > MAX_STEM {
        let mut cut = MAX_STEM;
        while !stem.is_char_boundary(cut) {
            cut -= 1;
        }
        stem.truncate(cut);
        stem = stem.trim_end_matches([' ', '.']).to_owned();
    }
    if stem.is_empty() {
        stem = "Untitled".into();
    }
    format!("{stem}.md")
}

/// What one export does for one book.
struct Planned {
    file: String,
    created: bool,
    markdown: String,
    ids: BTreeSet<String>,
    total: usize,
}

fn ids_of(item: &Item) -> Vec<String> {
    match item {
        Item::Highlight { entry, notes } => {
            std::iter::once(entry).chain(notes).map(entry_id).collect()
        }
        Item::Note { entry, .. } => vec![entry_id(entry)],
    }
}

/// Whether `name` is in the folder; with no folder chosen yet, nothing is.
fn on_disk(dir: Option<&Path>, name: &str) -> bool {
    dir.is_some_and(|d| d.join(name).exists())
}

fn taken(name: &str, key: &str, dir: Option<&Path>, state: &State) -> bool {
    state.books.iter().any(|(k, b)| k != key && b.file == name) || on_disk(dir, name)
}

/// A free name for a book Deckle hasn't written yet: the title, then the title and author,
/// then numbered.
fn choose(book: &Book, dir: Option<&Path>, state: &State) -> String {
    let first = file_name(&book.title);
    let mut candidates = vec![first.clone()];
    if !book.author.is_empty() {
        candidates.push(file_name(&format!("{} ({})", book.title, book.author)));
    }
    let stem = first.trim_end_matches(".md").to_owned();
    candidates
        .into_iter()
        .chain((2..).map(|n| format!("{stem} {n}.md")))
        .find(|c| !taken(c, &book.key, dir, state))
        .unwrap_or(first)
}

fn plan(book: &Book, dir: Option<&Path>, state: &State) -> Planned {
    let total = book.items.iter().map(|i| ids_of(i).len()).sum();
    let known = state
        .books
        .get(&book.key)
        .filter(|b| dir.is_none() || on_disk(dir, &b.file));
    let Some(known) = known else {
        // New, or its file is gone: everything, in a fresh file.
        let file = match state.books.get(&book.key) {
            Some(b) => b.file.clone(),
            None => choose(book, dir, state),
        };
        return Planned {
            file,
            created: true,
            markdown: render_new_file(book),
            ids: book.items.iter().flat_map(ids_of).collect(),
            total,
        };
    };
    let mut new_items: Vec<Item> = Vec::new();
    let mut ids = BTreeSet::new();
    for item in &book.items {
        match item {
            Item::Highlight { entry, notes } if known.written.contains(&entry_id(entry)) => {
                // Its passage is in the file already: new notes go at the end, quoting it.
                for n in notes {
                    let id = entry_id(n);
                    if !known.written.contains(&id) {
                        ids.insert(id);
                        new_items.push(Item::Note {
                            entry: n.clone(),
                            of: Some(entry.text.clone()),
                        });
                    }
                }
            }
            item => {
                let item_ids = ids_of(item);
                if item_ids.iter().any(|id| !known.written.contains(id)) {
                    ids.extend(item_ids);
                    new_items.push(item.clone());
                }
            }
        }
    }
    Planned {
        file: known.file.clone(),
        created: false,
        markdown: render_items(book, &new_items.iter().collect::<Vec<_>>()),
        ids,
        total,
    }
}

fn record(state: &mut State, book: &Book, planned: &Planned) {
    let b = state
        .books
        .entry(book.key.clone())
        .or_insert_with(|| BookState {
            file: planned.file.clone(),
            written: BTreeSet::new(),
        });
    if planned.created {
        b.written.clear();
    }
    b.file = planned.file.clone();
    b.written.extend(planned.ids.iter().cloned());
}

/// What [`export`] would write into `dir`, without writing it (`None`: no folder chosen yet).
pub fn pending(books: &[Book], dir: Option<&Path>, state: &State) -> Vec<Pending> {
    let mut state = state.clone();
    books
        .iter()
        .map(|book| {
            let p = plan(book, dir, &state);
            record(&mut state, book, &p);
            Pending {
                key: book.key.clone(),
                title: book.title.clone(),
                author: book.author.clone(),
                file: p.file,
                total: p.total,
                new: p.ids.len(),
                markdown: p.markdown,
            }
        })
        .collect()
}

fn write(path: &Path, created: bool, markdown: &str) -> std::io::Result<()> {
    if created {
        return OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .write_all(markdown.as_bytes());
    }
    let mut f = OpenOptions::new().read(true).append(true).open(path)?;
    let mut last = [0u8; 1];
    let ends_in_newline = match f.seek(SeekFrom::End(-1)) {
        Ok(_) => f.read_exact(&mut last).is_ok() && last[0] == b'\n',
        Err(_) => true, // empty file
    };
    if !ends_in_newline {
        f.write_all(b"\n")?;
    }
    f.write_all(markdown.as_bytes())
}

/// Writes what's new into `dir`, updating `state` only for the files that were written.
pub fn export(books: &[Book], dir: &Path, state: &mut State) -> ExportReport {
    let mut report = ExportReport::default();
    for book in books {
        let p = plan(book, Some(dir), state);
        if p.ids.is_empty() {
            continue;
        }
        let path = dir.join(&p.file);
        match write(&path, p.created, &p.markdown) {
            Ok(()) => {
                record(state, book, &p);
                report.written.push(Written {
                    file: p.file,
                    new: p.ids.len(),
                    created: p.created,
                });
            }
            Err(e) => report
                .failed
                .push((p.file, format!("{}: {e}", path.display()))),
        }
    }
    report
}
