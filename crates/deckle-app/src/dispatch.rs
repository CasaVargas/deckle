// SPDX-License-Identifier: GPL-3.0-or-later
//! Every command by name, with the JSON arguments the frontend sends over Tauri (camelCase
//! keys) and the JSON result serde gives: the web UI's one way in. Commands that work the
//! desktop way only (the file manager, the keychain, the system's job scheduler) answer the
//! web app's way when `Ctx::web` is set.

use std::path::PathBuf;
use std::sync::Arc;

use deckle_digest::Config;
use deckle_digest::config::DayConfig;
use deckle_digest::cookbook::store::Picture;
use deckle_library::details::Details;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::{AppState, CoverChoice, RunRequest, RunTarget, Settings};
use crate::cookbook::{ConfigDto, PatchDto};
use crate::events::Events;
use crate::press::ExpectedExport;
use crate::{books, capabilities, commands, cookbook, daily, kindle, morgue, next, press};
use crate::{readership, shelf, upgrades};

/// What a command can reach besides its arguments.
pub struct Ctx {
    pub state: AppState,
    pub events: Arc<dyn Events>,
    /// The binary the commands that run the CLI start.
    pub exe: PathBuf,
    /// Serving the web UI: no file manager, keychain, updater or system scheduler.
    pub web: bool,
    /// In the web UI, the folders a path from the browser must lie in (`/config`, `/data`,
    /// `DECKLE_WEB_ROOTS`); `None` on the desktop, where the owner picked each path.
    pub roots: Option<Vec<PathBuf>>,
}

const NOT_IN_WEB: &str = "not available in the web app";

/// A path from the frontend. On the desktop it is used as given (the owner chose it in a file
/// dialog); in the web UI it must lie inside [`Ctx::roots`]. It can only be read through
/// [`UiPath::read`] or [`UiPath::write`], so a command can't forget the check.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct UiPath(String);

/// A path, or an `http(s)://` address (a saved page, or a story to fetch).
#[derive(Deserialize)]
#[serde(transparent)]
pub struct UiPathOrUrl(String);

impl UiPath {
    /// The path, for a command that reads it.
    pub fn read(self, ctx: &Ctx) -> Result<String, String> {
        ctx.path(&self.0)
    }

    /// The path, for a command that writes there.
    pub fn write(self, ctx: &Ctx) -> Result<String, String> {
        ctx.write_path(&self.0)
    }
}

impl UiPathOrUrl {
    pub fn read(self, ctx: &Ctx) -> Result<String, String> {
        // As `web_book::source` tells them apart: anything else is a file.
        if self.0.starts_with("http://") || self.0.starts_with("https://") {
            Ok(self.0)
        } else {
            ctx.path(&self.0)
        }
    }
}

fn read_all<T>(
    ctx: &Ctx,
    v: Vec<T>,
    f: fn(T, &Ctx) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    v.into_iter().map(|p| f(p, ctx)).collect()
}

impl Ctx {
    /// `raw` as a command may use it: as given on the desktop; in the web UI, resolved and
    /// inside the roots.
    pub fn path(&self, raw: &str) -> Result<String, String> {
        match &self.roots {
            None => Ok(raw.to_owned()),
            Some(roots) => {
                let roots: Vec<PathBuf> =
                    roots.iter().filter_map(|r| r.canonicalize().ok()).collect();
                deckle_core::confine::confine_to(&roots, raw)
                    .map(|p| p.to_string_lossy().into_owned())
            }
        }
    }

    /// [`Ctx::path`] for somewhere a command writes. In the web UI that is never Deckle's own
    /// settings folder, whose files say which programs the server runs.
    pub fn write_path(&self, raw: &str) -> Result<String, String> {
        let p = self.path(raw)?;
        if self.roots.is_some()
            && let Some(dir) = self
                .state
                .settings_file
                .parent()
                .and_then(|d| d.canonicalize().ok())
            && std::path::Path::new(&p).starts_with(&dir)
        {
            return Err(format!("{raw}: inside Deckle's own settings folder"));
        }
        Ok(p)
    }

    /// The target with a folder's path checked: a send writes there.
    fn target(&self, t: RunTarget) -> Result<RunTarget, String> {
        match t {
            RunTarget::Folder { path } => Ok(RunTarget::Folder {
                path: self.write_path(&path)?,
            }),
            other => Ok(other),
        }
    }

    /// Each book's file, checked.
    fn books(&self, books: Vec<RunRequest>) -> Result<Vec<RunRequest>, String> {
        books
            .into_iter()
            .map(|mut b| {
                b.path = UiPathOrUrl(b.path).read(self)?;
                Ok(b)
            })
            .collect()
    }

    /// A saved path being set again: checked when it changed, left to its use when the owner
    /// is only saving it back as it was (it may predate the check, or be hand-edited).
    fn saved(
        &self,
        new: Option<String>,
        old: Option<&str>,
        write: bool,
    ) -> Result<Option<String>, String> {
        match new {
            Some(n) if self.roots.is_some() && !n.trim().is_empty() && Some(n.as_str()) != old => {
                if write {
                    self.write_path(&n)
                } else {
                    self.path(&n)
                }
                .map(|_| Some(n))
            }
            other => Ok(other),
        }
    }
}

/// Runs `command` with `args` (an object of camelCase keys; `null` for none). Blocks.
pub fn dispatch(ctx: &Ctx, command: &str, args: Value) -> Result<Value, String> {
    let args = if args.is_null() {
        Value::Object(Default::default())
    } else {
        args
    };
    call(ctx, command, args)
}

fn json<T: Serialize>(r: Result<T, String>) -> Result<Value, String> {
    r.and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
}

/// One line per command: `name { arg: Type, … } => body`, where `body` is a `Result` and
/// `ctx` is in scope. Each command's arguments are a struct of their own, camelCased as Tauri
/// does; a missing `Option` is `None`, as Tauri allows.
macro_rules! commands {
    ($ctx:ident; $( $name:ident { $($arg:ident : $ty:ty),* $(,)? } => $body:expr ),* $(,)?) => {
        /// Every command [`dispatch`] knows, sorted.
        pub const COMMANDS: &[&str] = &[$(stringify!($name)),*];

        fn call($ctx: &Ctx, command: &str, args: Value) -> Result<Value, String> {
            match command {
                $(stringify!($name) => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Args { $($arg: $ty),* }
                    let Args { $($arg),* } = serde_json::from_value(args)
                        .map_err(|e| format!("{command}: {e}"))?;
                    json($body)
                })*
                _ => Err(format!("unknown command: {command}")),
            }
        }
    };
}

commands! { ctx;
    app_version {} => Ok(commands::app_version()),
    bind_books { paths: Vec<UiPathOrUrl>, title: String, author: Option<String> } =>
        commands::bind_books(read_all(ctx, paths, UiPathOrUrl::read)?, title, author),
    book_preflight { path: UiPath, cover: Option<CoverChoice> } =>
        books::book_preflight(&ctx.state, path.read(ctx)?, cover),
    bookmarklet {} => daily::bookmarklet(),
    bookmarklet_page_open {} => if ctx.web {
        Err(NOT_IN_WEB.to_owned())
    } else {
        daily::bookmarklet_page_open()
    },
    calibre_create_columns {} => crate::calibre::calibre_create_columns(),
    calibre_set_password { password: String } => crate::calibre::calibre_set_password(password),
    calibre_writeback_confirm { keys: Vec<String> } => crate::calibre::calibre_writeback_confirm(keys),
    calibre_writeback_set { patch: crate::calibre::WritebackPatch } => if ctx.web {
        crate::calibre::calibre_writeback_set_web(patch)
    } else {
        crate::calibre::calibre_writeback_set(patch)
    },
    calibre_writeback_status {} => Ok(crate::calibre::calibre_writeback_status()),
    calibre_writeback_write {} => crate::calibre::calibre_writeback_write(),
    capabilities {} => Ok(capabilities::capabilities(ctx.web, &ctx.state)),
    cookbook_add { target: UiPathOrUrl, chapter: Option<String>, note: Option<String>, picture: Option<Picture> } =>
        cookbook::cookbook_add(target.read(ctx)?, chapter, note, picture),
    cookbook_book {} => cookbook::cookbook_book(),
    cookbook_delivery {} => Ok(cookbook::cookbook_delivery()),
    cookbook_illustrate { id: Option<String>, all: bool, redraw: bool } =>
        cookbook::cookbook_illustrate(ctx.events.clone(), &ctx.state, id, all, redraw),
    cookbook_illustrate_stop {} => {
        cookbook::cookbook_illustrate_stop();
        Ok(())
    },
    cookbook_preview { target: UiPathOrUrl } => cookbook::cookbook_preview(target.read(ctx)?),
    cookbook_preview_book {} => cookbook::cookbook_preview_book(),
    cookbook_remove { id: String } => cookbook::cookbook_remove(id),
    cookbook_save_config { cfg: ConfigDto } => cookbook::cookbook_save_config(cfg),
    cookbook_send_now {} => cookbook::cookbook_send_now(),
    cookbook_set_kindle { id: String, on: bool } => cookbook::cookbook_set_kindle(id, on),
    cookbook_update { id: String, patch: PatchDto } => cookbook::cookbook_update(id, patch),
    cover_candidates { title: String, author: String, isbn: Option<String> } =>
        books::cover_candidates(&ctx.state, title, author, isbn),
    cover_fetch { large: String } => books::cover_fetch(large),
    daily_day_preview { day: DayConfig } => Ok(daily::daily_day_preview(day)),
    daily_next { paper: Option<String> } => next::daily_next(paper),
    daily_next_refresh { paper: Option<String> } => next::daily_next_refresh(paper),
    daily_run { send: bool, paper: Option<String> } =>
        daily::daily_run(ctx.events.clone(), ctx.exe.clone(), send, paper),
    daily_save { config: Config, paper: Option<String> } => {
        crate::confine_saved::daily(ctx, &config, paper.as_deref())?;
        daily::daily_save(config, paper)
    },
    daily_setup {} => daily::daily_setup(),
    daily_state { paper: Option<String> } => Ok(daily::daily_state(paper)),
    device_backup { target: RunTarget, into: Option<UiPath> } =>
        kindle::device_backup(ctx.target(target)?, into.map(|p| p.write(ctx)).transpose()?),
    device_books { target: RunTarget } => kindle::device_books(ctx.target(target)?),
    device_edit_source { target: RunTarget, file: String } =>
        upgrades::device_edit_source(ctx.target(target)?, file),
    device_put_back { target: RunTarget, file: String } =>
        upgrades::device_put_back(ctx.target(target)?, file),
    device_rebuild { target: RunTarget, files: Vec<String> } =>
        upgrades::device_rebuild(ctx.target(target)?, files),
    device_remove { target: RunTarget, file: String } => kindle::device_remove(ctx.target(target)?, file),
    device_remove_many { target: RunTarget, files: Vec<String> } =>
        kindle::device_remove_many(ctx.target(target)?, files),
    device_restore { target: RunTarget, dir: UiPath } =>
        kindle::device_restore(ctx.target(target)?, dir.read(ctx)?),
    device_upgrades { target: RunTarget } => upgrades::device_upgrades(ctx.target(target)?),
    edition_preview { paper: Option<String> } => Ok(daily::edition_preview(paper)),
    email_set_password { password: String } => if ctx.web {
        let _ = password;
        Err(deckle_digest::secret::no_keychain(deckle_digest::secret::ENV))
    } else {
        daily::email_set_password(password)
    },
    email_status { paper: Option<String> } => Ok(daily::email_status(paper)),
    email_test { paper: Option<String> } => daily::email_test(paper),
    geocode { query: String } => daily::geocode(query),
    get_settings {} => Ok(if ctx.web {
        commands::get_settings_masked(&ctx.state)
    } else {
        commands::get_settings(&ctx.state)
    }),
    highlights_set { folder: Option<String>, clippings: bool } => {
        let old = kindle::highlights_status().folder;
        kindle::highlights_set(ctx.saved(folder, old.as_deref(), true)?, clippings)
    },
    highlights_status {} => Ok(kindle::highlights_status()),
    highlights_write { target: RunTarget } => kindle::highlights_write(ctx.target(target)?),
    import_articles { targets: Vec<UiPathOrUrl> } =>
        Ok(commands::import_articles(read_all(ctx, targets, UiPathOrUrl::read)?)),
    inspect_books { paths: Vec<UiPath> } => Ok(commands::inspect_books(read_all(ctx, paths, UiPath::read)?)),
    interests_accept { key: String, paper: Option<String> } => readership::interests_accept(key, paper),
    interests_dismiss { key: String, paper: Option<String> } => readership::interests_dismiss(key, paper),
    koreader_install { folder: Option<UiPath>, source: String } =>
        crate::koreader::koreader_install(folder.map(|f| f.write(ctx)).transpose()?, plugin_source(ctx, source)?),
    koreader_preview { folder: Option<UiPath>, source: String } =>
        crate::koreader::koreader_preview(folder.map(|f| f.write(ctx)).transpose()?, plugin_source(ctx, source)?),
    koreader_remove { folder: Option<UiPath>, name: String } =>
        crate::koreader::koreader_remove(folder.map(|f| f.write(ctx)).transpose()?, name),
    koreader_rollback { folder: Option<UiPath>, name: String } =>
        crate::koreader::koreader_rollback(folder.map(|f| f.write(ctx)).transpose()?, name),
    koreader_set_enabled { folder: Option<UiPath>, name: String, on: bool } =>
        crate::koreader::koreader_set_enabled(folder.map(|f| f.write(ctx)).transpose()?, name, on),
    koreader_set_patches { folder: Option<UiPath>, on: bool } =>
        crate::koreader::koreader_set_patches(folder.map(|f| f.write(ctx)).transpose()?, on),
    koreader_status { folder: Option<UiPath> } => Ok(crate::koreader::koreader_status(folder.map(|f| f.write(ctx)).transpose()?)),
    koreader_store_list {} => Ok(crate::koreader::koreader_store_list()),
    koreader_store_readme { repo: String } => crate::koreader::koreader_store_readme(repo),
    koreader_store_search { text: String, updated: bool } =>
        crate::koreader::koreader_store_search(text, updated),
    koreader_store_updates { folder: Option<UiPath> } =>
        crate::koreader::koreader_store_updates(folder.map(|f| f.write(ctx)).transpose()?),
    koreader_update { folder: Option<UiPath>, name: String } =>
        crate::koreader::koreader_update(folder.map(|f| f.write(ctx)).transpose()?, name),
    library_catalog { device: Option<String> } => Ok(books::library_catalog(device)),
    library_covers { paths: Vec<UiPath> } => Ok(books::library_covers(read_all(ctx, paths, UiPath::read)?)),
    library_fix_details { dir: UiPath, details: Details } =>
        books::library_fix_details(dir.write(ctx)?, details),
    library_read_details { dir: UiPath } => books::library_read_details(dir.read(ctx)?),
    library_search { query: String } => Ok(books::library_search(query)),
    library_sent_kindles {} => Ok(books::library_sent_kindles()),
    library_set { root: Option<String>, auto_add: bool } => {
        let old = kindle::library_status().root;
        kindle::library_set(ctx.saved(root, old.as_deref(), true)?, auto_add)
    },
    library_sources {} => Ok(upgrades::library_sources()),
    library_sources_set { find_in_calibre: bool, calibre: Option<String>, folders: Vec<String> } => {
        let old = upgrades::library_sources();
        let calibre = ctx.saved(calibre, old.calibre.as_deref(), false)?;
        for f in &folders {
            let was = old.folders.iter().any(|o| o == f);
            ctx.saved(Some(f.clone()), was.then_some(f.as_str()), false)?;
        }
        upgrades::library_sources_set(find_in_calibre, calibre, folders)
    },
    library_status {} => Ok(kindle::library_status()),
    library_suggest_details { dir: UiPath, asked: bool } =>
        books::library_suggest_details(&ctx.state, dir.read(ctx)?, asked),
    library_watch_scan {} => books::library_watch_scan(),
    library_watch_set { dir: Option<String> } => {
        let old = upgrades::library_sources().watch;
        books::library_watch_set(ctx.saved(dir, old.as_deref(), false)?)
    },
    list_devices {} => Ok(commands::list_devices(&ctx.state)),
    llm_check { endpoint: String, model: String } => Ok(crate::llm::llm_check(endpoint, model)),
    llm_pull { endpoint: String, model: String } =>
        crate::llm::llm_pull(ctx.events.clone(), endpoint, model),
    llm_pull_stop {} => {
        crate::llm::llm_pull_stop();
        Ok(())
    },
    llm_recommended {} => Ok(crate::llm::llm_recommended()),
    masthead_missing { title: String } => Ok(daily::masthead_missing(title)),
    morgue_month { year: i32, month: u32 } => morgue::morgue_month(year, month),
    morgue_open { date: String } => if ctx.web {
        let _ = date;
        Err(NOT_IN_WEB.to_owned())
    } else {
        morgue::morgue_open(date)
    },
    morgue_search { query: String } => morgue::morgue_search(query),
    morgue_send { date: String } => morgue::morgue_send(date, &ctx.exe),
    morgue_show { date: String } => morgue::morgue_show(date),
    name_kindle { sync_id: String, name: String } => commands::name_kindle(sync_id, name),
    paper_create { title: String, reader: String } => daily::paper_create(title, reader),
    papers_list {} => daily::papers_list(),
    plan_books { books: Vec<RunRequest>, target: RunTarget } =>
        Ok(commands::plan_books(&ctx.state, ctx.books(books)?, ctx.target(target)?)),
    press_setup { paper: Option<String>, export_index: usize, enabled: bool, expected: Option<ExpectedExport> } =>
        press::press_setup(paper, export_index, enabled, expected, &ctx.exe),
    press_stop { on: bool, paper: Option<String> } => next::press_stop(on, paper),
    previewer_status {} => Ok(commands::previewer_status(&ctx.state)),
    puzzles_bundle_targets {} => Ok(crate::puzzles::puzzles_bundle_targets()),
    puzzles_kinds {} => Ok(crate::puzzles::puzzles_kinds()),
    puzzles_list {} => Ok(crate::puzzles::puzzles_list()),
    puzzles_make { request: crate::puzzles::BookRequest } =>
        crate::puzzles::puzzles_make(ctx.events.clone(), puzzle_request(ctx, request)?),
    puzzles_preview { request: crate::puzzles::BookRequest } =>
        crate::puzzles::puzzles_preview(puzzle_request(ctx, request)?),
    puzzles_send { path: UiPath, target: RunTarget, replace: bool, as_bundle: Option<bool> } =>
        crate::puzzles::puzzles_send(path.read(ctx)?, ctx.target(target)?, replace, as_bundle.unwrap_or(false)),
    puzzles_sources {} => Ok(crate::puzzles::puzzles_sources()),
    puzzles_sunday_get { paper: Option<String> } => crate::puzzles::puzzles_sunday_get(paper),
    puzzles_sunday_set { paper: Option<String>, config: deckle_digest::config::PuzzlesConfig } =>
        crate::puzzles::puzzles_sunday_set(paper, config),
    reader_server_set_password { password: String } =>
        crate::reader_server::reader_server_set_password(password),
    reader_server_status {} => Ok(crate::reader_server::reader_server_status()),
    readers_backup { key: String, into: Option<UiPath> } =>
        crate::readers::readers_backup(key, into.map(|p| p.write(ctx)).transpose()?),
    readers_edit { key: String, id: String, edit: crate::commands::BookEdit } =>
        crate::readers::readers_edit(key, id, edit),
    readers_last_backup { key: String } => Ok(crate::readers::readers_last_backup(key)),
    readers_library { key: String } => crate::readers::readers_library(key),
    readers_list {} => Ok(crate::readers::readers_list()),
    readers_remove { key: String, ids: Vec<String> } => crate::readers::readers_remove(key, ids),
    readership_apply { id: String, paper: Option<String> } => readership::readership_apply(id, paper),
    readership_dismiss { id: String, paper: Option<String> } => readership::readership_dismiss(id, paper),
    readership_report { paper: Option<String> } => readership::readership_report(paper),
    readership_set { on: bool } => readership::readership_set(on),
    reading_list {} => Ok(daily::reading_list()),
    reading_remove { id: String } => daily::reading_remove(id),
    reading_save { url: String } => daily::reading_save(url),
    recipe_bookmarklet {} => daily::recipe_bookmarklet(),
    reveal { path: String } => if ctx.web {
        let _ = path;
        Err(NOT_IN_WEB.to_owned())
    } else {
        commands::reveal(path)
    },
    run_books { books: Vec<RunRequest>, target: RunTarget } =>
        commands::run_books(ctx.events.clone(), &ctx.state, ctx.books(books)?, ctx.target(target)?),
    schedule_get {} => if ctx.web {
        deckle_digest::schedule::daemon_status()
    } else {
        Ok(daily::schedule_get())
    },
    schedule_set { at: Option<String>, paper: Option<String> } => if ctx.web {
        web_schedule_set(at, paper)
    } else {
        daily::schedule_set(at, paper)
    },
    set_device_options { key: String, cde_override: Option<String>, verified_real_asin: bool } =>
        commands::set_device_options(&ctx.state, key, cde_override, verified_real_asin),
    set_illustrations_key { key: String } => if ctx.web && key == commands::MASKED_KEY {
        Ok(())
    } else {
        commands::set_illustrations_key(&ctx.state, key)
    },
    set_kindle_paper { sync_id: String, paper: deckle_digest::config::PaperFormat } =>
        commands::set_kindle_paper(sync_id, paper),
    set_settings { settings: Settings } => if ctx.web {
        commands::set_settings_masked(&ctx.state, settings)
    } else {
        commands::set_settings(&ctx.state, settings)
    },
    shelf_forget {} => shelf::shelf_forget(),
    shelf_set { on: bool } => shelf::shelf_set(on),
    shelf_status {} => Ok(shelf::shelf_status()),
    shelves_apply {} => crate::shelves::shelves_apply(),
    shelves_kobo_ok { key: String } => crate::shelves::shelves_kobo_ok(key),
    shelves_set { config: deckle_library::shelves::Config } => crate::shelves::shelves_set(config),
    shelves_status {} => Ok(crate::shelves::shelves_status()),
    start_tab {} => Ok(if ctx.web { "daily" } else { commands::start_tab(&ctx.state) }),
    subscription_forget { site: String } => daily::subscription_forget(site),
    subscription_set_password { site: String, password: String } => if ctx.web {
        let _ = password;
        Err(deckle_digest::secret::no_keychain(&deckle_digest::secret::site_env(&site)))
    } else {
        daily::subscription_set_password(site, password)
    },
    subscription_test { site: String } => daily::subscription_test(site),
    subscriptions_status {} => daily::subscriptions_status(),
    take_preloaded_books {} => Ok(if ctx.web {
        Vec::new()
    } else {
        commands::take_preloaded_books(&ctx.state)
    }),
    take_preloaded_target {} => Ok(if ctx.web {
        None
    } else {
        commands::take_preloaded_target(&ctx.state)
    }),
    wireless_cancel { id: String } => crate::wireless::wireless_cancel(id),
    wireless_queue { reader: String, paths: Vec<UiPath> } =>
        crate::wireless::wireless_queue(reader, read_all(ctx, paths, UiPath::read)?),
    wireless_remove { reader: String, lpaths: Vec<String> } =>
        crate::wireless::wireless_remove(reader, lpaths),
    wireless_set_password { password: Option<String> } =>
        crate::wireless::wireless_set_password(password),
    wireless_status {} => Ok(crate::wireless::wireless_status()),
    words_anki { target: RunTarget, to: UiPath } => kindle::words_anki(ctx.target(target)?, to.write(ctx)?),
}

/// A KOReader plugin's source: a name, a GitHub repository, or a zip or folder on disk, which
/// is checked.
fn plugin_source(ctx: &Ctx, source: String) -> Result<String, String> {
    let t = source.trim();
    if ctx.roots.is_none() || t == deckle_core::koreader_plugins::DECKLE {
        return Ok(source);
    }
    if t.starts_with('/') || t.starts_with('~') || std::path::Path::new(t).is_absolute() {
        return ctx.path(t);
    }
    if std::path::Path::new(t).exists() {
        // Relative to the server's own folder: never meant from a browser.
        return Err(format!("{t}: {}", deckle_core::confine::OUTSIDE));
    }
    Ok(source)
}

/// A puzzle book's picture file, checked.
fn puzzle_request(
    ctx: &Ctx,
    mut r: crate::puzzles::BookRequest,
) -> Result<crate::puzzles::BookRequest, String> {
    if let Some(p) = r.picture.take().filter(|p| !p.trim().is_empty()) {
        r.picture = Some(UiPath(p).read(ctx)?);
    }
    Ok(r)
}

/// Saves the paper's `[schedule] at` (its days kept) for the daemon; nothing is installed.
fn web_schedule_set(
    at: Option<String>,
    paper: Option<String>,
) -> Result<deckle_digest::schedule::Status, String> {
    let main = deckle_digest::config::default_path();
    {
        let _lock = crate::config_write()?;
        deckle_digest::schedule::save_config_schedule(
            &main,
            paper.as_deref(),
            at.as_deref(),
            None,
        )?;
    }
    deckle_digest::schedule::daemon_status()
}
