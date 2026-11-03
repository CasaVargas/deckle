// SPDX-License-Identifier: GPL-3.0-or-later
//! A book's theme from what Deckle already keeps on this machine (Puzzles tab design §2): the
//! headline words of your paper's sections (your interests, or the whole week), the words you
//! looked up on the Kindle, and your cookbook. Each becomes a list of clued words, as a
//! `--words` file would, so the generators don't know where a theme came from.

use std::path::Path;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::cookbook::store::Store as Cookbook;
use crate::papers;
use crate::puzzle::store::Store as Puzzles;
use crate::puzzle::theme::{self, STOP};
use crate::shelf::store::Shelf;

/// Where a theme's words come from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Source {
    /// The headline words of these interest sections (their names), over the last `days`.
    Interests { names: Vec<String>, days: u32 },
    /// Every news section's headline words over the last `days`.
    Week { days: u32 },
    /// The words looked up on the Kindles that share their vocabulary.
    Vocab,
    /// Dish and ingredient names from the cookbook.
    Cookbook,
}

/// A theme resolved to clued words, with a line on what it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Themed {
    /// `(word, clue)`, the word folded to a–z, each once, best first.
    pub words: Vec<(String, String)>,
    /// A title for the book, when none is given: "Rust & Panthers, November".
    pub title: String,
    /// "41 words from the last 30 days of Rust and Panthers".
    pub note: String,
}

/// What each source could give right now, for the tab to show.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sources {
    /// `(paper title, interest name, words in the last 30 days)`.
    pub interests: Vec<(String, String, usize)>,
    /// Headline words in the last 7 days.
    pub week: usize,
    pub vocab: usize,
    pub cookbook: usize,
}

const INTEREST_DAYS: u32 = 30;
const WEEK_DAYS: u32 = 7;

fn fold(w: &str) -> String {
    deunicode::deunicode(w)
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The headline words of `sections` (all when `None`) over the last `days` before `today`,
/// newest first, each word once, clued from its headline with the section and the day.
pub fn from_headlines(
    store: &Puzzles,
    today: NaiveDate,
    days: u32,
    sections: Option<&[String]>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let since = today - chrono::Duration::days(days as i64);
    for (date, day) in store.days.range(since..=today).rev() {
        for t in &day.headlines {
            let wanted = sections.is_none_or(|s| {
                s.iter()
                    .any(|n| n.trim().eq_ignore_ascii_case(t.section.trim()))
            });
            if !wanted {
                continue;
            }
            let w = fold(&t.word);
            if w.len() < 3 || !seen.insert(w.clone()) {
                continue;
            }
            let when = if *date == today - chrono::Duration::days(1) {
                "yesterday".to_owned()
            } else if today.signed_duration_since(*date).num_days() < 7 {
                date.format("%A").to_string()
            } else {
                date.format("%-d %b").to_string()
            };
            let clue = theme::clue(t).replace(", yesterday)", &format!(", {when})"));
            out.push((w, clue));
        }
    }
    out
}

/// The looked-up words of every Kindle in the shelf, newest first, clued with the dictionary's
/// definition when the Kindle kept one, else the sentence it was looked up in.
pub fn from_shelf(shelf: &Shelf) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for snap in shelf.kindles.values() {
        for w in &snap.words {
            let word = fold(&w.word);
            if word.len() < 3 || word.len() > 15 {
                continue;
            }
            let clue = match &w.definition {
                Some(d) if !d.trim().is_empty() => d.trim().to_owned(),
                _ => {
                    let usage = w.usage.trim();
                    if usage.is_empty() {
                        format!("You looked it up in {}", w.book)
                    } else {
                        let blanked = blank_word(usage, &w.word);
                        format!("{blanked} ({})", w.book)
                    }
                }
            };
            rows.push((w.at.clone(), word, clue));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    rows.into_iter()
        .filter(|(_, w, _)| seen.insert(w.clone()))
        .map(|(_, w, c)| (w, c))
        .collect()
}

/// Dish names and ingredients from the cookbook: each word of a title ("In the title of a
/// recipe: Thursday ____") and each ingredient's last word ("In the Thursday soup").
pub fn from_cookbook(store: &Cookbook) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let Ok(all) = store.all() else {
        return out;
    };
    for s in &all {
        let title = s
            .title_override
            .as_deref()
            .unwrap_or(&s.recipe.title)
            .trim()
            .to_owned();
        for word in title.split(|c: char| !c.is_alphanumeric()) {
            let w = fold(word);
            if w.len() < 3 || STOP.contains(&w.as_str()) || !seen.insert(w.clone()) {
                continue;
            }
            out.push((
                w,
                format!("{} (a recipe you saved)", blank_word(&title, word)),
            ));
        }
        for g in &s.recipe.ingredients {
            for item in &g.items {
                // "2 cups chopped onion" → onion
                let last = item
                    .split(|c: char| !c.is_alphanumeric())
                    .rfind(|t| t.len() >= 3)
                    .unwrap_or("");
                let w = fold(last);
                if w.len() < 3 || STOP.contains(&w.as_str()) || !seen.insert(w.clone()) {
                    continue;
                }
                out.push((w, format!("Goes into {title}")));
            }
        }
    }
    out
}

/// `text` with every whole-word use of `word` blanked.
fn blank_word(text: &str, word: &str) -> String {
    let mut out = String::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut String| {
        if cur.eq_ignore_ascii_case(word) {
            out.push_str("____");
        } else {
            out.push_str(cur);
        }
        cur.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() {
            cur.push(c);
        } else {
            flush(&mut cur, &mut out);
            out.push(c);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// What a tab can offer, from the main paper's config at `main`: every paper's interests with
/// their word counts, the week, the shelf and the cookbook.
pub fn sources(main: &Path, today: NaiveDate) -> Sources {
    let mut out = Sources::default();
    let Ok(cfg) = Config::load(main) else {
        return out;
    };
    let puzzles = Puzzles::load(&cfg.puzzle_store());
    if let Ok(loaded) = papers::load_all(main) {
        for p in &loaded.papers {
            for i in &p.interests {
                let n = from_headlines(
                    &puzzles,
                    today,
                    INTEREST_DAYS,
                    Some(std::slice::from_ref(&i.name)),
                )
                .len();
                out.interests.push((p.title.clone(), i.name.clone(), n));
            }
        }
    }
    out.week = from_headlines(&puzzles, today, WEEK_DAYS, None).len();
    out.vocab = from_shelf(&Shelf::load(&cfg.shelf_store())).len();
    out.cookbook = from_cookbook(&Cookbook::new(Cookbook::default_dir())).len();
    out
}

/// `source` resolved against the main paper at `main`.
pub fn resolve(main: &Path, source: &Source, today: NaiveDate) -> Result<Themed, String> {
    let cfg = Config::load(main)?;
    let month = today.format("%B").to_string();
    let _ = today.month();
    Ok(match source {
        Source::Interests { names, days } => {
            let puzzles = Puzzles::load(&cfg.puzzle_store());
            let days = if *days == 0 { INTEREST_DAYS } else { *days };
            let filter = (!names.is_empty()).then_some(names.as_slice());
            let words = from_headlines(&puzzles, today, days, filter);
            let list = if names.is_empty() {
                "every section".to_owned()
            } else {
                names.join(" & ")
            };
            Themed {
                note: format!("{} words from the last {days} days of {list}", words.len()),
                title: format!("{list}, {month}"),
                words,
            }
        }
        Source::Week { days } => {
            let puzzles = Puzzles::load(&cfg.puzzle_store());
            let days = if *days == 0 { WEEK_DAYS } else { *days };
            let words = from_headlines(&puzzles, today, days, None);
            Themed {
                note: format!(
                    "{} words from the last {days} days of {}",
                    words.len(),
                    cfg.title
                ),
                title: format!("The week in {}", cfg.title),
                words,
            }
        }
        Source::Vocab => {
            let words = from_shelf(&Shelf::load(&cfg.shelf_store()));
            Themed {
                note: format!("{} words you looked up on the Kindle", words.len()),
                title: format!("Words you looked up, {month}"),
                words,
            }
        }
        Source::Cookbook => {
            let words = from_cookbook(&Cookbook::new(Cookbook::default_dir()));
            Themed {
                note: format!("{} words from your cookbook", words.len()),
                title: "From the kitchen".to_owned(),
                words,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzle::store::Day;
    use crate::puzzle::theme::ThemeWord;
    use crate::shelf::store::{KindleSnapshot, WordSnap};

    fn day(words: &[(&str, &str, &str)]) -> Day {
        Day {
            puzzle: crate::puzzle::store::Puzzle {
                number: 1,
                rows: None,
                clues: Vec::new(),
                ladder: None,
                search: None,
            },
            headlines: words
                .iter()
                .map(|(w, h, s)| ThemeWord {
                    word: (*w).to_owned(),
                    headline: (*h).to_owned(),
                    section: (*s).to_owned(),
                })
                .collect(),
            solve: None,
            search_found: None,
        }
    }

    #[test]
    fn headlines_filter_by_section_and_name_the_day() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let mut store = Puzzles::default();
        store.days.insert(
            today - chrono::Duration::days(1),
            day(&[
                ("cargo", "Cargo build gets faster", "Rust"),
                ("young", "Young returns Sunday", "Panthers"),
            ]),
        );
        store.days.insert(
            today - chrono::Duration::days(3),
            day(&[
                ("crate", "A new crate a day", "Rust"),
                ("cargo", "Cargo again", "Rust"),
            ]),
        );
        store.days.insert(
            today - chrono::Duration::days(40),
            day(&[("rust", "Rust is old news", "Rust")]),
        );
        let all = from_headlines(&store, today, 30, None);
        assert_eq!(
            all.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>(),
            ["cargo", "young", "crate"]
        );
        assert_eq!(all[0].1, "____ build gets faster (Rust, yesterday)");
        assert_eq!(all[2].1, "A new ____ a day (Rust, Friday)");
        let rust = from_headlines(&store, today, 30, Some(&["rust".to_owned()]));
        assert_eq!(
            rust.len(),
            2,
            "case-insensitive section match, the old day left out"
        );
        assert!(from_headlines(&store, today, 7, Some(&["Panthers".into()])).len() == 1);
    }

    #[test]
    fn the_shelf_gives_definitions_or_usage() {
        let mut shelf = Shelf::default();
        let snap = KindleSnapshot {
            words: vec![
                WordSnap {
                    word: "tenebrous".into(),
                    stem: "tenebrous".into(),
                    usage: "The tenebrous halls ran on.".into(),
                    book: "Wolf Hall".into(),
                    at: "2026-10-30".into(),
                    definition: Some("dark, gloomy".into()),
                },
                WordSnap {
                    word: "Sere".into(),
                    stem: "sere".into(),
                    usage: "A sere field.".into(),
                    book: "Wolf Hall".into(),
                    at: "2026-10-31".into(),
                    definition: None,
                },
                WordSnap {
                    word: "ox".into(),
                    stem: "ox".into(),
                    usage: String::new(),
                    book: "x".into(),
                    at: "2026-10-01".into(),
                    definition: None,
                },
            ],
            ..Default::default()
        };
        shelf.put("k1", snap);
        let words = from_shelf(&shelf);
        assert_eq!(words.len(), 2, "two letters is too short");
        assert_eq!(
            words[0],
            ("sere".to_owned(), "A ____ field. (Wolf Hall)".to_owned()),
            "newest first"
        );
        assert_eq!(
            words[1],
            ("tenebrous".to_owned(), "dark, gloomy".to_owned())
        );
    }

    #[test]
    fn blanking_is_whole_word() {
        assert_eq!(
            blank_word("Cargo build, cargo run", "cargo"),
            "____ build, ____ run"
        );
        assert_eq!(blank_word("Cargoes sail", "cargo"), "Cargoes sail");
    }
}
