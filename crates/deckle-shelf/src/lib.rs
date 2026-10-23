// SPDX-License-Identifier: GPL-3.0-or-later
//! A snapshot of what a Kindle shares, for From Your Shelf (docs/superpowers/specs/
//! 2026-10-22-from-your-shelf-design.md): the books in progress from their `.yjf`, the
//! highlights from `My Clippings.txt` and the words looked up from `vocab.db`. The page itself
//! is chosen from the snapshot in `deckle_digest::shelf`.

pub mod anki;
pub mod books;
pub mod highlights;
pub mod words;

use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use deckle_core::inventory::DeviceBook;
use deckle_core::vocab::VocabFiles;
use deckle_digest::shelf::store::{KindleSnapshot, Shelf};
use deckle_library::Library;

/// Whether the Kindle's own `book.info.store` fraction can stand in for a percent ("about
/// N%") when the book's end position is unknown. Off until the fraction has been compared with
/// the progress the Kindles show. Checked 2026-10-22: Scribe 2024 shows 16% for a stored
/// 15.05%; Paperwhite 5.19.2, three books, within 4.9 points (`docs/devices.md`). One Scribe
/// book only, since store books aren't visible over MTP.
pub const KINDLE_FRACTION_OK: bool = true;

/// Days of lookups the snapshot keeps; the page prints the last seven.
pub const WORD_DAYS: i64 = 14;

/// What one connection read off a Kindle.
pub struct Read {
    pub books: Vec<DeviceBook>,
    /// `.yjf` left unread past the per-connection cap.
    pub skipped: usize,
    /// `documents/My Clippings.txt`, when the Kindle has one.
    pub clippings: Option<Vec<u8>>,
    /// `system/vocabulary/vocab.db`, when the Kindle shares it.
    pub vocab: Option<VocabFiles>,
    /// Reading the clippings failed (not "there is no file"): keep the last snapshot's.
    pub clippings_failed: bool,
    /// Reading the vocabulary failed: keep the last snapshot's.
    pub vocab_failed: bool,
    /// This Kindle's last snapshot, for what a failed read can't replace.
    pub previous: Option<KindleSnapshot>,
}

/// A Kindle's name for the page: an MTP product string can end in the serial number
/// ("Kindle Scribe G6G3X308536500ME"), which doesn't belong in print.
pub fn friendly_name(raw: &str) -> String {
    let raw = raw.trim();
    // A mass-storage Kindle is named by its firmware line: "Kindle 5.19.2 (467076 040)".
    if let Some(rest) = raw.strip_prefix("Kindle ")
        && rest.starts_with(|c: char| c.is_ascii_digit())
    {
        return "Kindle".to_owned();
    }
    if let Some((head, tail)) = raw.rsplit_once(' ')
        && tail.starts_with(|c: char| c.is_ascii_digit())
        && tail.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return head.trim().to_owned();
    }
    match raw.rsplit_once(' ') {
        Some((head, last))
            if last.len() >= 10
                && last
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && last.bytes().any(|b| b.is_ascii_digit())
                && last.bytes().any(|b| b.is_ascii_uppercase()) =>
        {
            head.trim().to_owned()
        }
        _ => raw.to_owned(),
    }
}

/// The snapshot of one Kindle (`device_key` is its profile key, as library send records name
/// it) and the per-book problems found on the way. A problem with one book never costs the
/// others.
pub fn snapshot(
    read: Read,
    name: &str,
    device_key: &str,
    library: Option<&Library>,
    now: DateTime<Utc>,
) -> (KindleSnapshot, Vec<String>) {
    let (books, mut errors) = books::build(read.books, library, device_key);
    let kept = read.previous.as_ref();
    let (highlights_shared, highlights) = match (&read.clippings, kept) {
        (Some(bytes), _) => (true, highlights::build(bytes)),
        (None, Some(k)) if read.clippings_failed => (k.highlights_shared, k.highlights.clone()),
        (None, _) => (false, Vec::new()),
    };
    let define = words::wordwise_definer();
    let kept_words = || kept.map_or((false, Vec::new()), |k| (k.words_shared, k.words.clone()));
    let (words_shared, words) = match &read.vocab {
        Some(files) => match words::read(files, now - Duration::days(WORD_DAYS), &*define) {
            Ok(w) => (true, w),
            Err(e) => {
                errors.push(format!("vocabulary: {e}"));
                kept_words()
            }
        },
        None if read.vocab_failed => kept_words(),
        None => (false, Vec::new()),
    };
    let snap = KindleSnapshot {
        name: friendly_name(name),
        seen: now.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        books,
        skipped: read.skipped,
        highlights_shared,
        highlights,
        words_shared,
        words,
    };
    (snap, errors)
}

/// What a recorded snapshot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub in_progress: usize,
    pub highlights: bool,
    pub words: usize,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Summary {
    /// "Shelf: 3 books in progress, 1 highlight source, 0 words".
    pub fn describe(&self) -> String {
        format!(
            "Shelf: {} in progress, {}, {}",
            plural(self.in_progress, "book", "books"),
            plural(
                usize::from(self.highlights),
                "highlight source",
                "highlight sources"
            ),
            plural(self.words, "word", "words"),
        )
    }
}

/// How many of the snapshot's books are being read as of `now`: read in the last 60 days, and not
/// yet (or nearly) done. The page's own rules add the hide list.
pub fn in_progress(snap: &KindleSnapshot, now: DateTime<Utc>) -> usize {
    snap.books
        .iter()
        .filter(|b| {
            b.last_read
                .as_deref()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .is_some_and(|t| (now - t.with_timezone(&Utc)).num_days() <= 60)
        })
        .filter(|b| {
            deckle_digest::shelf::select::percent(b, KINDLE_FRACTION_OK)
                .is_none_or(|(p, _)| (1..=97).contains(&p))
        })
        .count()
}

/// Stores `snap` as Kindle `id` in the shelf file at `path`, replacing that Kindle's last
/// snapshot and no other's.
pub fn record(path: &Path, id: &str, snap: KindleSnapshot) -> Result<Summary, String> {
    let summary = Summary {
        in_progress: in_progress(&snap, Utc::now()),
        highlights: snap.highlights_shared,
        words: snap.words.len(),
    };
    let mut shelf = Shelf::load(path);
    shelf.put(id, snap);
    shelf.save(path)?;
    Ok(summary)
}
