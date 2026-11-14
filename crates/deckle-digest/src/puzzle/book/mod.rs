// SPDX-License-Identifier: GPL-3.0-or-later
//! Puzzle books (docs/superpowers/specs/2026-11-01-puzzle-books-design.md): a book of one kind
//! of puzzle, one per page with the answers at the back, written as a PDF you solve in ink on
//! a Kindle Scribe. The grids are always Deckle's own; a book's words can be your own.

pub mod bundle;
pub mod pdf;
pub mod plan;
pub mod sunday;
pub mod theme;

use std::collections::HashSet;
use std::fmt;

use chrono::NaiveDate;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::cross::{self, Crossword, Lexicon};
use super::cryptogram::{self, Cryptogram};
use super::ladder::{self, Ladder};
use super::maze::{self, Maze};
use super::nonogram::{self, Nonogram};
use super::search::{self, Options, Search, Way};
use super::sudoku::{self, Grade, Sudoku};
use super::words::Words;

/// Grid sizes a word-search book takes (puzzle books §3.2).
pub const SEARCH_SIZES: std::ops::RangeInclusive<usize> = 8..=20;
/// Grid sizes a crossword book takes (puzzle books §3.3).
pub const CROSS_SIZES: std::ops::RangeInclusive<usize> = 5..=15;
/// Puzzles in a book.
pub const COUNTS: std::ops::RangeInclusive<usize> = 1..=200;
/// Words a word search hides at least, whatever its size.
const MIN_WORDS: usize = 8;
/// Clued words a crossword book needs, at least, and the theme answers a puzzle should have
/// before the book warns (puzzle books §3.5).
const MIN_CLUED: usize = 3;
/// Fresh seeds tried for one puzzle before the book gives up.
const RESEEDS: u64 = 8;
/// Ladders on a page: one is a few rungs, three make an evening.
pub const LADDERS_PER_PAGE: usize = 3;
/// Quotes a cryptogram book needs, at least, when they're your own.
const MIN_QUOTES: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Search,
    Crossword,
    Sudoku,
    Ladder,
    Cryptogram,
    Nonogram,
    Maze,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Crossword,
        Kind::Search,
        Kind::Sudoku,
        Kind::Ladder,
        Kind::Cryptogram,
        Kind::Nonogram,
        Kind::Maze,
    ];

    /// "word search", "crossword".
    pub fn name(self) -> &'static str {
        match self {
            Kind::Search => "word search",
            Kind::Crossword => "crossword",
            Kind::Sudoku => "sudoku",
            Kind::Ladder => "word ladder",
            Kind::Cryptogram => "cryptogram",
            Kind::Nonogram => "nonogram",
            Kind::Maze => "maze",
        }
    }

    /// The `deckle puzzles` subcommand, and the kind's name in a saved book's options.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Search => "search",
            Kind::Crossword => "crossword",
            Kind::Sudoku => "sudoku",
            Kind::Ladder => "ladder",
            Kind::Cryptogram => "cryptogram",
            Kind::Nonogram => "nonogram",
            Kind::Maze => "maze",
        }
    }

    /// The sizes its grids take. A ladder, a cryptogram and a sudoku have one size.
    pub fn sizes(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Kind::Search => SEARCH_SIZES,
            Kind::Crossword => CROSS_SIZES,
            Kind::Sudoku => 9..=9,
            Kind::Ladder => 4..=4,
            Kind::Cryptogram => 0..=0,
            Kind::Nonogram => nonogram::SIZES,
            Kind::Maze => maze::SIZES,
        }
    }

    /// The size a book of this kind takes when none is asked for.
    pub fn default_size(self) -> usize {
        match self {
            Kind::Search => 12,
            Kind::Crossword => 11,
            Kind::Sudoku => 9,
            Kind::Ladder => 4,
            Kind::Cryptogram => 0,
            Kind::Nonogram => 15,
            Kind::Maze => 20,
        }
    }

    /// Whether `--size` means anything for it.
    pub fn sized(self) -> bool {
        let s = self.sizes();
        s.start() != s.end()
    }

    pub fn title(self) -> &'static str {
        match self {
            Kind::Search => "Word Search",
            Kind::Crossword => "Crossword",
            Kind::Sudoku => "Sudoku",
            Kind::Ladder => "Word Ladders",
            Kind::Cryptogram => "Cryptograms",
            Kind::Nonogram => "Nonograms",
            Kind::Maze => "Mazes",
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

    /// The sudoku grade it asks for (puzzle books §3.4).
    pub fn grade(self) -> Grade {
        match self {
            Difficulty::Easy => Grade::Easy,
            Difficulty::Medium => Grade::Medium,
            Difficulty::Hard => Grade::Hard,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Medium => "medium",
            Difficulty::Hard => "hard",
        }
    }

    /// A ladder's rungs (words between the ends, plus one): 3 for easy, 4, 5 and 6 for hard.
    pub fn rungs(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Difficulty::Easy => 3..=3,
            Difficulty::Medium => 4..=4,
            Difficulty::Hard => 5..=6,
        }
    }

    /// Letters a cryptogram gives away to start.
    pub fn reveals(self) -> usize {
        match self {
            Difficulty::Easy => 3,
            Difficulty::Medium => 1,
            Difficulty::Hard => 0,
        }
    }

    /// The letters a cryptogram's quote has, at most: short for easy, any for hard.
    pub fn quote_letters(self) -> usize {
        match self {
            Difficulty::Easy => 60,
            Difficulty::Medium => 90,
            Difficulty::Hard => *cryptogram::LETTERS.end(),
        }
    }
}

/// A page of word ladders (puzzle books: three to a page).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LadderPage {
    pub ladders: Vec<Ladder>,
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
    /// A crossword's own words, each with its clue (`--words` for a crossword), already
    /// through [`clued_words_from_file`].
    pub clued: Option<Vec<(String, String)>>,
    pub title: Option<String>,
    pub seed: Option<u64>,
    /// Printed on the title page, and part of the default seed.
    pub date: NaiveDate,
    /// A cryptogram book's own quotes, each with who said it, already through
    /// [`cryptogram::quotes_from_file`].
    pub quotes: Option<Vec<(String, String)>>,
    /// A nonogram book's own picture (the file's bytes and its name): puzzle 1; the rest are
    /// Deckle's pictograms and shapes.
    pub picture: Option<(Vec<u8>, String)>,
}

impl BookOptions {
    pub fn with_count(mut self, count: usize) -> BookOptions {
        self.count = count;
        self
    }

    /// A book of `kind` with its defaults, dated `date`.
    pub fn new(kind: Kind, date: NaiveDate) -> BookOptions {
        BookOptions {
            kind,
            count: 10,
            size: kind.default_size(),
            difficulty: Difficulty::Medium,
            words: None,
            clued: None,
            title: None,
            seed: None,
            date,
            quotes: None,
            picture: None,
        }
    }
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
    Sudoku(Vec<Sudoku>),
    Ladder(Vec<LadderPage>),
    Cryptogram(Vec<Cryptogram>),
    Nonogram(Vec<Nonogram>),
    Maze(Vec<Maze>),
}

impl Puzzles {
    pub fn len(&self) -> usize {
        match self {
            Puzzles::Search(p) => p.len(),
            Puzzles::Crossword(p) => p.len(),
            Puzzles::Sudoku(p) => p.len(),
            Puzzles::Ladder(p) => p.len(),
            Puzzles::Cryptogram(p) => p.len(),
            Puzzles::Nonogram(p) => p.len(),
            Puzzles::Maze(p) => p.len(),
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
            Puzzles::Sudoku(_) => 9,
            Puzzles::Ladder(_) => 4,
            Puzzles::Cryptogram(_) => 0,
            Puzzles::Nonogram(p) => p.first().map_or(0, Nonogram::size),
            Puzzles::Maze(p) => p.first().map_or(0, |m| m.size),
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Puzzles::Search(_) => Kind::Search,
            Puzzles::Crossword(_) => Kind::Crossword,
            Puzzles::Sudoku(_) => Kind::Sudoku,
            Puzzles::Ladder(_) => Kind::Ladder,
            Puzzles::Cryptogram(_) => Kind::Cryptogram,
            Puzzles::Nonogram(_) => Kind::Nonogram,
            Puzzles::Maze(_) => Kind::Maze,
        }
    }
}

impl Book {
    /// Its word searches; none in a crossword book.
    pub fn searches(&self) -> &[Search] {
        match &self.puzzles {
            Puzzles::Search(p) => p,
            _ => &[],
        }
    }

    /// Its crosswords; none in a word-search book.
    pub fn crosswords(&self) -> &[Crossword] {
        match &self.puzzles {
            Puzzles::Crossword(p) => p,
            _ => &[],
        }
    }

    /// Its sudoku; none in another kind of book.
    pub fn sudokus(&self) -> &[Sudoku] {
        match &self.puzzles {
            Puzzles::Sudoku(p) => p,
            _ => &[],
        }
    }

    pub fn ladders(&self) -> &[LadderPage] {
        match &self.puzzles {
            Puzzles::Ladder(p) => p,
            _ => &[],
        }
    }

    pub fn cryptograms(&self) -> &[Cryptogram] {
        match &self.puzzles {
            Puzzles::Cryptogram(p) => p,
            _ => &[],
        }
    }

    pub fn nonograms(&self) -> &[Nonogram] {
        match &self.puzzles {
            Puzzles::Nonogram(p) => p,
            _ => &[],
        }
    }

    pub fn mazes(&self) -> &[Maze] {
        match &self.puzzles {
            Puzzles::Maze(p) => p,
            _ => &[],
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
    /// Your clued words that fit a crossword, fewer than one needs.
    TooFewClued(usize),
    /// `--words` for a kind that takes none: a crossword (until LLM clues) or a sudoku.
    NoWords(Kind),
    /// A grid too fine to write in on this screen, and the largest that fits.
    DoesNotFit {
        kind: Kind,
        size: usize,
        largest: usize,
    },
    /// Puzzle N couldn't be made with any of its seeds.
    Unfilled(usize),
    /// `--quotes` had no quote of a usable length.
    TooFewQuotes,
    /// `--picture` couldn't be read, with why.
    BadPicture(String),
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BookError::Size { kind, min, max } if min == max => {
                write!(
                    f,
                    "a {} is always {min}×{min}: leave out --size",
                    kind.name()
                )
            }
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
            BookError::TooFewClued(n) => write!(
                f,
                "--words has {n} usable words; a crossword needs at least {MIN_CLUED} (3 to {} \
                 letters, each with a clue after a colon or a tab)",
                cross::LONGEST
            ),
            BookError::NoWords(Kind::Sudoku) => write!(f, "--words isn't for sudoku"),
            BookError::NoWords(_) => write!(
                f,
                "--words isn't for crosswords yet: their words need clues"
            ),
            BookError::DoesNotFit {
                kind: Kind::Sudoku, ..
            } => write!(
                f,
                "a sudoku's squares would be too small to write in on this screen"
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
                write!(f, "puzzle {n} couldn't be made; try another --seed")
            }
            BookError::TooFewQuotes => write!(
                f,
                "--quotes has no quote of {} to {} letters",
                cryptogram::LETTERS.start(),
                cryptogram::LETTERS.end()
            ),
            BookError::BadPicture(why) => write!(f, "--picture: {why}"),
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

/// A crossword's `--words` file: `word: clue` lines (a tab instead of the colon works too),
/// `#` lines skipped. Words fold as in [`words_from_file`]; each is kept once, its first clue.
/// Also the words that had no clue, folded, for a warning.
pub fn clued_words_from_file(text: &str) -> (Vec<(String, String)>, Vec<String>) {
    let fold = |w: &str| -> String {
        deunicode::deunicode(w)
            .chars()
            .filter(char::is_ascii_alphabetic)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    };
    let mut seen = HashSet::new();
    let (mut words, mut skipped) = (Vec::new(), Vec::new());
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let split = line
            .find(['\t', ':'])
            .map(|i| (&line[..i], line[i + 1..].trim()));
        match split {
            Some((w, clue)) if !clue.is_empty() => {
                let w = fold(w);
                if !w.is_empty() && seen.insert(w.clone()) {
                    words.push((w, clue.to_owned()));
                }
            }
            _ => {
                let w = fold(line.split(['\t', ':']).next().unwrap_or(""));
                if !w.is_empty() {
                    skipped.push(w);
                }
            }
        }
    }
    (words, skipped)
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
    let picture = o
        .picture
        .as_ref()
        .map(|(bytes, name)| format!("{name}:{}", bytes.len()));
    fnv(&format!(
        "{:?}|{}|{}|{:?}|{:?}|{:?}|{:?}|{}|{:?}|{:?}",
        o.kind, o.count, o.size, o.difficulty, o.words, o.clued, o.title, o.date, o.quotes, picture
    ))
}

/// Words a `size` search hides: about one per nine squares, 8 to 40.
pub fn words_wanted(size: usize) -> usize {
    (size * size / 9).clamp(MIN_WORDS, 40)
}

/// The book `o` asks for. The same options (and seed) make the same book on any machine.
pub fn make(o: &BookOptions, words: &Words) -> Result<Book, BookError> {
    make_with(o, words, &mut |_, _| {})
}

/// [`make`], telling `progress` after each puzzle: `(made, of)`.
pub fn make_with(
    o: &BookOptions,
    words: &Words,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Book, BookError> {
    match o.kind {
        Kind::Search => make_search(o, words, progress),
        Kind::Crossword => make_crossword(o, Lexicon::embedded(), progress),
        Kind::Sudoku => make_sudoku(o, progress),
        Kind::Ladder => make_ladders(o, words, progress),
        Kind::Cryptogram => make_cryptograms(o, progress),
        Kind::Nonogram => make_nonograms(o, progress),
        Kind::Maze => make_mazes(o, progress),
    }
}

/// A book of mazes: each from its own seed.
fn make_mazes(o: &BookOptions, progress: &mut dyn FnMut(usize, usize)) -> Result<Book, BookError> {
    check(o)?;
    let seed = o.seed.unwrap_or_else(|| default_seed(o));
    let mut puzzles = Vec::with_capacity(o.count);
    for i in 0..o.count {
        let mut rng = StdRng::seed_from_u64(seed ^ ((i as u64) << 20));
        puzzles.push(maze::make(o.size, &mut rng));
        progress(i + 1, o.count);
    }
    Ok(finish(o, seed, Puzzles::Maze(puzzles), Vec::new()))
}

/// A book of nonograms: your picture first when there is one, then Deckle's pictograms in
/// an order from the seed, then random shapes; every one solves by line logic, or the book
/// says which don't.
fn make_nonograms(
    o: &BookOptions,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Book, BookError> {
    check(o)?;
    let seed = o.seed.unwrap_or_else(|| default_seed(o));
    let mut warnings = Vec::new();
    let mut puzzles = Vec::with_capacity(o.count);
    if let Some((bytes, name)) = &o.picture {
        let n = nonogram::from_picture(bytes, o.size, name).map_err(BookError::BadPicture)?;
        if !n.line_solvable() {
            warnings.push("puzzle 1, your picture, needs a guess or two to finish".to_owned());
        }
        puzzles.push(n);
        progress(1, o.count);
    }
    let mut names: Vec<&str> = nonogram::PICTOGRAMS.iter().map(|(n, _)| *n).collect();
    names.shuffle(&mut StdRng::seed_from_u64(seed));
    let mut next = 0;
    while puzzles.len() < o.count {
        let i = puzzles.len();
        let mut rng = StdRng::seed_from_u64(seed ^ ((i as u64) << 20));
        let n = names
            .get(next)
            .and_then(|name| nonogram::pictogram(name))
            .map(|p| nonogram::scaled(&p, o.size))
            .filter(Nonogram::line_solvable);
        next += 1;
        puzzles.push(n.unwrap_or_else(|| nonogram::random(o.size, &mut rng)));
        progress(i + 1, o.count);
    }
    Ok(finish(o, seed, Puzzles::Nonogram(puzzles), warnings))
}

/// A book of cryptograms: your quotes, or Deckle's, each with a key of its own and the
/// difficulty's reveals; quotes go round the book, never twice until all are used.
fn make_cryptograms(
    o: &BookOptions,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Book, BookError> {
    check(o)?;
    let seed = o.seed.unwrap_or_else(|| default_seed(o));
    let own = o.quotes.as_ref();
    let max = o.difficulty.quote_letters();
    let letters = |q: &str| q.chars().filter(char::is_ascii_alphabetic).count();
    let mut pool: Vec<(String, String)> = match own {
        Some(q) => q.clone(),
        None => cryptogram::QUOTES
            .iter()
            .filter_map(|(q, s)| cryptogram::clean(q).map(|q| (q, (*s).to_owned())))
            .collect(),
    };
    if own.is_some() && pool.len() < MIN_QUOTES {
        return Err(BookError::TooFewQuotes);
    }
    // the difficulty's length first; longer ones only when the short run out
    pool.sort_by_key(|(q, _)| letters(q) > max);
    let short = pool.iter().filter(|(q, _)| letters(q) <= max).count();
    let mut rng = StdRng::seed_from_u64(seed);
    pool[..short].shuffle(&mut rng);
    pool[short..].shuffle(&mut rng);
    let mut puzzles = Vec::with_capacity(o.count);
    for i in 0..o.count {
        let (q, s) = &pool[i % pool.len()];
        let mut rng = StdRng::seed_from_u64(seed ^ ((i as u64) << 20));
        puzzles.push(cryptogram::make(q, s, o.difficulty.reveals(), &mut rng));
        progress(i + 1, o.count);
    }
    let mut warnings = Vec::new();
    if o.count > pool.len() {
        warnings.push(format!(
            "{} quotes for {} puzzles: some repeat with a new key",
            pool.len(),
            o.count
        ));
    }
    Ok(finish(o, seed, Puzzles::Cryptogram(puzzles), warnings))
}

/// A book of ladder pages: [`LADDERS_PER_PAGE`] ladders a page, each the difficulty's rungs,
/// from Deckle's pairs first and then from any two words that far apart; no pair twice.
fn make_ladders(
    o: &BookOptions,
    words: &Words,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Book, BookError> {
    check(o)?;
    let seed = o.seed.unwrap_or_else(|| default_seed(o));
    let want = o.difficulty.rungs();
    let mut rng = StdRng::seed_from_u64(seed);
    let mut pairs: Vec<(&str, &str)> = ladder::PAIRS.to_vec();
    pairs.shuffle(&mut rng);
    let mut fitting = pairs.into_iter().filter_map(|(from, to)| {
        let path = ladder::shortest(words, from, to)?;
        want.contains(&(path.len() - 1)).then_some(Ladder {
            from: from.to_owned(),
            to: to.to_owned(),
            path,
        })
    });
    let mut used: HashSet<(String, String)> = HashSet::new();
    let mut pages = Vec::with_capacity(o.count);
    let warnings = Vec::new();
    for i in 0..o.count {
        let mut ladders = Vec::with_capacity(LADDERS_PER_PAGE);
        while ladders.len() < LADDERS_PER_PAGE {
            let next = fitting
                .next()
                .or_else(|| ladder::random(words, &want, &mut rng, &used));
            match next {
                Some(l) if used.insert((l.from.clone(), l.to.clone())) => ladders.push(l),
                Some(_) => continue,
                None => return Err(BookError::Unfilled(i + 1)),
            }
        }
        pages.push(LadderPage { ladders });
        progress(i + 1, o.count);
    }
    Ok(finish(o, seed, Puzzles::Ladder(pages), warnings))
}

/// A sudoku book: each puzzle of the difficulty's grade (puzzle books §3.4).
fn make_sudoku(o: &BookOptions, progress: &mut dyn FnMut(usize, usize)) -> Result<Book, BookError> {
    check(o)?;
    if o.words.is_some() {
        return Err(BookError::NoWords(Kind::Sudoku));
    }
    let seed = o.seed.unwrap_or_else(|| default_seed(o));
    let mut puzzles = Vec::with_capacity(o.count);
    for i in 0..o.count {
        let mut rng = StdRng::seed_from_u64(seed ^ ((i as u64) << 20));
        puzzles
            .push(sudoku::make(o.difficulty.grade(), &mut rng).ok_or(BookError::Unfilled(i + 1))?);
        progress(i + 1, o.count);
    }
    Ok(finish(o, seed, Puzzles::Sudoku(puzzles), Vec::new()))
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
    let what = match o.kind {
        Kind::Ladder => {
            let r = o.difficulty.rungs();
            if r.start() == r.end() {
                format!("{} rungs", r.start())
            } else {
                format!("{} to {} rungs", r.start(), r.end())
            }
        }
        Kind::Cryptogram => format!("{} letters given", o.difficulty.reveals()),
        _ => format!("{size}×{size}"),
    };
    let count = match o.kind {
        Kind::Ladder => format!("{} pages", o.count),
        _ => format!("{} {plural}", o.count),
    };
    Book {
        title: o.title.clone().unwrap_or_else(|| o.kind.title().to_owned()),
        subtitle: format!("{what} · {} · {count}", o.difficulty.name()),
        date: o.date,
        seed,
        puzzles,
        warnings,
    }
}

/// The first fill `fill` makes at `d`'s spread, then at each wider one, with the spread that
/// made it (puzzle books §3.3).
fn first_fill(
    d: Difficulty,
    mut fill: impl FnMut(f32) -> Option<Crossword>,
) -> Option<(f32, Crossword)> {
    [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard]
        .into_iter()
        .map(Difficulty::spread)
        .filter(|s| *s >= d.spread())
        .find_map(|s| fill(s).map(|x| (s, x)))
}

/// A crossword book: each puzzle filled from the whole list, preferring common words as the
/// difficulty asks; one that won't fill so takes rarer words, with a warning.
fn make_crossword(
    o: &BookOptions,
    lex: &Lexicon,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Book, BookError> {
    check(o)?;
    if o.words.is_some() {
        return Err(BookError::NoWords(Kind::Crossword));
    }
    let seed = o.seed.unwrap_or_else(|| default_seed(o));
    let mut warnings = Vec::new();
    // your words that fit the grids, ahead of the list's
    let longest = cross::LONGEST.min(o.size);
    let (fit, long): (Vec<_>, Vec<_>) = o
        .clued
        .iter()
        .flatten()
        .cloned()
        .partition(|(w, _)| (3..=longest).contains(&w.len()));
    if o.clued.is_some() && fit.len() < MIN_CLUED {
        return Err(BookError::TooFewClued(fit.len()));
    }
    if !long.is_empty() {
        let names: Vec<String> = long.iter().map(|(w, _)| w.to_uppercase()).collect();
        warnings.push(format!(
            "skipped, not 3 to {longest} letters: {}",
            names.join(", ")
        ));
    }
    let merged;
    let lex = if fit.is_empty() {
        lex
    } else {
        merged = lex.with(&fit);
        &merged
    };
    let seeds: Vec<String> = fit.into_iter().map(|(w, _)| w).collect();
    let mut puzzles = Vec::with_capacity(o.count);
    for i in 0..o.count {
        let mut again = 0u64;
        let (spread, x) = first_fill(o.difficulty, |spread| {
            let mut rng = StdRng::seed_from_u64(seed ^ ((i as u64) << 20) ^ (again << 40));
            again += 1;
            cross::fill_themed(lex, o.size, spread, &seeds, &mut rng)
        })
        .ok_or(BookError::Unfilled(i + 1))?;
        let mine = x.clues.iter().filter(|c| c.theme).count();
        if !seeds.is_empty() && mine < MIN_CLUED {
            warnings.push(format!("puzzle {} has {mine} of your words", i + 1));
        }
        if spread != o.difficulty.spread() {
            warnings.push(format!(
                "puzzle {} has rarer words than {} asks: it wouldn't fill otherwise",
                i + 1,
                o.difficulty.name()
            ));
        }
        puzzles.push(x);
        progress(i + 1, o.count);
    }
    Ok(finish(o, seed, Puzzles::Crossword(puzzles), warnings))
}

fn make_search(
    o: &BookOptions,
    words: &Words,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Book, BookError> {
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
        progress(i + 1, o.count);
    }
    Ok(finish(o, seed, Puzzles::Search(puzzles), warnings))
}

/// `<Title> <YYYY-MM-DD> <seed4>.pdf`, filesystem-safe and at most 120 characters: two books
/// never share a name, so one never lands on another's ink (puzzle books §5.2).
pub fn file_name(b: &Book) -> String {
    // All 64 bits folded into the four hex digits: the top 16 alone named every small seed
    // `0000` (#77).
    let fold = (0..4).fold(0u64, |acc, i| acc ^ (b.seed >> (16 * i))) & 0xffff;
    let tail = format!(" {} {fold:04x}.pdf", b.date);
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
            clued: None,
            title: None,
            seed: None,
            date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            quotes: None,
            picture: None,
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

    fn kind_opts(kind: Kind, count: usize) -> BookOptions {
        BookOptions {
            seed: Some(11),
            ..BookOptions::new(kind, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap())
        }
        .with_count(count)
    }

    #[test]
    fn ladders_cryptograms_nonograms_and_mazes_make_and_render() {
        let w = Words::embedded();
        for kind in [Kind::Ladder, Kind::Cryptogram, Kind::Nonogram, Kind::Maze] {
            let o = kind_opts(kind, 3);
            let mut told = Vec::new();
            let b = make_with(&o, w, &mut |i, n| told.push((i, n))).unwrap();
            assert_eq!(b.puzzles.len(), 3, "{kind:?}");
            assert_eq!(told, [(1, 3), (2, 3), (3, 3)], "{kind:?}");
            assert_eq!(b.puzzles.kind(), kind);
            let again = make(&o, w).unwrap();
            assert_eq!(again.puzzles, b.puzzles, "{kind:?} is deterministic");
            let bytes = pdf::render(&b, pdf::PageSize::KindleScribe)
                .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert!(bytes.len() > 2000, "{kind:?}");
            assert!(pdf::pages(&b, pdf::PageSize::KindleScribe) >= 5, "{kind:?}");
        }
    }

    #[test]
    fn ladder_pages_keep_to_their_rungs_and_never_repeat_a_pair() {
        let w = Words::embedded();
        for (d, rungs) in [
            (Difficulty::Easy, 3..=3),
            (Difficulty::Medium, 4..=4),
            (Difficulty::Hard, 5..=6),
        ] {
            let mut o = kind_opts(Kind::Ladder, 12);
            o.difficulty = d;
            let b = make(&o, w).unwrap();
            let mut seen = HashSet::new();
            for page in b.ladders() {
                assert_eq!(page.ladders.len(), LADDERS_PER_PAGE);
                for l in &page.ladders {
                    assert!(rungs.contains(&(l.path.len() - 1)), "{d:?}: {l:?}");
                    assert!(seen.insert((l.from.clone(), l.to.clone())), "{l:?} twice");
                    assert!(l.path.iter().all(|x| w.contains(x)));
                }
            }
        }
        assert_eq!(
            make(&kind_opts(Kind::Ladder, 2), w).unwrap().subtitle,
            "4 rungs · medium · 2 pages"
        );
    }

    #[test]
    fn cryptograms_take_your_quotes_and_the_difficulty_reveals() {
        let w = Words::embedded();
        let mut o = kind_opts(Kind::Cryptogram, 4);
        o.difficulty = Difficulty::Easy;
        o.quotes = Some(vec![
            (
                "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG.".into(),
                "A typist".into(),
            ),
            (
                "ALL THAT GLITTERS IS NOT GOLD, THEY SAY.".into(),
                "Proverb".into(),
            ),
        ]);
        let b = make(&o, w).unwrap();
        let cs = b.cryptograms();
        assert_eq!(cs.len(), 4);
        assert!(cs.iter().all(|c| c.given.len() == 3));
        assert_ne!(cs[0].plain, cs[1].plain, "both quotes before a repeat");
        assert_ne!(cs[0].key, cs[2].key, "a repeat gets a new key");
        assert!(b.warnings[0].contains("some repeat"), "{:?}", b.warnings);
        assert_eq!(b.subtitle, "3 letters given · easy · 4 puzzles");
        o.quotes = Some(vec![]);
        assert_eq!(make(&o, w).unwrap_err(), BookError::TooFewQuotes);
        let hard = make(
            &BookOptions {
                difficulty: Difficulty::Hard,
                ..kind_opts(Kind::Cryptogram, 2)
            },
            w,
        )
        .unwrap();
        assert!(hard.cryptograms().iter().all(|c| c.given.is_empty()));
    }

    #[test]
    fn nonograms_start_with_your_picture_then_pictograms() {
        let w = Words::embedded();
        let mut img = image::GrayImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                img.put_pixel(x, y, image::Luma([if x < 2 { 0 } else { 255 }]));
            }
        }
        let mut bytes = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let mut o = kind_opts(Kind::Nonogram, 3);
        o.size = 10;
        o.picture = Some((bytes, "half.png".into()));
        let b = make(&o, w).unwrap();
        let ns = b.nonograms();
        assert_eq!(ns[0].title, "half.png");
        assert!(ns[0].cells.iter().all(|r| r[0] && !r[9]));
        assert!(
            ns[1].title != "Shapes" || ns[2].title != "Shapes",
            "pictograms come next"
        );
        assert!(ns.iter().all(|n| n.size() == 10));
        assert!(ns.iter().skip(1).all(Nonogram::line_solvable));
        o.picture = Some((b"junk".to_vec(), "x".into()));
        assert!(matches!(make(&o, w), Err(BookError::BadPicture(_))));
    }

    #[test]
    fn mazes_grow_with_the_size_and_a_tiny_screen_refuses_a_big_one() {
        let w = Words::embedded();
        let mut o = kind_opts(Kind::Maze, 2);
        o.size = 40;
        let b = make(&o, w).unwrap();
        assert!(
            b.mazes()
                .iter()
                .all(|m| m.size == 40 && m.solution.len() > 79)
        );
        let small = pdf::PageSize::Screen(crate::pdf::Screen::RemarkablePaperProMove);
        match pdf::render(&b, small) {
            Err(BookError::DoesNotFit { largest, .. }) => {
                assert!((6..40).contains(&largest), "{largest}")
            }
            other => panic!("{other:?}"),
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
        assert_eq!(
            make(&o, Words::embedded()),
            Err(BookError::NoWords(Kind::Crossword))
        );
    }

    /// A puzzle that won't fill with everyday words tries a wider spread before the book gives
    /// up.
    #[test]
    fn a_crossword_that_wont_fill_tries_a_wider_spread() {
        let x = Crossword {
            rows: Vec::new(),
            clues: Vec::new(),
        };
        let mut tried = Vec::new();
        let got = first_fill(Difficulty::Easy, |s| {
            tried.push(s);
            (s > 1.0).then(|| x.clone())
        });
        assert_eq!(got, Some((Difficulty::Medium.spread(), x)));
        assert_eq!(tried, [0.5, 1.5]);
        assert_eq!(first_fill(Difficulty::Hard, |_| None), None);
    }

    fn sudoku(count: usize, difficulty: Difficulty) -> BookOptions {
        let mut o = opts(count, 9);
        o.kind = Kind::Sudoku;
        o.difficulty = difficulty;
        o
    }

    #[test]
    fn a_sudoku_book_has_its_count_and_grade() {
        let b = make(&sudoku(3, Difficulty::Hard), Words::embedded()).unwrap();
        assert_eq!(b.sudokus().len(), 3);
        assert!(b.searches().is_empty() && b.crosswords().is_empty());
        assert_eq!(b.title, "Sudoku");
        assert_eq!(b.subtitle, "9×9 · hard · 3 puzzles");
        for s in b.sudokus() {
            assert_eq!(sudoku::grade(&s.givens), sudoku::Grade::Hard);
        }
        assert_eq!(make(&sudoku(3, Difficulty::Hard), Words::embedded()), Ok(b));
    }

    #[test]
    fn a_sudoku_is_nine_by_nine_with_no_words() {
        let mut o = sudoku(1, Difficulty::Easy);
        o.size = 12;
        let err = make(&o, Words::embedded()).unwrap_err();
        assert_eq!(err.to_string(), "a sudoku is always 9×9: leave out --size");
        let mut o = sudoku(1, Difficulty::Easy);
        o.words = Some(vec!["nine".into()]);
        assert_eq!(
            make(&o, Words::embedded()),
            Err(BookError::NoWords(Kind::Sudoku))
        );
    }

    /// Messages that fit every kind: a sudoku isn't "filled", nor does it come in sizes.
    #[test]
    fn errors_read_right_for_sudoku() {
        assert_eq!(
            BookError::Unfilled(3).to_string(),
            "puzzle 3 couldn't be made; try another --seed"
        );
        assert_eq!(
            BookError::DoesNotFit {
                kind: Kind::Sudoku,
                size: 9,
                largest: 0
            }
            .to_string(),
            "a sudoku's squares would be too small to write in on this screen"
        );
    }

    #[test]
    fn clued_words_from_file_takes_a_colon_or_a_tab() {
        let (words, skipped) = clued_words_from_file(
            "Jobs: Co-founder who came back\nWi-Fi\tWireless networking\nnoclue\n# note\n\nMacintosh: The 1984 computer\njobs: again\n",
        );
        assert_eq!(
            words,
            [
                ("jobs".to_owned(), "Co-founder who came back".to_owned()),
                ("wifi".to_owned(), "Wireless networking".to_owned()),
                ("macintosh".to_owned(), "The 1984 computer".to_owned()),
            ]
        );
        assert_eq!(skipped, ["noclue"]);
    }

    fn apple() -> Vec<(String, String)> {
        [
            ("jobs", "Co-founder who returned in 1997"),
            ("woz", "The other Steve, for short"),
            ("lisa", "1983 computer named for a daughter"),
            ("newton", "Apple's early handheld"),
            ("imac", "1998's candy-coloured desktop"),
            ("ipod", "A thousand songs in your pocket"),
            ("pixar", "Studio its co-founder ran"),
            ("siri", "Voice assistant since 2011"),
            ("swift", "Language of 2014"),
            ("safari", "Apple's web browser"),
            ("finder", "The Mac's file manager"),
            ("macintosh", "The 1984 computer"),
            ("sculley", "CEO from Pepsi"),
        ]
        .map(|(w, c)| (w.to_owned(), c.to_owned()))
        .into()
    }

    /// Your words with your clues go into each puzzle first; ones too long for the grids are
    /// named in a warning.
    #[test]
    fn own_crossword_words_go_in_with_their_clues() {
        let mut o = crossword(2, 11, Difficulty::Medium);
        o.clued = Some(apple());
        let b = make(&o, Words::embedded()).unwrap();
        for x in b.crosswords() {
            let mine: Vec<_> = x.clues.iter().filter(|c| c.theme).collect();
            assert!(mine.len() >= 2, "{mine:?}");
            for c in mine {
                assert!(
                    apple()
                        .iter()
                        .any(|(w, clue)| *w == c.answer && *clue == c.clue)
                );
            }
        }
        assert!(
            b.warnings
                .iter()
                .any(|w| w.contains("MACINTOSH") && w.contains("SCULLEY")),
            "{:?}",
            b.warnings
        );
        assert_eq!(make(&o, Words::embedded()), Ok(b));
        o.clued = Some(apple()[..2].to_vec());
        assert_eq!(make(&o, Words::embedded()), Err(BookError::TooFewClued(2)));
    }

    #[test]
    fn file_name_is_dated_and_seeded() {
        let mut b = make(&opts(1, 8), Words::embedded()).unwrap();
        b.seed = 0x7f3a_0000_0000_0000;
        assert_eq!(file_name(&b), "Word Search 2026-11-01 7f3a.pdf");
        b.title = "Tech: A/B?".into();
        assert_eq!(file_name(&b), "Tech_ A_B_ 2026-11-01 7f3a.pdf");
        // Small seeds name different books.
        let names: Vec<String> = [1, 2]
            .map(|s| {
                b.seed = s;
                file_name(&b)
            })
            .into();
        assert_ne!(names[0], names[1]);
    }
}
