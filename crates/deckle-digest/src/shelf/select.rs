// SPDX-License-Identifier: GPL-3.0-or-later
//! What From Your Shelf prints, chosen from the snapshots (shelf design §4): the books in
//! progress, a past highlight and the week's looked-up words. Pure functions of the stored
//! snapshots and the edition's date, so a rebuild prints the same page.

use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, NaiveDate};
use chrono_tz::Tz;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::Serialize;

use super::store::{BookSnap, HighlightSnap, Shelf, WordSnap};
use crate::EDITION_PREFIX;
use crate::config::ShelfConfig;

/// A book counts as in progress when it was read within this many days.
const IN_PROGRESS_DAYS: i64 = 60;
const NIGHTSTAND_BOOKS: usize = 3;
/// A "nearly done" book is at least this far in.
const NEARLY_DONE: u8 = 90;
/// Past this many days unopened the line says so.
const NOT_OPENED_DAYS: i64 = 14;
/// The nudge goes to a book read less than this many days ago.
const NUDGE_DAYS: i64 = 7;
const HIGHLIGHT_CHARS: std::ops::RangeInclusive<usize> = 40..=600;
const HIGHLIGHT_OLD_DAYS: i64 = 180;
const HIGHLIGHT_MIN_DAYS: i64 = 30;
/// A highlight is not printed again within this many days.
const NO_REPEAT_DAYS: i64 = 90;
const WORD_DAYS: i64 = 7;
const WORDS: usize = 3;

/// One book's latest read: (read on, timestamp, book key, percent, line, Kindle name).
type Read = (NaiveDate, String, String, Option<(u8, bool)>, Line, String);

/// One book on the nightstand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Line {
    pub title: String,
    pub percent: Option<u8>,
    /// The percent is the Kindle's own, "about".
    pub about: bool,
    /// "nearly done", "last read Tuesday", "not opened in 23 days".
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Nightstand {
    pub books: Vec<Line>,
    pub nudge: Option<String>,
    /// The Kindle the newest book was read on.
    pub from: String,
}

/// How far into the book, as a whole percent and whether it is only "about": exact from the
/// book's end position, else the Kindle's own fraction when `fraction_ok`. Rounded down, and
/// 99 until the last position is reached.
pub fn percent(b: &BookSnap, fraction_ok: bool) -> Option<(u8, bool)> {
    if let (Some(last), Some(end)) = (b.last, b.end)
        && end > 0
    {
        let p = (i128::from(last.max(0)) * 100 / i128::from(end)).min(100);
        return Some((if last < end { p.min(99) } else { 100 } as u8, false));
    }
    let f = b.kindle_fraction.filter(|_| fraction_ok)?;
    Some(((f * 100.0).floor().clamp(0.0, 99.0) as u8, true))
}

/// The local date of a UTC `YYYY-MM-DDTHH:MM:SSZ` timestamp.
fn local_date(utc: &str, tz: Tz) -> Option<NaiveDate> {
    Some(
        DateTime::parse_from_rfc3339(utc)
            .ok()?
            .with_timezone(&tz)
            .date_naive(),
    )
}

/// The paper itself: a date-named edition, or one sent by hand as "<Author> - <Title>" with
/// Deckle as the author.
fn is_edition(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.starts_with(EDITION_PREFIX) || name.starts_with("Deckle - ")
}

fn state(days: i64, pct: Option<u8>, read: NaiveDate) -> String {
    if pct.is_some_and(|p| p >= NEARLY_DONE) {
        "nearly done".into()
    } else if days >= NOT_OPENED_DAYS {
        format!("not opened in {days} days")
    } else if days <= 0 {
        "last read today".into()
    } else if days == 1 {
        "last read yesterday".into()
    } else if days <= 6 {
        format!("last read {}", read.format("%A"))
    } else {
        format!("last read {}", read.format("%b %-d"))
    }
}

/// The books in progress across the fresh snapshots, newest first, at most three. The same
/// book on two Kindles is one line, from whichever read it last.
pub fn nightstand(
    shelf: &Shelf,
    cfg: &ShelfConfig,
    date: NaiveDate,
    tz: Tz,
    fraction_ok: bool,
) -> Option<Nightstand> {
    if !cfg.nightstand {
        return None;
    }
    let hide: Vec<String> = cfg.hide.iter().map(|h| h.to_lowercase()).collect();
    // (read on, timestamp, key, percent, state, title, kindle): every book, so the latest read of
    // a book decides whether it is in progress, whichever Kindle it was on.
    let mut found: Vec<Read> = Vec::new();
    let labels = shelf.labels();
    for (id, k) in shelf.fresh(date, tz) {
        for b in &k.books {
            let Some(raw) = b.last_read.as_deref() else {
                continue;
            };
            let Some(read) = local_date(raw, tz) else {
                continue;
            };
            let title_lc = b.title.to_lowercase();
            if is_edition(&b.file) || hide.iter().any(|h| !h.is_empty() && title_lc.contains(h)) {
                continue;
            }
            let days = (date - read).num_days();
            let pct = percent(b, fraction_ok);
            // The same book on two Kindles may carry an ASIN on one and none on the other, so the
            // key is the title and first author.
            let key = format!(
                "{title_lc}\u{1f}{}",
                b.authors
                    .first()
                    .map(|a| a.to_lowercase())
                    .unwrap_or_default()
            );
            let line = Line {
                title: b.title.clone(),
                percent: pct.map(|(p, _)| p),
                about: pct.is_some_and(|(_, a)| a),
                state: state(days, pct.map(|(p, _)| p), read),
            };
            found.push((read, raw.to_owned(), key, pct, line, labels[id].clone()));
        }
    }
    // Newest read first (the timestamp breaks same-day ties), one entry per book, and only then
    // the in-progress rules.
    found.sort_by(|a, b| (b.0, &b.1).cmp(&(a.0, &a.1)));
    let mut seen = BTreeSet::new();
    found.retain(|f| seen.insert(f.2.clone()));
    found.retain(|f| {
        (date - f.0).num_days() <= IN_PROGRESS_DAYS
            && f.3.is_none_or(|(p, _)| (1..=97).contains(&p))
    });
    found.truncate(NIGHTSTAND_BOOKS);
    let newest = found.first()?;
    let days = (date - newest.0).num_days();
    let nudge = (days < NUDGE_DAYS).then(|| nudge(&newest.4));
    let from = newest.5.clone();
    Some(Nightstand {
        books: found.into_iter().map(|f| f.4).collect(),
        nudge,
        from,
    })
}

fn nudge(l: &Line) -> String {
    match l.percent {
        Some(p) if p >= NEARLY_DONE => {
            format!("*{}*: one more evening should finish it.", l.title)
        }
        Some(p) => format!(
            "Pick up *{}* tonight? You left off {}{p}% in.",
            l.title,
            if l.about { "about " } else { "" }
        ),
        None => format!("Pick up *{}* tonight?", l.title),
    }
}

/// How long ago, for a highlight: weeks under two months, months under two years, else years.
pub fn age(added: NaiveDate, date: NaiveDate) -> String {
    let mut months = (date.year() - added.year()) * 12 + date.month() as i32 - added.month() as i32;
    if date.day() < added.day() {
        months -= 1;
    }
    if months < 2 {
        format!("{} weeks", (date - added).num_days() / 7)
    } else if months < 24 {
        format!("{months} months")
    } else {
        format!("{} years", months / 12)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pick {
    pub highlight: HighlightSnap,
    pub age: String,
    /// The Kindle it came from.
    pub from: String,
}

/// A highlight from the past, stable for a given date: 40–600 characters and 180+ days old
/// (else 30+), one with your note first, none shown in the last 90 days. Entries in `shown`
/// dated `date` itself are ignored, so a rebuild the same day prints the same one.
pub fn highlight(shelf: &Shelf, date: NaiveDate, tz: Tz) -> Option<Pick> {
    let labels = shelf.labels();
    let mut all: Vec<(&HighlightSnap, &str, i64)> = Vec::new();
    for (id, k) in shelf.fresh(date, tz) {
        if !k.highlights_shared {
            continue;
        }
        for h in &k.highlights {
            let Some(added) = h.added else { continue };
            let days = (date - added).num_days();
            let recent = shelf
                .shown
                .get(&h.id)
                .is_some_and(|d| *d != date && (date - *d).num_days() < NO_REPEAT_DAYS);
            if HIGHLIGHT_CHARS.contains(&h.text.chars().count()) && !recent {
                all.push((h, labels[id].as_str(), days));
            }
        }
    }
    let mut tier: Vec<_> = all
        .iter()
        .filter(|(_, _, d)| *d >= HIGHLIGHT_OLD_DAYS)
        .cloned()
        .collect();
    if tier.is_empty() {
        tier = all
            .into_iter()
            .filter(|(_, _, d)| *d >= HIGHLIGHT_MIN_DAYS)
            .collect();
    }
    if tier.iter().any(|(h, _, _)| !h.notes.is_empty()) {
        tier.retain(|(h, _, _)| !h.notes.is_empty());
    }
    // Sorting by id makes the pick independent of the order the Kindle listed them in.
    tier.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    if tier.is_empty() {
        return None;
    }
    let mut rng = StdRng::seed_from_u64(date.num_days_from_ce() as u64);
    let (h, from, _) = tier[rng.random_range(0..tier.len())];
    Some(Pick {
        highlight: h.clone(),
        age: age(h.added?, date),
        from: from.to_owned(),
    })
}

/// The week's looked-up words, newest first, one per stem, at most three, each with
/// "<Book>, <Weekday>".
pub fn words(shelf: &Shelf, date: NaiveDate, tz: Tz) -> Vec<(WordSnap, String)> {
    let mut all: Vec<(NaiveDate, &WordSnap)> = Vec::new();
    for (_, k) in shelf.fresh(date, tz) {
        if !k.words_shared {
            continue;
        }
        for w in &k.words {
            if let Some(day) = local_date(&w.at, tz)
                && (0..=WORD_DAYS).contains(&(date - day).num_days())
            {
                all.push((day, w));
            }
        }
    }
    all.sort_by(|a, b| (b.0, &b.1.at).cmp(&(a.0, &a.1.at)));
    let mut stems = BTreeSet::new();
    all.into_iter()
        .filter(|(_, w)| stems.insert(w.stem.clone()))
        .take(WORDS)
        .map(|(day, w)| (w.clone(), format!("{}, {}", w.book, day.format("%A"))))
        .collect()
}
