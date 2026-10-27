// SPDX-License-Identifier: GPL-3.0-or-later
//! The Cookbook tab's commands (cookbook design §8.2): add a recipe from a link, keep and file
//! the book, where it goes (the Kindles), and the optional engravings. The book is the plain
//! folder `deckle_digest::cookbook::store::Store` keeps, so everything here is the CLI's
//! `deckle cookbook …` with a window on it. Each command is a thin wrapper over a function
//! that takes the store (and paths) as arguments, so the tests never touch the real folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use base64::Engine;
use deckle_cli::cookbook_api::{self, KindleStatus};
use deckle_digest::chrono::{DateTime, Utc};
use deckle_digest::cookbook::add::{self, AddOptions};
use deckle_digest::cookbook::illustrate::{self, Post};
use deckle_digest::cookbook::store::{
    BookConfig, IllustrationsConfig, ImageKind, Picture, Saved, Store, Stored,
};
use deckle_digest::cookbook::{Chapter, EVERYTHING_ELSE, book};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::{AppState, Settings, load_settings_from};

/// The folder the commands work on: `DECKLE_COOKBOOK_DIR`, else beside `digest.toml`.
fn store() -> Store {
    Store::new(Store::default_dir())
}

fn digest_path() -> PathBuf {
    deckle_digest::config::default_path()
}

// ---------------------------------------------------------------------------------------------
// What the tab is sent
// ---------------------------------------------------------------------------------------------

/// The book's settings (`cookbook.toml`) as the tab edits them. `endpoint` and `model` are
/// always present here; empty means the provider's default (and keeps them out of the file).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConfigDto {
    pub title: String,
    pub subtitle: String,
    /// The book's default picture: `site` | `illustration` | `none`.
    pub picture: Picture,
    /// In book order. Each chapter's `match` keywords are edited in `cookbook.toml` only.
    pub chapters: Vec<Chapter>,
    pub illustrations: IllustrationsDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IllustrationsDto {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
}

impl From<BookConfig> for ConfigDto {
    fn from(c: BookConfig) -> Self {
        ConfigDto {
            title: c.title,
            subtitle: c.subtitle,
            picture: c.picture,
            chapters: c.chapters,
            illustrations: IllustrationsDto {
                enabled: c.illustrations.enabled,
                endpoint: c.illustrations.endpoint,
                model: c.illustrations.model,
            },
        }
    }
}

impl From<ConfigDto> for BookConfig {
    fn from(c: ConfigDto) -> Self {
        BookConfig {
            title: c.title,
            subtitle: c.subtitle,
            picture: c.picture,
            chapters: c.chapters,
            illustrations: IllustrationsConfig {
                enabled: c.illustrations.enabled,
                endpoint: c.illustrations.endpoint,
                model: c.illustrations.model,
            },
        }
    }
}

/// A fetched recipe, before anything is written (the Add card).
#[derive(Clone, Debug, Serialize)]
pub struct PreviewDto {
    pub title: String,
    pub site_name: String,
    pub source_url: String,
    pub description: Option<String>,
    #[serde(rename = "yield")]
    pub yield_: Option<String>,
    /// Minutes.
    pub prep: Option<u32>,
    pub cook: Option<u32>,
    pub total: Option<u32>,
    /// Ingredient lines, over every group.
    pub ingredients: usize,
    /// Steps, over every group.
    pub steps: usize,
    /// `false`: read from the page text, not recipe data.
    pub structured: bool,
    /// The chapter id it would be filed in (its own when it is already in the book).
    pub chapter: String,
    pub chapter_name: String,
    /// Already in the book: adding says "Updated".
    pub exists: bool,
    /// `data:image/jpeg;base64,…`: the picture as it would print. None when the page has none.
    pub picture: Option<String>,
}

/// One recipe of the book.
#[derive(Clone, Debug, Serialize)]
pub struct RecipeDto {
    pub id: String,
    /// What the book prints: the owner's title, else the site's.
    pub title: String,
    pub site_title: String,
    /// The owner's title; None while the site's is used.
    pub title_override: Option<String>,
    /// The recipe's own chapter id (may name a chapter since deleted).
    pub chapter: String,
    /// The chapter it prints in: `chapter`, or `other` when that one is gone.
    pub home: String,
    pub home_name: String,
    /// RFC 3339.
    pub added: String,
    /// Carries the contents' star (added within 7 days of the newest recipe).
    pub is_new: bool,
    pub structured: bool,
    pub site_name: String,
    pub url: String,
    pub note: Option<String>,
    /// The owner's choice: `site` | `illustration` | `none` | `default` (follow the book).
    pub picture: Picture,
    /// What actually prints: `site` | `illustration` | `none`. An illustration not drawn yet
    /// prints the site's photo.
    pub printed: Picture,
    pub has_illustration: bool,
    /// `data:image/jpeg;base64,…`, small: the picture that prints.
    pub thumb: Option<String>,
    /// Every ingredient line, for searching.
    pub ingredients: Vec<String>,
}

/// A chapter with its recipes, alphabetically.
#[derive(Clone, Debug, Serialize)]
pub struct ChapterDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "match")]
    pub match_: Vec<String>,
    /// Empty: the chapter is left out of the book ("Nothing yet").
    pub recipes: Vec<RecipeDto>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NewestDto {
    pub id: String,
    pub title: String,
    pub added: String,
}

/// The whole book, for the Recipes list and the book card.
#[derive(Clone, Debug, Serialize)]
pub struct BookDto {
    pub config: ConfigDto,
    /// In book order; "Everything else" is last if the file lost it.
    pub chapters: Vec<ChapterDto>,
    pub recipes: usize,
    /// Chapters holding at least one recipe: the "m chapters" of the book card.
    pub printed_chapters: usize,
    pub newest: Option<NewestDto>,
    /// `data:image/jpeg;base64,…`: the cover as it will print, small.
    pub cover: Option<String>,
    /// Changes no build has picked up.
    pub pending_build: bool,
    pub build_error: Option<String>,
    /// The path of `cookbook.toml` ("Show the file").
    pub config_path: String,
}

/// A recipe just added or changed.
#[derive(Clone, Debug, Serialize)]
pub struct SavedDto {
    /// `added` | `updated`.
    pub stored: &'static str,
    /// Why there is no printed picture, or that the earlier one stayed.
    pub picture_note: Option<String>,
    #[serde(flatten)]
    pub recipe: RecipeDto,
}

/// What `cookbook_update` changes; what is left out stays. An empty `title_override` or `note`
/// clears it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PatchDto {
    pub title_override: Option<String>,
    pub chapter: Option<String>,
    pub picture: Option<Picture>,
    pub note: Option<String>,
}

/// What Send now did.
#[derive(Clone, Debug, Serialize)]
pub struct SendDto {
    pub build: String,
    /// `sent` | `up_to_date` | `off` | `all_current` | `queued`.
    pub result: String,
    /// The Kindle's name, when one took part.
    pub kindle: Option<String>,
    /// The line `deckle cookbook send` prints.
    pub message: String,
    /// Recipes in the book, when this call built it first.
    pub built: Option<usize>,
}

/// The Delivery tab.
#[derive(Clone, Debug, Serialize)]
pub struct DeliveryDto {
    pub kindles: Vec<KindleStatus>,
    pub watcher_installed: bool,
    /// Why where each Kindle stands can't be told (`kindles` is empty then).
    pub error: Option<String>,
}

/// Progress of `cookbook_illustrate`, emitted as `cookbook-illustrate`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IllustrateEvent {
    /// How many are done (0 on `start`).
    pub n: usize,
    pub total: usize,
    pub id: String,
    pub title: String,
    /// `start` | `drawn` | `kept_photo`.
    pub result: &'static str,
}

/// What `cookbook_illustrate` did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IllustrateDto {
    pub total: usize,
    pub drawn: usize,
    /// Couldn't be drawn; the photo stays.
    pub kept: usize,
    /// Skipped for having a drawing already.
    pub already: usize,
    /// Stop was pressed before the end.
    pub stopped: bool,
}

// ---------------------------------------------------------------------------------------------
// The functions behind the commands
// ---------------------------------------------------------------------------------------------

fn nothing_saved(who: &str) -> String {
    format!("nothing saved as {who}")
}

/// `id` must be one of the book's chapters; the error lists the valid ids (as the CLI's does).
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
/// since been deleted.
fn home<'a>(chapters: &[Chapter], id: &'a str) -> &'a str {
    if chapters.iter().any(|c| c.id == id) {
        id
    } else {
        EVERYTHING_ELSE
    }
}

/// The chapters in book order, with the catch-all last if the file lost it (as the build does).
fn book_chapters(cfg: &BookConfig) -> Vec<Chapter> {
    let mut chapters = cfg.chapters.clone();
    if !chapters.iter().any(|c| c.id == EVERYTHING_ELSE) {
        chapters.push(Chapter {
            id: EVERYTHING_ELSE.to_owned(),
            name: "Everything else".to_owned(),
            match_: Vec::new(),
        });
    }
    chapters
}

fn data_url(jpeg: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    )
}

/// `jpeg` (or any image) scaled to fit `w` x `h`, as a JPEG data URL.
fn thumbnail(bytes: &[u8], w: u32, h: u32) -> Option<String> {
    let small = image::load_from_memory(bytes).ok()?.thumbnail(w, h);
    let mut out = Vec::new();
    small
        .to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut out, 75,
        ))
        .ok()?;
    Some(data_url(&out))
}

type ThumbKey = (PathBuf, std::time::SystemTime, u64);

/// Recipe thumbnails already made, by file and when it last changed: the Recipes list asks for
/// every recipe's each time the tab opens.
static THUMBS: std::sync::LazyLock<Mutex<std::collections::HashMap<ThumbKey, Option<String>>>> =
    std::sync::LazyLock::new(Default::default);

/// How many thumbnails are kept before the cache starts over.
const THUMBS_KEPT: usize = 4000;

fn picture_thumb(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let key = (path.to_path_buf(), meta.modified().ok()?, meta.len());
    let mut cache = THUMBS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(hit) = cache.get(&key) {
        return hit.clone();
    }
    let made = std::fs::read(path)
        .ok()
        .and_then(|bytes| thumbnail(&bytes, 144, 96));
    if cache.len() >= THUMBS_KEPT {
        cache.clear();
    }
    cache.insert(key, made.clone());
    made
}

/// The last cover drawn, by what it was drawn from: rendering one is the slow part of opening
/// the tab, and it only changes when the title, subtitle, count or newest recipe does.
static COVER: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);

fn cover_thumb(title: &str, subtitle: &str, count: usize, last: Option<&str>) -> Option<String> {
    let key = format!(
        "{title}\u{1}{subtitle}\u{1}{count}\u{1}{}",
        last.unwrap_or("")
    );
    let mut cached = COVER.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((k, made)) = cached.as_ref()
        && *k == key
    {
        return made.clone();
    }
    let made = deckle_digest::cover::render_cookbook(title, subtitle, count, last)
        .ok()
        .and_then(|jpeg| thumbnail(&jpeg, 200, 320));
    *cached = Some((key, made.clone()));
    made
}

fn ingredient_lines(saved: &Saved) -> Vec<String> {
    saved
        .recipe
        .ingredients
        .iter()
        .flat_map(|g| g.items.iter().cloned())
        .collect()
}

fn recipe_dto(
    store: &Store,
    saved: &Saved,
    cfg: &BookConfig,
    chapters: &[Chapter],
    newest: Option<DateTime<Utc>>,
) -> RecipeDto {
    let printed = book::printed_picture(store, saved, cfg);
    let thumb = match printed {
        Picture::Illustration => picture_thumb(&store.image_path(&saved.id, ImageKind::Ill)),
        Picture::Site => picture_thumb(&store.image_path(&saved.id, ImageKind::Printed)),
        Picture::None | Picture::Default => None,
    };
    let home_id = home(chapters, &saved.chapter);
    RecipeDto {
        id: saved.id.clone(),
        title: book::shown_title(saved),
        site_title: saved.recipe.title.clone(),
        title_override: saved
            .title_override
            .clone()
            .filter(|t| !t.trim().is_empty()),
        chapter: saved.chapter.clone(),
        home: home_id.to_owned(),
        home_name: chapters
            .iter()
            .find(|c| c.id == home_id)
            .map(|c| c.name.clone())
            .unwrap_or_default(),
        added: saved.added.to_rfc3339(),
        is_new: newest.is_some_and(|n| book::is_latest_addition(saved.added, n)),
        structured: saved.recipe.structured,
        site_name: saved.recipe.site_name.clone(),
        url: saved.recipe.source_url.clone(),
        note: saved.note.clone(),
        picture: saved.picture,
        printed,
        has_illustration: store.image_path(&saved.id, ImageKind::Ill).is_file(),
        thumb,
        ingredients: ingredient_lines(saved),
    }
}

fn counted(groups: &[deckle_digest::cookbook::Group]) -> usize {
    groups.iter().map(|g| g.items.len()).sum()
}

fn preview_dto(
    store: &Store,
    net: &dyn deckle_digest::Net,
    target: &str,
) -> Result<PreviewDto, String> {
    let p = add::preview(target, net, store)?;
    let chapters = book_chapters(&store.config()?);
    let chapter_name = chapters
        .iter()
        .find(|c| c.id == home(&chapters, &p.chapter))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let r = p.recipe;
    Ok(PreviewDto {
        ingredients: counted(&r.ingredients),
        steps: counted(&r.steps),
        title: r.title,
        site_name: r.site_name,
        source_url: r.source_url,
        description: r.description,
        yield_: r.yield_,
        prep: r.prep,
        cook: r.cook,
        total: r.total,
        structured: r.structured,
        chapter: p.chapter,
        chapter_name,
        exists: p.exists,
        picture: p.picture_jpeg.as_deref().map(data_url),
    })
}

/// A recipe as the tab shows it right after a change.
fn saved_dto(
    store: &Store,
    saved: &Saved,
    stored: &'static str,
    picture_note: Option<&str>,
) -> Result<SavedDto, String> {
    let cfg = store.config()?;
    let chapters = book_chapters(&cfg);
    let newest = store.all()?.iter().map(|s| s.added).max();
    Ok(SavedDto {
        stored,
        picture_note: picture_note.map(str::to_owned),
        recipe: recipe_dto(store, saved, &cfg, &chapters, newest),
    })
}

fn add_dto(
    store: &Store,
    net: &dyn deckle_digest::Net,
    target: &str,
    chapter: Option<&str>,
    note: Option<&str>,
    picture: Option<Picture>,
    now: DateTime<Utc>,
) -> Result<SavedDto, String> {
    let chapter = chapter.filter(|c| !c.is_empty());
    if let Some(c) = chapter {
        known_chapter(&store.config()?.chapters, c)?;
    }
    let outcome = add::add(store, target, net, &AddOptions { chapter, note }, now)?;
    let (verb, mut saved) = match outcome.stored {
        Stored::Added(s) => ("added", s),
        Stored::Updated(s) => ("updated", s),
    };
    if let Some(p) = picture
        && p != saved.picture
    {
        saved.picture = p;
        store.update(&saved)?;
    }
    saved_dto(store, &saved, verb, outcome.picture_note)
}

fn book_dto(store: &Store) -> Result<BookDto, String> {
    let cfg = store.config()?;
    let all = store.all()?;
    let chapters = book_chapters(&cfg);
    let newest_at = all.iter().map(|s| s.added).max();
    let mut slot = std::collections::HashMap::new();
    for (i, c) in chapters.iter().enumerate() {
        slot.entry(c.id.as_str()).or_insert(i);
    }
    let mut bins: Vec<Vec<RecipeDto>> = chapters.iter().map(|_| Vec::new()).collect();
    for saved in &all {
        let at = slot[home(&chapters, &saved.chapter)];
        bins[at].push(recipe_dto(store, saved, &cfg, &chapters, newest_at));
    }
    let mut printed_chapters = 0;
    let chapter_dtos: Vec<ChapterDto> = chapters
        .iter()
        .zip(bins)
        .map(|(c, mut recipes)| {
            recipes.sort_by_cached_key(|r| (r.title.to_lowercase(), r.id.clone()));
            printed_chapters += usize::from(!recipes.is_empty());
            ChapterDto {
                id: c.id.clone(),
                name: c.name.clone(),
                match_: c.match_.clone(),
                recipes,
            }
        })
        .collect();
    // The newest recipe is the last one `all` returns, as the cover names it.
    let last = all.last();
    let last_title = last.map(book::shown_title);
    let title = match cfg.title.trim() {
        "" => BookConfig::default().title,
        t => t.to_owned(),
    };
    let (pending_build, build_error) = cookbook_api::unbuilt(store);
    Ok(BookDto {
        cover: cover_thumb(&title, &cfg.subtitle, all.len(), last_title.as_deref()),
        recipes: all.len(),
        printed_chapters,
        newest: last.zip(last_title).map(|(s, title)| NewestDto {
            id: s.id.clone(),
            title,
            added: s.added.to_rfc3339(),
        }),
        pending_build,
        build_error,
        config_path: store
            .dir
            .join("cookbook.toml")
            .to_string_lossy()
            .into_owned(),
        config: cfg.into(),
        chapters: chapter_dtos,
    })
}

fn update_dto(store: &Store, id: &str, patch: PatchDto) -> Result<SavedDto, String> {
    let mut saved = store.find(id)?.ok_or_else(|| nothing_saved(id))?;
    if let Some(c) = patch.chapter.as_deref() {
        known_chapter(&store.config()?.chapters, c)?;
        saved.chapter = c.to_owned();
    }
    if let Some(t) = patch.title_override {
        saved.title_override = Some(t.trim().to_owned()).filter(|t| !t.is_empty());
    }
    if let Some(p) = patch.picture {
        saved.picture = p;
    }
    if let Some(n) = patch.note {
        saved.note = Some(n).filter(|n| !n.is_empty());
    }
    store.update(&saved)?;
    // Re-read: the store caps the note.
    let saved = store.find(&saved.id)?.ok_or_else(|| nothing_saved(id))?;
    saved_dto(store, &saved, "updated", None)
}

fn remove_recipe(store: &Store, id: &str) -> Result<(), String> {
    match store.remove(id)? {
        true => Ok(()),
        false => Err(nothing_saved(id)),
    }
}

fn save_config_dto(store: &Store, cfg: ConfigDto) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for c in &cfg.chapters {
        let id = c.id.trim();
        if id.is_empty() || id != c.id {
            return Err("a chapter needs an id with no spaces around it".into());
        }
        if !seen.insert(id) {
            return Err(format!(
                "there are two chapters with the id \"{id}\"; each needs its own"
            ));
        }
    }
    if !seen.contains(EVERYTHING_ELSE) {
        return Err("\"Everything else\" can't be removed".into());
    }
    store.save_config(&cfg.into())
}

fn send_dto(store: &Store) -> Result<SendDto, String> {
    let s = cookbook_api::send_now(store)?;
    Ok(SendDto {
        build: s.build,
        result: s.result.to_owned(),
        kindle: s.kindle,
        message: s.message,
        built: s.built,
    })
}

fn preview_book_at(store: &Store, out_dir: &Path) -> Result<String, String> {
    let built = book::build(store, &out_dir.join("cookbook.epub"))?;
    Ok(built.path.to_string_lossy().into_owned())
}

fn delivery_at(store: &Store, digest: &Path, watcher_installed: bool) -> DeliveryDto {
    let (kindles, error) = match cookbook_api::sync_config_at(digest)
        .and_then(|sync| cookbook_api::statuses(store, &sync))
    {
        Ok(k) => (k, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    DeliveryDto {
        kindles,
        watcher_installed,
        error,
    }
}

/// Writes `[sync.devices."<id>"] cookbook = on` into `digest.toml` (a missing file is started),
/// keeping its comments.
fn set_kindle_at(digest: &Path, id: &str, on: bool) -> Result<(), String> {
    let mut cfg = match deckle_digest::Config::load(digest) {
        Ok(c) => c,
        Err(_) if !digest.exists() => deckle_digest::Config::default(),
        Err(e) => return Err(e),
    };
    cfg.sync.devices.entry(id.to_owned()).or_default().cookbook = Some(on);
    deckle_digest::config::save(digest, &cfg)
}

/// The key to draw with: the image key typed in Book settings (`illustrations_api_key`), else
/// `OPENAI_API_KEY`. X-Ray's `llm_api_key` is never used: it may belong to another provider.
fn image_key(s: &Settings, env: Option<String>) -> Option<String> {
    let typed = s.illustrations_api_key.trim();
    match typed.is_empty() {
        false => Some(typed.to_owned()),
        true => env.filter(|k| !k.trim().is_empty()),
    }
}

fn has_illustration(store: &Store, saved: &Saved) -> bool {
    store.image_path(&saved.id, ImageKind::Ill).is_file()
}

/// Draws one recipe (`id`), or every one (`all`) that has no drawing yet unless `redraw`, one at
/// a time. `stop` is looked at before each; `emit` hears of each as it finishes. A recipe that
/// can't be drawn keeps its photo; asked for by name, that is an error (the one generic line).
#[allow(clippy::too_many_arguments)]
fn illustrate_run(
    store: &Store,
    id: Option<&str>,
    all: bool,
    redraw: bool,
    key: Option<&str>,
    post: Post,
    stop: &AtomicBool,
    emit: &mut dyn FnMut(IllustrateEvent),
) -> Result<IllustrateDto, String> {
    let cfg = store.config()?.illustrations;
    if !cfg.enabled {
        return Err("Turn on illustrations in Book settings first".into());
    }
    let key = key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or("Add an image key in Book settings, or set OPENAI_API_KEY, to draw illustrations")?;
    let wanted = match id {
        Some(id) => vec![store.find(id)?.ok_or_else(|| nothing_saved(id))?],
        None if all => store.all()?,
        None => return Err("name a recipe, or draw them all".into()),
    };
    let (todo, had): (Vec<Saved>, Vec<Saved>) = wanted
        .into_iter()
        .partition(|s| redraw || !has_illustration(store, s));
    let mut done = IllustrateDto {
        total: todo.len(),
        drawn: 0,
        kept: 0,
        already: had.len(),
        stopped: false,
    };
    let event = |n, saved: &Saved, result| IllustrateEvent {
        n,
        total: todo.len(),
        id: saved.id.clone(),
        title: book::shown_title(saved),
        result,
    };
    if let Some(first) = todo.first() {
        emit(event(0, first, "start"));
    }
    for (i, saved) in todo.iter().enumerate() {
        if stop.load(Ordering::SeqCst) {
            done.stopped = true;
            break;
        }
        let result = match illustrate::illustrate(store, saved, &cfg, Some(key), post) {
            Ok(()) => {
                done.drawn += 1;
                "drawn"
            }
            Err(_) => {
                done.kept += 1;
                "kept_photo"
            }
        };
        emit(event(i + 1, saved, result));
    }
    if id.is_some() && done.kept > 0 {
        return Err(illustrate::FAILED.into());
    }
    Ok(done)
}

// ---------------------------------------------------------------------------------------------
// The commands
// ---------------------------------------------------------------------------------------------

fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> impl std::future::Future<Output = Result<T, String>> {
    let work = tauri::async_runtime::spawn_blocking(f);
    async move { work.await.map_err(|e| e.to_string())? }
}

/// Fetches and reads a recipe from a web address, a link file's path or a saved page's path,
/// writing nothing.
#[tauri::command]
pub async fn cookbook_preview(target: String) -> Result<PreviewDto, String> {
    blocking(move || preview_dto(&store(), &deckle_digest::Http, target.trim())).await
}

/// Adds (or updates) the recipe at `target`. `chapter` is a chapter id, else it is filed by its
/// categories and title. `picture` is the Add card's chip (`site` | `illustration` | `none`);
/// choosing `illustration` only records the choice: call `cookbook_illustrate` to draw it.
#[tauri::command]
pub async fn cookbook_add(
    target: String,
    chapter: Option<String>,
    note: Option<String>,
    picture: Option<Picture>,
) -> Result<SavedDto, String> {
    blocking(move || {
        add_dto(
            &store(),
            &deckle_digest::Http,
            target.trim(),
            chapter.as_deref(),
            note.as_deref(),
            picture,
            Utc::now(),
        )
    })
    .await
}

/// The whole book: settings, chapters with their recipes, counts, the cover.
#[tauri::command]
pub async fn cookbook_book() -> Result<BookDto, String> {
    blocking(|| book_dto(&store())).await
}

/// Changes a saved recipe. Choosing `illustration` only records the choice (see `cookbook_add`).
#[tauri::command]
pub fn cookbook_update(id: String, patch: PatchDto) -> Result<SavedDto, String> {
    update_dto(&store(), &id, patch)
}

#[tauri::command]
pub fn cookbook_remove(id: String) -> Result<(), String> {
    remove_recipe(&store(), &id)
}

/// Saves the book's settings (`cookbook.toml`), keeping comments and keys it doesn't know.
#[tauri::command]
pub fn cookbook_save_config(cfg: ConfigDto) -> Result<(), String> {
    save_config_dto(&store(), cfg)
}

/// `deckle cookbook send`: builds the book if it changed and sends it to the connected Kindle,
/// or queues it for each Kindle's next plug-in.
#[tauri::command]
pub async fn cookbook_send_now() -> Result<SendDto, String> {
    blocking(|| send_dto(&store())).await
}

/// Builds the book into the app's cache and returns the EPUB's path, to open with the opener.
#[tauri::command]
pub async fn cookbook_preview_book() -> Result<String, String> {
    blocking(|| {
        let out = deckle_core::wordwise::default_cache_dir().join("cookbook");
        preview_book_at(&store(), &out)
    })
    .await
}

#[tauri::command]
pub fn cookbook_delivery() -> DeliveryDto {
    delivery_at(&store(), &digest_path(), cookbook_api::watcher_installed())
}

/// Puts the book on a Kindle (`on`) or leaves it out: `[sync.devices."<id>"] cookbook`.
#[tauri::command]
pub fn cookbook_set_kindle(id: String, on: bool) -> Result<(), String> {
    set_kindle_at(&digest_path(), &id, on)
}

static ILLUSTRATING: AtomicBool = AtomicBool::new(false);
static STOP_ILLUSTRATING: AtomicBool = AtomicBool::new(false);

/// Lets go of [`ILLUSTRATING`] however the run ends.
struct Drawing;

impl Drop for Drawing {
    fn drop(&mut self) {
        ILLUSTRATING.store(false, Ordering::SeqCst);
    }
}

/// Draws recipe `id`, or with `all` every recipe without a drawing (every one with `redraw`),
/// one at a time, emitting `cookbook-illustrate` ([`IllustrateEvent`]) as each finishes.
/// `cookbook_illustrate_stop` ends it after the one being drawn. The key is Settings'
/// `illustrations_api_key`, else `OPENAI_API_KEY` (never X-Ray's `llm_api_key`).
#[tauri::command]
pub async fn cookbook_illustrate(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Option<String>,
    all: bool,
    redraw: bool,
) -> Result<IllustrateDto, String> {
    let key = image_key(
        &load_settings_from(&state.settings_file),
        illustrate::api_key(),
    );
    if ILLUSTRATING.swap(true, Ordering::SeqCst) {
        return Err("Already drawing; stop that first".into());
    }
    let guard = Drawing;
    STOP_ILLUSTRATING.store(false, Ordering::SeqCst);
    blocking(move || {
        let _guard = guard;
        let post: Post = &deckle_xray::llm::http_post;
        illustrate_run(
            &store(),
            id.as_deref(),
            all,
            redraw,
            key.as_deref(),
            post,
            &STOP_ILLUSTRATING,
            &mut |e| {
                let _ = app.emit("cookbook-illustrate", e);
            },
        )
    })
    .await
}

/// Ends a `cookbook_illustrate` run after the recipe being drawn.
#[tauri::command]
pub fn cookbook_illustrate_stop() {
    STOP_ILLUSTRATING.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_digest::chrono::TimeZone;

    const SOUP_URL: &str = "https://kitchen.test/soup";
    const BREAD_URL: &str = "https://kitchen.test/bread";
    const IMG_URL: &str = "https://img.test/soup.png";

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap()
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        }))
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
        out
    }

    fn page(name: &str, category: &str, image: Option<&str>) -> String {
        let image = image
            .map(|i| format!(r#""image": "{i}","#))
            .unwrap_or_default();
        format!(
            r#"<!doctype html><html><head><title>{name}</title>
<script type="application/ld+json">
{{"@context": "https://schema.org", "@type": "Recipe", "name": "{name}",
  "recipeCategory": "{category}", {image}
  "recipeIngredient": ["2 cups tomatoes", "1 onion"], "recipeInstructions": ["Simmer.", "Blend."]}}
</script></head><body><h1>{name}</h1></body></html>"#
        )
    }

    /// The kitchen's pages and the soup's photo; everything else fails.
    fn net(url: &str) -> Result<Vec<u8>, String> {
        match url {
            SOUP_URL => Ok(page("Tomato soup", "Soup", Some(IMG_URL)).into_bytes()),
            BREAD_URL => Ok(page("Sourdough", "Bread", None).into_bytes()),
            IMG_URL => Ok(png(900, 700)),
            other => Err(format!("no such page: {other}")),
        }
    }

    fn fresh() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("cookbook"));
        (tmp, store)
    }

    fn add_soup(store: &Store) -> SavedDto {
        add_dto(store, &net, SOUP_URL, None, None, None, at(1)).unwrap()
    }

    #[test]
    fn preview_reads_the_page_and_writes_nothing() {
        let (_t, store) = fresh();
        let p = preview_dto(&store, &net, SOUP_URL).unwrap();
        assert_eq!(p.title, "Tomato soup");
        assert_eq!((p.ingredients, p.steps), (2, 2));
        assert!(p.structured);
        assert_eq!(
            (p.chapter.as_str(), p.chapter_name.as_str()),
            ("soups", "Soups and stews")
        );
        assert!(!p.exists);
        let picture = p.picture.expect("the page has a photo");
        assert!(picture.starts_with("data:image/jpeg;base64,"));
        // The Add card is a preview: the book is still empty.
        assert!(store.all().unwrap().is_empty());
        assert!(!store.dir.join("images").exists());
    }

    #[test]
    fn preview_of_a_non_recipe_says_why() {
        let (_t, store) = fresh();
        let err = preview_dto(&store, &net, "https://kitchen.test/nope").unwrap_err();
        assert!(err.contains("no such page"), "{err}");
    }

    #[test]
    fn add_files_the_recipe_and_keeps_the_picture_choice() {
        let (_t, store) = fresh();
        let saved = add_dto(
            &store,
            &net,
            SOUP_URL,
            Some("mains"),
            Some("Mom's"),
            Some(Picture::None),
            at(1),
        )
        .unwrap();
        assert_eq!(saved.stored, "added");
        assert_eq!(saved.picture_note, None);
        let r = &saved.recipe;
        assert_eq!(
            (r.title.as_str(), r.chapter.as_str()),
            ("Tomato soup", "mains")
        );
        assert_eq!(r.note.as_deref(), Some("Mom's"));
        assert_eq!(r.picture, Picture::None);
        assert_eq!(r.printed, Picture::None);
        let on_disk = store.find(&r.id).unwrap().unwrap();
        assert_eq!(on_disk.picture, Picture::None);
        assert!(store.image_path(&r.id, ImageKind::Printed).is_file());
    }

    #[test]
    fn add_without_a_photo_says_why_and_a_second_add_is_an_update() {
        let (_t, store) = fresh();
        let first = add_dto(&store, &net, BREAD_URL, None, None, None, at(1)).unwrap();
        assert_eq!(first.stored, "added");
        assert_eq!(first.picture_note.as_deref(), Some(add::NO_PICTURE));
        assert_eq!(first.recipe.chapter, "baking");
        let again = add_dto(&store, &net, BREAD_URL, None, None, None, at(2)).unwrap();
        assert_eq!(again.stored, "updated");
        assert_eq!(again.recipe.id, first.recipe.id);
        assert_eq!(store.all().unwrap().len(), 1);
    }

    #[test]
    fn add_to_an_unknown_chapter_errors_and_writes_nothing() {
        let (_t, store) = fresh();
        let err = add_dto(&store, &net, SOUP_URL, Some("nope"), None, None, at(1)).unwrap_err();
        assert!(err.contains("no chapter \"nope\""), "{err}");
        assert!(store.all().unwrap().is_empty());
    }

    #[test]
    fn book_lists_chapters_in_order_with_their_recipes() {
        let (_t, store) = fresh();
        let soup = add_soup(&store);
        add_dto(&store, &net, BREAD_URL, None, None, None, at(20)).unwrap();
        // A chapter since deleted from the config: its recipe prints under Everything else.
        let mut cfg = store.config().unwrap();
        cfg.chapters.retain(|c| c.id != "baking");
        store.save_config(&cfg).unwrap();

        let book = book_dto(&store).unwrap();
        assert_eq!(book.recipes, 2);
        assert_eq!(book.printed_chapters, 2);
        let ids: Vec<&str> = book.chapters.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"breakfast"));
        assert_eq!(ids.last(), Some(&"other"));
        let soups = book.chapters.iter().find(|c| c.id == "soups").unwrap();
        assert_eq!(soups.recipes.len(), 1);
        assert_eq!(soups.recipes[0].id, soup.recipe.id);
        assert!(
            soups.recipes[0]
                .thumb
                .as_deref()
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
        assert_eq!(soups.recipes[0].ingredients, ["2 cups tomatoes", "1 onion"]);
        let other = book.chapters.iter().find(|c| c.id == "other").unwrap();
        assert_eq!(other.recipes.len(), 1);
        assert_eq!(other.recipes[0].chapter, "baking");
        assert_eq!(other.recipes[0].home, "other");
        assert_eq!(other.recipes[0].home_name, "Everything else");
        // Chapters nobody is in are listed, empty.
        assert!(
            book.chapters
                .iter()
                .any(|c| c.id == "salads" && c.recipes.is_empty())
        );
        // The newest is the one added last; the soup, 19 days older, carries no star.
        assert_eq!(book.newest.as_ref().unwrap().title, "Sourdough");
        assert!(other.recipes[0].is_new);
        assert!(!soups.recipes[0].is_new);
        assert!(
            book.cover
                .as_deref()
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
        assert!(book.pending_build);
        assert_eq!(book.config.title, "The Kitchen Book");
        assert!(book.config_path.ends_with("cookbook.toml"));
    }

    #[test]
    fn an_empty_book_is_a_book() {
        let (_t, store) = fresh();
        let book = book_dto(&store).unwrap();
        assert_eq!((book.recipes, book.printed_chapters), (0, 0));
        assert!(book.newest.is_none());
        assert!(!book.pending_build);
        assert_eq!(book.chapters.len(), 10);
    }

    #[test]
    fn recipes_in_a_chapter_are_alphabetical_by_the_title_printed() {
        let (_t, store) = fresh();
        let soup = add_soup(&store);
        let bread = add_dto(&store, &net, BREAD_URL, Some("soups"), None, None, at(2)).unwrap();
        let rename = PatchDto {
            title_override: Some("Aaa first".into()),
            ..Default::default()
        };
        update_dto(&store, &bread.recipe.id, rename).unwrap();
        let book = book_dto(&store).unwrap();
        let soups = book.chapters.iter().find(|c| c.id == "soups").unwrap();
        let titles: Vec<&str> = soups.recipes.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Aaa first", "Tomato soup"]);
        assert_eq!(soups.recipes[1].id, soup.recipe.id);
        assert_eq!(soups.recipes[0].site_title, "Sourdough");
    }

    #[test]
    fn update_changes_only_what_is_given() {
        let (_t, store) = fresh();
        let id = add_soup(&store).recipe.id;
        let saved = update_dto(
            &store,
            &id,
            PatchDto {
                title_override: Some("  Gran's soup ".into()),
                chapter: Some("mains".into()),
                picture: Some(Picture::Illustration),
                note: Some("Less salt".into()),
            },
        )
        .unwrap();
        assert_eq!(saved.stored, "updated");
        let r = &saved.recipe;
        assert_eq!(r.title, "Gran's soup");
        assert_eq!(r.title_override.as_deref(), Some("Gran's soup"));
        assert_eq!(
            (r.chapter.as_str(), r.picture),
            ("mains", Picture::Illustration)
        );
        // No drawing yet: the site's photo prints.
        assert_eq!((r.printed, r.has_illustration), (Picture::Site, false));
        assert_eq!(r.note.as_deref(), Some("Less salt"));
        // Another change leaves those alone, and an empty string clears.
        let saved = update_dto(
            &store,
            &id,
            PatchDto {
                note: Some(String::new()),
                title_override: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        let r = &saved.recipe;
        assert_eq!(
            (r.note.as_deref(), r.title_override.as_deref()),
            (None, None)
        );
        assert_eq!(
            (r.chapter.as_str(), r.picture),
            ("mains", Picture::Illustration)
        );
        assert_eq!(r.title, "Tomato soup");
        assert!(store.is_dirty());
    }

    #[test]
    fn update_unknown_id_errors() {
        let (_t, store) = fresh();
        let err = update_dto(&store, "0123456789ab", PatchDto::default()).unwrap_err();
        assert!(err.contains("nothing saved as 0123456789ab"), "{err}");
    }

    #[test]
    fn update_to_an_unknown_chapter_errors_and_changes_nothing() {
        let (_t, store) = fresh();
        let id = add_soup(&store).recipe.id;
        let patch = PatchDto {
            chapter: Some("nope".into()),
            note: Some("x".into()),
            ..Default::default()
        };
        let err = update_dto(&store, &id, patch).unwrap_err();
        assert!(err.contains("no chapter \"nope\""), "{err}");
        assert_eq!(store.find(&id).unwrap().unwrap().note, None);
    }

    #[test]
    fn remove_deletes_the_recipe_and_its_pictures() {
        let (_t, store) = fresh();
        let id = add_soup(&store).recipe.id;
        remove_recipe(&store, &id).unwrap();
        assert!(store.find(&id).unwrap().is_none());
        assert!(!store.image_path(&id, ImageKind::Printed).exists());
        let err = remove_recipe(&store, &id).unwrap_err();
        assert!(err.contains("nothing saved as"), "{err}");
    }

    fn config_of(store: &Store) -> ConfigDto {
        store.config().unwrap().into()
    }

    #[test]
    fn save_config_writes_the_book_settings() {
        let (_t, store) = fresh();
        let mut cfg = config_of(&store);
        cfg.title = "The Vargas Kitchen Book".into();
        cfg.picture = Picture::Illustration;
        cfg.chapters[0].name = "Morning".into();
        cfg.chapters.push(Chapter {
            id: "snacks".into(),
            name: "Snacks".into(),
            match_: vec![],
        });
        cfg.illustrations = IllustrationsDto {
            enabled: true,
            endpoint: "http://localhost:8080/v1".into(),
            model: String::new(),
        };
        save_config_dto(&store, cfg.clone()).unwrap();
        assert_eq!(config_of(&store), cfg);
        let text = std::fs::read_to_string(store.dir.join("cookbook.toml")).unwrap();
        assert!(!text.contains("model"), "{text}");
    }

    #[test]
    fn save_config_keeps_unknown_keys() {
        let (_t, store) = fresh();
        save_config_dto(&store, config_of(&store)).unwrap();
        let path = store.dir.join("cookbook.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("# mine\nfuture = 1\n{text}")).unwrap();
        let mut cfg = config_of(&store);
        cfg.subtitle = "Gathered".into();
        save_config_dto(&store, cfg).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mine\nfuture = 1"), "{text}");
        assert!(text.contains("subtitle = \"Gathered\""), "{text}");
    }

    #[test]
    fn save_config_refuses_a_book_without_everything_else_or_with_a_twin() {
        let (_t, store) = fresh();
        let mut cfg = config_of(&store);
        cfg.chapters.retain(|c| c.id != EVERYTHING_ELSE);
        let err = save_config_dto(&store, cfg).unwrap_err();
        assert!(err.contains("Everything else"), "{err}");
        let mut cfg = config_of(&store);
        cfg.chapters[1].id = cfg.chapters[0].id.clone();
        let err = save_config_dto(&store, cfg).unwrap_err();
        assert!(err.contains("two chapters"), "{err}");
        let mut cfg = config_of(&store);
        cfg.chapters[1].id = "  ".into();
        assert!(save_config_dto(&store, cfg).is_err());
        assert!(!store.dir.join("cookbook.toml").exists());
    }

    #[test]
    fn send_now_of_an_empty_book_says_so() {
        let (_t, store) = fresh();
        let err = send_dto(&store).unwrap_err();
        assert!(err.contains("no recipes yet"), "{err}");
    }

    #[test]
    fn preview_book_writes_the_epub() {
        let (t, store) = fresh();
        add_soup(&store);
        let out = t.path().join("preview");
        let path = preview_book_at(&store, &out).unwrap();
        assert_eq!(PathBuf::from(&path), out.join("cookbook.epub"));
        assert!(std::fs::metadata(&path).unwrap().len() > 1000);
        // An export: the book still has changes a delivery hasn't picked up.
        assert!(store.is_dirty());
    }

    fn digest_with(dir: &Path, text: &str) -> PathBuf {
        let path = dir.join("digest.toml");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn delivery_lists_the_kindles_and_the_watcher() {
        let (t, store) = fresh();
        let digest = digest_with(
            t.path(),
            "[sync.devices.G000AAAA]\nname = \"Mom's Kindle\"\n\n\
             [sync.devices.G000BBBB]\nname = \"Attic\"\ncookbook = false\n",
        );
        let d = delivery_at(&store, &digest, true);
        assert_eq!(d.error, None);
        assert!(d.watcher_installed);
        let rows: Vec<(&str, &str)> = d
            .kindles
            .iter()
            .map(|k| (k.name.as_str(), k.status.as_str()))
            .collect();
        assert_eq!(rows, [("Attic", "Off"), ("Mom's Kindle", "Never sent")]);
    }

    #[test]
    fn delivery_with_no_digest_file_has_no_kindles_and_no_error() {
        let (t, store) = fresh();
        let d = delivery_at(&store, &t.path().join("digest.toml"), false);
        assert!(d.kindles.is_empty());
        assert_eq!(d.error, None);
        assert!(!d.watcher_installed);
    }

    #[test]
    fn delivery_says_when_the_config_wont_load() {
        let (t, store) = fresh();
        let digest = digest_with(t.path(), "[sync\nbroken");
        let d = delivery_at(&store, &digest, false);
        assert!(d.kindles.is_empty());
        assert!(d.error.is_some());
    }

    #[test]
    fn set_kindle_writes_sync_device() {
        let (t, _store) = fresh();
        let digest = digest_with(
            t.path(),
            "# my paper\n[sync]\npush = true\n\n[sync.devices.\"Kindle/Kindle#G000AAAA\"]\nname = \"Mom's Kindle\"\n",
        );
        set_kindle_at(&digest, "Kindle/Kindle#G000AAAA", false).unwrap();
        let cfg = deckle_digest::Config::load(&digest).unwrap();
        assert!(!cfg.sync.for_device("Kindle/Kindle#G000AAAA").cookbook);
        // The name stayed, and so did the comment.
        assert_eq!(
            cfg.sync.device_name("Kindle/Kindle#G000AAAA").as_deref(),
            Some("Mom's Kindle")
        );
        let text = std::fs::read_to_string(&digest).unwrap();
        assert!(text.contains("# my paper"), "{text}");
        assert!(text.contains("cookbook = false"), "{text}");
        // Back on is written too, so it beats a household-wide `cookbook = false`.
        set_kindle_at(&digest, "Kindle/Kindle#G000AAAA", true).unwrap();
        let cfg = deckle_digest::Config::load(&digest).unwrap();
        assert!(cfg.sync.for_device("Kindle/Kindle#G000AAAA").cookbook);
    }

    #[test]
    fn set_kindle_starts_a_digest_file_when_there_is_none() {
        let (t, _store) = fresh();
        let digest = t.path().join("digest.toml");
        set_kindle_at(&digest, "G000CCCC", false).unwrap();
        let cfg = deckle_digest::Config::load(&digest).unwrap();
        assert!(!cfg.sync.for_device("G000CCCC").cookbook);
        assert!(cfg.sync.for_device("other").cookbook);
    }

    // --- illustrations -------------------------------------------------------------------

    /// A 256x256 gray PNG as the endpoint's answer.
    fn drawing() -> String {
        let mut bytes = Vec::new();
        image::DynamicImage::ImageLuma8(image::GrayImage::from_fn(256, 256, |x, y| {
            image::Luma([((x * 4 + y) % 256) as u8])
        }))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        format!(
            r#"{{"data":[{{"b64_json":"{}"}}]}}"#,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }

    fn turn_on_illustrations(store: &Store) {
        let mut cfg = store.config().unwrap();
        cfg.illustrations.enabled = true;
        store.save_config(&cfg).unwrap();
    }

    fn both(store: &Store) -> (String, String) {
        (
            add_soup(store).recipe.id,
            add_dto(store, &net, BREAD_URL, None, None, None, at(2))
                .unwrap()
                .recipe
                .id,
        )
    }

    fn ok_post(_: &str, _: &[(String, String)], _: &str) -> Result<String, String> {
        Ok(drawing())
    }

    #[test]
    fn illustrate_all_draws_each_and_reports_progress() {
        let (_t, store) = fresh();
        let (soup, bread) = both(&store);
        turn_on_illustrations(&store);
        let mut events = Vec::new();
        let done = illustrate_run(
            &store,
            None,
            true,
            false,
            Some("k"),
            &ok_post,
            &AtomicBool::new(false),
            &mut |e| events.push(e),
        )
        .unwrap();
        assert_eq!(
            done,
            IllustrateDto {
                total: 2,
                drawn: 2,
                kept: 0,
                already: 0,
                stopped: false
            }
        );
        assert!(store.image_path(&soup, ImageKind::Ill).is_file());
        assert!(store.image_path(&bread, ImageKind::Ill).is_file());
        let seen: Vec<(usize, &str)> = events.iter().map(|e| (e.n, e.result)).collect();
        assert_eq!(seen, [(0, "start"), (1, "drawn"), (2, "drawn")]);
        assert!(events.iter().all(|e| e.total == 2));
        // Drawn ones are skipped next time unless redrawn.
        let again = illustrate_run(
            &store,
            None,
            true,
            false,
            Some("k"),
            &ok_post,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!((again.total, again.already), (0, 2));
        let redo = illustrate_run(
            &store,
            None,
            true,
            true,
            Some("k"),
            &ok_post,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!((redo.total, redo.drawn), (2, 2));
    }

    #[test]
    fn illustrate_one_recipe() {
        let (_t, store) = fresh();
        let (soup, bread) = both(&store);
        turn_on_illustrations(&store);
        let done = illustrate_run(
            &store,
            Some(&soup),
            false,
            false,
            Some("k"),
            &ok_post,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!((done.total, done.drawn), (1, 1));
        assert!(store.image_path(&soup, ImageKind::Ill).is_file());
        assert!(!store.image_path(&bread, ImageKind::Ill).exists());
    }

    #[test]
    fn stop_ends_the_run_between_recipes() {
        let (_t, store) = fresh();
        both(&store);
        turn_on_illustrations(&store);
        let stop = AtomicBool::new(false);
        let done = illustrate_run(
            &store,
            None,
            true,
            false,
            Some("k"),
            &ok_post,
            &stop,
            // Stop pressed as the first one finishes.
            &mut |e| {
                if e.n == 1 {
                    stop.store(true, Ordering::SeqCst);
                }
            },
        )
        .unwrap();
        assert_eq!((done.total, done.drawn, done.stopped), (2, 1, true));
    }

    #[test]
    fn a_failed_drawing_keeps_the_photo() {
        let (_t, store) = fresh();
        let (soup, _) = both(&store);
        turn_on_illustrations(&store);
        let down = |_: &str, _: &[(String, String)], _: &str| -> Result<String, String> {
            Err("503 secret provider detail".into())
        };
        let mut events = Vec::new();
        let done = illustrate_run(
            &store,
            None,
            true,
            false,
            Some("k"),
            &down,
            &AtomicBool::new(false),
            &mut |e| events.push(e),
        )
        .unwrap();
        assert_eq!((done.drawn, done.kept), (0, 2));
        assert!(events.iter().skip(1).all(|e| e.result == "kept_photo"));
        assert!(!store.image_path(&soup, ImageKind::Ill).exists());
        // Asked for one by name, a failure is the one generic line, nothing the provider said.
        let err = illustrate_run(
            &store,
            Some(&soup),
            false,
            false,
            Some("k"),
            &down,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(err, illustrate::FAILED);
    }

    #[test]
    fn illustrate_needs_the_switch_a_key_and_something_to_draw() {
        let (_t, store) = fresh();
        let (soup, _) = both(&store);
        let run = |id: Option<&str>, all: bool, key: Option<&str>| {
            illustrate_run(
                &store,
                id,
                all,
                false,
                key,
                &ok_post,
                &AtomicBool::new(false),
                &mut |_| {},
            )
        };
        let off = run(None, true, Some("k")).unwrap_err();
        assert!(off.contains("Turn on illustrations"), "{off}");
        turn_on_illustrations(&store);
        let nokey = run(None, true, None).unwrap_err();
        assert!(nokey.contains("key"), "{nokey}");
        let blank = run(None, true, Some("  ")).unwrap_err();
        assert!(blank.contains("key"), "{blank}");
        let nothing = run(None, false, Some("k")).unwrap_err();
        assert!(nothing.contains("name a recipe"), "{nothing}");
        let unknown = run(Some("0123456789ab"), false, Some("k")).unwrap_err();
        assert!(unknown.contains("nothing saved as"), "{unknown}");
        assert!(store.find(&soup).unwrap().is_some());
    }

    #[test]
    fn the_key_is_the_image_key_else_the_environment_and_never_the_llm_key() {
        let mut s = Settings {
            llm_api_key: "sk-ant-x".into(),
            ..Settings::default()
        };
        assert_eq!(image_key(&s, None), None, "X-Ray's key is not for images");
        assert_eq!(
            image_key(&s, Some("env".to_owned())).as_deref(),
            Some("env")
        );
        s.illustrations_api_key = " sk-typed ".into();
        assert_eq!(
            image_key(&s, Some("env".to_owned())).as_deref(),
            Some("sk-typed")
        );
        s.illustrations_api_key = "  ".into();
        assert_eq!(image_key(&s, None), None);
        assert_eq!(image_key(&s, Some("  ".to_owned())), None);
    }
}
