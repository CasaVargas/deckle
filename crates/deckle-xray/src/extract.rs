// SPDX-License-Identifier: GPL-3.0-or-later
//! Heuristic entity extraction for the no-LLM path: recurring capitalized names and places,
//! merged across aliases, with first-mention quotes and KFX positions.

use std::collections::HashMap;

use deckle_kfx::text::Run;
use regex::Regex;

use crate::asc::EntityKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub label: String,
    pub kind: EntityKind,
    /// First-mention sentence, for the fallback description.
    pub quote: String,
    /// `(start position, length in characters)` in reading order.
    pub occurrences: Vec<(i64, i64)>,
    pub aliases: Vec<String>,
}

const HONORIFICS: [&str; 14] = [
    "Mr.",
    "Mrs.",
    "Ms.",
    "Miss",
    "Dr.",
    "Sir",
    "Lady",
    "Lord",
    "Captain",
    "Colonel",
    "Professor",
    "Mr",
    "Mrs",
    "Dr",
];

fn stopwords(_lang: &str) -> &'static [&'static str] {
    &[
        "the",
        "a",
        "an",
        "and",
        "but",
        "or",
        "nor",
        "so",
        "yet",
        "for",
        "of",
        "in",
        "on",
        "at",
        "to",
        "by",
        "with",
        "from",
        "as",
        "it",
        "its",
        "he",
        "she",
        "they",
        "we",
        "you",
        "i",
        "his",
        "her",
        "their",
        "our",
        "your",
        "my",
        "this",
        "that",
        "these",
        "those",
        "there",
        "here",
        "then",
        "than",
        "when",
        "where",
        "while",
        "what",
        "which",
        "who",
        "whom",
        "why",
        "how",
        "if",
        "not",
        "no",
        "yes",
        "all",
        "any",
        "some",
        "such",
        "very",
        "more",
        "most",
        "much",
        "one",
        "two",
        "three",
        "first",
        "second",
        "last",
        "next",
        "new",
        "old",
        "good",
        "bad",
        "well",
        "now",
        "soon",
        "after",
        "before",
        "again",
        "once",
        "never",
        "ever",
        "always",
        "still",
        "just",
        "only",
        "even",
        "also",
        "chapter",
        "book",
        "part",
        "volume",
        "prologue",
        "epilogue",
        "contents",
        "page",
        "oh",
        "ah",
        "yet",
        "though",
        "however",
        "indeed",
        "perhaps",
        "upon",
        "into",
        "about",
        "over",
        "under",
        "between",
        "am",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "do",
        "does",
        "did",
        "have",
        "has",
        "had",
        "will",
        "would",
        "shall",
        "should",
        "can",
        "could",
        "may",
        "might",
        "must",
        "let",
        "us",
        "me",
        "him",
        "them",
        "dear",
        "god",
        "lord",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "hundred",
        "thousand",
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
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
        "christmas",
        "easter",
    ]
}

struct Mention {
    text: String,
    start: i64,
    len: i64,
    run_idx: usize,
}

fn is_cap_word(w: &str) -> bool {
    let mut chars = w.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {
            chars.all(|c| c.is_alphabetic() || c == '\'' || c == '’' || c == '-' || c == '.')
        }
        _ => false,
    }
}

/// Collects capitalized n-grams (1–3 words, honorific-aware) with positions.
fn mentions(runs: &[Run], stop: &[&str]) -> Vec<Mention> {
    let word_re = Regex::new(r"[\p{L}][\p{L}'’\-]*").unwrap();
    let mut out = Vec::new();
    for (ri, run) in runs.iter().enumerate() {
        // char offsets for position arithmetic
        let byte_to_char: Vec<usize> = {
            let mut v = vec![0; run.text.len() + 1];
            let mut count = 0;
            for (b, _) in run.text.char_indices() {
                v[b] = count;
                count += 1;
            }
            v[run.text.len()] = count;
            v
        };
        let words: Vec<(usize, usize, &str)> = word_re
            .find_iter(&run.text)
            .map(|m| (m.start(), m.end(), m.as_str()))
            .collect();
        let mut i = 0;
        while i < words.len() {
            let (s, _, w) = words[i];
            let is_honorific =
                HONORIFICS.contains(&w) || HONORIFICS.contains(&format!("{w}.").as_str());
            let shouting = w.chars().count() > 1 && w.chars().all(|c| !c.is_lowercase());
            if !(is_cap_word(w) || is_honorific)
                || shouting
                || (!is_honorific && stop.contains(&w.to_lowercase().as_str()))
            {
                i += 1;
                continue;
            }
            // extend while the following words are capitalized (max 3 words after an honorific);
            // a plain word before an honorific ("Consider Mr. Collins") is not part of the name
            let mut j = i + 1;
            let next_is_honorific =
                j < words.len() && HONORIFICS.contains(&format!("{}.", words[j].2).as_str());
            while j < words.len()
                && j - i < 3
                && !(j == i + 1 && !is_honorific && next_is_honorific)
            {
                let (ps, _, _) = words[j];
                let (_, pe, _) = words[j - 1];
                let gap = &run.text[pe..ps];
                let prev_honorific = HONORIFICS.contains(&format!("{}.", words[j - 1].2).as_str());
                let bridged = gap == " " || (prev_honorific && gap == ". ");
                if !bridged
                    || !is_cap_word(words[j].2)
                    || stop.contains(&words[j].2.to_lowercase().as_str())
                {
                    break;
                }
                j += 1;
            }
            let (_, e, _) = words[j - 1];
            let text = run.text[s..e].trim_end_matches('.').to_owned();
            let text = text
                .strip_suffix("’s")
                .or_else(|| text.strip_suffix("'s"))
                .map(str::to_owned)
                .unwrap_or(text);
            let text = if is_honorific && j == i + 1 {
                String::new()
            } else {
                text
            };
            let clean_lower = text.to_lowercase();
            if !text.is_empty() && !stop.contains(&clean_lower.as_str()) && text.chars().count() > 1
            {
                out.push(Mention {
                    len: text.chars().count() as i64,
                    text,
                    start: run.pid + byte_to_char[s] as i64,
                    run_idx: ri,
                });
            }
            i = j;
        }
    }
    out
}

/// Is the `.` at `dot` the end of an honorific ("Mr.") or an initial ("F.") rather than a
/// sentence?
fn abbreviation_dot(text: &str, dot: usize) -> bool {
    let before = &text[..dot];
    let word_start = before
        .rfind(|c: char| !c.is_alphabetic())
        .map(|i| i + 1)
        .unwrap_or(0);
    let word = &before[word_start..];
    word.chars().count() == 1 && word.chars().all(char::is_uppercase)
        || HONORIFICS.contains(&format!("{word}.").as_str())
}

fn is_terminator(text: &str, i: usize) -> bool {
    match text.as_bytes()[i] {
        b'!' | b'?' => true,
        b'.' => !abbreviation_dot(text, i),
        _ => false,
    }
}

/// The sentence containing the mention at `byte` (whose label spans `len` bytes).
fn sentence_around(text: &str, byte: usize, len: usize) -> String {
    let start = (0..byte)
        .rev()
        .find(|&i| is_terminator(text, i))
        .map(|i| i + 1)
        .unwrap_or(0);
    let from = (byte + len).min(text.len());
    let end = (from..text.len())
        .find(|&i| is_terminator(text, i))
        .map(|i| i + 1)
        .unwrap_or(text.len());
    let mut out = text[start..end].trim().to_owned();
    if out.chars().count() > MAX_QUOTE_CHARS {
        let cut = out
            .char_indices()
            .nth(MAX_QUOTE_CHARS)
            .map(|(b, _)| b)
            .unwrap_or(out.len());
        out.truncate(cut);
        out = out.trim_end().to_owned() + "…";
    }
    out
}

/// Longest first-mention quote we keep as a description (Gutenberg prefaces have 500-char
/// sentences).
pub const MAX_QUOTE_CHARS: usize = 300;

/// Entities from the text alone: recurring proper nouns, aliases merged, persons by honorific
/// or multi-word name, first-mention quote.
pub fn heuristic(runs: &[Run], lang: &str) -> Vec<Candidate> {
    let stop = stopwords(lang);
    let ms = mentions(runs, stop);
    // How often each word appears in lowercase: "Allow" at sentence starts is the verb when
    // "allow" shows up at least half as often; "Paris" never does.
    let word_re = Regex::new(r"[\p{L}][\p{L}'’\-]*").unwrap();
    let mut lower_counts: HashMap<String, usize> = HashMap::new();
    for r in runs {
        for m in word_re.find_iter(&r.text) {
            if m.as_str().chars().next().is_some_and(char::is_lowercase) {
                *lower_counts.entry(m.as_str().to_owned()).or_default() += 1;
            }
        }
    }
    // Group identical surface forms.
    let mut groups: HashMap<String, Vec<&Mention>> = HashMap::new();
    for m in &ms {
        groups.entry(m.text.clone()).or_default().push(m);
    }
    // Everything recurring is a candidate; stop words were excluded while scanning.
    let keep: Vec<(String, Vec<&Mention>)> = groups.into_iter().collect();
    // Alias merge: a single-word form that is a token of a multi-word form joins it.
    let mut labels: Vec<String> = keep.iter().map(|(l, _)| l.clone()).collect();
    labels.sort_by_key(|l| std::cmp::Reverse(l.len()));
    let counts: HashMap<&str, usize> = keep.iter().map(|(l, v)| (l.as_str(), v.len())).collect();
    let mut canonical: HashMap<String, String> = HashMap::new();
    let strip = |s: &str| {
        HONORIFICS.iter().fold(s.to_owned(), |acc, h| {
            acc.trim_start_matches(h).trim().to_owned()
        })
    };
    let has_honorific = |s: &str| {
        HONORIFICS
            .iter()
            .any(|h| s.strip_prefix(h).is_some_and(|r| r.starts_with(' ')))
    };
    for l in &labels {
        let base = strip(l);
        // "Mr. Bennet" is a different person from "Elizabeth Bennet": honorific forms and full
        // names stand on their own; only bare single tokens are aliases.
        if has_honorific(l) || base.contains(' ') {
            canonical.insert(l.clone(), l.clone());
            continue;
        }
        let my_count = counts.get(l.as_str()).copied().unwrap_or(0);
        let is_canonical = |other: &String| canonical.get(other).is_none_or(|c| c == other);
        // A longer form only absorbs a bare token when it is itself common: at least 3 mentions
        // and at least a third as frequent as the token (so "Elizabeth" is not filed under a rare
        // "Miss Elizabeth"). Honorific forms win over full names.
        let frequent = |o: &String| {
            let c = counts.get(o.as_str()).copied().unwrap_or(0);
            // a token too rare to stand alone joins any matching form rather than being lost
            my_count < 2 || (c >= 3 && c * 3 >= my_count)
        };
        let mut candidates: Vec<&String> = labels
            .iter()
            .filter(|o| *o != l && is_canonical(o) && frequent(o))
            .filter(|o| {
                (has_honorific(o) && strip(o) == base)
                    || (strip(o).contains(' ') && strip(o).split(' ').any(|t| t == base))
            })
            .collect();
        candidates.sort_by_key(|o| {
            (
                !has_honorific(o),
                std::cmp::Reverse(counts.get(o.as_str()).copied().unwrap_or(0)),
            )
        });
        canonical.insert(
            l.clone(),
            candidates
                .first()
                .map(|o| (*o).clone())
                .unwrap_or_else(|| l.clone()),
        );
    }
    // resolve chains
    let resolve = |l: &str| {
        let mut cur = l.to_owned();
        for _ in 0..4 {
            let next = canonical.get(&cur).cloned().unwrap_or(cur.clone());
            if next == cur {
                break;
            }
            cur = next;
        }
        cur
    };
    type Merged = (Vec<(i64, i64)>, Vec<String>, Option<(usize, i64)>);
    let mut merged: HashMap<String, Merged> = HashMap::new();
    let known: std::collections::HashSet<&str> = keep.iter().map(|(l, _)| l.as_str()).collect();
    for (label, v) in &keep {
        // A one-off full name ("Elizabeth Bennet") is not an entity of its own, but each of its
        // words that *is* a recurring name still counts as a mention of that name.
        if v.len() < 2 && !has_honorific(label) && label.contains(' ') {
            for m in v {
                let mut offset = 0i64;
                for w in label.split(' ') {
                    if known.contains(w) && !stop.contains(&w.to_lowercase().as_str()) {
                        let canon = resolve(w);
                        let e = merged
                            .entry(canon)
                            .or_insert_with(|| (Vec::new(), Vec::new(), None));
                        let start = m.start + offset;
                        e.0.push((start, w.chars().count() as i64));
                        if e.2.is_none_or(|(_, s)| start < s) {
                            e.2 = Some((m.run_idx, start));
                        }
                    }
                    offset += w.chars().count() as i64 + 1;
                }
            }
            continue;
        }
        let canon = resolve(label);
        let e = merged
            .entry(canon.clone())
            .or_insert_with(|| (Vec::new(), Vec::new(), None));
        if *label != canon {
            e.1.push(label.clone());
        }
        for m in v {
            e.0.push((m.start, m.len));
            if e.2.is_none_or(|(_, s)| m.start < s) {
                e.2 = Some((m.run_idx, m.start));
            }
        }
    }
    let mut out: Vec<Candidate> = merged
        .into_iter()
        .filter(|(_, (occ, _, _))| occ.len() >= 2)
        .filter(|(label, (occ, _, _))| {
            label.contains(' ')
                || lower_counts
                    .get(&label.to_lowercase())
                    .is_none_or(|&n| n * 2 < occ.len())
        })
        .map(|(label, (mut occ, aliases, first))| {
            occ.sort();
            let honorific = HONORIFICS
                .iter()
                .any(|h| label.strip_prefix(h).is_some_and(|r| r.starts_with(' ')))
                || aliases.iter().any(|a| {
                    HONORIFICS
                        .iter()
                        .any(|h| a.strip_prefix(h).is_some_and(|r| r.starts_with(' ')))
                });
            let speaks = speaks(runs, &label) || aliases.iter().any(|a| speaks(runs, a));
            let kind = if honorific || speaks {
                EntityKind::Person
            } else {
                EntityKind::Term
            };
            let quote = first
                .map(|(ri, start)| {
                    let run = &runs[ri];
                    let byte = run
                        .text
                        .char_indices()
                        .nth((start - run.pid).max(0) as usize)
                        .map(|(b, _)| b)
                        .unwrap_or(0);
                    sentence_around(&run.text, byte, label.len())
                })
                .unwrap_or_default();
            Candidate {
                label,
                kind,
                quote,
                occurrences: occ,
                aliases,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.occurrences
            .len()
            .cmp(&a.occurrences.len())
            .then(a.label.cmp(&b.label))
    });
    out.truncate(150);
    out
}

const SPEECH_VERBS: [&str; 16] = [
    "said",
    "says",
    "asked",
    "replied",
    "cried",
    "answered",
    "exclaimed",
    "continued",
    "added",
    "observed",
    "thought",
    "whispered",
    "laughed",
    "smiled",
    "sighed",
    "shouted",
];

/// Does the name appear directly before or after a speech verb anywhere? (novel heuristic)
fn speaks(runs: &[Run], name: &str) -> bool {
    let bare = HONORIFICS.iter().fold(name.to_owned(), |acc, h| {
        acc.trim_start_matches(h).trim().to_owned()
    });
    let last = bare.rsplit(' ').next().unwrap_or(&bare);
    if last.is_empty() {
        return false;
    }
    runs.iter().any(|r| {
        SPEECH_VERBS.iter().any(|v| {
            r.text.contains(&format!("{last} {v}")) || r.text.contains(&format!("{v} {last}"))
        })
    })
}
