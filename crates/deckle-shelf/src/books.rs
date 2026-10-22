// SPDX-License-Identifier: GPL-3.0-or-later
//! The books on a Kindle with a `.yjf`, as snapshot entries.

use deckle_core::inventory::DeviceBook;
use deckle_digest::shelf::store::BookSnap;
use deckle_library::freshness::Current;
use deckle_library::{Library, shelf::shelve};

/// A title read off a file name when nothing else names the book: the extension, a trailing
/// `_EBOK`/`_PDOC` and a trailing `-B0XXXXXXXX`/`_B0XXXXXXXX` ASIN stripped.
pub fn title_from_file(file: &str) -> Option<String> {
    let name = file.rsplit('/').next().unwrap_or(file);
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let mut title = stem;
    for suffix in ["_EBOK", "_PDOC"] {
        title = title.strip_suffix(suffix).unwrap_or(title);
    }
    if let Some((head, asin)) = title.rsplit_once(['-', '_'])
        && asin.len() == 10
        && asin.starts_with("B0")
        && asin.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        title = head;
    }
    let title = title.trim();
    // A store book named only by its ASIN has no title to show.
    let is_asin = title.len() == 10
        && title.starts_with("B0")
        && title.bytes().all(|b| b.is_ascii_alphanumeric());
    (!title.is_empty() && !is_asin).then(|| title.to_owned())
}

/// The snapshot entry of each book that has a readable `.yjf` and a title, and one message per
/// `.yjf` that couldn't be read.
pub fn build(
    books: Vec<DeviceBook>,
    library: Option<&Library>,
    device_key: &str,
) -> (Vec<BookSnap>, Vec<String>) {
    let current = Current {
        xray_engine: 0,
        wordwise_db: None,
    };
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for shelved in shelve(books, library, device_key, &current) {
        let b = shelved.book;
        let Some(bytes) = &b.yjf else { continue };
        let title = b
            .title
            .clone()
            .filter(|t| !t.is_empty())
            .or_else(|| title_from_file(&b.file));
        let label = title.clone().unwrap_or_else(|| b.file.clone());
        let p = match deckle_readership::parse_yjf(bytes) {
            Ok(p) => p,
            Err(e) => {
                errors.push(format!("{label}: {e}"));
                continue;
            }
        };
        let Some(title) = title else { continue };
        out.push(BookSnap {
            file: b.file,
            title,
            authors: b.authors,
            asin: b.asin,
            last: p.last,
            furthest: p.furthest,
            last_read: p.last_read,
            end: b.end,
            kindle_fraction: p.kindle_fraction,
            words_read: p.words_read,
            wpm: p.wpm,
        });
    }
    (out, errors)
}
