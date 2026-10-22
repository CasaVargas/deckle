// SPDX-License-Identifier: GPL-3.0-or-later
//! Entries grouped into books (highlights export design §4–5): overlapping highlights collapse to
//! the latest, notes attach to their highlight, and every Deckle Daily edition shares one book.

use serde::Serialize;
use sha1::{Digest, Sha1};

use crate::parse::{Entry, Kind};

/// The book every Deckle Daily edition goes into, whatever the paper is called.
pub const EDITIONS_KEY: &str = "deckle-daily";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Item {
    Highlight {
        entry: Entry,
        notes: Vec<Entry>,
    },
    /// A note with no highlight to sit under; `of` is the start of its passage when known.
    Note {
        entry: Entry,
        of: Option<String>,
    },
}

impl Item {
    pub fn entry(&self) -> &Entry {
        match self {
            Item::Highlight { entry, .. } | Item::Note { entry, .. } => entry,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Book {
    pub key: String,
    pub title: String,
    pub author: String,
    /// Deckle Daily editions, one `## <date>` section each.
    pub editions: bool,
    pub items: Vec<Item>,
}

fn normalise(s: &str) -> String {
    s.trim().to_lowercase()
}

fn is_edition(e: &Entry) -> bool {
    e.author == "Deckle" && e.title.contains(" — ")
}

/// The date part of an edition's title (`The Morning Deckle — Tuesday, October 20, 2026`).
pub fn edition_date(entry: &Entry) -> Option<&str> {
    is_edition(entry)
        .then(|| entry.title.split_once(" — ").map(|(_, d)| d))
        .flatten()
}

fn book_key(e: &Entry) -> String {
    if is_edition(e) {
        EDITIONS_KEY.to_owned()
    } else {
        format!("{}\u{1F}{}", normalise(&e.title), normalise(&e.author))
    }
}

/// A stable id for an entry: the same highlight from another Kindle, or another export, has the
/// same id; an edited one (other range or text) doesn't.
pub fn entry_id(e: &Entry) -> String {
    let loc = e
        .location
        .map(|(a, b)| format!("{a}-{b}"))
        .unwrap_or_default();
    let mut h = Sha1::new();
    for part in [
        normalise(&e.title),
        normalise(&e.author),
        format!("{:?}", e.kind),
        loc,
        e.text.trim().to_owned(),
    ] {
        h.update(part.as_bytes());
        h.update([0x1F]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// `b` is `a` extended (or trimmed) on the Kindle: the ranges overlap and one passage holds the
/// other. Two different passages that only share a location (a location is ~150 bytes) stay
/// apart.
fn extends(a: &Entry, b: &Entry) -> bool {
    // The edges move with the selection ("life." becomes "life, and …"): compare the words.
    let core = |t: &str| t.trim_matches(|c: char| !c.is_alphanumeric()).to_owned();
    let (ta, tb) = (core(&a.text), core(&b.text));
    let (ta, tb) = (ta.as_str(), tb.as_str());
    let related = ta.contains(tb) || tb.contains(ta);
    match (a.location, b.location) {
        (Some((a0, a1)), Some((b0, b1))) => a.title == b.title && a0 <= b1 && b0 <= a1 && related,
        _ => false,
    }
}

/// Books in the order they first appear; bookmarks are dropped.
pub fn group(entries: Vec<Entry>) -> Vec<Book> {
    let mut books: Vec<(Book, Vec<Entry>)> = Vec::new();
    for e in entries {
        if e.kind == Kind::Bookmark {
            continue;
        }
        let key = book_key(&e);
        let at = match books.iter().position(|(b, _)| b.key == key) {
            Some(i) => i,
            None => {
                let editions = key == EDITIONS_KEY;
                let title = match editions {
                    true => e.title.split_once(" — ").map_or(&*e.title, |(t, _)| t),
                    false => &e.title,
                };
                books.push((
                    Book {
                        key,
                        title: title.trim().to_owned(),
                        author: e.author.clone(),
                        editions,
                        items: Vec::new(),
                    },
                    Vec::new(),
                ));
                books.len() - 1
            }
        };
        books[at].1.push(e);
    }
    books
        .into_iter()
        .map(|(mut book, entries)| {
            book.items = items(entries);
            book
        })
        .collect()
}

fn items(entries: Vec<Entry>) -> Vec<Item> {
    // Editions in order of first appearance, so a book's sections stay chronological.
    let mut editions: Vec<String> = Vec::new();
    for e in &entries {
        if !editions.contains(&e.title) {
            editions.push(e.title.clone());
        }
    }
    let (highlights, notes): (Vec<Entry>, Vec<Entry>) =
        entries.into_iter().partition(|e| e.kind == Kind::Highlight);
    // An extended highlight is a new entry over the old range and text: keep the later one.
    let mut kept: Vec<Entry> = Vec::new();
    for h in highlights {
        kept.retain(|k| !extends(k, &h));
        kept.push(h);
    }
    // A Kindle keeps one note per location: editing or re-saving one appends a new entry and
    // leaves the old, so the latest at each location is the note.
    let mut latest: Vec<Entry> = Vec::new();
    for n in notes {
        latest
            .retain(|k| !(k.title == n.title && k.location.is_some() && k.location == n.location));
        latest.push(n);
    }
    let notes = latest;
    let mut items: Vec<Item> = kept
        .into_iter()
        .map(|entry| Item::Highlight {
            entry,
            notes: Vec::new(),
        })
        .collect();
    for n in notes {
        let home = n.location.and_then(|(at, _)| {
            items.iter().position(|i| {
                let e = i.entry();
                matches!(i, Item::Highlight { .. })
                    && e.title == n.title
                    && e.location.is_some_and(|(a, b)| a <= at && at <= b)
            })
        });
        match home {
            Some(i) => {
                if let Item::Highlight { notes, .. } = &mut items[i] {
                    notes.push(n);
                }
            }
            None => items.push(Item::Note { entry: n, of: None }),
        }
    }
    items.sort_by_key(|i| {
        let e = i.entry();
        let edition = editions.iter().position(|t| *t == e.title);
        (edition, e.location.is_none(), e.location.map(|(a, _)| a))
    });
    items
}
