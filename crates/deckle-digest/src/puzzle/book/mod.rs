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

use super::cross::{self, Crossword, Lexicon};
use super::search::{self, Options, Search, Way};
use super::words::Words;

/// Grid sizes a word-search book takes (puzzle books §3.2).
pub const SEARCH_SIZES: std::ops::RangeInclusive<usize> = 8..=20;
/// Grid sizes a crossword book takes (puzzle books §3.3).
pub const CROSS_SIZES: std::ops::RangeInclusive<usize> = 5..=15;
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
    Crossword,
}

impl Kind {
    /// "word search", "crossword".
    pub fn name(self) -> &'static str {
        match self {
            Kind::Search => "word search",
            Kind::Crossword => "crossword",
        }
    }

    /// The sizes its grids take.
    pub fn sizes(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Kind::Search => SEARCH_SIZES,
            Kind::Crossword => CROSS_SIZES,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Kind::Search => "Word Search",
            Kind::Crossword => "Crossword",
        }
    }
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

    /// How far a crossword's fill lets rarer words in among the common ones (puzzle books
    /// §3.3): everyday words for `easy`, the whole list mixed for `hard`.
    pub fn spread(self) -> f32 {
        match self {
            Difficulty::Easy => 0.5,
            Difficulty::Medium => 1.5,
            Difficulty::Hard => 4.0,
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
    pub puzzles: Puzzles,
    pub warnings: Vec<String>,
}

/// A book's puzzles: all of one kind.
#[derive(Clone, Debug, PartialEq)]
pub enum Puzzles {
    Search(Vec<Search>),
    Crossword(Vec<Crossword>),
}

impl Puzzles {
    pub fn len(&self) -> usize {
        match self {
            Puzzles::Search(p) => p.len(),
            Puzzles::Crossword(p) => p.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Squares on a side of its grids.
    pub fn size(&self) -> usize {
        match self {
            Puzzles::Search(p) => p.first().map_or(0, Search::size),
            Puzzles::Crossword(p) => p.first().map_or(0, |x| x.rows.len()),
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Puzzles::Search(_) => Kind::Search,
            Puzzles::Crossword(_) => Kind::Crossword,
        }
    }
}

impl Book {
    /// Its word searches; none in a crossword book.
    pub fn searches(&self) -> &[Search] {
        match &self.puzzles {
            Puzzles::Search(p) => p,
            Puzzles::Crossword(_) => &[],
        }
    }

    /// Its crosswords; none in a word-search book.
    pub fn crosswords(&self) -> &[Crossword] {
        match &self.puzzles {
            Puzzles::Crossword(p) => p,
            Puzzles::Search(_) => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookError {
    Size {
        kind: Kind,
        min: usize,
        max: usize,
    },
    Count,
    /// Your words that fit the grid, fewer than a word search needs.
    TooFewWords(usize),
    /// `--words` for a kind whose words need clues (a crossword, until LLM clues).
    WordsNeedClues,
    /// A grid too fine to write in on this screen, and the largest that fits.
    DoesNotFit {
        kind: Kind,
        size: usize,
        largest: usize,
    },
    /// Puzzle N wouldn't fill with any of its seeds.
    Unfilled(usize),
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BookError::Size { kind, min, max } => {
                write!(
                    f,
                    "--size must be from {min} to {max} for a {}",
                    kind.name()
                )
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
            BookError::WordsNeedClues => write!(
                f,
                "--words isn't for crosswords yet: their words need clues"
            ),
            BookError::DoesNotFit {
                kind,
                size,
                largest,
            } if *largest == 0 => write!(
                f,
                "a {size}×{size} grid is too fine to write in on this screen, and so is every \
                 {} size",
                kind.name()
            ),
            BookError::DoesNotFit { size, largest, .. } => write!(
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
        Kind::Crossword => make_crossword(o, Lexicon::embedded()),
    }
}

/// The size and count checks every kind shares.
fn check(o: &BookOptions) -> Result<(), BookError> {
    let sizes = o.kind.sizes();
    if !sizes.contains(&o.size) {
        return Err(BookError::Size {
            kind: o.kind,
            min: *sizes.start(),
            max: *sizes.end(),
        });
    }
    if !COUNTS.contains(&o.count) {
        return Err(BookError::Count);
    }
    Ok(())
}

fn finish(o: &BookOptions, seed: u64, puzzles: Puzzles, warnings: Vec<String>) -> Book {
    let plural = if o.count == 1 { "puzzle" } else { "puzzles" };
    let size = o.size;
    Book {
        title: o.title.clone().unwrap_or_else(|| o.kind.title().to_owned()),
        subtitle: format!(
            "{size}×{size} · {} · {} {plural}",
            o.difficulty.name(),
            o.count
        ),
        date: o.date,
        seed,
        puzzles,
        warnings,
    }
}

/// A crossword book: each puzzle filled from the whole list, preferring common words as the
/// difficulty asks.
fn make_crossword(o: &BookOptions, lex: &Lexicon) -> Result<Book, BookError> {
    check(o)?;
    if o.words.is_some() {
        return Err(BookError::WordsNeedClues);
    }
    let seed = o.seed.unwrap_or_else(|| default_seed(o));
    let mut puzzles = Vec::with_capacity(o.count);
    for i in 0..o.count {
        let mut rng = StdRng::seed_from_u64(seed ^ ((i as u64) << 20));
        let x = cross::fill(lex, o.size, o.difficulty.spread(), &mut rng)
            .ok_or(BookError::Unfilled(i + 1))?;
        puzzles.push(x);
    }
    Ok(finish(o, seed, Puzzles::Crossword(puzzles), Vec::new()))
}

fn make_search(o: &BookOptions, words: &Words) -> Result<Book, BookError> {
    check(o)?;
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
                    book: true,
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
    Ok(finish(o, seed, Puzzles::Search(puzzles), warnings))
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
        assert!(b.searches().iter().all(|p| p.rows.len() == 15));
        assert!(
            b.searches()
                .iter()
                .all(|p| (20..=25).contains(&p.words.len()))
        );
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
        for p in b.searches() {
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
        for p in b.searches() {
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
                Err(BookError::Size {
                    kind: Kind::Search,
                    min: 8,
                    max: 20
                })
            );
        }
        for count in [0, 201] {
            assert_eq!(
                make(&opts(count, 12), Words::embedded()),
                Err(BookError::Count)
            );
        }
    }

    fn with_words(size: usize, difficulty: Difficulty, words: &[&str]) -> BookOptions {
        let mut o = opts(3, size);
        o.difficulty = difficulty;
        o.words = Some(words.iter().map(|w| w.to_string()).collect());
        o
    }

    /// Your words are taken as given (§3.2): CLASS spells a blocklisted string inside itself,
    /// which isn't the filler's doing.
    #[test]
    fn own_words_with_blocked_strings_inside_still_fill() {
        let words = [
            "classroom",
            "title",
            "night",
            "shell",
            "skill",
            "japan",
            "muffin",
            "canal",
            "soldier",
            "studies",
        ];
        let b = make(
            &with_words(12, Difficulty::Medium, &words),
            Words::embedded(),
        )
        .unwrap();
        assert!(
            b.searches()
                .iter()
                .all(|p| p.words.iter().filter(|h| h.theme).count() >= 8)
        );
    }

    /// A palindrome reads backwards on its own squares, and STAR reads RATS backwards: in a
    /// hard book neither is a second copy to circle.
    #[test]
    fn hard_takes_palindromes_and_reversed_pairs() {
        let words = [
            "kayak", "level", "noon", "radar", "star", "rats", "live", "evil", "stressed",
            "desserts",
        ];
        let b = make(&with_words(12, Difficulty::Hard, &words), Words::embedded()).unwrap();
        for p in b.searches() {
            assert!(p.words.iter().any(|h| h.word == "kayak"));
        }
    }

    /// The filler mustn't spell a short word again: a big hard grid of three-letter words
    /// still fills.
    #[test]
    fn short_words_fill_big_hard_grids() {
        let words = [
            "the", "tea", "sea", "ten", "ant", "art", "ear", "eat", "oat", "rat", "tan", "net",
        ];
        let b = make(&with_words(20, Difficulty::Hard, &words), Words::embedded()).unwrap();
        assert!(
            b.searches()
                .iter()
                .all(|p| p.words.iter().filter(|h| h.theme).count() >= 6)
        );
    }

    fn crossword(count: usize, size: usize, difficulty: Difficulty) -> BookOptions {
        let mut o = opts(count, size);
        o.kind = Kind::Crossword;
        o.difficulty = difficulty;
        o
    }

    #[test]
    fn a_crossword_book_has_its_count_and_size() {
        let b = make(&crossword(2, 11, Difficulty::Medium), Words::embedded()).unwrap();
        assert_eq!(b.crosswords().len(), 2);
        assert!(b.searches().is_empty());
        assert!(b.crosswords().iter().all(|x| x.rows.len() == 11));
        assert_eq!(b.title, "Crossword");
        assert_eq!(b.subtitle, "11×11 · medium · 2 puzzles");
        assert_eq!(
            make(&crossword(2, 11, Difficulty::Medium), Words::embedded()),
            Ok(b)
        );
    }

    /// The commonest words at the biggest size still fill.
    #[test]
    fn an_easy_fifteen_still_builds() {
        let b = make(&crossword(1, 15, Difficulty::Easy), Words::embedded()).unwrap();
        assert_eq!(b.crosswords()[0].rows.len(), 15);
    }

    #[test]
    fn crossword_sizes_and_words_are_checked() {
        for size in [4, 16] {
            assert_eq!(
                make(&crossword(1, size, Difficulty::Medium), Words::embedded()),
                Err(BookError::Size {
                    kind: Kind::Crossword,
                    min: 5,
                    max: 15
                })
            );
        }
        let mut o = crossword(1, 9, Difficulty::Medium);
        o.words = Some(vec!["router".into()]);
        assert_eq!(make(&o, Words::embedded()), Err(BookError::WordsNeedClues));
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
