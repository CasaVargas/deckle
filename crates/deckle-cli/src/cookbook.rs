// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle cookbook`: keep, file and tidy the recipes of the Kitchen Book (cookbook design §8.1).
//! These commands change the folder and nothing else; the book is built from it later.

use std::path::Path;
use std::process::ExitCode;

use deckle_digest::chrono::Utc;
use deckle_digest::cookbook::add::{self, AddOptions};
use deckle_digest::cookbook::book;
use deckle_digest::cookbook::illustrate::{self, Post};
use deckle_digest::cookbook::store::{
    IllustrationsConfig, ImageKind, Picture, Saved, Store, Stored,
};
use deckle_digest::cookbook::{Chapter, EVERYTHING_ELSE};
use serde::Serialize;

use super::cli::{CookbookCommand, PictureArg};
use super::cookbook_delivery::{self, KindleState, KindleStatus};
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
            if !store.remove(&key(&who))? {
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
            if saved.picture == Picture::Illustration {
                draw_on_request(&store, &saved)?;
            }
        }
        CookbookCommand::Chapters => chapters(json, &store)?,
        CookbookCommand::Illustrate { who, all, redraw } => {
            illustrate_cmd(&store, who.as_deref(), all, redraw)?
        }
        CookbookCommand::Build { out } => build(&store, &out)?,
        CookbookCommand::Send { to } => send(json, &store, to.as_deref())?,
    }
    Ok(ExitCode::SUCCESS)
}

fn nothing_saved(who: &str) -> String {
    format!("nothing saved as {who}")
}

/// What `who` names to the store: a saved page given as a path is keyed by its canonical
/// path, as `add` keyed it; an id or a web address is as given.
fn key(who: &str) -> String {
    let path = std::path::Path::new(who);
    let is_page = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"));
    if is_page
        && path.is_file()
        && let Ok(full) = std::fs::canonicalize(path)
    {
        return full.to_string_lossy().into_owned();
    }
    who.to_owned()
}

fn recipe(store: &Store, who: &str) -> Result<Saved, String> {
    store.find(&key(who))?.ok_or_else(|| nothing_saved(who))
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

/// The chapter a recipe is printed in: its own, or "Everything else" when that chapter has
/// since been deleted. `list --chapter` and `chapters` both go by this.
fn home<'a>(chapters: &[Chapter], id: &'a str) -> &'a str {
    if chapters.iter().any(|c| c.id == id) {
        id
    } else {
        EVERYTHING_ELSE
    }
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
                // Only a recipe with no picture at all says so; one that kept its earlier
                // picture still has a site photo.
                let picture = if outcome.picture_note == Some(add::NO_PICTURE) {
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
        .filter(|s| chapter.is_none_or(|c| home(&chapters, &s.chapter) == c))
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
    // Where each Kindle stands and what's not built yet (cookbook design §9). A record that
    // can't be read is said, never shown as "Never sent".
    let (kindles, kindles_error) =
        match sync_config().and_then(|sync| cookbook_delivery::statuses(store, &sync)) {
            Ok(k) => (k, None),
            Err(e) => (Vec::new(), Some(e)),
        };
    let (pending_build, build_error) = cookbook_delivery::unbuilt(store);
    #[derive(Serialize)]
    struct Listing<'a> {
        recipes: &'a [Row<'a>],
        kindles: &'a [KindleStatus],
        #[serde(skip_serializing_if = "Option::is_none")]
        kindles_error: Option<&'a str>,
        pending_build: bool,
        build_error: Option<&'a str>,
    }
    emit(
        json,
        &Listing {
            recipes: &rows,
            kindles: &kindles,
            kindles_error: kindles_error.as_deref(),
            pending_build,
            build_error: build_error.as_deref(),
        },
        || {
            let mut lines: Vec<String> = if recipes.is_empty() {
                vec!["No recipes yet".to_owned()]
            } else {
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
                    .collect()
            };
            let mut footer = Vec::new();
            if pending_build {
                footer.push(format!(
                    "Changes not built yet: {}",
                    build_error
                        .as_deref()
                        .unwrap_or("waiting for the next build")
                ));
            }
            if let Some(e) = &kindles_error {
                footer.push(format!("Kindles: can't tell where each stands: {e}"));
            }
            footer.extend(kindles.iter().map(|k| format!("{}: {}", k.name, k.status)));
            if !footer.is_empty() {
                lines.push(String::new());
                lines.extend(footer);
            }
            lines.join("\n")
        },
    )
}

/// `digest.toml`'s `[sync]`: the household's settings and the Kindles it names. No file is no
/// settings; one that won't load is an error.
fn sync_config() -> Result<deckle_digest::config::SyncConfig, String> {
    let path = deckle_digest::config::default_path();
    if !path.exists() {
        return Ok(Default::default());
    }
    Ok(deckle_digest::Config::load(&path)?.sync)
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
    let count = |id: &str| {
        recipes
            .iter()
            .filter(|s| home(&chapters, &s.chapter) == id)
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

/// The book's file name inside the folder `build` writes to.
const EPUB_NAME: &str = "cookbook.epub";

/// Writes the book to `<out>/cookbook.epub`. An export: it leaves the dirty flag alone, which
/// only a delivery clears.
fn build(store: &Store, out: &Path) -> Result<(), String> {
    let built = book::build(store, &out.join(EPUB_NAME))?;
    println!(
        "{} {} in {} {}",
        built.recipes,
        if built.recipes == 1 {
            "recipe"
        } else {
            "recipes"
        },
        built.chapters,
        if built.chapters == 1 {
            "chapter"
        } else {
            "chapters"
        },
    );
    println!("{}", built.path.display());
    println!("build {}", built.build_id);
    Ok(())
}

/// What `send` did, as `--json` gives it.
#[derive(Serialize)]
struct Sent<'a> {
    build: &'a str,
    /// "sent", "up_to_date", "off", "all_current", "queued" or "exported".
    result: &'static str,
    /// The Kindle's name (or sync id).
    #[serde(skip_serializing_if = "Option::is_none")]
    kindle: Option<String>,
    /// The EPUB, for an export.
    #[serde(skip_serializing_if = "Option::is_none")]
    exported: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<&'a super::DigestOutcome>,
}

/// Builds the book when it changed (a book that hasn't is sent as it is), then sends it: to a
/// connected Kindle through the same offer, push and record `digest push` uses, or as an EPUB
/// into the folder `--to` names, which no Kindle's record hears of. With no Kindle connected it
/// waits for each one's next plug-in. It says "up to date" only when the record agrees.
fn send(json: bool, store: &Store, to: Option<&str>) -> Result<(), String> {
    // A book that was never built is due, so this builds it; it waits if the watcher is
    // building right now.
    if let Some(b) = cookbook_delivery::rebuild_waiting(store, None)? {
        eprintln!(
            "built {} {}",
            b.recipes,
            if b.recipes == 1 { "recipe" } else { "recipes" }
        );
    }
    let build = cookbook_delivery::current_build(store)
        .ok_or("no recipes yet; add one with `deckle cookbook add`")?;
    let reply = |s: Sent, text: String| emit(json, &s, || text);
    let sent = |result, kindle, exported, outcome| Sent {
        build: &build,
        result,
        kindle,
        exported,
        outcome,
    };
    if let Some(folder) = to.filter(|t| *t != "device") {
        let dir = Path::new(folder);
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let out = dir.join(EPUB_NAME);
        std::fs::copy(cookbook_delivery::epub_path(store), &out)
            .map_err(|e| format!("{}: {e}", out.display()))?;
        return reply(
            sent("exported", None, Some(&out), None),
            format!("Exported to {}", out.display()),
        );
    }
    let path = deckle_digest::config::default_path();
    let Some(target) = super::detect_kindle() else {
        // Nobody to hand it to: say whether anybody is waiting for it.
        let current = sync_config()
            .and_then(|sync| cookbook_delivery::statuses(store, &sync))
            .is_ok_and(|all| {
                all.iter().any(|k| k.state == KindleState::UpToDate)
                    && all
                        .iter()
                        .all(|k| matches!(k.state, KindleState::UpToDate | KindleState::Off))
            });
        return if current {
            reply(
                sent("all_current", None, None, None),
                "Every Kindle already has this build".to_owned(),
            )
        } else {
            reply(
                sent("queued", None, None, None),
                "Queued: goes to each Kindle on its next plug-in".to_owned(),
            )
        };
    };
    let id = deckle_core::sync::device_id(&target);
    let papers = if path.exists() {
        super::all_papers(&path)?
    } else {
        vec![deckle_digest::Config::default()]
    };
    let cfg = &papers[0];
    let name = cfg.sync.device_name(&id).unwrap_or_else(|| id.clone());
    let on = cfg.sync.for_device(&id).cookbook;
    let queue = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
    let outcome = super::push_queued(&path, &queue, &papers, Some(target))?;
    let flush = outcome.flush.as_ref();
    let took = flush.is_some_and(|f| {
        f.delivered
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == book::DEVICE_NAME))
    });
    // Other books that went along (the Daily, say) are listed too.
    let others = flush.is_some_and(|f| !f.delivered.is_empty());
    let (result, mut text) = if took {
        ("sent", format!("Sent to {name}"))
    } else if !on {
        (
            "off",
            format!("{name} is left out of the cookbook (cookbook = false in digest.toml)"),
        )
    } else {
        // Nothing went. "Up to date" only if the record says it has this build; else why not.
        match cookbook_delivery::Delivered::load(store) {
            Ok(d) if d.get(&id) == Some(build.as_str()) => {
                ("up_to_date", format!("{name} is up to date"))
            }
            other => {
                let why = outcome
                    .cookbook_error
                    .clone()
                    .or(other.err())
                    .unwrap_or_else(|| "the Kindle didn't take the book".to_owned());
                return Err(format!("not sent to {name}: {why}"));
            }
        }
    };
    if others {
        text = format!("{text}\n{}", outcome.describe(&queue));
    }
    // It went, but couldn't be written down: the next push would send it again.
    if let (true, Some(e)) = (took, &outcome.cookbook_error) {
        text = format!("{text}\nwarning: cookbook: {e}");
    }
    reply(sent(result, Some(name), None, Some(&outcome)), text)
}

/// A 256x256 gray PNG, the stand-in image endpoint's answer.
const FAKE_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAQAAAAEACAAAAAB5Gfe6AAABOElEQVR42u3QgQAAAAwEofcnO6yBrBBaz02AAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAgQIECAAAECBAioAzVNC7mBbeXUAAAAAElFTkSuQmCC";

/// Where a drawing is asked for: the real endpoint, or (`DECKLE_COOKBOOK_FAKE_IMAGES=1`, a test
/// hook like `DECKLE_REMARKABLE_URL`) a stand-in that answers [`FAKE_PNG`] and sends nothing.
struct Artist {
    cfg: IllustrationsConfig,
    key: String,
    fake: bool,
}

impl Artist {
    /// The book's illustrations settings and a key, or why there can be no drawing.
    fn new(store: &Store) -> Result<Self, String> {
        let cfg = store.config()?.illustrations;
        if !cfg.enabled {
            return Err("Turn on illustrations in cookbook.toml first".into());
        }
        let fake = std::env::var_os("DECKLE_COOKBOOK_FAKE_IMAGES").is_some_and(|v| v == "1");
        let key = match fake {
            true => "fake".to_owned(),
            false => illustrate::api_key().ok_or("Set OPENAI_API_KEY to draw illustrations")?,
        };
        Ok(Artist { cfg, key, fake })
    }

    /// Draws one recipe. `Err` is the one generic line; nothing the endpoint said gets out.
    fn draw(&self, store: &Store, saved: &Saved) -> Result<(), &'static str> {
        let fake = |_: &str, _: &[(String, String)], _: &str| -> Result<String, String> {
            Ok(format!(r#"{{"data":[{{"b64_json":"{FAKE_PNG}"}}]}}"#))
        };
        let real: Post = &deckle_xray::llm::http_post;
        let post: Post = if self.fake { &fake } else { real };
        illustrate::illustrate(store, saved, &self.cfg, Some(&self.key), post)
    }
}

fn has_illustration(store: &Store, saved: &Saved) -> bool {
    store.image_path(&saved.id, ImageKind::Ill).is_file()
}

/// `picture <who> illustration`: draws it when there is none yet, if the book allows. The choice
/// is already saved; with illustrations off or no drawing made, the site photo prints meanwhile.
fn draw_on_request(store: &Store, saved: &Saved) -> Result<(), String> {
    if has_illustration(store, saved) {
        return Ok(());
    }
    if !store.config()?.illustrations.enabled {
        println!("Illustrations are off; the photo prints until you turn them on in cookbook.toml");
        return Ok(());
    }
    let artist = Artist::new(store)?;
    artist.draw(store, saved).map_err(str::to_owned)?;
    println!("{}: drawn", title(saved));
    Ok(())
}

/// `illustrate`: one recipe, or `--all` one at a time, skipping those with a drawing unless
/// `--redraw`. Progress is `<n>/<total> <title>: drawn | kept the photo`.
fn illustrate_cmd(store: &Store, who: Option<&str>, all: bool, redraw: bool) -> Result<(), String> {
    let artist = Artist::new(store)?;
    let wanted = match who {
        Some(who) => vec![recipe(store, who)?],
        None if all => store.all()?,
        None => return Err("name a recipe, or use --all".into()),
    };
    let (todo, had): (Vec<Saved>, Vec<Saved>) = wanted
        .into_iter()
        .partition(|s| redraw || !has_illustration(store, s));
    if who.is_some() && todo.is_empty() {
        println!("Already has an illustration; --redraw draws it again");
        return Ok(());
    }
    if todo.is_empty() {
        println!("Nothing to draw");
        return Ok(());
    }
    let mut kept = 0;
    for (i, saved) in todo.iter().enumerate() {
        use std::io::Write as _;
        print!("{}/{} {}: ", i + 1, todo.len(), title(saved));
        let _ = std::io::stdout().flush();
        match artist.draw(store, saved) {
            Ok(()) => println!("drawn"),
            Err(_) => {
                println!("kept the photo");
                kept += 1;
            }
        }
    }
    if !had.is_empty() {
        println!(
            "{} already {} one (--redraw draws again)",
            had.len(),
            if had.len() == 1 { "has" } else { "have" }
        );
    }
    match (kept, who) {
        (0, _) => Ok(()),
        (_, Some(_)) => Err(illustrate::FAILED.into()),
        (n, None) => Err(format!("{n} of {} couldn't be drawn", todo.len())),
    }
}
