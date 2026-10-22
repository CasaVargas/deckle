// SPDX-License-Identifier: GPL-3.0-or-later
//! Answers written as Kindle notes: a note on a highlighted clue is that clue's answer
//! (puzzle design §5).

use chrono::{Days, NaiveDate};
use deckle_highlights::book::edition_date;
use deckle_highlights::parse::{Entry, Kind, parse};

use super::store::{Puzzle, Solve, Store};
use super::words::Words;

/// What a Kindle's connection said about its notes.
#[derive(Clone, Copy, Debug)]
pub enum Notes<'a> {
    /// `My Clippings.txt`, read.
    Read(&'a [u8]),
    /// No file yet: a Kindle makes it at the first highlight.
    NoFile,
    /// A Kindle that keeps its notes where USB can't reach them (Scribe 2024 over MTP, #99).
    Unreachable,
}

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
pub fn score(puzzle: &Puzzle, date: NaiveDate, entries: &[Entry], words: &Words) -> Option<Solve> {
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
        if let Some(l) = &puzzle.ladder
            && on_ladder_line(&text, &l.from, &l.to)
        {
            ladder = Some(climbs(&normalise(&note.text), l, words));
            continue;
        }
        // whole words only, and the longest clue the highlight names: "Help" is inside "To help
        // someone in need" but isn't that clue
        let padded = format!(" {text} ");
        let clue = clues
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                !c.is_empty()
                    && (padded.contains(&format!(" {c} "))
                        || (text.len() >= MIN_PARTIAL && format!(" {c} ").contains(&padded)))
            })
            .max_by_key(|(i, c)| (c.len(), std::cmp::Reverse(*i)))
            .map(|(i, _)| i);
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

/// The highlight takes in the ladder line: its FROM word, then later its TO word.
fn on_ladder_line(text: &str, from: &str, to: &str) -> bool {
    let ws: Vec<&str> = text.split(' ').collect();
    ws.iter()
        .position(|w| *w == from)
        .is_some_and(|i| ws[i + 1..].contains(&to))
}

/// A note climbs the ladder when it fills the blank rungs (with or without the ends) with list
/// words, each one letter from the last, ending at TO. Any such path counts, not only the one
/// printed in the answers.
fn climbs(note: &str, l: &super::ladder::Ladder, words: &Words) -> bool {
    let mut rungs: Vec<&str> = note.split(' ').filter(|w| !w.is_empty()).collect();
    if rungs.first() == Some(&l.from.as_str()) {
        rungs.remove(0);
    }
    if rungs.last() == Some(&l.to.as_str()) {
        rungs.pop();
    }
    if rungs.len() != l.path.len().saturating_sub(2) {
        return false;
    }
    let mut path = vec![l.from.as_str()];
    path.extend(rungs);
    path.push(&l.to);
    let one_apart = |a: &str, b: &str| {
        a.len() == b.len() && a.bytes().zip(b.bytes()).filter(|(x, y)| x != y).count() == 1
    };
    path.windows(2).all(|p| one_apart(p[0], p[1]))
        && path[1..path.len() - 1].iter().all(|w| words.contains(w))
}

/// Scores the last week's stored puzzles from a Kindle's notes. A day the notes say nothing
/// about keeps its solve. Only a Kindle whose notes can't be reached hides the page's hint; one
/// with no file yet leaves it as it was.
pub fn record(store: &mut Store, notes: Notes<'_>, today: NaiveDate, words: &Words) {
    let bytes = match notes {
        Notes::Read(b) => b,
        Notes::NoFile => return,
        Notes::Unreachable => {
            store.notes_readable = Some(false);
            return;
        }
    };
    store.notes_readable = Some(true);
    let entries = parse(bytes).entries;
    let since = today - Days::new(RECENT_DAYS);
    for (date, day) in store.days.range_mut(since..=today) {
        if day.puzzle.clues.is_empty() && day.puzzle.ladder.is_none() {
            continue;
        }
        if let Some(s) = score(&day.puzzle, *date, &entries, words) {
            day.solve = Some(s);
        }
    }
}
