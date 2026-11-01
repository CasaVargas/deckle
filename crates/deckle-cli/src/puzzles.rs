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
        } => search(json, size, difficulty, words, book),
    }
}

fn search(
    json: bool,
    size: usize,
    difficulty: DifficultyArg,
    words: Option<PathBuf>,
    args: BookArgs,
) -> Result<ExitCode, String> {
    let own = match &words {
        Some(p) => Some(book::words_from_file(
            &std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
        )),
        None => None,
    };
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
    let opts = BookOptions {
        kind: Kind::Search,
        count: args.count,
        size,
        difficulty,
        words: own,
        title: args.title.clone(),
        seed: args.seed,
        date,
    };
    let b = book::make(&opts, Words::embedded()).map_err(|e| e.to_string())?;
    let bytes = pdf::render(&b, page).map_err(|e| e.to_string())?;
    let name = book::file_name(&b);
    let path = deliver(&bytes, &name, &args)?;

    let mut again = format!(
        "deckle puzzles search --count {} --size {size} --difficulty {}",
        b.puzzles.len(),
        difficulty.name()
    );
    if let Some(w) = &words {
        again += &format!(" --words {}", quote(&w.to_string_lossy()));
    }
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
        pages: pdf::pages(&b),
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
