// SPDX-License-Identifier: GPL-3.0-or-later
//! The Puzzles tab (Puzzles tab design §1, §3): a book made from the app, previewed first,
//! kept in `~/Deckle/Puzzles` beside the Morgue with the options that made it, and sent to a
//! Kindle or a folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use deckle_core::deliver::{Conn, DeliverOptions, Target, deliver_named_on};
use deckle_digest::chrono::{Local, NaiveDate};
use deckle_digest::puzzle::book::pdf::{self, PageSize};
use deckle_digest::puzzle::book::{self, Book, BookOptions, Difficulty, Kind, Puzzles};
use deckle_digest::puzzle::cryptogram;
use deckle_digest::puzzle::words::Words;
use serde::{Deserialize, Serialize};

use crate::commands::{RunTarget, resolve_run_target};
use crate::events::Events;

/// Where the books go: beside the Morgue and the Inbox.
pub fn folder() -> PathBuf {
    deckle_digest::inbox::expand_home("~/Deckle/Puzzles")
}

/// One kind as the tab describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindInfo {
    pub key: String,
    pub name: String,
    pub title: String,
    /// "Shade the squares the row and column counts describe."
    pub blurb: String,
    pub sizes: Option<(usize, usize)>,
    pub default_size: usize,
    /// What the kind takes as its own: `words`, `clued`, `quotes`, `picture` or `none`.
    pub takes: String,
    /// Whether difficulty means anything to it.
    pub graded: bool,
    /// What each difficulty means, easy, medium, hard.
    pub difficulties: [String; 3],
}

pub fn puzzles_kinds() -> Vec<KindInfo> {
    Kind::ALL
        .iter()
        .map(|&k| {
            let sizes = k.sized().then(|| (*k.sizes().start(), *k.sizes().end()));
            let (blurb, takes, graded, difficulties) = match k {
                Kind::Crossword => (
                    "Numbered grids, 5×5 to 15×15, with clues below.",
                    "clued",
                    true,
                    ["everyday words", "some less common", "the whole list"],
                ),
                Kind::Search => (
                    "Words hidden in a grid of letters, 8×8 to 20×20.",
                    "words",
                    true,
                    ["across and down", "and the diagonals", "backwards too"],
                ),
                Kind::Sudoku => (
                    "Classic 9×9, one solution, never a guess.",
                    "none",
                    true,
                    ["singles solve it", "needs pairs", "needs triples or X-wing"],
                ),
                Kind::Ladder => (
                    "One word into another, a letter a rung, three a page.",
                    "none",
                    true,
                    ["3 rungs", "4 rungs", "5 or 6 rungs"],
                ),
                Kind::Cryptogram => (
                    "A sentence enciphered letter for letter, with a key to work in.",
                    "quotes",
                    true,
                    [
                        "short quotes, 3 letters given",
                        "1 letter given",
                        "none given",
                    ],
                ),
                Kind::Nonogram => (
                    "Shade the squares the row and column counts describe: a picture appears.",
                    "picture",
                    false,
                    ["", "", ""],
                ),
                Kind::Maze => (
                    "In at the top left, out at the bottom right.",
                    "none",
                    false,
                    ["", "", ""],
                ),
            };
            KindInfo {
                key: k.key().to_owned(),
                name: k.name().to_owned(),
                title: k.title().to_owned(),
                blurb: blurb.to_owned(),
                sizes,
                default_size: k.default_size(),
                takes: takes.to_owned(),
                graded,
                difficulties: difficulties.map(String::from),
            }
        })
        .collect()
}

/// What the tab asks for. Saved beside each book, so it can be made again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookRequest {
    pub kind: String,
    pub count: usize,
    pub size: usize,
    pub difficulty: String,
    pub title: String,
    /// A `PageSize` name: `kindle-scribe`, `remarkable-2`, …
    pub screen: String,
    /// Your own words (a word search) or `word: clue` lines (a crossword), as typed.
    pub words: String,
    /// Your own quotes, `quote — who` a line.
    pub quotes: String,
    /// A picture file for a nonogram's first puzzle.
    pub picture: Option<String>,
    pub seed: Option<u64>,
    /// YYYY-MM-DD; empty is today.
    pub date: String,
}

impl Default for BookRequest {
    fn default() -> Self {
        BookRequest {
            kind: "crossword".into(),
            count: 10,
            size: 0,
            difficulty: "medium".into(),
            title: String::new(),
            screen: "kindle-scribe".into(),
            words: String::new(),
            quotes: String::new(),
            picture: None,
            seed: None,
            date: String::new(),
        }
    }
}

fn kind_of(key: &str) -> Result<Kind, String> {
    Kind::ALL
        .iter()
        .copied()
        .find(|k| k.key() == key)
        .ok_or_else(|| format!("no puzzle kind {key}"))
}

/// The request as the engine's options, with any warnings about what it couldn't use.
fn options(r: &BookRequest) -> Result<(BookOptions, PageSize, Vec<String>), String> {
    let kind = kind_of(&r.kind)?;
    let date = if r.date.trim().is_empty() {
        Local::now().date_naive()
    } else {
        NaiveDate::parse_from_str(r.date.trim(), "%Y-%m-%d")
            .map_err(|_| format!("the date must be YYYY-MM-DD, not {}", r.date))?
    };
    let page = PageSize::parse(&r.screen).ok_or_else(|| format!("no screen {}", r.screen))?;
    let difficulty = match r.difficulty.as_str() {
        "easy" => Difficulty::Easy,
        "hard" => Difficulty::Hard,
        _ => Difficulty::Medium,
    };
    let mut warnings = Vec::new();
    let mut o = BookOptions {
        count: r.count.clamp(1, 200),
        size: if r.size == 0 {
            kind.default_size()
        } else {
            r.size
        },
        difficulty,
        title: Some(r.title.trim().to_owned()).filter(|t| !t.is_empty()),
        seed: r.seed,
        ..BookOptions::new(kind, date)
    };
    let text = r.words.trim();
    if !text.is_empty() {
        match kind {
            Kind::Crossword => {
                let (clued, skipped) = book::clued_words_from_file(text);
                if !skipped.is_empty() {
                    let names: Vec<String> = skipped.iter().map(|w| w.to_uppercase()).collect();
                    warnings.push(format!("skipped, no clue: {}", names.join(", ")));
                }
                o.clued = Some(clued);
            }
            Kind::Search => o.words = Some(book::words_from_file(text)),
            _ => {}
        }
    }
    let quotes = r.quotes.trim();
    if !quotes.is_empty() && kind == Kind::Cryptogram {
        let (q, skipped) = cryptogram::quotes_from_file(quotes);
        if !skipped.is_empty() {
            warnings.push(format!(
                "skipped, too short or too long: {}",
                skipped.join("; ")
            ));
        }
        o.quotes = Some(q);
    }
    if let Some(p) = r.picture.as_deref().filter(|p| !p.trim().is_empty()) {
        if kind == Kind::Nonogram {
            let bytes = std::fs::read(p).map_err(|e| format!("can't read the picture: {e}"))?;
            let name = Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            o.picture = Some((bytes, name));
        }
    }
    Ok((o, page, warnings))
}

/// The first puzzle, as the tab draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PuzzleDto {
    Search {
        rows: Vec<String>,
        words: Vec<(String, bool)>,
    },
    Crossword {
        rows: Vec<String>,
        /// `(number, "across" | "down", clue, theme)`
        clues: Vec<(u8, String, String, bool)>,
    },
    Sudoku {
        givens: Vec<u8>,
    },
    Ladder {
        /// `(from, to, rungs)`
        ladders: Vec<(String, String, usize)>,
    },
    Cryptogram {
        words: Vec<String>,
        given: Vec<(char, char)>,
    },
    Nonogram {
        size: usize,
        rows: Vec<Vec<usize>>,
        cols: Vec<Vec<usize>>,
    },
    Maze {
        size: usize,
        walls: Vec<u8>,
    },
}

fn first_puzzle(b: &Book) -> Option<PuzzleDto> {
    Some(match &b.puzzles {
        Puzzles::Search(p) => PuzzleDto::Search {
            rows: p.first()?.rows.clone(),
            words: p
                .first()?
                .words
                .iter()
                .map(|h| (h.word.to_uppercase(), h.theme))
                .collect(),
        },
        Puzzles::Crossword(p) => {
            let x = p.first()?;
            PuzzleDto::Crossword {
                rows: x.rows.clone(),
                clues: x
                    .clues
                    .iter()
                    .map(|c| {
                        (
                            c.number,
                            match c.dir {
                                deckle_digest::puzzle::grid::Dir::Across => "across".to_owned(),
                                deckle_digest::puzzle::grid::Dir::Down => "down".to_owned(),
                            },
                            c.clue.clone(),
                            c.theme,
                        )
                    })
                    .collect(),
            }
        }
        Puzzles::Sudoku(p) => PuzzleDto::Sudoku {
            givens: p.first()?.givens.to_vec(),
        },
        Puzzles::Ladder(p) => PuzzleDto::Ladder {
            ladders: p
                .first()?
                .ladders
                .iter()
                .map(|l| (l.from.to_uppercase(), l.to.to_uppercase(), l.path.len() - 1))
                .collect(),
        },
        Puzzles::Cryptogram(p) => {
            let c = p.first()?;
            PuzzleDto::Cryptogram {
                words: c.words().iter().map(|w| (*w).to_owned()).collect(),
                given: c
                    .given
                    .iter()
                    .filter_map(|&g| c.decode(g).map(|p| (g, p)))
                    .collect(),
            }
        }
        Puzzles::Nonogram(p) => {
            let n = p.first()?;
            PuzzleDto::Nonogram {
                size: n.size(),
                rows: n.row_clues(),
                cols: n.col_clues(),
            }
        }
        Puzzles::Maze(p) => {
            let m = p.first()?;
            PuzzleDto::Maze {
                size: m.size,
                walls: m.walls.clone(),
            }
        }
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewDto {
    pub title: String,
    pub subtitle: String,
    pub seed: u64,
    pub date: String,
    /// Pages the whole book will have.
    pub pages: usize,
    pub warnings: Vec<String>,
    pub first: Option<PuzzleDto>,
}

/// The first puzzle of the book `r` asks for: made alone, with the seed the whole book would
/// have, so it's the same puzzle 1 as **Make** gives.
pub fn puzzles_preview(r: BookRequest) -> Result<PreviewDto, String> {
    let (full, page, mut warnings) = options(&r)?;
    let seed = full.seed.unwrap_or_else(|| book::default_seed(&full));
    let one = BookOptions {
        count: 1,
        seed: Some(seed),
        ..full.clone()
    };
    let b = book::make(&one, Words::embedded()).map_err(|e| e.to_string())?;
    // pages as the whole book will have them: the title, each puzzle, the answers
    let per_puzzle = pdf::pages(&b, page).saturating_sub(2).max(1);
    let answers = full.count.div_ceil(match full.kind {
        Kind::Sudoku | Kind::Nonogram => 6,
        Kind::Ladder => 10,
        Kind::Cryptogram => 6,
        _ => 4,
    });
    warnings.extend(b.warnings.iter().cloned());
    let mut sub = b.subtitle.clone();
    // the subtitle counts the one puzzle; say the book's count
    if let Some(i) = sub.rfind(" · ") {
        let plural = match full.kind {
            Kind::Ladder => "pages",
            _ if full.count == 1 => "puzzle",
            _ => "puzzles",
        };
        sub = format!("{} · {} {plural}", &sub[..i], full.count);
    }
    Ok(PreviewDto {
        title: b.title.clone(),
        subtitle: sub,
        seed,
        date: b.date.to_string(),
        pages: 1 + per_puzzle * full.count + answers,
        warnings,
        first: first_puzzle(&b),
    })
}

/// One `puzzles-progress` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub made: usize,
    pub of: usize,
}

/// A book in the folder, with the request that made it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MadeBook {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub date: String,
    pub seed: u64,
    pub pages: usize,
    pub puzzles: usize,
    pub bytes: u64,
    pub warnings: Vec<String>,
    pub request: BookRequest,
}

/// Makes the book, telling `puzzles-progress` as it goes, and keeps it in [`folder`] with its
/// request beside it.
pub fn puzzles_make(events: Arc<dyn Events>, r: BookRequest) -> Result<MadeBook, String> {
    let (o, page, mut warnings) = options(&r)?;
    let mut b = book::make_with(&o, Words::embedded(), &mut |made, of| {
        events.emit(
            "puzzles-progress",
            serde_json::to_value(ProgressEvent { made, of }).unwrap_or(serde_json::Value::Null),
        );
    })
    .map_err(|e| e.to_string())?;
    warnings.append(&mut b.warnings);
    b.warnings = warnings;
    let bytes = pdf::render(&b, page).map_err(|e| e.to_string())?;
    let dir = folder();
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let name = book::file_name(&b);
    let path = dir.join(&name);
    std::fs::write(&path, &bytes).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    let made = MadeBook {
        path: path.to_string_lossy().into_owned(),
        name: name.clone(),
        kind: o.kind.key().to_owned(),
        title: b.title.clone(),
        subtitle: b.subtitle.clone(),
        date: b.date.to_string(),
        seed: b.seed,
        pages: pdf::pages(&b, page),
        puzzles: b.puzzles.len(),
        bytes: bytes.len() as u64,
        warnings: b.warnings.clone(),
        request: BookRequest {
            seed: Some(b.seed),
            date: b.date.to_string(),
            ..r
        },
    };
    let sidecar = path.with_extension("json");
    let json = serde_json::to_vec_pretty(&made).map_err(|e| e.to_string())?;
    std::fs::write(&sidecar, json)
        .map_err(|e| format!("can't write {}: {e}", sidecar.display()))?;
    Ok(made)
}

/// The books in [`folder`], newest first: every PDF with its sidecar.
pub fn puzzles_list() -> Vec<MadeBook> {
    list_in(&folder())
}

pub fn list_in(dir: &Path) -> Vec<MadeBook> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(std::time::SystemTime, MadeBook)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let text = std::fs::read(e.path()).ok()?;
            let mut made: MadeBook = serde_json::from_slice(&text).ok()?;
            let pdf = e.path().with_extension("pdf");
            let meta = std::fs::metadata(&pdf).ok()?;
            made.path = pdf.to_string_lossy().into_owned();
            made.bytes = meta.len();
            Some((meta.modified().ok()?, made))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, m)| m).collect()
}

/// Copies the book at `path` to `target`, under its own name; a book already there is left
/// alone (its ink may be on it) unless `replace`.
pub fn puzzles_send(path: String, target: RunTarget, replace: bool) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("can't read the book: {e}"))?;
    let name = Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("a book needs a name")?;
    let target = resolve_run_target(&target)?;
    let mut conn = Conn::new(&target);
    let there = match conn.target {
        Target::Folder(dir) => dir.join(&name).exists(),
        Target::MassStorage(dev) => dev.root.join("documents").join(&name).exists(),
        Target::Mtp(_) => false,
    };
    if there && !replace {
        return Err(format!(
            "{name} is already there; send again to replace it (its ink may stay)"
        ));
    }
    deliver_named_on(&bytes, &name, &mut conn, &DeliverOptions::default())
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::NoEvents;

    fn req(kind: &str) -> BookRequest {
        BookRequest {
            kind: kind.into(),
            count: 3,
            seed: Some(5),
            date: "2026-11-02".into(),
            ..Default::default()
        }
    }

    #[test]
    fn every_kind_is_described() {
        let kinds = puzzles_kinds();
        assert_eq!(kinds.len(), 7);
        let cross = kinds.iter().find(|k| k.key == "crossword").unwrap();
        assert_eq!(
            (cross.sizes, cross.default_size, cross.takes.as_str()),
            (Some((5, 15)), 11, "clued")
        );
        let nono = kinds.iter().find(|k| k.key == "nonogram").unwrap();
        assert!(!nono.graded && nono.takes == "picture");
        assert!(
            kinds
                .iter()
                .find(|k| k.key == "sudoku")
                .unwrap()
                .sizes
                .is_none()
        );
    }

    #[test]
    fn the_preview_is_the_books_first_puzzle() {
        for kind in [
            "crossword",
            "search",
            "sudoku",
            "ladder",
            "cryptogram",
            "nonogram",
            "maze",
        ] {
            let p = puzzles_preview(req(kind)).unwrap();
            assert!(p.first.is_some(), "{kind}");
            assert!(p.pages >= 5, "{kind}: {}", p.pages);
            assert!(
                p.subtitle.ends_with("3 puzzles") || p.subtitle.ends_with("3 pages"),
                "{kind}: {}",
                p.subtitle
            );
            // the full book's first puzzle matches
            let (o, _, _) = options(&req(kind)).unwrap();
            let full = book::make(&o, Words::embedded()).unwrap();
            assert_eq!(first_puzzle(&full), p.first, "{kind}");
        }
        assert!(puzzles_preview(req("riddle")).is_err());
    }

    #[test]
    fn a_made_book_is_kept_with_its_request_and_listed() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: this test's process; the folder follows HOME
        unsafe { std::env::set_var("HOME", tmp.path()) };
        let mut r = req("maze");
        r.title = "Mazes for Mia".into();
        let made = puzzles_make(Arc::new(NoEvents), r).unwrap();
        assert!(made.name.starts_with("Mazes for Mia 2026-11-02 "));
        assert!(Path::new(&made.path).exists());
        assert_eq!(made.request.seed, Some(made.seed));
        let listed = list_in(&folder());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Mazes for Mia");
        assert_eq!(listed[0].bytes, made.bytes);
        // make again: the same book
        let again = puzzles_make(Arc::new(NoEvents), listed[0].request.clone()).unwrap();
        assert_eq!(again.seed, made.seed);
        assert_eq!(again.name, made.name);
        // send to a folder
        let out = tmp.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        let target = RunTarget::Folder {
            path: out.to_string_lossy().into_owned(),
        };
        puzzles_send(made.path.clone(), target.clone(), false).unwrap();
        assert!(out.join(&made.name).exists());
        assert!(
            puzzles_send(made.path.clone(), target.clone(), false)
                .unwrap_err()
                .contains("already there")
        );
        puzzles_send(made.path, target, true).unwrap();
    }
}
