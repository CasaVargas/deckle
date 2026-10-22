// SPDX-License-Identifier: GPL-3.0-or-later
//! The highlights in `My Clippings.txt`, as snapshot entries.

use chrono::NaiveDate;
use deckle_digest::shelf::store::HighlightSnap;
use deckle_highlights::{Item, entry_id, group, parse};

/// Every highlight with its notes attached; Deckle Daily editions, bookmarks and notes with no
/// highlight are left out.
pub fn build(clippings: &[u8]) -> Vec<HighlightSnap> {
    let mut out = Vec::new();
    for book in group(parse(clippings).entries) {
        if book.editions {
            continue;
        }
        for item in book.items {
            let Item::Highlight { entry, notes } = item else {
                continue;
            };
            out.push(HighlightSnap {
                id: entry_id(&entry),
                title: book.title.clone(),
                author: book.author.clone(),
                notes: notes.iter().map(|n| n.text.trim().to_owned()).collect(),
                page: entry.page.clone(),
                added: entry
                    .added
                    .date
                    .as_deref()
                    .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
                text: entry.text.trim().to_owned(),
            });
        }
    }
    out
}
