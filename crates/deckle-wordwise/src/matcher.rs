// SPDX-License-Identifier: GPL-3.0-or-later
//! Finds hint-worthy words in KFX text runs: longest phrase first, case-insensitive, proper
//! nouns (capitalized mid-sentence) and all-caps headings skipped, positions in code points.

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
        let sentence_start = out.is_empty()
            || gap.iter().any(|c| {
                matches!(
                    c,
                    '.' | '!' | '?' | ':' | '"' | '\u{201C}' | '\u{2018}' | '(' | '[' | '\u{2014}'
                )
            });
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

/// Hints for every run, keeping senses with difficulty ≤ `level` (1–5).
pub fn glosses(runs: &[Run], lexicon: &Lexicon, level: i64) -> Vec<Gloss> {
    let mut out = Vec::new();
    for run in runs {
        let toks = tokenize(&run.text);
        let mut i = 0;
        'tok: while i < toks.len() {
            let max = lexicon.max_words.min(toks.len() - i);
            for n in (1..=max).rev() {
                let span = &toks[i..i + n];
                if span[1..].iter().any(|t| !t.joined) {
                    continue;
                }
                let usable = span.iter().enumerate().all(|(k, t)| {
                    !all_caps(&t.text) && (!capitalized(&t.text) || (k == 0 && t.sentence_start))
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
