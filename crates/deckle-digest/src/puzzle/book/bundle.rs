// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle book bundle (docs/superpowers/specs/2026-11-03-puzzle-book-bundle-design.md): a
//! book as data for the iPad, the way an edition is. A
//! `deckle-puzzles-<slug>-<date>-<seed4>.decklepuzzles` folder holds `book.json` (§3) and
//! `cover.jpg`, written through the same staging as the edition bundle: a hidden `.tmp` folder
//! renamed into place, `book.json` last, listing every other file with its size and SHA-256.
//!
//! The same `Book` the PDF draws is serialized here: no puzzle is made twice. The reader-side
//! types ([`BookJson`] and its puzzles) read a bundle back too, and [`to_book`] turns one into
//! the `Book` it was written from.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::plan::Theme;
use super::theme::Source;
use super::{Book, BookOptions, Difficulty, Kind, LADDERS_PER_PAGE, LadderPage, Puzzles};
use crate::bundle::package::{self, Existing, FileEntry, Files};
use crate::puzzle::ai::Style;
use crate::puzzle::cross::Crossword;
use crate::puzzle::cryptogram::Cryptogram;
use crate::puzzle::grid::Dir;
use crate::puzzle::ladder::Ladder;
use crate::puzzle::maze::Maze;
use crate::puzzle::nonogram::Nonogram;
use crate::puzzle::search::{Hidden, Search, Way};
use crate::puzzle::store::Clue;
use crate::puzzle::sudoku::Sudoku;

/// The format's name and the version this writer writes (§3, §7).
pub const FORMAT: &str = "deckle-puzzles";
pub const VERSION: &str = "1.0";
/// The folder's extension, the package type the reader declares.
pub const EXTENSION: &str = "decklepuzzles";
/// The manifest, written last.
pub const MANIFEST: &str = "book.json";
/// The namespace of a book's id (§3.1): fixed, so the same book has the same id on every
/// machine.
const NAMESPACE: Uuid = Uuid::from_u128(0x5b2c_8e1a_7d04_4c3f_9a6e_2f81_d0b7_c4e9);
/// A slug's length, at most (§2).
const SLUG_CHARS: usize = 40;

/// What the bundle says about a book beyond the `Book` and its options: where its theme came
/// from, how its clues read, which paper's Sunday it is, and when it was built.
#[derive(Clone, Debug, PartialEq)]
pub struct Extras {
    pub theme: Theme,
    pub style: Style,
    pub paper_id: Option<Uuid>,
    pub sunday_of: Option<NaiveDate>,
    pub built_at: DateTime<Utc>,
    /// Overwrite a bundle of the same name. Otherwise one already there is an error: the
    /// reader may hold ink on it.
    pub replace: bool,
}

impl Extras {
    /// No theme beyond the options' own words, plain clues, not a Sunday, built now.
    pub fn new(options: &BookOptions, built_at: DateTime<Utc>) -> Extras {
        Extras {
            theme: if options.words.is_some() || options.clued.is_some() {
                Theme::Own
            } else {
                Theme::None
            },
            style: Style::Plain,
            paper_id: None,
            sunday_of: None,
            built_at,
            replace: false,
        }
    }
}

/// `book.json` (§3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BookJson {
    pub format: String,
    pub version: String,
    pub generator: String,
    pub book: BookInfo,
    pub puzzles: Vec<PuzzleJson>,
    pub files: Vec<FileEntry>,
}

/// §3.1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BookInfo {
    pub id: Uuid,
    pub title: String,
    pub subtitle: String,
    pub kind: Kind,
    pub date: NaiveDate,
    /// The `u64` as 16 lower-case hex digits.
    pub seed: String,
    pub difficulty: Difficulty,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeJson>,
    /// Crosswords only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clue_style: Option<String>,
    pub built_at: String,
    pub cover: String,
    #[serde(default)]
    pub paper_id: Option<Uuid>,
    #[serde(default)]
    pub sunday_of: Option<NaiveDate>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeJson {
    pub kind: String,
    pub label: String,
    /// A `subject`'s text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// One puzzle (§3.2), in page order; `n` is 1-based.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PuzzleJson {
    Crossword {
        n: usize,
        size: usize,
        grid: Vec<String>,
        /// `[row, column, number]` for each numbered square.
        numbers: Vec<[u32; 3]>,
        clues: Vec<ClueJson>,
    },
    Search {
        n: usize,
        size: usize,
        grid: Vec<String>,
        words: Vec<HiddenJson>,
    },
    Sudoku {
        n: usize,
        /// Nine rows of nine, `.` an empty square.
        givens: Vec<String>,
        solution: Vec<String>,
        grade: String,
    },
    Ladder {
        n: usize,
        from: String,
        to: String,
        /// The blank rungs between the ends; `answer` has `steps + 2` words.
        steps: usize,
        answer: Vec<String>,
    },
    Cryptogram {
        n: usize,
        cipher: String,
        plain: String,
        /// Plaintext letters the page prints as solved.
        given: Vec<String>,
        source: String,
    },
    Nonogram {
        n: usize,
        size: usize,
        title: String,
        /// The clue runs of each row and column; an empty array is a blank line.
        rows: Vec<Vec<usize>>,
        cols: Vec<Vec<usize>>,
        /// `#` shaded, `.` clear.
        solution: Vec<String>,
        /// A `media/` path, or `null`.
        picture: Option<String>,
    },
    Maze {
        n: usize,
        size: usize,
        /// Row-major, each square's standing walls as bits N=1, E=2, S=4, W=8.
        walls: Vec<u8>,
        start: [usize; 2],
        end: [usize; 2],
        solution: Vec<[usize; 2]>,
    },
}

impl PuzzleJson {
    pub fn kind(&self) -> Kind {
        match self {
            PuzzleJson::Crossword { .. } => Kind::Crossword,
            PuzzleJson::Search { .. } => Kind::Search,
            PuzzleJson::Sudoku { .. } => Kind::Sudoku,
            PuzzleJson::Ladder { .. } => Kind::Ladder,
            PuzzleJson::Cryptogram { .. } => Kind::Cryptogram,
            PuzzleJson::Nonogram { .. } => Kind::Nonogram,
            PuzzleJson::Maze { .. } => Kind::Maze,
        }
    }

    pub fn n(&self) -> usize {
        match self {
            PuzzleJson::Crossword { n, .. }
            | PuzzleJson::Search { n, .. }
            | PuzzleJson::Sudoku { n, .. }
            | PuzzleJson::Ladder { n, .. }
            | PuzzleJson::Cryptogram { n, .. }
            | PuzzleJson::Nonogram { n, .. }
            | PuzzleJson::Maze { n, .. } => *n,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClueJson {
    pub number: u8,
    pub dir: String,
    pub clue: String,
    pub answer: String,
    pub theme: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenJson {
    pub word: String,
    pub row: u8,
    pub col: u8,
    /// The word reads forward from `row`, `col` in this direction.
    pub dir: String,
    pub theme: bool,
}

/// The eight ways and their names (§3.2).
const WAYS: [(Way, &str); 8] = [
    (Way::Across, "across"),
    (Way::Down, "down"),
    (Way::DownRight, "down-right"),
    (Way::UpRight, "up-right"),
    (Way::Left, "left"),
    (Way::Up, "up"),
    (Way::UpLeft, "up-left"),
    (Way::DownLeft, "down-left"),
];

fn way_name(w: Way) -> &'static str {
    WAYS.iter()
        .find(|(x, _)| *x == w)
        .map_or("across", |(_, n)| n)
}

fn way_of(name: &str) -> Result<Way, String> {
    WAYS.iter()
        .find(|(_, n)| *n == name)
        .map(|(w, _)| *w)
        .ok_or_else(|| format!("no direction {name}"))
}

/// The title's lower-case ASCII words joined by `-`, at most [`SLUG_CHARS`] long (§2).
pub fn slug(title: &str) -> String {
    let ascii = deunicode::deunicode(title).to_ascii_lowercase();
    let mut out = String::new();
    for word in ascii
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let next = if out.is_empty() {
            word.to_owned()
        } else {
            format!("{out}-{word}")
        };
        if next.len() > SLUG_CHARS {
            break;
        }
        out = next;
    }
    if out.is_empty() {
        "puzzles".to_owned()
    } else {
        out
    }
}

/// The bundle's folder name for `b`: `deckle-puzzles-<slug>-<date>-<seed4>.decklepuzzles`,
/// `seed4` the seed's first four hex digits as the PDF's file name has them.
pub fn bundle_name(b: &Book) -> String {
    format!(
        "deckle-puzzles-{}-{}-{:04x}.{EXTENSION}",
        slug(&b.title),
        b.date,
        b.seed >> 48
    )
}

/// The book's id (§3.1): a UUID v5 of its kind, seed, date, size, difficulty and count in a
/// fixed namespace, the same on every machine.
pub fn book_id(
    kind: Kind,
    seed: u64,
    date: NaiveDate,
    size: usize,
    difficulty: Difficulty,
    count: usize,
) -> Uuid {
    let name = format!(
        "{}|{seed:016x}|{date}|{size}|{}|{count}",
        kind.key(),
        difficulty.name()
    );
    let mut data = NAMESPACE.as_bytes().to_vec();
    data.extend_from_slice(name.as_bytes());
    let hash = sha1(&data);
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    uuid::Builder::from_sha1_bytes(bytes).into_uuid()
}

/// SHA-1, for the v5 id alone (RFC 4122 §4.3): twenty bytes of `data`'s digest. Small enough
/// to keep here rather than add a crate for one hash.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (hi, v) in h.iter_mut().zip([a, b, c, d, e]) {
            *hi = hi.wrapping_add(v);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// The theme as the shelf line names it (§3.1).
fn theme_json(t: &Theme) -> ThemeJson {
    let (kind, label, text) = match t {
        Theme::None => ("none", "No theme".to_owned(), None),
        Theme::Own => ("own", "Your own words".to_owned(), None),
        Theme::Source(Source::Interests { names, .. }) => (
            "interests",
            if names.is_empty() {
                "Your interests".to_owned()
            } else {
                names.join(", ")
            },
            None,
        ),
        Theme::Source(Source::Week { .. }) => ("week", "This week's paper".to_owned(), None),
        Theme::Source(Source::Vocab) => ("vocab", "Words you looked up".to_owned(), None),
        Theme::Source(Source::Cookbook) => ("cookbook", "Your cookbook".to_owned(), None),
        Theme::Subject { text } => ("subject", text.clone(), Some(text.clone())),
    };
    ThemeJson {
        kind: kind.to_owned(),
        label,
        text,
    }
}

fn dir_name(d: Dir) -> &'static str {
    match d {
        Dir::Across => "across",
        Dir::Down => "down",
    }
}

fn crossword_json(n: usize, x: &Crossword) -> PuzzleJson {
    PuzzleJson::Crossword {
        n,
        size: x.rows.len(),
        grid: x.rows.clone(),
        numbers: crate::bundle::numbers(&x.rows),
        clues: x
            .clues
            .iter()
            .map(|c| ClueJson {
                number: c.number,
                dir: dir_name(c.dir).to_owned(),
                clue: c.clue.clone(),
                answer: c.answer.to_uppercase(),
                theme: c.theme,
            })
            .collect(),
    }
}

fn search_json(n: usize, s: &Search) -> PuzzleJson {
    PuzzleJson::Search {
        n,
        size: s.size(),
        grid: s.rows.clone(),
        words: s
            .words
            .iter()
            .map(|h| HiddenJson {
                word: h.word.to_uppercase(),
                row: h.row,
                col: h.col,
                dir: way_name(h.way).to_owned(),
                theme: h.theme,
            })
            .collect(),
    }
}

/// Nine rows of nine, `0` as `.`.
fn sudoku_rows(grid: &[u8; 81]) -> Vec<String> {
    grid.chunks(9)
        .map(|row| {
            row.iter()
                .map(|&d| if d == 0 { '.' } else { (b'0' + d) as char })
                .collect()
        })
        .collect()
}

fn sudoku_json(n: usize, s: &Sudoku, difficulty: Difficulty) -> PuzzleJson {
    PuzzleJson::Sudoku {
        n,
        givens: sudoku_rows(&s.givens),
        solution: sudoku_rows(&s.solution),
        grade: difficulty.name().to_owned(),
    }
}

fn ladder_json(n: usize, l: &Ladder) -> PuzzleJson {
    PuzzleJson::Ladder {
        n,
        from: l.from.to_uppercase(),
        to: l.to.to_uppercase(),
        steps: l.path.len().saturating_sub(2),
        answer: l.path.iter().map(|w| w.to_uppercase()).collect(),
    }
}

fn cryptogram_json(n: usize, c: &Cryptogram) -> PuzzleJson {
    PuzzleJson::Cryptogram {
        n,
        cipher: c.cipher.clone(),
        plain: c.plain.clone(),
        given: c
            .given
            .iter()
            .filter_map(|&ch| c.decode(ch))
            .map(String::from)
            .collect(),
        source: c.source.clone(),
    }
}

/// A line's runs as the schema has them: a blank line is `[]`, not `[0]`.
fn runs(line: Vec<usize>) -> Vec<usize> {
    if line == [0] { Vec::new() } else { line }
}

fn nonogram_json(n: usize, p: &Nonogram) -> PuzzleJson {
    PuzzleJson::Nonogram {
        n,
        size: p.size(),
        title: p.title.clone(),
        rows: p.row_clues().into_iter().map(runs).collect(),
        cols: p.col_clues().into_iter().map(runs).collect(),
        solution: p
            .cells
            .iter()
            .map(|r| r.iter().map(|&on| if on { '#' } else { '.' }).collect())
            .collect(),
        picture: None,
    }
}

fn maze_json(n: usize, m: &Maze) -> PuzzleJson {
    let cell = |(r, c): (usize, usize)| [r, c];
    PuzzleJson::Maze {
        n,
        size: m.size,
        walls: m.walls.clone(),
        start: cell(m.entrance()),
        end: cell(m.exit()),
        solution: m.solution.iter().map(|&p| cell(p)).collect(),
    }
}

/// The book's puzzles in page order, `n` from 1; a ladder page's three are three entries.
pub fn puzzles_json(b: &Book, difficulty: Difficulty) -> Vec<PuzzleJson> {
    match &b.puzzles {
        Puzzles::Crossword(xs) => xs
            .iter()
            .enumerate()
            .map(|(i, x)| crossword_json(i + 1, x))
            .collect(),
        Puzzles::Search(ss) => ss
            .iter()
            .enumerate()
            .map(|(i, s)| search_json(i + 1, s))
            .collect(),
        Puzzles::Sudoku(ss) => ss
            .iter()
            .enumerate()
            .map(|(i, s)| sudoku_json(i + 1, s, difficulty))
            .collect(),
        Puzzles::Ladder(pages) => pages
            .iter()
            .flat_map(|p| &p.ladders)
            .enumerate()
            .map(|(i, l)| ladder_json(i + 1, l))
            .collect(),
        Puzzles::Cryptogram(cs) => cs
            .iter()
            .enumerate()
            .map(|(i, c)| cryptogram_json(i + 1, c))
            .collect(),
        Puzzles::Nonogram(ns) => ns
            .iter()
            .enumerate()
            .map(|(i, p)| nonogram_json(i + 1, p))
            .collect(),
        Puzzles::Maze(ms) => ms
            .iter()
            .enumerate()
            .map(|(i, m)| maze_json(i + 1, m))
            .collect(),
    }
}

/// `book.json` for `b`, made from `options`, listing `files`.
pub fn book_json(b: &Book, options: &BookOptions, extras: &Extras, files: &Files) -> BookJson {
    let kind = b.puzzles.kind();
    BookJson {
        format: FORMAT.to_owned(),
        version: VERSION.to_owned(),
        generator: format!("Deckle {}", env!("CARGO_PKG_VERSION")),
        book: BookInfo {
            id: book_id(
                kind,
                b.seed,
                b.date,
                options.size,
                options.difficulty,
                options.count,
            ),
            title: b.title.clone(),
            subtitle: b.subtitle.clone(),
            kind,
            date: b.date,
            seed: format!("{:016x}", b.seed),
            difficulty: options.difficulty,
            size: kind.sized().then_some(options.size),
            theme: Some(theme_json(&extras.theme)),
            clue_style: (kind == Kind::Crossword).then(|| extras.style.name().to_owned()),
            built_at: extras.built_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            cover: "cover.jpg".to_owned(),
            paper_id: extras.paper_id,
            sunday_of: extras.sunday_of,
            warnings: b.warnings.clone(),
        },
        puzzles: puzzles_json(b, options.difficulty),
        files: files.entries(),
    }
}

/// Writes the bundle of `book` (made from `options`) into `dir`, as [`bundle_name`], and
/// returns its path. Never over a bundle already there: see [`write_with`].
pub fn write(book: &Book, options: &BookOptions, dir: &Path) -> Result<PathBuf, String> {
    write_with(book, options, &Extras::new(options, Utc::now()), dir)
}

/// [`write`] with the bundle's [`Extras`]. Without `replace`, a bundle of the same name
/// already in `dir` is an error and is left as it is.
pub fn write_with(
    book: &Book,
    options: &BookOptions,
    extras: &Extras,
    dir: &Path,
) -> Result<PathBuf, String> {
    let name = bundle_name(book);
    if !extras.replace && dir.join(&name).exists() {
        return Err(format!(
            "{name} is already there; pass --replace to overwrite it (its ink may stay)"
        ));
    }
    let mut files = Files::default();
    let date = book.date.format("%-d %B %Y").to_string();
    let cover = crate::cover::render_puzzle_book(
        book.puzzles.kind().name(),
        &book.title,
        &book.subtitle,
        &date,
    )
    .map_err(|e| format!("cover: {e}"))?;
    files.insert("cover.jpg", cover);
    let json = book_json(book, options, extras, &files);
    let bytes = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
    let existing = if extras.replace {
        Existing::Replace
    } else {
        Existing::Keep
    };
    package::write_package(dir, &name, &files, MANIFEST, &bytes, existing)
}

/// `book.json` at `path`, a bundle folder or the file itself: a `deckle-puzzles` manifest of
/// a 1.x version. The files it lists aren't checked; see [`verify`].
pub fn read(path: &Path) -> Result<BookJson, String> {
    let file = if path.is_dir() {
        path.join(MANIFEST)
    } else {
        path.to_path_buf()
    };
    let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let json: BookJson =
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", file.display()))?;
    if json.format != FORMAT {
        return Err(format!(
            "{}: not a puzzle book ({})",
            file.display(),
            json.format
        ));
    }
    if json.version.split('.').next() != Some("1") {
        return Err(format!(
            "{}: version {} is newer than this Deckle reads",
            file.display(),
            json.version
        ));
    }
    Ok(json)
}

/// Whether the bundle at `dir` is whole (§3.3): every file `book.json` lists is there at its
/// size and SHA-256, and nothing else is in the folder.
pub fn verify(dir: &Path) -> Result<(), String> {
    package::verify(dir, MANIFEST, true)
}

fn upper_ascii(word: &str, what: &str) -> Result<String, String> {
    if word.is_empty() || !word.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(format!("{what}: {word:?} isn't upper-case letters"));
    }
    Ok(word.to_ascii_lowercase())
}

fn sudoku_grid(rows: &[String], what: &str) -> Result<[u8; 81], String> {
    let cells: Vec<u8> = rows
        .iter()
        .flat_map(|r| r.chars())
        .map(|c| match c {
            '.' => Ok(0),
            '1'..='9' => Ok(c as u8 - b'0'),
            other => Err(format!("{what}: {other:?} in a sudoku")),
        })
        .collect::<Result<_, _>>()?;
    cells
        .try_into()
        .map_err(|_| format!("{what}: a sudoku has 81 squares"))
}

/// The `Book` a manifest was written from: its title, date, seed and puzzles as the writer
/// had them. A cryptogram's key is known only for the letters its quote uses; the rest are
/// filled in the alphabet's order. A manifest whose puzzles aren't all the book's kind is an
/// error.
pub fn to_book(json: &BookJson) -> Result<Book, String> {
    let kind = json.book.kind;
    let seed = u64::from_str_radix(&json.book.seed, 16)
        .map_err(|e| format!("seed {:?}: {e}", json.book.seed))?;
    if let Some(p) = json.puzzles.iter().find(|p| p.kind() != kind) {
        return Err(format!(
            "puzzle {} is a {}, in a book of {}",
            p.n(),
            p.kind().name(),
            kind.name()
        ));
    }
    let mut crosswords = Vec::new();
    let mut searches = Vec::new();
    let mut sudokus = Vec::new();
    let mut ladders = Vec::new();
    let mut cryptograms = Vec::new();
    let mut nonograms = Vec::new();
    let mut mazes = Vec::new();
    for p in &json.puzzles {
        let what = format!("puzzle {}", p.n());
        match p {
            PuzzleJson::Crossword { grid, clues, .. } => crosswords.push(Crossword {
                rows: grid.clone(),
                clues: clues
                    .iter()
                    .map(|c| {
                        Ok(Clue {
                            number: c.number,
                            dir: match c.dir.as_str() {
                                "across" => Dir::Across,
                                "down" => Dir::Down,
                                other => return Err(format!("{what}: no direction {other}")),
                            },
                            answer: upper_ascii(&c.answer, &what)?,
                            clue: c.clue.clone(),
                            theme: c.theme,
                        })
                    })
                    .collect::<Result<_, String>>()?,
            }),
            PuzzleJson::Search { grid, words, .. } => searches.push(Search {
                rows: grid.clone(),
                words: words
                    .iter()
                    .map(|h| {
                        Ok(Hidden {
                            word: upper_ascii(&h.word, &what)?,
                            row: h.row,
                            col: h.col,
                            way: way_of(&h.dir).map_err(|e| format!("{what}: {e}"))?,
                            theme: h.theme,
                        })
                    })
                    .collect::<Result<_, String>>()?,
            }),
            PuzzleJson::Sudoku {
                givens, solution, ..
            } => sudokus.push(Sudoku {
                givens: sudoku_grid(givens, &what)?,
                solution: sudoku_grid(solution, &what)?,
            }),
            PuzzleJson::Ladder {
                from, to, answer, ..
            } => ladders.push(Ladder {
                from: upper_ascii(from, &what)?,
                to: upper_ascii(to, &what)?,
                path: answer
                    .iter()
                    .map(|w| upper_ascii(w, &what))
                    .collect::<Result<_, _>>()?,
            }),
            PuzzleJson::Cryptogram {
                cipher,
                plain,
                given,
                source,
                ..
            } => {
                if cipher.chars().count() != plain.chars().count() {
                    return Err(format!("{what}: cipher and plain differ in length"));
                }
                // the key from the pairs; letters the quote doesn't use take what's left
                let mut key = [u8::MAX; 26];
                for (c, p) in cipher.chars().zip(plain.chars()) {
                    if p.is_ascii_uppercase() {
                        key[(p as u8 - b'A') as usize] = c as u8 - b'A';
                    }
                }
                let used = key;
                let mut free = (0..26u8).filter(|c| !used.contains(c));
                for k in key.iter_mut().filter(|k| **k == u8::MAX) {
                    *k = free.next().unwrap_or(0);
                }
                let encode = |p: char| (b'A' + key[(p as u8 - b'A') as usize]) as char;
                cryptograms.push(Cryptogram {
                    plain: plain.clone(),
                    cipher: cipher.clone(),
                    key,
                    given: given
                        .iter()
                        .map(|g| match g.chars().next() {
                            Some(p) if p.is_ascii_uppercase() && g.len() == 1 => Ok(encode(p)),
                            _ => Err(format!("{what}: given {g:?} isn't a letter")),
                        })
                        .collect::<Result<_, _>>()?,
                    source: source.clone(),
                });
            }
            PuzzleJson::Nonogram {
                title, solution, ..
            } => nonograms.push(Nonogram {
                cells: solution
                    .iter()
                    .map(|r| r.chars().map(|c| c == '#').collect())
                    .collect(),
                title: title.clone(),
            }),
            PuzzleJson::Maze {
                size,
                walls,
                solution,
                ..
            } => mazes.push(Maze {
                size: *size,
                walls: walls.clone(),
                solution: solution.iter().map(|&[r, c]| (r, c)).collect(),
            }),
        }
    }
    let puzzles = match kind {
        Kind::Crossword => Puzzles::Crossword(crosswords),
        Kind::Search => Puzzles::Search(searches),
        Kind::Sudoku => Puzzles::Sudoku(sudokus),
        Kind::Ladder => Puzzles::Ladder(
            ladders
                .chunks(LADDERS_PER_PAGE)
                .map(|page| LadderPage {
                    ladders: page.to_vec(),
                })
                .collect(),
        ),
        Kind::Cryptogram => Puzzles::Cryptogram(cryptograms),
        Kind::Nonogram => Puzzles::Nonogram(nonograms),
        Kind::Maze => Puzzles::Maze(mazes),
    };
    Ok(Book {
        title: json.book.title.clone(),
        subtitle: json.book.subtitle.clone(),
        date: json.book.date,
        seed,
        puzzles,
        warnings: json.book.warnings.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_the_standard_vectors() {
        let hex = |d: [u8; 20]| package::hex(&d);
        assert_eq!(hex(sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn the_id_is_a_v5_uuid_and_the_same_for_the_same_book() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let a = book_id(Kind::Search, 7, date, 12, Difficulty::Medium, 10);
        let b = book_id(Kind::Search, 7, date, 12, Difficulty::Medium, 10);
        assert_eq!(a, b);
        assert_eq!(a.get_version_num(), 5);
        assert_eq!(a.get_variant(), uuid::Variant::RFC4122);
        assert_ne!(
            a,
            book_id(Kind::Search, 8, date, 12, Difficulty::Medium, 10)
        );
        assert_ne!(
            a,
            book_id(Kind::Crossword, 7, date, 12, Difficulty::Medium, 10)
        );
        // the standard namespace and name agree with the RFC's own v5 construction
        let dns = uuid::Builder::from_sha1_bytes({
            let mut data = Uuid::NAMESPACE_DNS.as_bytes().to_vec();
            data.extend_from_slice(b"www.example.com");
            let h = sha1(&data);
            let mut b = [0u8; 16];
            b.copy_from_slice(&h[..16]);
            b
        })
        .into_uuid();
        assert_eq!(dns.to_string(), "2ed6657d-e927-568b-95e1-2665a8aea6a2");
    }

    #[test]
    fn slugs_are_lower_case_ascii_words_at_most_forty_long() {
        assert_eq!(slug("Word Search"), "word-search");
        assert_eq!(slug("Rust & Panthers, November"), "rust-panthers-november");
        assert_eq!(slug("Café — Été!"), "cafe-ete");
        assert_eq!(slug("   "), "puzzles");
        let long = slug("one two three four five six seven eight nine ten eleven");
        assert!(long.len() <= SLUG_CHARS, "{long}");
        assert_eq!(long, "one-two-three-four-five-six-seven-eight");
    }

    #[test]
    fn ways_and_dirs_name_each_other() {
        for (w, n) in WAYS {
            assert_eq!(way_name(w), n);
            assert_eq!(way_of(n).unwrap(), w);
        }
        assert!(way_of("sideways").is_err());
        assert_eq!(runs(vec![0]), Vec::<usize>::new());
        assert_eq!(runs(vec![1, 2]), vec![1, 2]);
    }
}
