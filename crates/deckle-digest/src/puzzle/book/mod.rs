// SPDX-License-Identifier: GPL-3.0-or-later
//! Puzzle books (docs/superpowers/specs/2026-11-01-puzzle-books-design.md): a book of one kind
//! of puzzle, one per page with the answers at the back, written as a PDF you solve in ink on
//! a Kindle Scribe. The grids are always Deckle's own; a book's words can be your own.

pub mod pdf;

use std::collections::HashSet;
use std::fmt;

use chrono::NaiveDate;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::search::{self, Options, Search, Way};
use super::words::Words;

/// Grid sizes a word-search book takes (puzzle books §3.2).
pub const SEARCH_SIZES: std::ops::RangeInclusive<usize> = 8..=20;
/// Puzzles in a book.
pub const COUNTS: std::ops::RangeInclusive<usize> = 1..=200;
/// Words a word search hides at least, whatever its size.
const MIN_WORDS: usize = 8;
/// Fresh seeds tried for one puzzle before the book gives up.
const RESEEDS: u64 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Search,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Difficulty {
    Easy,
    #[default]
    Medium,
    Hard,
}

impl Difficulty {
    /// The ways a hidden word may read (puzzle books §3.2).
    pub fn ways(self) -> &'static [Way] {
        match self {
            Difficulty::Easy => &[Way::Across, Way::Down],
            Difficulty::Medium => &Way::ALL,
            Difficulty::Hard => &Way::EVERY,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Medium => "medium",
            Difficulty::Hard => "hard",
        }
    }
}

/// What to make. `seed` `None` takes [`default_seed`].
#[derive(Clone, Debug, PartialEq)]
pub struct BookOptions {
    pub kind: Kind,
    pub count: usize,
    pub size: usize,
    pub difficulty: Difficulty,
    /// Your own words (`--words`), already through [`words_from_file`].
    pub words: Option<Vec<String>>,
    pub title: Option<String>,
    pub seed: Option<u64>,
    /// Printed on the title page, and part of the default seed.
    pub date: NaiveDate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    pub title: String,
    /// "12×12 · medium · 10 puzzles".
    pub subtitle: String,
    pub date: NaiveDate,
    pub seed: u64,
    pub puzzles: Vec<Search>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookError {
    Size {
        min: usize,
        max: usize,
    },
    Count,
    /// Your words that fit the grid, fewer than a word search needs.
    TooFewWords(usize),
    /// A grid too fine to write in on this screen, and the largest that fits.
    DoesNotFit {
        size: usize,
        largest: usize,
    },
    /// Puzzle N wouldn't fill with any of its seeds.
    Unfilled(usize),
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BookError::Size { min, max } => {
                write!(f, "--size must be from {min} to {max} for a word search")
            }
            BookError::Count => write!(
                f,
                "--count must be from {} to {}",
                COUNTS.start(),
                COUNTS.end()
            ),
            BookError::TooFewWords(n) => write!(
                f,
                "--words has {n} usable words; a word search needs at least {MIN_WORDS} \
                 (3 letters or more, and no longer than the grid)"
            ),
            BookError::DoesNotFit { size, largest } if *largest == 0 => write!(
                f,
                "a {size}×{size} grid is too fine to write in on this screen, and so is every \
                 word search size"
            ),
            BookError::DoesNotFit { size, largest } => write!(
                f,
                "a {size}×{size} grid is too fine to write in on this screen; \
                 the largest that fits is {largest}×{largest}"
            ),
            BookError::Unfilled(n) => {
                write!(f, "puzzle {n} wouldn't fill; try another --seed")
            }
        }
    }
}

impl std::error::Error for BookError {}

/// A `--words` file's words: one a line, `#` lines skipped, folded to lowercase a–z
/// ("Café" → "cafe", "Wi-Fi" → "wifi"), each once, in the file's order.
pub fn words_from_file(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .map(|l| {
            deunicode::deunicode(l)
                .chars()
                .filter(char::is_ascii_alphabetic)
                .map(|c| c.to_ascii_lowercase())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty() && seen.insert(w.clone()))
        .collect()
}

/// FNV-1a, 64-bit: stable across machines and Rust versions, unlike `DefaultHasher`.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The seed a book takes when none is given: its options and its date, so one day's runs make
/// the same book and the next day's a new one (puzzle books §5.2).
pub fn default_seed(o: &BookOptions) -> u64 {
    fnv(&format!(
        "{:?}|{}|{}|{:?}|{:?}|{:?}|{}",
        o.kind, o.count, o.size, o.difficulty, o.words, o.title, o.date
    ))
}

/// Words a `size` search hides: about one per nine squares, 8 to 40.
pub fn words_wanted(size: usize) -> usize {
    (size * size / 9).clamp(MIN_WORDS, 40)
}

/// The book `o` asks for. The same options (and seed) make the same book on any machine.
pub fn make(o: &BookOptions, words: &Words) -> Result<Book, BookError> {
    match o.kind {
        Kind::Search => make_search(o, words),
    }
}

fn make_search(o: &BookOptions, words: &Words) -> Result<Book, BookError> {
    if !SEARCH_SIZES.contains(&o.size) {
        return Err(BookError::Size {
            min: *SEARCH_SIZES.start(),
            max: *SEARCH_SIZES.end(),
        });
    }
    if !COUNTS.contains(&o.count) {
        return Err(BookError::Count);
    }
    let size = o.size;
    let seed = o.seed.unwrap_or_else(|| default_seed(o));
    let want = words_wanted(size);
    let mine: Vec<String> = o
        .words
        .iter()
        .flatten()
        .filter(|w| (3..=size).contains(&w.len()))
        .cloned()
        .collect();
    if o.words.is_some() && mine.len() < MIN_WORDS {
        return Err(BookError::TooFewWords(mine.len()));
    }
    let mut list = mine.clone();
    list.shuffle(&mut StdRng::seed_from_u64(seed));
    let pool: Vec<String> = words
        .of_len(5)
        .iter()
        .chain(words.of_len(4))
        .filter(|w| !mine.contains(w))
        .cloned()
        .collect();
    let take = want.min(list.len());
    let mut puzzles = Vec::with_capacity(o.count);
    let mut warnings = Vec::new();
    for i in 0..o.count {
        // your words go round the book: each puzzle the next `want` of them, none twice in one
        let first: Vec<String> = (0..take)
            .map(|k| list[(i * want + k) % list.len()].clone())
            .collect();
        let mut made = None;
        'seeds: for again in 0..RESEEDS {
            let puzzle_seed = seed ^ ((i as u64) << 20) ^ (again << 40);
            for min in [(want * 4).div_ceil(5).max(MIN_WORDS), MIN_WORDS] {
                let opts = Options {
                    size,
                    ways: o.difficulty.ways(),
                    max: want,
                    min,
                };
                let mut rng = StdRng::seed_from_u64(puzzle_seed);
                if let Some(s) = search::fill_with(&first, &pool, &opts, &mut rng) {
                    made = Some(s);
                    break 'seeds;
                }
            }
        }
        let s = made.ok_or(BookError::Unfilled(i + 1))?;
        if !list.is_empty() && s.words.iter().filter(|h| h.theme).count() * 2 < s.words.len() {
            warnings.push(format!("puzzle {} is mostly built-in words", i + 1));
        }
        puzzles.push(s);
    }
    let plural = if o.count == 1 { "puzzle" } else { "puzzles" };
    Ok(Book {
        title: o.title.clone().unwrap_or_else(|| "Word Search".to_owned()),
        subtitle: format!(
            "{size}×{size} · {} · {} {plural}",
            o.difficulty.name(),
            o.count
        ),
        date: o.date,
        seed,
        puzzles,
        warnings,
    })
}

/// `<Title> <YYYY-MM-DD> <seed4>.pdf`, filesystem-safe and at most 120 characters: two books
/// never share a name, so one never lands on another's ink (puzzle books §5.2).
pub fn file_name(b: &Book) -> String {
    let tail = format!(" {} {:04x}.pdf", b.date, b.seed >> 48);
    let title = deckle_core::deliver::sanitize(&b.title);
    let title = if title.is_empty() {
        "Puzzles".to_owned()
    } else {
        title
    };
    let stem: String = title.chars().take(120 - tail.chars().count()).collect();
    format!("{}{tail}", stem.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn opts(count: usize, size: usize) -> BookOptions {
        BookOptions {
            kind: Kind::Search,
            count,
            size,
            difficulty: Difficulty::Medium,
            words: None,
            title: None,
            seed: None,
            date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        }
    }

    #[test]
    fn same_options_same_book() {
        let w = Words::embedded();
        let a = make(&opts(3, 12), w).unwrap();
        let b = make(&opts(3, 12), w).unwrap();
        assert_eq!(a.seed, b.seed);
        assert_eq!(a.puzzles, b.puzzles);
        let mut other = opts(3, 12);
        other.date = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        assert_ne!(
            make(&other, w).unwrap().seed,
            a.seed,
            "tomorrow's book is new"
        );
    }

    #[test]
    fn count_and_size_respected() {
        let b = make(&opts(3, 15), Words::embedded()).unwrap();
        assert_eq!(b.puzzles.len(), 3);
        assert!(b.puzzles.iter().all(|p| p.rows.len() == 15));
        assert!(b.puzzles.iter().all(|p| (20..=25).contains(&p.words.len())));
        assert_eq!(b.title, "Word Search");
        assert_eq!(b.subtitle, "15×15 · medium · 3 puzzles");
    }

    #[test]
    fn user_words_lead_and_repeat_only_across_puzzles() {
        let list: Vec<String> = [
            "python", "kernel", "router", "pixel", "laptop", "server", "cache", "socket", "binary",
            "cursor", "widget", "browser",
        ]
        .map(String::from)
        .into();
        let mut o = opts(3, 12);
        o.words = Some(list.clone());
        let b = make(&o, Words::embedded()).unwrap();
        for p in &b.puzzles {
            let themes: Vec<&String> = p
                .words
                .iter()
                .filter(|h| h.theme)
                .map(|h| &h.word)
                .collect();
            assert!(themes.len() >= 8, "{themes:?}");
            assert!(themes.iter().all(|t| list.contains(t)));
            let mut seen = std::collections::HashSet::new();
            assert!(p.words.iter().all(|h| seen.insert(h.word.clone())));
        }
    }

    #[test]
    fn words_from_file_folds_and_dedupes() {
        assert_eq!(
            words_from_file("Café\nWi-Fi\nwifi\n# x\n\nnaïve\n"),
            ["cafe", "wifi", "naive"]
        );
    }

    #[test]
    fn nested_user_words_still_fill() {
        let mut o = opts(2, 10);
        o.words = Some(
            [
                "sun", "sunset", "moon", "star", "comet", "orbit", "venus", "mars", "pluto",
            ]
            .map(String::from)
            .into(),
        );
        let b = make(&o, Words::embedded()).unwrap();
        for p in &b.puzzles {
            assert!(p.words.len() >= 8);
            let both = ["sun", "sunset"]
                .iter()
                .filter(|w| p.words.iter().any(|h| h.word == **w))
                .count();
            assert!(both <= 1, "a word inside another is found twice");
        }
    }

    #[test]
    fn too_few_user_words_is_refused() {
        let mut o = opts(2, 10);
        o.words = Some(
            ["sun", "moon", "a", "elevenletter"]
                .map(String::from)
                .into(),
        );
        assert_eq!(make(&o, Words::embedded()), Err(BookError::TooFewWords(2)));
    }

    #[test]
    fn size_and_count_out_of_range_are_refused() {
        for size in [7, 21] {
            assert_eq!(
                make(&opts(1, size), Words::embedded()),
                Err(BookError::Size { min: 8, max: 20 })
            );
        }
        for count in [0, 201] {
            assert_eq!(
                make(&opts(count, 12), Words::embedded()),
                Err(BookError::Count)
            );
        }
    }

    #[test]
    fn file_name_is_dated_and_seeded() {
        let mut b = make(&opts(1, 8), Words::embedded()).unwrap();
        b.seed = 0x7f3a_0000_0000_0001;
        assert_eq!(file_name(&b), "Word Search 2026-11-01 7f3a.pdf");
        b.title = "Tech: A/B?".into();
        assert_eq!(file_name(&b), "Tech_ A_B_ 2026-11-01 7f3a.pdf");
    }
}
