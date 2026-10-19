// SPDX-License-Identifier: GPL-3.0-or-later
//! Kindle Word Wise: the `LanguageLayer.<lang>.<ASIN>.kll` sidecar, Deckle's Wiktionary-derived
//! lexicon, phrase matching over KFX text runs, and the `.klld` dictionaries the sidecar's
//! sense ids point into (Deckle's own, or the Kindle's when it has one). Formats from the
//! WordDumb and Proficiency projects (GPL-3).

pub mod device;
pub mod kll;
pub mod klld;
pub mod lexicon;
pub mod matcher;

/// Kindle part-of-speech labels (`pos_types` in a `.klld`), by id.
pub const POS_TYPES: [&str; 12] = [
    "noun",
    "verb",
    "adjective",
    "adverb",
    "article",
    "number",
    "conjunction",
    "other",
    "preposition",
    "pronoun",
    "particle",
    "punctuation",
];

/// Kindle `pos_types` id for a label; unknown labels are `other`.
pub fn pos_id(label: &str) -> i64 {
    POS_TYPES.iter().position(|p| *p == label).unwrap_or(7) as i64
}
