// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle page (docs/superpowers/specs/2026-10-22-puzzle-page-design.md): a 5×5 mini
//! themed on yesterday's headlines, a word ladder, and yesterday's answers.

pub mod grid;
pub mod ladder;
pub mod store;
pub mod theme;
pub mod words;

use std::time::Duration;

use chrono::{Datelike, Days, NaiveDate};
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::config::PuzzleConfig;
use store::{Clue, Day, Puzzle, Store};
use words::Words;

/// How long one template's fill may search (spec §3: the whole puzzle in under a second).
pub const FILL_CAP: Duration = Duration::from_millis(500);

/// Theme answers placed in a mini.
const THEME_WORDS: usize = 2;

/// The puzzle for `date`, seeded by the date: the same inputs make the same puzzle.
///
/// - The theme words are yesterday's headlines (`store.days[date - 1]`), when there are any.
/// - The ladder avoids the pairs of the last 60 days.
pub fn make(date: NaiveDate, cfg: &PuzzleConfig, words: &Words, store: &Store) -> Puzzle {
    let mut rng = StdRng::seed_from_u64(date.num_days_from_ce() as u64);
    let themes = match (cfg.theme, store.days.get(&(date - Days::new(1)))) {
        (true, Some(y)) => theme::pick(&y.headlines, THEME_WORDS),
        _ => Vec::new(),
    };
    let seeds: Vec<String> = themes.iter().map(|t| t.word.clone()).collect();
    let fill = grid::fill(words, &seeds, &mut rng, FILL_CAP);
    let clues = fill
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
    }
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
