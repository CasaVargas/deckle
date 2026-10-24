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
    // A trailing store or library id: `_B0XXXXXXXX`, `-B0XXXXXXXX`, or a longer `_5R5D…` token.
    if let Some((head, id)) = title.rsplit_once(['-', '_'])
        && is_book_id(id)
    {
        title = head;
    }
    let mut title = strip_store_tags(title.trim());
    // "Author, First - Title" (how Deckle names a book) or "Title, The - Author".
    if let Some((left, right)) = title.split_once(" - ") {
        let article_title = [", The", ", A", ", An"].iter().any(|a| left.ends_with(a));
        if article_title {
            title = left;
        } else if left.contains(", ") {
            title = right;
        }
    }
    let title = title.trim();
    // A store book named only by its ASIN has no title to show.
    let is_asin = title.len() == 10
        && title.starts_with("B0")
        && title.bytes().all(|b| b.is_ascii_alphanumeric());
    (!title.is_empty() && !is_asin).then(|| title.to_owned())
}

/// "(Z-Library)" and its kin, left by where a file came from, dropped from the end of a title.
fn strip_store_tags(mut title: &str) -> &str {
    while let Some(head) = title
        .strip_suffix(')')
        .and_then(|t| t.rsplit_once(" ("))
        .and_then(|(head, tag)| {
            let tag = tag.to_lowercase();
            (tag.contains("z-lib") || tag.contains("zlib") || tag.contains("1lib")).then_some(head)
        })
    {
        title = head.trim_end();
    }
    title
}

/// A store ASIN (`B0…`, ten characters) or a longer upper-case id token with a digit.
fn is_book_id(s: &str) -> bool {
    let upper = s
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    let asin = s.len() == 10 && s.starts_with("B0") && s.bytes().all(|b| b.is_ascii_alphanumeric());
    asin || (upper && (10..=40).contains(&s.len()) && s.bytes().any(|b| b.is_ascii_digit()))
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
        // KFX only: an AZW3's `.azw3f` positions aren't KFX ids (From Your Shelf design §2).
        if b.format != "kfx" {
            continue;
        }
        let Some(bytes) = &b.yjf else { continue };
        let title = b
            .title
            .clone()
            .filter(|t| !t.is_empty())
            .or_else(|| title_from_file(&b.file));
        let p = match deckle_readership::parse_yjf(bytes) {
            Ok(p) => p,
            Err(e) => {
                // The file name, never the title: these lines reach `watch` and launchd logs.
                errors.push(format!("{}: {e}", b.file));
                continue;
            }
        };
        let Some(title) = title else { continue };
        let title = strip_store_tags(&title).to_owned();
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
