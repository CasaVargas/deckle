// SPDX-License-Identifier: GPL-3.0-or-later
//! Finds hint-worthy words in KFX text runs: longest phrase first, case-insensitive, proper
//! nouns and all-caps headings skipped, positions in code points. A word is a proper noun for
//! the whole book when it is capitalized mid-sentence more often than it is lowercase
//! ("Darcy", "Mrs", but not "Pride" of a title-case heading).

use std::collections::{HashMap, HashSet};

use deckle_kfx::text::Run;

use crate::kll::Gloss;
use crate::lexicon::{Lexicon, normalize_form};

#[derive(Debug)]
struct Token {
    /// Code-point offsets within the run.
    start: usize,
    end: usize,
    text: String,
    /// Starts a sentence (first in the run, or after `.`, `!`, `?`, `:` or an opening quote).
    sentence_start: bool,
    /// Only whitespace between this token and the previous one.
    joined: bool,
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic()
}

fn is_inner(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '-' | '\u{2010}' | '\u{2011}')
}

/// Abbreviations whose `.` does not end a sentence ("Mr. Darcy").
const HONORIFICS: &[&str] = &[
    "mr", "mrs", "ms", "messrs", "mme", "mlle", "dr", "st", "capt", "col", "gen", "lt", "rev",
    "prof", "sr", "jr", "hon", "gov", "sgt", "fr", "mt",
];

fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut gap_start = 0;
    while i < chars.len() {
        if !is_word_char(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len()
            && (is_word_char(chars[i])
                || (is_inner(chars[i]) && i + 1 < chars.len() && is_word_char(chars[i + 1])))
        {
            i += 1;
        }
        let gap = &chars[gap_start..start];
        let after_honorific = out.last().is_some_and(|t: &Token| {
            HONORIFICS.contains(&t.text.to_lowercase().as_str())
                && gap.iter().filter(|c| !c.is_whitespace()).eq(['.'].iter())
        });
        let sentence_start = out.is_empty()
            || (!after_honorific
                && gap.iter().any(|c| {
                    matches!(
                        c,
                        '.' | '!'
                            | '?'
                            | ':'
                            | '"'
                            | '\u{201C}'
                            | '\u{2018}'
                            | '('
                            | '['
                            | '\u{2014}'
                    )
                }));
        let joined = !out.is_empty() && gap.iter().all(|c| c.is_whitespace());
        out.push(Token {
            start,
            end: i,
            text: chars[start..i].iter().collect(),
            sentence_start,
            joined,
        });
        gap_start = i;
    }
    out
}

fn capitalized(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
}

fn all_caps(s: &str) -> bool {
    s.chars().filter(|c| c.is_alphabetic()).count() > 1 && !s.chars().any(char::is_lowercase)
}

/// Lowercase words used as names in this book (see the module doc).
fn proper_nouns(tokenized: &[Vec<Token>]) -> HashSet<String> {
    let mut counts: HashMap<String, (u32, u32)> = HashMap::new(); // (capitalized mid, lowercase)
    for t in tokenized.iter().flatten() {
        if all_caps(&t.text) {
            continue;
        }
        if capitalized(&t.text) {
            if !t.sentence_start {
                counts.entry(normalize_form(&t.text)).or_default().0 += 1;
            }
        } else {
            counts.entry(normalize_form(&t.text)).or_default().1 += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, (cap, low))| cap > low)
        .map(|(k, _)| k)
        .collect()
}

/// Hints for every run, keeping senses with difficulty ≤ `level` (1–5).
pub fn glosses(runs: &[Run], lexicon: &Lexicon, level: i64) -> Vec<Gloss> {
    let mut out = Vec::new();
    let tokenized: Vec<Vec<Token>> = runs.iter().map(|r| tokenize(&r.text)).collect();
    let proper = proper_nouns(&tokenized);
    for (run, toks) in runs.iter().zip(&tokenized) {
        let mut i = 0;
        'tok: while i < toks.len() {
            let max = lexicon.max_words.min(toks.len() - i);
            for n in (1..=max).rev() {
                let span = &toks[i..i + n];
                if span[1..].iter().any(|t| !t.joined) {
                    continue;
                }
                let usable = span.iter().enumerate().all(|(k, t)| {
                    !all_caps(&t.text)
                        && (!capitalized(&t.text) || (k == 0 && t.sentence_start))
                        && !proper.contains(&normalize_form(&t.text))
                });
                if !usable {
                    continue;
                }
                let key = normalize_form(
                    &span
                        .iter()
                        .map(|t| t.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                let Some(sense) = lexicon.get(&key) else {
                    continue;
                };
                if sense.difficulty > level {
                    continue;
                }
                let start = run.pid + span[0].start as i64;
                out.push(Gloss {
                    start,
                    end: (n > 1).then(|| run.pid + span[n - 1].end as i64),
                    difficulty: sense.difficulty,
                    sense_id: sense.id,
                });
                i += n;
                continue 'tok;
            }
            i += 1;
        }
    }
    out.sort_by_key(|g| g.start);
    out.dedup_by_key(|g| g.start);
    out
}
