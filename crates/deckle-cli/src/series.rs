// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle library series` and `library list --series` (series design §3): the library
//! grouped by series, from `deckle_library::series`.

use std::process::ExitCode;

use deckle_library::series::{Entry, Series, series_key};
use deckle_library::{Book, Library};

use crate::{emit, library_line};

/// `library series [NAME] [--missing]`.
pub fn series(
    json: bool,
    lib: &Library,
    name: Option<&str>,
    missing: bool,
) -> Result<ExitCode, String> {
    let mut all = deckle_library::series::series(&lib.list());
    if let Some(name) = name {
        all = vec![find(all, name)?];
    }
    if missing {
        all.retain(|s| !s.missing.is_empty());
    }
    let rows: Vec<_> = all.iter().map(json_row).collect();
    emit(json, &rows, || {
        if all.is_empty() {
            return if missing {
                "no series with gaps".into()
            } else {
                format!("no series in {}", lib.root.display())
            };
        }
        match (name, missing) {
            (_, true) => all
                .iter()
                .map(|s| format!("{} — missing {}", s.name, numbers(&s.missing)))
                .collect::<Vec<_>>()
                .join("\n"),
            (Some(_), false) => reading_order(&all[0]),
            (None, false) => all.iter().map(summary).collect::<Vec<_>>().join("\n"),
        }
    })?;
    Ok(ExitCode::SUCCESS)
}

/// `library list --series`: [`library_line`]s under series headings, books outside a series
/// last.
pub fn list(json: bool, lib: &Library) -> Result<ExitCode, String> {
    let books = lib.list();
    let all = deckle_library::series::series(&books);
    let mut groups: Vec<(Option<&str>, Vec<&Book>)> = all
        .iter()
        .map(|s| (Some(s.name.as_str()), s.books().map(|(_, b)| b).collect()))
        .collect();
    let keyed = |b: &Book| {
        b.sidecar
            .metadata
            .series
            .as_ref()
            .is_some_and(|s| !series_key(&s.name).is_empty())
    };
    let loose: Vec<&Book> = books.iter().filter(|b| !keyed(b)).collect();
    if !loose.is_empty() {
        groups.push((None, loose));
    }
    let rows: Vec<_> = groups
        .iter()
        .map(|(name, books)| {
            let books: Vec<_> = books
                .iter()
                .map(|b| serde_json::json!({ "dir": b.dir, "book": b.sidecar }))
                .collect();
            serde_json::json!({ "series": name, "books": books })
        })
        .collect();
    emit(json, &rows, || {
        if books.is_empty() {
            return format!("no books in {}", lib.root.display());
        }
        groups
            .iter()
            .map(|(name, books)| {
                let lines: String = books
                    .iter()
                    .flat_map(|b| {
                        library_line(b)
                            .lines()
                            .map(|l| format!("\n  {l}"))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                format!("{}{lines}", name.unwrap_or("No series"))
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    })?;
    Ok(ExitCode::SUCCESS)
}

/// The series whose key is `name`'s, else the one whose key starts with it.
fn find(all: Vec<Series>, name: &str) -> Result<Series, String> {
    let key = series_key(name);
    if let Some(i) = all.iter().position(|s| s.key == key) {
        return Ok(all.into_iter().nth(i).expect("found"));
    }
    let mut hits: Vec<Series> = all
        .into_iter()
        .filter(|s| !key.is_empty() && s.key.starts_with(&key))
        .collect();
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => Err(format!("no series matches \"{name}\"")),
        _ => Err(format!(
            "\"{name}\" matches several series: {}",
            hits.iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn numbers(ns: &[u32]) -> String {
    ns.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
}

/// `Hainish Cycle — 4 books, missing 3`
fn summary(s: &Series) -> String {
    let n = s.books().count();
    let mut line = format!("{} — {n} book{}", s.name, if n == 1 { "" } else { "s" });
    if !s.missing.is_empty() {
        line.push_str(&format!(", missing {}", numbers(&s.missing)));
    }
    line
}

/// Distinct first authors, in reading order.
fn authors(s: &Series) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for (_, b) in s.books() {
        if let Some(a) = b.sidecar.metadata.authors.first()
            && !out.contains(&a.as_str())
        {
            out.push(a);
        }
    }
    out
}

/// The Kindles the book was last sent to.
fn kindles(b: &Book) -> String {
    let mut names: Vec<&str> = Vec::new();
    for s in &b.sidecar.sends {
        if !names.contains(&s.device_name.as_str()) {
            names.push(&s.device_name);
        }
    }
    names.join(", ")
}

/// The header, then one line per book or gap: number, title (and author when the series has
/// several), Kindles.
fn reading_order(s: &Series) -> String {
    let authors = authors(s);
    let header = if authors.is_empty() {
        s.name.clone()
    } else {
        format!("{} ({})", s.name, authors.join(", "))
    };
    let several = authors.len() > 1;
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for e in &s.entries {
        rows.push(match e {
            Entry::Missing(n) => (n.to_string(), "— missing —".into(), String::new()),
            Entry::Book { index, book } => {
                let m = &book.sidecar.metadata;
                let mut title = m.title.clone();
                match index {
                    None => title.push_str(" (unnumbered)"),
                    Some(i) if s.duplicates.contains(i) => {
                        let n = s.books().filter(|(j, _)| j == index).count();
                        title.push_str(&format!(" ({n} editions)"));
                    }
                    Some(_) => {}
                }
                if several && let Some(a) = m.authors.first() {
                    title.push_str(&format!(", {a}"));
                }
                (
                    index.map(|i| i.to_string()).unwrap_or_default(),
                    title,
                    kindles(book),
                )
            }
        });
    }
    let width = rows
        .iter()
        .filter(|r| !r.2.is_empty())
        .map(|r| r.1.chars().count())
        .max()
        .unwrap_or(0);
    let lines = rows.iter().map(|(n, title, kindles)| {
        let pad = width.saturating_sub(title.chars().count());
        format!("  {n:<4} {title}{} {kindles}", " ".repeat(pad))
            .trim_end()
            .to_owned()
    });
    std::iter::once(header)
        .chain(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn json_row(s: &Series) -> serde_json::Value {
    let books: Vec<_> = s
        .books()
        .map(|(index, b)| {
            let m = &b.sidecar.metadata;
            serde_json::json!({
                "index": index,
                "id": b.sidecar.id,
                "title": m.title,
                "authors": m.authors,
                "folder": b.dir,
                "sends": b.sidecar.sends,
            })
        })
        .collect();
    serde_json::json!({
        "name": s.name,
        "key": s.key,
        "books": books,
        "missing": s.missing,
        "duplicates": s.duplicates,
        "unnumbered": s.unnumbered(),
    })
}
