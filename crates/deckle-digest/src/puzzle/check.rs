// SPDX-License-Identifier: GPL-3.0-or-later
//! Answers written as Kindle notes: a note on a highlighted clue is that clue's answer
//! (puzzle design §5).

use chrono::{Days, NaiveDate};
use deckle_highlights::book::edition_date;
use deckle_highlights::parse::{Entry, Kind, parse};

use super::store::{Puzzle, Solve, Store};

/// Editions looked at on each connection.
const RECENT_DAYS: u64 = 7;

/// A highlight shorter than this, inside a clue, is too little to say which clue it is.
const MIN_PARTIAL: usize = 12;

/// Lower-case letters, single spaces: "1 A flat, stiff piece." → "a flat stiff piece".
fn normalise(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphabetic() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Just the letters, upper-case: " card. " and "c a r d" are both "CARD".
fn letters(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// How the notes on `date`'s edition answered its puzzle, or `None` when no note is on a clue
/// or the ladder. The edition is matched by its date, so a renamed paper still counts.
pub fn score(puzzle: &Puzzle, date: NaiveDate, entries: &[Entry]) -> Option<Solve> {
    let want = crate::long_date(date);
    let mine: Vec<&Entry> = entries
        .iter()
        .filter(|e| edition_date(e) == Some(want.as_str()))
        .collect();
    let clues: Vec<String> = puzzle.clues.iter().map(|c| normalise(&c.clue)).collect();
    let mut answers: Vec<Option<String>> = vec![None; clues.len()];
    let mut ladder: Option<bool> = None;
    for note in mine.iter().filter(|e| e.kind == Kind::Note) {
        let Some((at, _)) = note.location else {
            continue;
        };
        // the highlight the note sits on: the latest of its edition that covers it
        let Some(h) = mine.iter().rev().find(|h| {
            h.kind == Kind::Highlight
                && h.title == note.title
                && h.location.is_some_and(|(a, b)| a <= at && at <= b)
        }) else {
            continue;
        };
        let text = normalise(&h.text);
        if let Some(l) = &puzzle.ladder {
            let words: Vec<&str> = text.split(' ').collect();
            if words.first() == Some(&l.from.as_str()) && words.last() == Some(&l.to.as_str()) {
                let path: Vec<String> = l.path.iter().map(|w| w.to_ascii_uppercase()).collect();
                let said: Vec<String> = normalise(&note.text)
                    .split(' ')
                    .map(|w| w.to_ascii_uppercase())
                    .collect();
                ladder = Some(said == path);
                continue;
            }
        }
        let clue = clues.iter().position(|c| {
            !c.is_empty()
                && (text.contains(c.as_str())
                    || (text.len() >= MIN_PARTIAL && c.contains(text.as_str())))
        });
        if let Some(i) = clue {
            answers[i] = Some(letters(&note.text)); // entries are in file order: latest wins
        }
    }
    if ladder.is_none() && answers.iter().all(Option::is_none) {
        return None;
    }
    let solved = puzzle
        .clues
        .iter()
        .zip(&answers)
        .filter(|(c, a)| a.as_deref() == Some(letters(&c.answer).as_str()))
        .count();
    Some(Solve {
        solved,
        of: puzzle.clues.len(),
        ladder: ladder.unwrap_or(false),
    })
}

/// Scores the last week's stored puzzles from a Kindle's `My Clippings.txt` (`None`: the Kindle
/// had none to read, such as a Scribe over MTP). A day the notes say nothing about keeps its
/// solve.
pub fn record(store: &mut Store, clippings: Option<&[u8]>, today: NaiveDate) {
    store.notes_readable = Some(clippings.is_some());
    let Some(bytes) = clippings else {
        return;
    };
    let entries = parse(bytes).entries;
    let since = today - Days::new(RECENT_DAYS);
    for (date, day) in store.days.range_mut(since..=today) {
        if day.puzzle.clues.is_empty() && day.puzzle.ladder.is_none() {
            continue;
        }
        if let Some(s) = score(&day.puzzle, *date, &entries) {
            day.solve = Some(s);
        }
    }
}
