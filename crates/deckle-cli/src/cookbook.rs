// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle cookbook`: keep, file and tidy the recipes of the Kitchen Book (cookbook design §8.1).
//! These commands change the folder and nothing else; the book is built from it later.

use std::process::ExitCode;

use deckle_digest::chrono::Utc;
use deckle_digest::cookbook::add::{self, AddOptions};
use deckle_digest::cookbook::store::{Picture, Saved, Store, Stored};
use deckle_digest::cookbook::{Chapter, EVERYTHING_ELSE};
use serde::Serialize;

use super::cli::{CookbookCommand, PictureArg};
use super::emit;

pub fn run(json: bool, action: CookbookCommand) -> Result<ExitCode, String> {
    let store = Store::new(Store::default_dir());
    match action {
        CookbookCommand::Add {
            targets,
            chapter,
            note,
        } => add(&store, &targets, chapter.as_deref(), note.as_deref())?,
        CookbookCommand::List { chapter } => list(json, &store, chapter.as_deref())?,
        CookbookCommand::Move { who, chapter } => {
            let chapters = store.config()?.chapters;
            known_chapter(&chapters, &chapter)?;
            let mut saved = recipe(&store, &who)?;
            saved.chapter = chapter;
            store.update(&saved)?;
            println!("Moved to {}", chapter_name(&chapters, &saved.chapter));
        }
        CookbookCommand::Remove { who } => {
            if !store.remove(&who)? {
                return Err(nothing_saved(&who));
            }
            println!("Removed");
        }
        CookbookCommand::Note { who, text } => {
            let mut saved = recipe(&store, &who)?;
            saved.note = Some(text).filter(|t| !t.is_empty());
            store.update(&saved)?;
            println!(
                "{}",
                if saved.note.is_some() {
                    "Noted"
                } else {
                    "Note cleared"
                }
            );
        }
        CookbookCommand::Picture { who, choice } => {
            let mut saved = recipe(&store, &who)?;
            saved.picture = match choice {
                PictureArg::Site => Picture::Site,
                PictureArg::Illustration => Picture::Illustration,
                PictureArg::None => Picture::None,
                PictureArg::Default => Picture::Default,
            };
            store.update(&saved)?;
        }
        CookbookCommand::Chapters => chapters(json, &store)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn nothing_saved(who: &str) -> String {
    format!("nothing saved as {who}")
}

fn recipe(store: &Store, who: &str) -> Result<Saved, String> {
    store.find(who)?.ok_or_else(|| nothing_saved(who))
}

/// `id` must be one of the book's chapters; the error lists the valid ids.
fn known_chapter(chapters: &[Chapter], id: &str) -> Result<(), String> {
    if chapters.iter().any(|c| c.id == id) {
        return Ok(());
    }
    let ids: Vec<&str> = chapters.iter().map(|c| c.id.as_str()).collect();
    Err(format!(
        "no chapter \"{id}\"; the chapters are: {}",
        ids.join(", ")
    ))
}

/// A chapter's name; a recipe left in a chapter since deleted reads as "Everything else", as
/// it does in the book.
fn chapter_name(chapters: &[Chapter], id: &str) -> String {
    let find = |id: &str| chapters.iter().find(|c| c.id == id).map(|c| c.name.clone());
    find(id)
        .or_else(|| find(EVERYTHING_ELSE))
        .unwrap_or_else(|| "Everything else".to_owned())
}

fn add(
    store: &Store,
    targets: &[String],
    chapter: Option<&str>,
    note: Option<&str>,
) -> Result<(), String> {
    if targets.is_empty() {
        return Err("give a web address, a link file or a saved page to add".into());
    }
    let chapters = store.config()?.chapters;
    if let Some(c) = chapter {
        known_chapter(&chapters, c)?;
    }
    let mut failed = 0;
    for target in targets {
        match add::add(
            store,
            target,
            &deckle_digest::Http,
            &AddOptions { chapter, note },
            Utc::now(),
        ) {
            Ok(outcome) => {
                let (verb, saved) = match &outcome.stored {
                    Stored::Added(s) => ("Added to", s),
                    Stored::Updated(s) => ("Updated ·", s),
                };
                let picture = if outcome.picture_note.is_some() {
                    "no picture"
                } else {
                    "site photo"
                };
                println!(
                    "{verb} {} · {picture}",
                    chapter_name(&chapters, &saved.chapter)
                );
                if let Some(why) = outcome.picture_note {
                    println!("{why}");
                }
                if !saved.recipe.structured {
                    println!("Read from the page text, not recipe data");
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("{target}: {e}");
            }
        }
    }
    match failed {
        0 => Ok(()),
        n => Err(format!("{n} of {} couldn't be added", targets.len())),
    }
}

/// A recipe as `list --json` gives it.
#[derive(Serialize)]
struct Row<'a> {
    id: &'a str,
    title: &'a str,
    chapter: &'a str,
    added: String,
    structured: bool,
    picture: Picture,
    note: Option<&'a str>,
    url: &'a str,
}

fn list(json: bool, store: &Store, chapter: Option<&str>) -> Result<(), String> {
    let chapters = store.config()?.chapters;
    if let Some(c) = chapter {
        known_chapter(&chapters, c)?;
    }
    let recipes: Vec<Saved> = store
        .all()?
        .into_iter()
        .filter(|s| chapter.is_none_or(|c| s.chapter == c))
        .collect();
    let rows: Vec<Row> = recipes
        .iter()
        .map(|s| Row {
            id: &s.id,
            title: title(s),
            chapter: &s.chapter,
            added: s.added.to_rfc3339(),
            structured: s.recipe.structured,
            picture: s.picture,
            note: s.note.as_deref(),
            url: &s.recipe.source_url,
        })
        .collect();
    emit(json, &rows, || {
        if recipes.is_empty() {
            return "No recipes yet".to_owned();
        }
        recipes
            .iter()
            .map(|s| {
                format!(
                    "{}  {}  {}  {}  {}  picture: {}",
                    s.id,
                    title(s),
                    chapter_name(&chapters, &s.chapter),
                    s.added.format("%Y-%m-%d"),
                    if s.recipe.structured {
                        "recipe data"
                    } else {
                        "page text"
                    },
                    picture_word(s.picture),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// The title the book prints: the owner's, else the site's.
fn title(s: &Saved) -> &str {
    s.title_override.as_deref().unwrap_or(&s.recipe.title)
}

fn picture_word(p: Picture) -> &'static str {
    match p {
        Picture::Site => "site",
        Picture::Illustration => "illustration",
        Picture::None => "none",
        Picture::Default => "default",
    }
}

/// A chapter as `chapters --json` gives it.
#[derive(Serialize)]
struct ChapterRow<'a> {
    id: &'a str,
    name: &'a str,
    #[serde(rename = "match")]
    match_: &'a [String],
    recipes: usize,
}

fn chapters(json: bool, store: &Store) -> Result<(), String> {
    let chapters = store.config()?.chapters;
    let recipes = store.all()?;
    // A recipe left in a deleted chapter is counted where the book prints it.
    let count = |id: &str| {
        recipes
            .iter()
            .filter(|s| {
                let home = if chapters.iter().any(|c| c.id == s.chapter) {
                    s.chapter.as_str()
                } else {
                    EVERYTHING_ELSE
                };
                home == id
            })
            .count()
    };
    let rows: Vec<ChapterRow> = chapters
        .iter()
        .map(|c| ChapterRow {
            id: &c.id,
            name: &c.name,
            match_: &c.match_,
            recipes: count(&c.id),
        })
        .collect();
    emit(json, &rows, || {
        rows.iter()
            .map(|r| {
                format!(
                    "{}  {}  {} {}",
                    r.id,
                    r.name,
                    r.recipes,
                    if r.recipes == 1 { "recipe" } else { "recipes" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}
