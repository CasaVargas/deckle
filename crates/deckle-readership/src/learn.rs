// SPDX-License-Identifier: GPL-3.0-or-later
//! Learning (interests design §5): what the paper notices about your reading. Stories you jumped
//! back from, text you highlighted in the paper and authors on your shelf become candidate
//! subjects in `interests.json`; one that recurs becomes a suggestion. Nothing here changes the
//! paper: `accept` is the only writer, and only a click calls it.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Days, NaiveDate, Utc};
use deckle_digest::Config;
use deckle_digest::config::{InterestConfig, SectionKind};
use deckle_digest::shelf::store::{Shelf, seen_on};
use serde::{Deserialize, Serialize};

use crate::layout::story_at;
use crate::state::State;

/// Days a dismissed subject stays off the list.
pub const DISMISSED_DAYS: u64 = 90;
/// Days since a book was last read for it to count as in progress on the shelf.
const SHELF_DAYS: i64 = 60;
/// How much of a highlight is kept as the hit's title.
const CLIP_CHARS: usize = 80;
/// Suggestions shown at most.
const TOP: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Signal {
    Tap,
    Highlight,
    Shelf,
}

/// One story (or highlight, or shelf day) that mentioned a candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    pub date: NaiveDate,
    pub section: String,
    pub title: String,
    pub signal: Signal,
}

/// A name and search the editor gave a candidate (increment 3); parsed now so files carry it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named {
    pub name: String,
    pub query: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Candidate {
    /// The spelling first seen.
    pub display: String,
    pub hits: Vec<Hit>,
    pub named: Option<Named>,
}

/// `interests.json`: candidates by key (the phrase, lowercase), and what you said about them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Learning {
    pub candidates: BTreeMap<String, Candidate>,
    pub dismissed: BTreeMap<String, NaiveDate>,
    pub accepted: BTreeMap<String, NaiveDate>,
}

impl Learning {
    /// The file at `path`; a missing or unreadable file is an empty store.
    pub fn load(path: &Path) -> Learning {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Moves a candidate and its acceptance or dismissal to a new key (a renamed interest).
    /// Returns whether anything was there to move.
    pub fn rename(&mut self, old: &str, new: &str) -> bool {
        let mut moved = false;
        if let Some(c) = self.candidates.remove(old) {
            self.candidates.insert(new.to_owned(), c);
            moved = true;
        }
        if let Some(d) = self.accepted.remove(old) {
            self.accepted.insert(new.to_owned(), d);
            moved = true;
        }
        if let Some(d) = self.dismissed.remove(old) {
            self.dismissed.insert(new.to_owned(), d);
            moved = true;
        }
        moved
    }

    /// Beside the paper's `readership.json`: `papers/<slug>/interests.json`, or next to the
    /// main paper's state.
    pub fn path_for(cfg: &Config) -> PathBuf {
        cfg.state_file("interests.json")
            .unwrap_or_else(|| crate::state::state_path().with_file_name("interests.json"))
    }
}

const BUILTIN: &[&str] = &[
    "breaking",
    "opinion",
    "live",
    "watch",
    "photos",
    "video",
    "update",
    "report",
    "news",
    "today",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Words a run may start with only because they start a sentence; dropped from its front.
const LEADERS: &[&str] = &[
    "why", "how", "what", "when", "where", "who", "whose", "which", "the", "a", "an", "this",
    "that", "these", "those", "as", "in", "on", "at", "of", "for", "after", "before", "inside",
    "outside", "behind", "over", "under", "meet", "watch", "see", "read", "your", "my", "our",
    "his", "her", "its", "their", "it", "is", "are", "was", "were", "will", "can", "could",
    "should", "would", "do", "does", "did", "not", "new", "big", "top", "with", "from", "to",
    "and", "but", "or", "if", "here", "there", "now", "then", "yes", "no",
];

/// Lowercase words allowed inside a run of capitalised words ("Trial of Alec Baldwin").
const JOINERS: &[&str] = &["of", "the", "and", "de", "for"];

fn capitalised(token: &str) -> bool {
    token.chars().next().is_some_and(char::is_uppercase)
}

fn clean(token: &str) -> &str {
    token.trim_matches(|c: char| !c.is_alphanumeric())
}

/// Candidate phrases in `text` (design §5.3): runs of capitalised words (joiners allowed
/// inside) and single capitalised words of four or more letters, as `(key, display)`, each once.
pub fn phrases(text: &str) -> Vec<(String, String)> {
    // "Headline - Outlet": the tag after the last separator is the outlet, not a subject
    let text = [" - ", " – ", " — ", " | "]
        .iter()
        .filter_map(|sep| text.rsplit_once(sep))
        .filter(|(head, tail)| {
            let n = tail.split_whitespace().count();
            (1..=6).contains(&n) && head.split_whitespace().count() >= 3
        })
        .map(|(head, _)| head)
        .min_by_key(|head| head.len())
        .unwrap_or(text);
    let raw: Vec<&str> = text.split_whitespace().collect();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut run: Vec<&str> = Vec::new();
    let mut flush = |run: &mut Vec<&str>| {
        while run.len() > 1 && LEADERS.contains(&run[0].to_lowercase().as_str()) {
            run.remove(0);
        }
        while run
            .last()
            .is_some_and(|t| JOINERS.contains(&t.to_lowercase().as_str()))
        {
            run.pop();
        }
        let keep = match run.as_slice() {
            [] => false,
            [one] => {
                one.chars().filter(|c| c.is_alphabetic()).count() >= 4
                    && !LEADERS.contains(&one.to_lowercase().as_str())
            }
            _ => true,
        };
        if keep {
            let display = run.join(" ");
            let key = display.to_lowercase();
            if !out.iter().any(|(k, _)| *k == key) {
                out.push((key, display));
            }
        }
        run.clear();
    };
    for (i, raw_t) in raw.iter().enumerate() {
        let t = clean(raw_t);
        if t.is_empty() {
            // a lone dash, quote or bullet ends the run
            flush(&mut run);
            continue;
        }
        let next = raw.get(i + 1).map(|n| clean(n)).filter(|n| !n.is_empty());
        let joiner = !run.is_empty()
            && JOINERS.contains(&t.to_lowercase().as_str())
            && next.is_some_and(capitalised);
        if capitalised(t) || joiner {
            run.push(t);
        } else {
            flush(&mut run);
        }
        // "Panthers: Bryce Young" are two subjects: punctuation closes the run
        if raw_t.ends_with([',', ':', ';', '.', '!', '?']) {
            flush(&mut run);
        }
    }
    flush(&mut run);
    out
}

/// Never suggested for this paper: the built-in words, the paper's title and place, every
/// section's title, and every interest's name, `also` and `not`.
pub fn stoplist(cfg: &Config) -> HashSet<String> {
    let mut s: HashSet<String> = BUILTIN.iter().map(|w| (*w).to_owned()).collect();
    let mut add = |t: &str| {
        let t = t.trim().to_lowercase();
        if t.is_empty() {
            return;
        }
        if let Some(rest) = t.strip_prefix("the ") {
            s.insert(rest.to_owned());
        }
        s.insert(t);
    };
    add(&cfg.title);
    for part in cfg.place.split(',') {
        add(part);
    }
    for kind in &cfg.sections {
        let name = match kind {
            SectionKind::Reading => "Your Reading",
            SectionKind::Weather => "Weather",
            SectionKind::Local => "Local",
            SectionKind::Interests | SectionKind::Feeds => "",
            SectionKind::Lottery => "Lottery",
            SectionKind::Horoscope => "Horoscope",
            SectionKind::Puzzle => "Puzzle",
            SectionKind::Shelf => "From Your Shelf",
            SectionKind::Onthisday => "On This Day",
        };
        add(name);
    }
    for p in &cfg.local.places {
        add(&p.name);
    }
    for f in &cfg.feeds {
        add(&f.name);
    }
    for i in &cfg.interests {
        add(&i.name);
        i.also.iter().chain(&i.not).for_each(|p| add(p));
    }
    s
}

/// Adds `hit` under the candidate `key` (spelling `display`) unless it's on the stoplist or
/// already there. Returns whether it was added.
fn note_key(
    learning: &mut Learning,
    stop: &HashSet<String>,
    key: &str,
    display: &str,
    hit: &Hit,
) -> bool {
    if stop.contains(key) {
        return false;
    }
    let c = learning
        .candidates
        .entry(key.to_owned())
        .or_insert_with(|| Candidate {
            display: display.to_owned(),
            ..Default::default()
        });
    if c.hits.contains(hit) {
        return false;
    }
    c.hits.push(hit.clone());
    true
}

/// Adds `hit` under every phrase of `text` that isn't on the stoplist; a hit already present
/// isn't added twice. Returns the hits added.
pub fn note(learning: &mut Learning, stop: &HashSet<String>, text: &str, hit: Hit) -> usize {
    phrases(text)
        .iter()
        .filter(|(k, d)| note_key(learning, stop, k, d, &hit))
        .count()
}

/// Taps: every page-history position of every read of every edition in `state`, resolved
/// through its layout to the story it fell in. Returns the hits added.
pub fn record_taps(learning: &mut Learning, state: &State, stop: &HashSet<String>) -> usize {
    let mut added = 0;
    for (date, reads) in &state.reads {
        let Some(layout) = state.editions.get(date) else {
            continue;
        };
        for read in reads.values() {
            for &pos in &read.history {
                if let Some((section, story)) = story_at(layout, pos) {
                    let hit = Hit {
                        date: *date,
                        section: section.key.clone(),
                        title: story.title.clone(),
                        signal: Signal::Tap,
                    };
                    added += note(learning, stop, &story.title, hit);
                }
            }
        }
    }
    added
}

/// The date in an edition's title: `<paper> — Tuesday, October 20, 2026`.
fn edition_day(title: &str, paper_title: &str) -> Option<NaiveDate> {
    let (paper, day) = title.split_once(" — ")?;
    if paper.trim() != paper_title.trim() {
        return None;
    }
    let day = day.trim();
    ["%A, %B %d, %Y", "%A, %B %e, %Y"]
        .iter()
        .find_map(|f| NaiveDate::parse_from_str(day, f).ok())
}

/// Paper highlights: `My Clippings.txt` entries by "Deckle" whose title is one of this paper's
/// editions; the clipped text (its first characters) is the hit's title. Returns hits added.
pub fn record_highlights(
    learning: &mut Learning,
    clippings: &[u8],
    paper_title: &str,
    stop: &HashSet<String>,
) -> usize {
    let mut added = 0;
    for e in deckle_highlights::parse(clippings).entries {
        if e.kind != deckle_highlights::Kind::Highlight || e.author.trim() != "Deckle" {
            continue;
        }
        let Some(date) = edition_day(&e.title, paper_title) else {
            continue;
        };
        let text = e.text.trim();
        let hit = Hit {
            date,
            section: "highlight".into(),
            title: text.chars().take(CLIP_CHARS).collect(),
            signal: Signal::Highlight,
        };
        added += note(learning, stop, text, hit);
    }
    added
}

/// Shelf authors: one Shelf hit per author of each book read in the last [`SHELF_DAYS`] days,
/// dated the day the book was last read (so plugging in again adds nothing until you read
/// again). Returns hits added.
pub fn record_shelf(
    learning: &mut Learning,
    shelf: &Shelf,
    now: DateTime<Utc>,
    stop: &HashSet<String>,
) -> usize {
    let mut added = 0;
    for k in shelf.kindles.values() {
        if seen_on(k).is_none() {
            continue;
        }
        for b in &k.books {
            let Some(read_at) = b
                .last_read
                .as_deref()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&Utc))
                .filter(|t| (now - *t).num_days() <= SHELF_DAYS)
            else {
                continue;
            };
            let day = read_at.date_naive();
            for author in &b.authors {
                let author = author.trim();
                if author.is_empty() {
                    continue;
                }
                let hit = Hit {
                    date: day,
                    section: "shelf".into(),
                    title: b.title.clone(),
                    signal: Signal::Shelf,
                };
                if note_key(learning, stop, &author.to_lowercase(), author, &hit) {
                    added += 1;
                }
            }
        }
    }
    added
}

/// Drops hits older than `window_days` before `today`, dismissals older than
/// [`DISMISSED_DAYS`], and candidates left without hits.
pub fn prune(learning: &mut Learning, today: NaiveDate, window_days: u32) {
    let first = today - Days::new(u64::from(window_days));
    for c in learning.candidates.values_mut() {
        c.hits.retain(|h| h.date >= first);
    }
    learning.candidates.retain(|_, c| !c.hits.is_empty());
    let keep_from = today - Days::new(DISMISSED_DAYS);
    learning.dismissed.retain(|_, d| *d >= keep_from);
}

/// A suggestion, as the Daily tab and the CLI show it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Noticed {
    pub key: String,
    pub name: String,
    pub why: String,
    /// Distinct stories (or highlights, or shelf days) behind it.
    pub stories: usize,
    /// Days between its first and last hit, inclusive.
    pub days: u32,
    pub signal: Signal,
    /// The search `accept` writes, when the plain name would be too wide (a shelf author).
    pub query: Option<String>,
}

fn is_dismissed(learning: &Learning, key: &str, today: NaiveDate) -> bool {
    learning
        .dismissed
        .get(key)
        .is_some_and(|d| (today - *d).num_days() < DISMISSED_DAYS as i64)
}

fn shelf_query(name: &str) -> String {
    format!("\"{name}\" author OR book")
}

/// The candidates worth suggesting (design §5.3): not on the stoplist, not accepted, not
/// dismissed lately, with `min_stories` distinct stories across at least two days inside
/// `window_days`; the top five by stories, then by their latest day.
pub fn suggest(learning: &Learning, cfg: &Config, today: NaiveDate) -> Vec<Noticed> {
    rank(learning, cfg, today).into_iter().take(TOP).collect()
}

/// Candidates the editor may name (design §6): the ones that would rank, not named yet, with
/// at least one Tap hit (highlight text and shelf authors never travel); each as `(key,
/// phrase, headlines)` with up to five Tap headlines, newest first; at most ten.
pub fn naming_candidates(
    learning: &Learning,
    cfg: &Config,
    today: NaiveDate,
) -> Vec<(String, String, Vec<String>)> {
    rank(learning, cfg, today)
        .into_iter()
        .filter_map(|n| {
            let c = learning.candidates.get(&n.key)?;
            if c.named.is_some() {
                return None;
            }
            let mut taps: Vec<&Hit> = c.hits.iter().filter(|h| h.signal == Signal::Tap).collect();
            if taps.is_empty() {
                return None;
            }
            taps.sort_by_key(|h| std::cmp::Reverse(h.date));
            let mut headlines: Vec<String> = Vec::new();
            for h in taps {
                if !headlines.contains(&h.title) {
                    headlines.push(h.title.clone());
                }
                if headlines.len() == 5 {
                    break;
                }
            }
            Some((n.key.clone(), c.display.clone(), headlines))
        })
        .take(10)
        .collect()
}

/// Stores the editor's names on known candidates as `(key, name, query)`; unknown keys are
/// ignored. Returns how many were set.
pub fn record_names(learning: &mut Learning, names: &[(String, String, String)]) -> usize {
    let mut n = 0;
    for (key, name, query) in names {
        if let Some(c) = learning.candidates.get_mut(key) {
            c.named = Some(Named {
                name: name.clone(),
                query: query.clone(),
            });
            n += 1;
        }
    }
    n
}

/// Every candidate that qualifies, best first (see [`suggest`]).
fn rank(learning: &Learning, cfg: &Config, today: NaiveDate) -> Vec<Noticed> {
    if !cfg.learning.enabled {
        return Vec::new();
    }
    let stop = stoplist(cfg);
    let first = today - Days::new(u64::from(cfg.learning.window_days));
    let mut out: Vec<(usize, NaiveDate, Noticed)> = Vec::new();
    for (key, c) in &learning.candidates {
        if stop.contains(key)
            || learning.accepted.contains_key(key)
            || is_dismissed(learning, key, today)
        {
            continue;
        }
        let hits: Vec<&Hit> = c.hits.iter().filter(|h| h.date >= first).collect();
        let stories: HashSet<(NaiveDate, &str)> =
            hits.iter().map(|h| (h.date, h.title.as_str())).collect();
        let dates: HashSet<NaiveDate> = hits.iter().map(|h| h.date).collect();
        if stories.len() < cfg.learning.min_stories as usize || dates.len() < 2 {
            continue;
        }
        let (Some(&min), Some(&max)) = (dates.iter().min(), dates.iter().max()) else {
            continue;
        };
        let days = (max - min).num_days() as u32 + 1;
        let shelf = hits.iter().all(|h| h.signal == Signal::Shelf);
        let signal = if shelf {
            Signal::Shelf
        } else if hits.iter().any(|h| h.signal == Signal::Tap) {
            Signal::Tap
        } else {
            Signal::Highlight
        };
        let name = c
            .named
            .as_ref()
            .map_or(c.display.clone(), |n| n.name.clone());
        let why = if shelf {
            format!("You've been reading {name} for {days} days.")
        } else {
            format!(
                "You read {} stories about {name} in the last {} days.",
                stories.len(),
                cfg.learning.window_days
            )
        };
        let query = match (&c.named, shelf) {
            (Some(n), _) => Some(n.query.clone()),
            (None, true) => Some(shelf_query(&name)),
            (None, false) => None,
        };
        out.push((
            stories.len(),
            max,
            Noticed {
                key: key.clone(),
                name,
                why,
                stories: stories.len(),
                days,
                signal,
                query,
            },
        ));
    }
    out.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(a.2.key.cmp(&b.2.key))
    });
    out.into_iter().map(|(_, _, n)| n).collect()
}

/// Makes candidate `key` an interest: writes its entry into `cfg` (a shelf author with a
/// narrower search), puts `"interests"` in `sections` before `"feeds"` when it's missing, and
/// records the acceptance. The caller saves both.
pub fn accept(
    learning: &mut Learning,
    cfg: &mut Config,
    key: &str,
    today: NaiveDate,
) -> Result<InterestConfig, String> {
    let c = learning
        .candidates
        .get(key)
        .ok_or_else(|| format!("no suggestion \"{key}\""))?;
    let name = c
        .named
        .as_ref()
        .map_or(c.display.clone(), |n| n.name.clone());
    let shelf = c.hits.iter().all(|h| h.signal == Signal::Shelf);
    let query = match (&c.named, shelf) {
        (Some(n), _) => n.query.clone(),
        (None, true) => shelf_query(&name),
        (None, false) => String::new(),
    };
    let entry = match cfg
        .interests
        .iter()
        .find(|i| i.name.trim().eq_ignore_ascii_case(name.trim()))
    {
        Some(have) => have.clone(),
        None => {
            let entry = InterestConfig {
                name: name.clone(),
                query,
                ..Default::default()
            };
            cfg.interests.push(entry.clone());
            entry
        }
    };
    cfg.ensure_section(SectionKind::Interests, SectionKind::Feeds);
    learning.accepted.insert(key.to_owned(), today);
    Ok(entry)
}

/// Learning is on for this paper (`[learning] enabled`): the clippings are read for its
/// highlights. Taps come with `[sync] readership`, which gates the `.yjf` read on its own.
pub fn learning_on(cfg: &Config) -> bool {
    cfg.learning.enabled
}

/// [`on_connection_at`] on the paper's own `interests.json` ([`Learning::path_for`]).
pub fn on_connection(
    cfg: &Config,
    state: &State,
    clippings: Option<&[u8]>,
    shelf: Option<&Shelf>,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    on_connection_at(cfg, &Learning::path_for(cfg), state, clippings, shelf, now)
}

/// Everything one connection teaches a paper: taps from `state`, highlights from `clippings`
/// (when they were read), shelf authors from `shelf`. Loads, updates, prunes and saves the
/// learning file at `path`; returns the hits added. Nothing happens with learning off.
pub fn on_connection_at(
    cfg: &Config,
    path: &Path,
    state: &State,
    clippings: Option<&[u8]>,
    shelf: Option<&Shelf>,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    if !cfg.learning.enabled {
        return Ok(0);
    }
    let mut learning = Learning::load(path);
    let stop = stoplist(cfg);
    let mut added = record_taps(&mut learning, state, &stop);
    if let Some(bytes) = clippings {
        added += record_highlights(&mut learning, bytes, &cfg.title, &stop);
    }
    if let Some(shelf) = shelf {
        added += record_shelf(&mut learning, shelf, now, &stop);
    }
    prune(&mut learning, now.date_naive(), cfg.learning.window_days);
    learning.save(path)?;
    Ok(added)
}

/// Sets candidate `key` aside for [`DISMISSED_DAYS`].
pub fn dismiss(learning: &mut Learning, key: &str, today: NaiveDate) -> Result<(), String> {
    if !learning.candidates.contains_key(key) {
        return Err(format!("no suggestion \"{key}\""));
    }
    learning.dismissed.insert(key.to_owned(), today);
    Ok(())
}
