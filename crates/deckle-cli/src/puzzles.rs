// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle puzzles` (puzzle books §5.2): a write-on puzzle book, made and delivered as a PDF.

use std::path::PathBuf;
use std::process::ExitCode;

use deckle_core::deliver::{Conn, DeliverOptions, Target, deliver_named_on};
use deckle_digest::chrono::{Local, NaiveDate};
use deckle_digest::puzzle::book::pdf::{self, PageSize};
use deckle_digest::puzzle::book::{self, BookOptions, Difficulty, Kind};
use deckle_digest::puzzle::words::Words;
use serde::Serialize;

use super::cli::{BookArgs, DifficultyArg, PuzzlesCommand};
use super::{emit, resolve_target};

#[derive(Serialize)]
struct Report {
    path: PathBuf,
    pages: usize,
    puzzles: usize,
    seed: u64,
    date: String,
    warnings: Vec<String>,
}

pub fn run(json: bool, action: PuzzlesCommand) -> Result<ExitCode, String> {
    match action {
        PuzzlesCommand::Search {
            size,
            difficulty,
            words,
            book,
        } => make(json, Kind::Search, size, difficulty, words, book),
        PuzzlesCommand::Crossword {
            size,
            difficulty,
            words,
            book,
        } => make(json, Kind::Crossword, size, difficulty, words, book),
        PuzzlesCommand::Sudoku { difficulty, book } => {
            make(json, Kind::Sudoku, 9, difficulty, None, book)
        }
        PuzzlesCommand::Ladder { difficulty, book } => {
            make_more(json, Kind::Ladder, 4, difficulty, None, None, book)
        }
        PuzzlesCommand::Cryptogram {
            difficulty,
            quotes,
            book,
        } => make_more(json, Kind::Cryptogram, 0, difficulty, quotes, None, book),
        PuzzlesCommand::Nonogram {
            size,
            picture,
            book,
        } => make_more(
            json,
            Kind::Nonogram,
            size,
            DifficultyArg::Medium,
            None,
            picture,
            book,
        ),
        PuzzlesCommand::Maze { size, book } => make_more(
            json,
            Kind::Maze,
            size,
            DifficultyArg::Medium,
            None,
            None,
            book,
        ),
    }
}

/// The options every kind shares, from the command line.
fn base_options(
    kind: Kind,
    size: usize,
    difficulty: DifficultyArg,
    args: &BookArgs,
) -> Result<(BookOptions, PageSize), String> {
    let date = match &args.date {
        Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|_| format!("--date must be YYYY-MM-DD, not {d}"))?,
        None => Local::now().date_naive(),
    };
    let page = PageSize::parse(&args.screen).ok_or_else(|| format!("no screen {}", args.screen))?;
    let difficulty = match difficulty {
        DifficultyArg::Easy => Difficulty::Easy,
        DifficultyArg::Medium => Difficulty::Medium,
        DifficultyArg::Hard => Difficulty::Hard,
    };
    Ok((
        BookOptions {
            count: args.count,
            size,
            difficulty,
            title: args.title.clone(),
            seed: args.seed,
            ..BookOptions::new(kind, date)
        },
        page,
    ))
}

fn make(
    json: bool,
    kind: Kind,
    size: usize,
    difficulty: DifficultyArg,
    words: Option<PathBuf>,
    args: BookArgs,
) -> Result<ExitCode, String> {
    let text = match &words {
        Some(p) => Some(
            std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
        ),
        None => None,
    };
    // a crossword's words come with clues; a word search's are bare
    let mut warnings = Vec::new();
    let (own, clued) = match (&text, kind) {
        (Some(t), Kind::Crossword) => {
            let (clued, skipped) = book::clued_words_from_file(t);
            if !skipped.is_empty() {
                let names: Vec<String> = skipped.iter().map(|w| w.to_uppercase()).collect();
                warnings.push(format!("skipped, no clue: {}", names.join(", ")));
            }
            (None, Some(clued))
        }
        (Some(t), _) => (Some(book::words_from_file(t)), None),
        (None, _) => (None, None),
    };
    let (mut opts, page) = base_options(kind, size, difficulty, &args)?;
    opts.words = own;
    opts.clued = clued;
    let extra = match &words {
        Some(w) => format!(" --words {}", quote(&w.to_string_lossy())),
        None => String::new(),
    };
    make_book(json, opts, page, &args, warnings, &extra)
}

/// `make` for the kinds that take quotes or a picture rather than words.
fn make_more(
    json: bool,
    kind: Kind,
    size: usize,
    difficulty: DifficultyArg,
    quotes: Option<PathBuf>,
    picture: Option<PathBuf>,
    args: BookArgs,
) -> Result<ExitCode, String> {
    let mut warnings = Vec::new();
    let (mut opts, page) = base_options(kind, size, difficulty, &args)?;
    let mut extra = String::new();
    if let Some(p) = &quotes {
        let text =
            std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
        let (q, skipped) = deckle_digest::puzzle::cryptogram::quotes_from_file(&text);
        if !skipped.is_empty() {
            warnings.push(format!(
                "skipped, too short or too long: {}",
                skipped.join("; ")
            ));
        }
        opts.quotes = Some(q);
        extra += &format!(" --quotes {}", quote(&p.to_string_lossy()));
    }
    if let Some(p) = &picture {
        let bytes = std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        opts.picture = Some((bytes, name));
        extra += &format!(" --picture {}", quote(&p.to_string_lossy()));
    }
    make_book(json, opts, page, &args, warnings, &extra)
}

/// Makes the book, writes or sends it, and prints the report with the command that remakes
/// it; `extra` is that command's kind-specific flags.
fn make_book(
    json: bool,
    opts: BookOptions,
    page: PageSize,
    args: &BookArgs,
    mut warnings: Vec<String>,
    extra: &str,
) -> Result<ExitCode, String> {
    let kind = opts.kind;
    let mut b = book::make(&opts, Words::embedded()).map_err(|e| e.to_string())?;
    warnings.append(&mut b.warnings);
    b.warnings = warnings;
    let bytes = pdf::render(&b, page).map_err(|e| e.to_string())?;
    let name = book::file_name(&b);
    let path = deliver(&bytes, &name, args)?;

    let mut again = format!("deckle puzzles {} --count {}", kind.key(), b.puzzles.len());
    if kind.sized() {
        again += &format!(" --size {}", opts.size);
    }
    if !matches!(kind, Kind::Nonogram | Kind::Maze) {
        again += &format!(" --difficulty {}", opts.difficulty.name());
    }
    again += extra;
    if let Some(t) = &args.title {
        again += &format!(" --title {}", quote(t));
    }
    if page != PageSize::KindleScribe {
        again += &format!(" --screen {}", args.screen);
    }
    again += &format!(" --seed {} --date {}", b.seed, b.date);
    match (&args.out, &args.to) {
        (Some(dir), _) => again += &format!(" -o {}", quote(&dir.to_string_lossy())),
        (None, Some(to)) => again += &format!(" --to {}", quote(to)),
        (None, None) => {}
    }
    let report = Report {
        path,
        pages: pdf::pages(&b, page),
        puzzles: b.puzzles.len(),
        seed: b.seed,
        date: b.date.to_string(),
        warnings: b.warnings.clone(),
    };
    emit(json, &report, || {
        let plural = if report.puzzles == 1 {
            "puzzle"
        } else {
            "puzzles"
        };
        let mut text = format!(
            "wrote {} ({} pages, {} {plural}, seed {})\nremake it with: {again}",
            report.path.display(),
            report.pages,
            report.puzzles,
            report.seed,
        );
        for w in &report.warnings {
            text += &format!("\nwarning: {w}");
        }
        text
    })?;
    Ok(ExitCode::SUCCESS)
}

/// The book to `-o` or `--to`, under `name`; never over a book already there unless
/// `--replace`, since the Kindle keeps a PDF's ink beside it (puzzle books §5.2).
fn deliver(bytes: &[u8], name: &str, args: &BookArgs) -> Result<PathBuf, String> {
    let target = match (&args.out, &args.to) {
        (Some(dir), _) => {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
            Target::Folder(dir.clone())
        }
        (None, Some(to)) => resolve_target(to)?,
        (None, None) => return Err("pass -o <folder> or --to device|<folder>".into()),
    };
    let mut conn = Conn::new(&target);
    if !args.replace && already_there(&mut conn, name)? {
        return Err(format!(
            "{name} is already there; pass --replace to overwrite it (its ink may stay)"
        ));
    }
    deliver_named_on(bytes, name, &mut conn, &DeliverOptions::default()).map_err(|e| e.to_string())
}

fn already_there(conn: &mut Conn, name: &str) -> Result<bool, String> {
    match conn.target {
        Target::Folder(dir) => Ok(dir.join(name).exists()),
        Target::MassStorage(dev) => Ok(dev.root.join("documents").join(name).exists()),
        Target::Mtp(_) => {
            let dev = conn
                .mtp()
                .expect("an MTP target")
                .map_err(|e| e.to_string())?;
            let storage = *dev
                .storages()
                .map_err(|e| e.to_string())?
                .first()
                .ok_or("the Kindle reports no storage")?;
            let docs = dev.find_documents(storage).map_err(|e| e.to_string())?;
            Ok(dev
                .find_child(storage, docs, name)
                .map_err(|e| e.to_string())?
                .is_some())
        }
    }
}

/// `s` as one shell word.
fn quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
    {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}
