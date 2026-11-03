// SPDX-License-Identifier: GPL-3.0-or-later
//! The Puzzles tab (Puzzles tab design §1, §3): a book made from the app, previewed first,
//! kept in `~/Deckle/Puzzles` beside the Morgue with the options that made it, and sent to a
//! Kindle or a folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deckle_core::deliver::{Conn, DeliverOptions, Target, deliver_named_on};
use deckle_digest::chrono::{Local, NaiveDate, Utc};
use deckle_digest::config::{ExportFormat, PuzzlesConfig};
use deckle_digest::editor::AppAi;
use deckle_digest::puzzle::ai::{AiConfig, Style};
use deckle_digest::puzzle::book::bundle::{self, Extras};
use deckle_digest::puzzle::book::pdf::{self, PageSize};
use deckle_digest::puzzle::book::plan::{self, Plan, Theme};
use deckle_digest::puzzle::book::theme::{self, Source};
use deckle_digest::puzzle::book::{self, Book, BookOptions, Difficulty, Kind, Puzzles, sunday};
use deckle_digest::puzzle::cryptogram;
use deckle_digest::puzzle::words::Words;
use deckle_digest::returns;
use serde::{Deserialize, Serialize};

use crate::commands::{RunTarget, resolve_run_target};
use crate::events::{Events, NoEvents};

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
    /// Where the theme words come from: `none`, `own`, `interests`, `week`, `vocab`,
    /// `cookbook` or `subject` (Puzzles tab design §2).
    pub theme: String,
    /// The interests picked, for `interests`; empty means all of them.
    pub interests: Vec<String>,
    /// What the AI writes about, for `subject`.
    pub subject: String,
    /// How the clues read: `plain`, or with the AI `trivia`, `cryptic`, `kid`.
    pub clue_style: String,
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
            theme: "none".into(),
            interests: Vec::new(),
            subject: String::new(),
            clue_style: "plain".into(),
        }
    }
}

impl BookRequest {
    /// The plan the request asks for.
    pub fn plan(&self) -> Result<Plan, String> {
        let style = Style::parse(&self.clue_style)
            .ok_or_else(|| format!("no clue style {}", self.clue_style))?;
        let theme = match self.theme.trim() {
            "" | "none" => Theme::None,
            "own" => Theme::Own,
            "interests" => Theme::Source(Source::Interests {
                names: self.interests.clone(),
                days: 30,
            }),
            "week" => Theme::Source(Source::Week { days: 7 }),
            "vocab" => Theme::Source(Source::Vocab),
            "cookbook" => Theme::Source(Source::Cookbook),
            "subject" => Theme::Subject {
                text: self.subject.clone(),
            },
            other => return Err(format!("no theme {other}")),
        };
        Ok(Plan { theme, style })
    }
}

/// What the tab can offer as a theme, and whether the AI is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcesDto {
    pub sources: theme::Sources,
    /// Where the AI runs, as Settings → AI names it; `None` when it's off.
    pub ai: Option<String>,
}

pub fn puzzles_sources() -> SourcesDto {
    let main = deckle_digest::config::default_path();
    SourcesDto {
        sources: theme::sources(&main, Local::now().date_naive()),
        ai: ai_where(),
    }
}

/// "Ollama on this computer", "Anthropic": the AI setting in a word, or `None` when off.
fn ai_where() -> Option<String> {
    match AppAi::read() {
        AppAi::On {
            provider, endpoint, ..
        } => Some(match provider {
            deckle_xray::llm::Provider::Ollama
                if endpoint.trim().is_empty()
                    || endpoint.contains("localhost")
                    || endpoint.contains("127.0.0.1") =>
            {
                "Ollama on this computer".into()
            }
            deckle_xray::llm::Provider::Ollama => "Ollama on your network".into(),
            deckle_xray::llm::Provider::Anthropic => "Anthropic".into(),
            deckle_xray::llm::Provider::OpenAi => "an OpenAI-compatible service".into(),
        }),
        _ => None,
    }
}

fn ai_config() -> Option<AiConfig> {
    AppAi::read().ai_config()
}

/// The options with the plan applied: the theme's words in place, and what was found.
fn prepared(
    r: &BookRequest,
) -> Result<(BookOptions, PageSize, Vec<String>, Plan, plan::Prepared), String> {
    let (mut o, page, mut warnings) = options(r)?;
    let plan = r.plan()?;
    let ai = ai_config();
    let date = o.date;
    let found = plan::prepare(
        &mut o,
        &plan,
        &deckle_digest::config::default_path(),
        date,
        ai.as_ref(),
        &mut deckle_xray::llm::http_post,
    )?;
    warnings.extend(found.warnings.iter().cloned());
    Ok((o, page, warnings, plan, found))
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
    if let Some(p) = r.picture.as_deref().filter(|p| !p.trim().is_empty())
        && kind == Kind::Nonogram
    {
        let bytes = std::fs::read(p).map_err(|e| format!("can't read the picture: {e}"))?;
        let name = Path::new(p)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        o.picture = Some((bytes, name));
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
    let (full, page, mut warnings, plan, found) = prepared(&r)?;
    let seed = full.seed.unwrap_or_else(|| book::default_seed(&full));
    let one = BookOptions {
        count: 1,
        seed: Some(seed),
        ..full.clone()
    };
    let mut b = book::make(&one, Words::embedded()).map_err(|e| e.to_string())?;
    plan::restyle(
        &mut b,
        &plan,
        &found.about,
        ai_config().as_ref(),
        &mut deckle_xray::llm::http_post,
    );
    if !found.note.is_empty() {
        warnings.insert(0, found.note.clone());
    }
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

/// One `puzzles-progress` event: where the make is, and how far through that step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// `words` (the theme's words, maybe from the AI), `puzzles` (one by one), `clues` (the AI
    /// restyling them, puzzle by puzzle), `pages` (the PDF laid out and saved).
    pub stage: String,
    pub made: usize,
    pub of: usize,
}

fn progress(events: &dyn Events, stage: &str, made: usize, of: usize) {
    events.emit(
        "puzzles-progress",
        serde_json::to_value(ProgressEvent {
            stage: stage.to_owned(),
            made,
            of,
        })
        .unwrap_or(serde_json::Value::Null),
    );
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
    /// The bundles written for the iPad (puzzle book bundle design §5): each
    /// `.decklepuzzles` folder **Send to <iPad folder>** put it in.
    #[serde(default)]
    pub bundles: Vec<String>,
    /// Solved on the iPad, `(n, of)`, from the returns beside those bundles; `None` until a
    /// reader has written about it. Filled by [`puzzles_list`], never saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solved_ipad: Option<(usize, usize)>,
}

/// A book as the engine made it, with what made it.
struct Made {
    book: Book,
    options: BookOptions,
    page: PageSize,
    plan: Plan,
}

/// Makes the book `r` asks for, telling `puzzles-progress` as it goes: one `make` call, so
/// the PDF and a bundle of the same request, seed and date are the same book.
fn make_book(events: &dyn Events, r: &BookRequest) -> Result<Made, String> {
    progress(events, "words", 0, 0);
    let (o, page, mut warnings, plan, found) = prepared(r)?;
    let mut b = book::make_with(&o, Words::embedded(), &mut |made, of| {
        progress(events, "puzzles", made, of);
    })
    .map_err(|e| e.to_string())?;
    plan::restyle_with(
        &mut b,
        &plan,
        &found.about,
        ai_config().as_ref(),
        &mut deckle_xray::llm::http_post,
        &mut |made, of| progress(events, "clues", made, of),
    );
    if !found.note.is_empty() {
        warnings.insert(0, found.note.clone());
    }
    warnings.append(&mut b.warnings);
    b.warnings = warnings;
    Ok(Made {
        book: b,
        options: o,
        page,
        plan,
    })
}

/// Makes the book, telling `puzzles-progress` as it goes, and keeps it in [`folder`] with its
/// request beside it.
pub fn puzzles_make(events: Arc<dyn Events>, r: BookRequest) -> Result<MadeBook, String> {
    let Made {
        book: b,
        options: o,
        page,
        ..
    } = make_book(events.as_ref(), &r)?;
    progress(events.as_ref(), "pages", 0, 0);
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
        bundles: Vec::new(),
        solved_ipad: None,
    };
    write_sidecar(&path, &made)?;
    Ok(made)
}

/// The book's sidecar, `<book>.json` beside the PDF.
fn write_sidecar(pdf: &Path, made: &MadeBook) -> Result<(), String> {
    let sidecar = pdf.with_extension("json");
    let json = serde_json::to_vec_pretty(made).map_err(|e| e.to_string())?;
    std::fs::write(&sidecar, json).map_err(|e| format!("can't write {}: {e}", sidecar.display()))
}

fn read_sidecar(pdf: &Path) -> Result<MadeBook, String> {
    let sidecar = pdf.with_extension("json");
    let text =
        std::fs::read(&sidecar).map_err(|e| format!("can't read {}: {e}", sidecar.display()))?;
    serde_json::from_slice(&text).map_err(|e| format!("{}: {e}", sidecar.display()))
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
    out.sort_by_key(|a| std::cmp::Reverse(a.0));
    // Each export folder's returns read once, for every book sent there.
    let mut read: HashMap<PathBuf, Vec<returns::PuzzleBookReturns>> = HashMap::new();
    for (_, made) in &mut out {
        made.solved_ipad = solved_on_ipad(&made.bundles, &mut read);
    }
    out.into_iter().map(|(_, m)| m).collect()
}

/// `(solved, of)` for a book from the returns beside each bundle it was sent as (puzzle book
/// bundle design §4): a bundle no longer there says nothing.
fn solved_on_ipad(
    bundles: &[String],
    read: &mut HashMap<PathBuf, Vec<returns::PuzzleBookReturns>>,
) -> Option<(usize, usize)> {
    let mut all = Vec::new();
    let mut id = None;
    for b in bundles {
        let path = Path::new(b);
        let Ok(json) = bundle::read(path) else {
            continue;
        };
        id = Some((json.book.id, json.puzzles.len()));
        let root = path.parent()?.to_path_buf();
        let r = read
            .entry(root.clone())
            .or_insert_with(|| returns::read_books(&root));
        all.extend(r.iter().cloned());
    }
    let (id, of) = id?;
    returns::solved(&all, id, of).map(|s| (s.done, s.of))
}

/// An export folder that takes bundles, where **Send to <name>** puts a book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleTarget {
    /// The export's `name`: "iPad", "Mia's iPad".
    pub name: String,
    pub dir: String,
}

/// Every `format = "bundle"` export of every paper, each folder once.
pub fn puzzles_bundle_targets() -> Vec<BundleTarget> {
    bundle_targets_in(&deckle_digest::config::default_path())
}

pub fn bundle_targets_in(main: &Path) -> Vec<BundleTarget> {
    let Ok(loaded) = deckle_digest::papers::load_all(main) else {
        return Vec::new();
    };
    let mut out: Vec<BundleTarget> = Vec::new();
    for e in loaded
        .papers
        .iter()
        .flat_map(|p| p.export.iter())
        .filter(|e| e.format == ExportFormat::Bundle)
    {
        let dir = deckle_digest::inbox::expand_home(e.dir.trim())
            .to_string_lossy()
            .into_owned();
        if !out.iter().any(|t| t.dir == dir) {
            out.push(BundleTarget {
                name: e.name.clone(),
                dir,
            });
        }
    }
    out
}

/// Copies the book at `path` to `target`, under its own name; a book already there is left
/// alone (its ink may be on it) unless `replace`. With `as_bundle`, the target is an iPad's
/// folder and the book goes as a `.decklepuzzles` bundle instead (puzzle book bundle design
/// §5), made again from its saved request with the same seed and date, so it's the PDF's
/// book.
pub fn puzzles_send(
    path: String,
    target: RunTarget,
    replace: bool,
    as_bundle: bool,
) -> Result<String, String> {
    if as_bundle {
        return send_bundle(Path::new(&path), &target, replace);
    }
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

fn send_bundle(pdf: &Path, target: &RunTarget, replace: bool) -> Result<String, String> {
    let RunTarget::Folder { path: dir } = target else {
        return Err("a bundle is for the iPad: send it to a folder its reader watches".into());
    };
    let dir = deckle_digest::inbox::expand_home(dir.trim());
    let mut made = read_sidecar(pdf)?;
    let Made {
        book: b,
        options: o,
        plan,
        ..
    } = make_book(&NoEvents, &made.request)?;
    if b.seed != made.seed || b.date.to_string() != made.date {
        return Err("the book didn't come out as it was made; Make again and send that".into());
    }
    let name = bundle::bundle_name(&b);
    if !replace && dir.join(&name).exists() {
        return Err(format!(
            "{name} is already there; send again to replace it (its ink may stay)"
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let extras = Extras {
        theme: plan.theme,
        style: plan.style,
        paper_id: None,
        sunday_of: None,
        built_at: Utc::now(),
        replace,
    };
    let written = bundle::write_with(&b, &o, &extras, &dir)?;
    let written = written.to_string_lossy().into_owned();
    if !made.bundles.contains(&written) {
        made.bundles.push(written.clone());
    }
    write_sidecar(pdf, &made)?;
    Ok(written)
}

/// The paper's `[puzzles]` table: Sunday's switch and recipe (`None`: the main paper).
pub fn puzzles_sunday_get(paper: Option<String>) -> Result<PuzzlesConfig, String> {
    let main = deckle_digest::config::default_path();
    Ok(crate::daily::paper_config_at(&main, paper.as_deref())?.puzzles)
}

/// Saves the paper's `[puzzles]` table, checked first: a kind, difficulty, theme or clue
/// style the engine doesn't know is refused.
pub fn puzzles_sunday_set(
    paper: Option<String>,
    config: PuzzlesConfig,
) -> Result<PuzzlesConfig, String> {
    let main = deckle_digest::config::default_path();
    sunday_set_at(&main, paper.as_deref(), config)
}

pub fn sunday_set_at(
    main: &Path,
    paper: Option<&str>,
    config: PuzzlesConfig,
) -> Result<PuzzlesConfig, String> {
    sunday::options(&config, Local::now().date_naive())?;
    let mut cfg = crate::daily::paper_config_at(main, paper)?;
    cfg.puzzles = config.clone();
    crate::daily::save_paper_at(main, paper, cfg)?;
    Ok(config)
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
    fn a_request_becomes_a_plan() {
        let r = BookRequest {
            theme: "interests".into(),
            interests: vec!["Rust".into()],
            clue_style: "cryptic".into(),
            ..Default::default()
        };
        let p = r.plan().unwrap();
        assert_eq!(
            p.theme,
            Theme::Source(Source::Interests {
                names: vec!["Rust".into()],
                days: 30
            })
        );
        assert_eq!(p.style, Style::Cryptic);
        assert_eq!(BookRequest::default().plan().unwrap(), Plan::default());
        assert!(
            BookRequest {
                theme: "moon".into(),
                ..Default::default()
            }
            .plan()
            .is_err()
        );
        assert!(
            BookRequest {
                clue_style: "loud".into(),
                ..Default::default()
            }
            .plan()
            .is_err()
        );
        let s = BookRequest {
            theme: "subject".into(),
            subject: "Rome".into(),
            ..Default::default()
        }
        .plan()
        .unwrap();
        assert_eq!(
            s.theme,
            Theme::Subject {
                text: "Rome".into()
            }
        );
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
        puzzles_send(made.path.clone(), target.clone(), false, false).unwrap();
        assert!(out.join(&made.name).exists());
        assert!(
            puzzles_send(made.path.clone(), target.clone(), false, false)
                .unwrap_err()
                .contains("already there")
        );
        puzzles_send(made.path, target, true, false).unwrap();
    }

    #[test]
    fn sent_as_a_bundle_the_book_is_the_pdfs_and_the_ipads_returns_count_as_solved() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: this test's process; the folder follows HOME
        unsafe { std::env::set_var("HOME", tmp.path()) };
        let mut r = req("maze");
        r.title = "Mazes for Mia".into();
        r.size = 8;
        let made = puzzles_make(Arc::new(NoEvents), r).unwrap();
        assert!(made.bundles.is_empty() && made.solved_ipad.is_none());
        let ipad = tmp.path().join("iPad");
        let target = RunTarget::Folder {
            path: ipad.to_string_lossy().into_owned(),
        };
        // a Kindle can't take one
        let e = puzzles_send(
            made.path.clone(),
            RunTarget::Device {
                root: "/Volumes/Kindle".into(),
            },
            false,
            true,
        )
        .unwrap_err();
        assert!(e.contains("folder"), "{e}");
        let written = puzzles_send(made.path.clone(), target.clone(), false, true).unwrap();
        let bundle_dir = PathBuf::from(&written);
        assert_eq!(bundle_dir.parent().unwrap(), ipad);
        assert_eq!(
            bundle_dir.file_name().unwrap().to_string_lossy(),
            format!(
                "deckle-puzzles-mazes-for-mia-2026-11-02-{:04x}.decklepuzzles",
                made.seed >> 48
            )
        );
        bundle::verify(&bundle_dir).unwrap();
        let json = bundle::read(&bundle_dir).unwrap();
        assert_eq!(json.book.seed, format!("{:016x}", made.seed));
        assert_eq!(json.puzzles.len(), made.puzzles);
        assert_eq!(json.book.sunday_of, None);
        // the sidecar remembers where it went, and the list shows it
        let listed = list_in(&folder());
        assert_eq!(listed[0].bundles, std::slice::from_ref(&written));
        assert_eq!(listed[0].solved_ipad, None, "no returns yet");
        // never over one already there, unless replace
        let e = puzzles_send(made.path.clone(), target.clone(), false, true).unwrap_err();
        assert!(e.contains("already there"), "{e}");
        puzzles_send(made.path.clone(), target.clone(), true, true).unwrap();
        assert_eq!(
            list_in(&folder())[0].bundles.len(),
            1,
            "sent twice, listed once"
        );
        // the iPad solved one of three
        let returns = ipad.join(".deckle-returns/ipad-1/puzzles");
        std::fs::create_dir_all(&returns).unwrap();
        std::fs::write(
            returns.join(format!("{}.json", json.book.id)),
            serde_json::json!({
                "format": "deckle-puzzle-returns", "version": "1.0",
                "reader": { "id": "ipad-1", "name": "Mia's iPad", "app": "Deckle Reader" },
                "book": json.book.id, "updated_at": "2026-11-02T20:00:00Z",
                "puzzles": [
                    { "n": 1, "done": true, "seconds": 90, "last": "2026-11-02T20:00:00Z" },
                    { "n": 3, "done": false, "seconds": 30, "last": "2026-11-02T20:01:00Z" }
                ]
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(list_in(&folder())[0].solved_ipad, Some((1, 3)));
        // the bundle gone from the iPad: nothing to say
        std::fs::remove_dir_all(&bundle_dir).unwrap();
        assert_eq!(list_in(&folder())[0].solved_ipad, None);
    }

    #[test]
    fn bundle_targets_are_every_papers_bundle_exports_each_folder_once() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("digest.toml");
        let ipad = tmp.path().join("iPad");
        let kid = tmp.path().join("Kid");
        std::fs::write(
            &main,
            format!(
                "title = \"T\"\nsections = [\"weather\"]\n\n[[export]]\nname = \"Kobo\"\ndir = \"/tmp/kobo\"\nformat = \"kepub\"\n\n[[export]]\nname = \"iPad\"\ndir = {:?}\nformat = \"bundle\"\n",
                ipad.display().to_string()
            ),
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("papers")).unwrap();
        std::fs::write(
            tmp.path().join("papers/kid.toml"),
            format!(
                "title = \"Kid\"\nsections = [\"weather\"]\n\n[[export]]\nname = \"Mia's iPad\"\ndir = {:?}\nformat = \"bundle\"\n\n[[export]]\nname = \"iPad again\"\ndir = {:?}\nformat = \"bundle\"\n",
                kid.display().to_string(),
                ipad.display().to_string()
            ),
        )
        .unwrap();
        let targets = bundle_targets_in(&main);
        assert_eq!(
            targets,
            [
                BundleTarget {
                    name: "iPad".into(),
                    dir: ipad.to_string_lossy().into_owned()
                },
                BundleTarget {
                    name: "Mia's iPad".into(),
                    dir: kid.to_string_lossy().into_owned()
                }
            ]
        );
        assert!(bundle_targets_in(&tmp.path().join("none.toml")).is_empty());
    }

    #[test]
    fn sundays_recipe_is_saved_to_the_paper_and_checked_first() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("digest.toml");
        std::fs::write(&main, "title = \"T\"\nsections = [\"weather\"]\n").unwrap();
        let mut p = PuzzlesConfig {
            sunday: true,
            kind: "search".into(),
            count: 6,
            ..Default::default()
        };
        assert_eq!(sunday_set_at(&main, None, p.clone()).unwrap(), p);
        let saved = crate::daily::paper_config_at(&main, None).unwrap();
        assert_eq!(saved.puzzles, p);
        let text = std::fs::read_to_string(&main).unwrap();
        assert!(
            text.contains("puzzles") && text.contains("sunday = true"),
            "{text}"
        );
        p.kind = "riddle".into();
        assert!(sunday_set_at(&main, None, p).unwrap_err().contains("kind"));
        assert_eq!(
            crate::daily::paper_config_at(&main, None)
                .unwrap()
                .puzzles
                .kind,
            "search"
        );
    }
}
