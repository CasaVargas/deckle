// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle page (docs/superpowers/specs/2026-10-22-puzzle-page-design.md): a 5×5 mini
//! themed on yesterday's headlines, a word ladder, and yesterday's answers.

pub mod check;
pub mod grid;
pub mod ladder;
pub mod search;
pub mod store;
pub mod theme;
pub mod words;

use std::collections::HashSet;
use std::time::Duration;

use chrono::{Datelike, Days, NaiveDate};
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::config::{Config, PuzzleConfig, SectionKind, section_key};
use crate::{Article, Section, esc};
use grid::Dir;
use store::{Clue, Day, Puzzle, Store};
use words::Words;

/// How long one template's fill may search (spec §3: the whole puzzle in under a second).
pub const FILL_CAP: Duration = Duration::from_millis(500);

/// Theme answers placed in a mini.
const THEME_WORDS: usize = 2;

/// Theme words in a word search, at most (word search design §2).
const SEARCH_THEMES: usize = 3;
/// Mixed into the date's seed for the word search, so its fill never moves the mini's or the
/// ladder's.
const SEARCH_SALT: u64 = 0x5EA8_C000;

/// Fills tried with the theme words, then as many without, while the fill matches a grid of the
/// last [`store::KEEP_DAYS`] days.
const FRESH_TRIES: u64 = 4;

/// The puzzle for `date`, seeded by the date: the same inputs make the same puzzle.
///
/// - The theme words are yesterday's headlines (`store.days[date - 1]`), when there are any.
/// - The grid is none of the last 60 days' (puzzle design §4): a 5×5 has few fills, and a
///   headline that runs two days gives the same theme word twice. A repeat is filled again with
///   the date's next seed, then without the theme words.
/// - The ladder avoids the pairs of the last 60 days.
/// - The word search (word search design §4) hides up to three other headline words and none
///   of the mini's, from its own seed, and is none of the last 60 days' either.
pub fn make(date: NaiveDate, cfg: &PuzzleConfig, words: &Words, store: &Store) -> Puzzle {
    let seed = date.num_days_from_ce() as u64;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut themes = match (cfg.theme, store.days.get(&(date - Days::new(1)))) {
        (true, Some(y)) => theme::pick(&y.headlines, THEME_WORDS),
        _ => Vec::new(),
    };
    let seeds: Vec<String> = themes.iter().map(|t| t.word.clone()).collect();
    let since = date - Days::new(store::KEEP_DAYS);
    let recent: HashSet<&[String; 5]> = store
        .days
        .range(since..date)
        .filter_map(|(_, d)| d.puzzle.rows.as_ref())
        .collect();
    let mut fill = grid::fill(words, &seeds, &mut rng, FILL_CAP);
    for attempt in 1..=2 * FRESH_TRIES {
        if fill.as_ref().is_none_or(|f| !recent.contains(&f.rows)) {
            break;
        }
        if attempt > FRESH_TRIES {
            themes.clear();
        }
        let seeds: Vec<String> = themes.iter().map(|t| t.word.clone()).collect();
        let mut again = StdRng::seed_from_u64(seed ^ (attempt << 32));
        fill = grid::fill(words, &seeds, &mut again, FILL_CAP);
    }
    let clues: Vec<Clue> = fill
        .as_ref()
        .map(|f| {
            f.answers
                .iter()
                .map(|(slot, answer)| {
                    let theme = themes.iter().find(|t| t.word == *answer);
                    Clue {
                        number: slot.number,
                        dir: slot.dir,
                        answer: answer.clone(),
                        clue: match theme {
                            Some(t) => theme::clue(t),
                            None => words.clue(answer).unwrap_or_default().to_owned(),
                        },
                        theme: theme.is_some(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let ladder = if cfg.ladder {
        let since = date - Days::new(store::KEEP_DAYS);
        let recent: Vec<(String, String)> = store
            .days
            .range(since..date)
            .filter_map(|(_, d)| d.puzzle.ladder.as_ref())
            .map(|l| (l.from.clone(), l.to.clone()))
            .collect();
        ladder::today(words, &mut rng, &recent)
    } else {
        None
    };
    let minis: HashSet<String> = clues
        .iter()
        .map(|c| c.answer.clone())
        .chain(themes.iter().map(|t| t.word.clone()))
        .collect();
    let search = word_search(date, cfg, words, store, &minis);
    Puzzle {
        number: 1 + store
            .days
            .values()
            .map(|d| d.puzzle.number)
            .max()
            .unwrap_or(0),
        rows: fill.map(|f| f.rows),
        clues,
        ladder,
        search,
    }
}

/// The day's word search: yesterday's 4–5-letter headline words not in `avoid` (the mini's
/// answers and themes) first, filled from the date's salted seed, again while it matches a
/// search of the last 60 days, then without the theme words.
fn word_search(
    date: NaiveDate,
    cfg: &PuzzleConfig,
    words: &Words,
    store: &Store,
    avoid: &HashSet<String>,
) -> Option<search::Search> {
    let mut themes: Vec<String> = match (cfg.theme, store.days.get(&(date - Days::new(1)))) {
        (true, Some(y)) => theme::pick(&y.headlines, THEME_WORDS + SEARCH_THEMES)
            .into_iter()
            .map(|t| t.word)
            .filter(|w| (4..=5).contains(&w.len()) && !avoid.contains(w))
            .take(SEARCH_THEMES)
            .collect(),
        _ => Vec::new(),
    };
    let since = date - Days::new(store::KEEP_DAYS);
    let recent: HashSet<&Vec<String>> = store
        .days
        .range(since..date)
        .filter_map(|(_, d)| d.puzzle.search.as_ref().map(|s| &s.rows))
        .collect();
    let seed = date.num_days_from_ce() as u64 ^ SEARCH_SALT;
    let mut found = None;
    for attempt in 0..=2 * FRESH_TRIES {
        if attempt > FRESH_TRIES {
            themes.clear();
        }
        let mut rng = StdRng::seed_from_u64(seed ^ (attempt << 32));
        found = search::fill(words, &themes, avoid, &mut rng, search::SEARCH_CAP);
        if found.as_ref().is_none_or(|s| !recent.contains(&s.rows)) {
            break;
        }
    }
    found
}

/// The stored puzzle for `date`, or a new one made and kept in `store`.
pub fn today<'s>(
    date: NaiveDate,
    cfg: &PuzzleConfig,
    words: &Words,
    store: &'s mut Store,
) -> &'s Day {
    if !store.days.contains_key(&date) {
        let puzzle = make(date, cfg, words, store);
        store.days.insert(
            date,
            Day {
                puzzle,
                headlines: Vec::new(),
                solve: None,
            },
        );
    }
    &store.days[&date]
}

/// The puzzle page as data, for a native reader (edition bundle design §6.6): today's, with
/// its answers, and yesterday's when there was one.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PuzzleData {
    pub today: store::Puzzle,
    pub yesterday: Option<store::Puzzle>,
}

const KICKER: &str = "Answers in tomorrow's paper";
const HINT: &str = "Highlight a clue and add your answer as a note; tomorrow's paper checks it.";
const CREDIT: &str = "Words and definitions from Wiktionary and wordfreq (CC BY-SA 4.0)";
const SEARCH_HOW: &str = "Find these words; they run across, down, or diagonally, never backwards.";
const FROM_HEADLINES: &str = " (from yesterday's paper)";

/// The Puzzle section for `date`: today's mini and ladder (made and kept in `store` when new),
/// then yesterday's answers when there was a puzzle yesterday.
pub fn section(date: NaiveDate, cfg: &Config, store: &mut Store, words: &Words) -> Section {
    let p = today(date, &cfg.puzzle, words, store).puzzle.clone();
    let mut articles = Vec::new();
    if let Some(rows) = &p.rows {
        let mut body = grid_table(rows, "grid", false);
        for (dir, name) in [(Dir::Across, "Across"), (Dir::Down, "Down")] {
            body.push_str(&format!("<h3 class=\"subhead\">{name}</h3>\n"));
            for c in p.clues.iter().filter(|c| c.dir == dir) {
                body.push_str(&format!(
                    "<p class=\"clue\"><b>{}</b> {}</p>\n",
                    c.number,
                    esc(&c.clue)
                ));
            }
        }
        if store.notes_readable != Some(false) {
            body.push_str(&format!("<p class=\"note\">{}</p>\n", esc(HINT)));
        }
        articles.push(Article {
            title: format!("The Mini · No. {}", p.number),
            body,
            ..Default::default()
        });
    }
    if let Some(l) = &p.ladder {
        let mut rungs = vec![l.from.to_uppercase()];
        rungs.extend(std::iter::repeat_n(
            "____".to_owned(),
            l.path.len().saturating_sub(2),
        ));
        rungs.push(l.to.to_uppercase());
        articles.push(Article {
            title: "Word ladder".into(),
            body: format!(
                "<p class=\"ladder\">{}</p>\n<p class=\"small\">Change one letter a rung; every \
                 rung is a word.</p>\n",
                esc(&rungs.join(" → "))
            ),
            ..Default::default()
        });
    }
    if let Some(ws) = &p.search {
        let mut body = search_table(ws, "grid search", false);
        body.push_str(&format!("<p class=\"small\">{}</p>\n", esc(SEARCH_HOW)));
        let listed: Vec<String> = ws
            .words
            .iter()
            .map(|h| {
                let suffix = if h.theme { FROM_HEADLINES } else { "" };
                format!(
                    "<p class=\"clue\">{}{}</p>",
                    h.word.to_uppercase(),
                    esc(suffix)
                )
            })
            .collect();
        let (left, right) = listed.split_at(listed.len().div_ceil(2));
        body.push_str(&format!(
            "<table class=\"cols\"><tr><td>{}</td><td>{}</td></tr></table>\n",
            left.concat(),
            right.concat()
        ));
        articles.push(Article {
            title: format!("Word search · No. {}", p.number),
            body,
            ..Default::default()
        });
    }
    if articles.is_empty() {
        let mut s = crate::sources::unavailable("Puzzle", "no puzzle could be made");
        s.key = section_key(SectionKind::Puzzle, None);
        return s;
    }
    if let Some(y) = store.days.get(&(date - Days::new(1))) {
        let mut body = String::new();
        if let Some(rows) = &y.puzzle.rows {
            body.push_str(&grid_table(rows, "grid small", true));
        }
        if let Some(l) = &y.puzzle.ladder {
            let line = match &y.solve {
                Some(s) if s.ladder => "Ladder: solved".to_owned(),
                _ => format!(
                    "Ladder: {}",
                    l.path
                        .iter()
                        .map(|w| w.to_uppercase())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            body.push_str(&format!("<p>{}</p>\n", esc(&line)));
        }
        if let Some(ws) = &y.puzzle.search {
            body.push_str(&search_table(ws, "grid search small", true));
            let list: Vec<String> = ws.words.iter().map(|h| h.word.to_uppercase()).collect();
            body.push_str(&format!(
                "<p>{}</p>\n",
                esc(&format!("Word search: {}", list.join(", ")))
            ));
        }
        if let Some(s) = &y.solve {
            body.push_str(&format!(
                "<p class=\"score\">You solved {} of {}</p>\n",
                s.solved, s.of
            ));
        }
        if !body.is_empty() {
            articles.push(Article {
                title: "Yesterday's answers".into(),
                body,
                ..Default::default()
            });
        }
    }
    if let Some(last) = articles.last_mut() {
        last.body
            .push_str(&format!("<p class=\"credit\">{}</p>\n", esc(CREDIT)));
    }
    Section {
        brief: Vec::new(),
        key: section_key(SectionKind::Puzzle, None),
        title: "Puzzle".into(),
        kicker: Some(KICKER.into()),
        articles,
        unavailable: None,
        weather: None,
        puzzle: Some(PuzzleData {
            today: p,
            yesterday: store
                .days
                .get(&(date - Days::new(1)))
                .map(|y| y.puzzle.clone()),
        }),
    }
}

/// The word search as a table of letters; with `mask`, only the hidden words' letters, the
/// other squares a middle dot (yesterday's answers).
fn search_table(ws: &search::Search, class: &str, mask: bool) -> String {
    let mut shown = [[!mask; search::SIZE]; search::SIZE];
    if mask {
        for h in &ws.words {
            for (r, c) in ws.cells(h) {
                shown[r][c] = true;
            }
        }
    }
    let mut out = format!("<table class=\"{class}\">\n");
    for (r, row) in ws.rows.iter().enumerate() {
        out.push_str("<tr>");
        for (c, ch) in row.chars().enumerate() {
            out.push_str("<td>");
            out.push(if shown[r][c] { ch } else { '·' });
            out.push_str("</td>");
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

/// The grid as a table, black squares filled: the clue numbers small in the corner, or the
/// letters when `letters` (yesterday's small grid has no room for both).
fn grid_table(rows: &[String; 5], class: &str, letters: bool) -> String {
    let mut numbers = [[None; 5]; 5];
    for s in grid::slots_in(rows) {
        numbers[s.row][s.col] = Some(s.number);
    }
    let mut out = format!("<table class=\"{class}\">\n");
    for (r, row) in rows.iter().enumerate() {
        out.push_str("<tr>");
        for (c, ch) in row.chars().enumerate() {
            if ch == '#' {
                out.push_str("<td class=\"b\"></td>");
                continue;
            }
            out.push_str("<td>");
            if let Some(n) = numbers[r][c].filter(|_| !letters) {
                out.push_str(&format!("<span class=\"n\">{n}</span>"));
            }
            if letters {
                out.push(ch);
            }
            out.push_str("</td>");
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}
