// SPDX-License-Identifier: GPL-3.0-or-later
//! The report and its suggestions (readership design §6): over the last editions, how often
//! you got to each section, and a few edits to the paper you can apply with one click.

use deckle_digest::Config;
use deckle_digest::config::{SectionKind, section_key};
use serde::Serialize;

use crate::layout::{Layout, section_at};
use crate::state::State;

/// Editions the report covers.
pub const WINDOW: usize = 14;
/// Opened editions needed before anything is suggested.
pub const MIN_OPENED: usize = 7;
/// Appearances a section needs before it gets a share.
pub const MIN_SEEN: usize = 5;
/// A share at or under this (percent) is "rarely".
pub const RARELY: u8 = 20;
/// Drop only sections starting this far in or later.
pub const LATE: f64 = 2.0 / 3.0;
/// "Where you stop" shows under this (percent).
pub const STOP_UNDER: u8 = 75;
pub const MAX_SUGGESTIONS: usize = 3;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub window: usize,
    pub opened: usize,
    /// Median of how far in the opened editions were read.
    pub median_percent: Option<u8>,
    /// The section that point usually falls in.
    pub stop_section: Option<String>,
    /// In the current `digest.toml` order; the front page is left out.
    pub sections: Vec<SectionRow>,
    pub suggestions: Vec<Suggestion>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionRow {
    pub key: String,
    pub name: String,
    /// Percent of opened editions (that had it) where you got to it; `None` until it has
    /// appeared in [`MIN_SEEN`] editions.
    pub share: Option<u8>,
    /// Editions in the window that had it.
    pub seen_in: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionKind {
    Drop,
    MoveUp,
    Stop,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// `drop:<key>`, `up:<key>` or `stop`.
    pub id: String,
    pub kind: SuggestionKind,
    pub key: String,
    pub title: String,
    pub why: String,
}

const KINDS: [SectionKind; 10] = [
    SectionKind::Reading,
    SectionKind::Weather,
    SectionKind::Local,
    SectionKind::Interests,
    SectionKind::Feeds,
    SectionKind::Shelf,
    SectionKind::Lottery,
    SectionKind::Horoscope,
    SectionKind::Puzzle,
    SectionKind::Onthisday,
];

fn kind_of(key: &str) -> Option<SectionKind> {
    KINDS.into_iter().find(|k| {
        *k != SectionKind::Feeds && *k != SectionKind::Interests && section_key(*k, None) == key
    })
}

/// What the paper prints for a section, for one no edition in the window had yet.
fn paper_name(key: &str) -> String {
    if let Some(name) = key
        .strip_prefix("feeds:")
        .or_else(|| key.strip_prefix("interests:"))
        .or_else(|| key.strip_prefix("local:"))
    {
        return name.to_owned();
    }
    match kind_of(key) {
        Some(SectionKind::Reading) => "Your Reading",
        Some(SectionKind::Weather) => "Weather",
        Some(SectionKind::Local) => "Local",
        Some(SectionKind::Lottery) => "Lottery",
        Some(SectionKind::Horoscope) => "Horoscope",
        Some(SectionKind::Puzzle) => "Puzzle",
        Some(SectionKind::Shelf) => "From Your Shelf",
        Some(SectionKind::Onthisday) => "On This Day",
        _ => key,
    }
    .to_owned()
}

/// The paper's section keys in `cfg`'s order, each feed on its own.
fn current_keys(cfg: &Config) -> Vec<String> {
    cfg.sections
        .iter()
        .flat_map(|k| match k {
            SectionKind::Feeds => cfg
                .feeds
                .iter()
                .map(|f| section_key(*k, Some(&f.name)))
                .collect(),
            // Local, then each other place's local news
            SectionKind::Local => std::iter::once(section_key(*k, None))
                .chain(
                    cfg.local
                        .places
                        .iter()
                        .map(|p| section_key(*k, Some(&p.name))),
                )
                .collect(),
            _ => vec![section_key(*k, None)],
        })
        .collect()
}

fn percent(x: f64) -> u8 {
    (x * 100.0).round().clamp(0.0, 100.0) as u8
}

/// The lower middle of `v` (sorted here).
pub(crate) fn median<T: PartialOrd + Copy>(mut v: Vec<T>) -> Option<T> {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v.get(v.len().checked_sub(1)? / 2).copied()
}

pub fn report(state: &State, cfg: &Config) -> Report {
    let window: Vec<&Layout> = state.editions.values().rev().take(WINDOW).collect();
    let opened_count = window
        .iter()
        .filter(|l| {
            state
                .reads
                .get(&l.date)
                .is_some_and(|m| m.values().any(|r| r.opened))
        })
        .count();
    // Each opened edition with the furthest any Kindle got. An opened read without a position
    // (a format this firmware writes differently) says nothing about how far: left out here,
    // so it can't turn into "you never got there".
    let opened: Vec<(&Layout, i64)> = window
        .iter()
        .filter_map(|l| {
            let reads = state.reads.get(&l.date)?;
            let f = reads
                .values()
                .filter(|r| r.opened)
                // The further of the two: a Paperwhite's `lpr` can be ahead of its `fpr`.
                .filter_map(|r| r.furthest.max(r.last))
                .max()?;
            Some((*l, f))
        })
        .collect();
    let fraction = |l: &Layout, p: i64| p as f64 / l.end.max(1) as f64;
    let mid = median(
        opened
            .iter()
            .map(|(l, f)| OrdF(fraction(l, *f), l, *f))
            .collect(),
    );
    let median_percent = mid.as_ref().map(|m| percent(m.0));
    let stop_section = mid.and_then(|m| section_at(m.1, m.2).map(|s| s.name.clone()));

    let keys = current_keys(cfg);
    let mut rows = Vec::new();
    // (row index, reached, opened-with, median start fraction, median size fraction)
    let mut stats = Vec::new();
    for key in &keys {
        let had: Vec<&Layout> = window
            .iter()
            .copied()
            .filter(|l| l.sections.iter().any(|s| &s.key == key))
            .collect();
        let name = had
            .first()
            .and_then(|l| l.sections.iter().find(|s| &s.key == key))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| paper_name(key));
        let start = |l: &Layout| l.sections.iter().find(|s| &s.key == key).map(|s| s.start);
        let with: Vec<(&Layout, i64)> = opened
            .iter()
            .copied()
            .filter(|(l, _)| start(l).is_some())
            .collect();
        let reached = with
            .iter()
            .filter(|(l, f)| start(l).is_some_and(|s| *f >= s))
            .count();
        let share = (had.len() >= MIN_SEEN && !with.is_empty())
            .then(|| percent(reached as f64 / with.len() as f64));
        let pos = median(
            had.iter()
                .filter_map(|l| Some(fraction(l, start(l)?)))
                .collect(),
        );
        let size = median(
            had.iter()
                .filter_map(|l| {
                    let i = l.sections.iter().position(|s| &s.key == key)?;
                    let next = l.sections.get(i + 1).map_or(l.end, |s| s.start);
                    Some(fraction(l, next - l.sections[i].start))
                })
                .collect(),
        );
        stats.push((reached, with.len(), pos.unwrap_or(0.0), size.unwrap_or(0.0)));
        rows.push(SectionRow {
            key: key.clone(),
            name,
            share,
            seen_in: had.len(),
        });
    }

    let mut suggestions = Vec::new();
    if opened.len() >= MIN_OPENED {
        let mut ranked: Vec<(u8, u8, Suggestion)> = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let Some(share) = row.share else { continue };
            if share > RARELY || row.key == "front" || row.key == "reading" {
                continue;
            }
            let (n, m, pos, size) = stats[i];
            if row.key.starts_with("feeds:") {
                let Some(prev) = i.checked_sub(1).map(|p| &rows[p]) else {
                    continue;
                };
                ranked.push((
                    1,
                    share,
                    Suggestion {
                        id: format!("up:{}", row.key),
                        kind: SuggestionKind::MoveUp,
                        key: row.key.clone(),
                        title: format!("Move {} up", row.name),
                        why: format!(
                            "You got to it on {n} of {m} days, about {}% in. Move it ahead of {}?",
                            percent(pos),
                            prev.name
                        ),
                    },
                ));
            } else if pos >= LATE {
                ranked.push((
                    0,
                    share,
                    Suggestion {
                        id: format!("drop:{}", row.key),
                        kind: SuggestionKind::Drop,
                        key: row.key.clone(),
                        title: format!("Drop {}", row.name),
                        why: format!(
                            "You got to it on {n} of {m} days. It's about {}% of the paper.",
                            percent(size)
                        ),
                    },
                ));
            }
        }
        if let (Some(p), Some(section)) = (median_percent, &stop_section)
            && p < STOP_UNDER
        {
            ranked.push((
                2,
                0,
                Suggestion {
                    id: "stop".into(),
                    kind: SuggestionKind::Stop,
                    key: String::new(),
                    title: "Where you stop".into(),
                    why: format!("You usually stop about {p}% in, in {section}."),
                },
            ));
        }
        ranked.sort_by_key(|(rule, share, _)| (*rule, *share));
        suggestions = ranked
            .into_iter()
            .map(|(_, _, s)| s)
            .filter(|s| !hidden(state, &s.id))
            .take(MAX_SUGGESTIONS)
            .collect();
    }

    Report {
        window: window.len(),
        opened: opened_count,
        median_percent,
        stop_section,
        sections: rows,
        suggestions,
    }
}

/// Dismissed and fewer than [`WINDOW`] editions newer than when it was.
fn hidden(state: &State, id: &str) -> bool {
    state
        .dismissed
        .get(id)
        .is_some_and(|at| state.editions.keys().filter(|d| *d > at).count() < WINDOW)
}

/// Hides `s` for the next [`WINDOW`] editions.
pub fn dismiss(state: &mut State, s: &Suggestion) {
    if let Some(newest) = state.editions.keys().next_back().copied() {
        state.dismissed.insert(s.id.clone(), newest);
    }
}

/// Makes the edit `s` suggests in `cfg`.
pub fn apply(s: &Suggestion, cfg: &mut Config) -> Result<(), String> {
    let feed = s.key.strip_prefix("feeds:");
    match (s.kind, feed) {
        (SuggestionKind::Stop, _) => Err("nothing to apply".into()),
        // another place's local news goes on its own; Local stays
        (SuggestionKind::Drop, None) if s.key.starts_with("local:") => {
            let name = &s.key["local:".len()..];
            let before = cfg.local.places.len();
            cfg.local.places.retain(|p| p.name != name);
            (cfg.local.places.len() < before)
                .then_some(())
                .ok_or_else(|| format!("no place named {name}"))
        }
        (SuggestionKind::Drop, Some(name)) => {
            let before = cfg.feeds.len();
            cfg.feeds.retain(|f| f.name != name);
            (cfg.feeds.len() < before)
                .then_some(())
                .ok_or_else(|| format!("no feed named {name}"))
        }
        (SuggestionKind::Drop, None) => {
            let kind = kind_of(&s.key).ok_or_else(|| format!("unknown section {}", s.key))?;
            cfg.sections.retain(|k| *k != kind);
            Ok(())
        }
        (SuggestionKind::MoveUp, Some(name)) => {
            let i = cfg
                .feeds
                .iter()
                .position(|f| f.name == name)
                .ok_or_else(|| format!("no feed named {name}"))?;
            if i > 0 {
                cfg.feeds.swap(i - 1, i);
                return Ok(());
            }
            match cfg.sections.iter().position(|k| *k == SectionKind::Feeds) {
                Some(j) if j > 0 => {
                    cfg.sections.swap(j - 1, j);
                    Ok(())
                }
                _ => Err(format!("{name} is already first")),
            }
        }
        (SuggestionKind::MoveUp, None) => Err("only feeds move up".into()),
    }
}

/// A fraction with the edition it came from, ordered by the fraction.
#[derive(Clone, Copy)]
struct OrdF<'a>(f64, &'a Layout, i64);

impl PartialEq for OrdF<'_> {
    fn eq(&self, o: &Self) -> bool {
        self.0 == o.0
    }
}

impl PartialOrd for OrdF<'_> {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&o.0)
    }
}
