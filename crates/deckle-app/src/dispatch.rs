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
}

const NOT_IN_WEB: &str = "not available in the web app";

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
    bind_books { paths: Vec<String>, title: String, author: Option<String> } =>
        commands::bind_books(paths, title, author),
    book_preflight { path: String, cover: Option<CoverChoice> } =>
        books::book_preflight(&ctx.state, path, cover),
    bookmarklet {} => daily::bookmarklet(),
    calibre_create_columns {} => crate::calibre::calibre_create_columns(),
    calibre_set_password { password: String } => crate::calibre::calibre_set_password(password),
    calibre_writeback_confirm { keys: Vec<String> } => crate::calibre::calibre_writeback_confirm(keys),
    calibre_writeback_set { patch: crate::calibre::WritebackPatch } =>
        crate::calibre::calibre_writeback_set(patch),
    calibre_writeback_status {} => Ok(crate::calibre::calibre_writeback_status()),
    calibre_writeback_write {} => crate::calibre::calibre_writeback_write(),
    capabilities {} => Ok(capabilities::capabilities(ctx.web, &ctx.state)),
    cookbook_add { target: String, chapter: Option<String>, note: Option<String>, picture: Option<Picture> } =>
        cookbook::cookbook_add(target, chapter, note, picture),
    cookbook_book {} => cookbook::cookbook_book(),
    cookbook_delivery {} => Ok(cookbook::cookbook_delivery()),
    cookbook_illustrate { id: Option<String>, all: bool, redraw: bool } =>
        cookbook::cookbook_illustrate(ctx.events.clone(), &ctx.state, id, all, redraw),
    cookbook_illustrate_stop {} => {
        cookbook::cookbook_illustrate_stop();
        Ok(())
    },
    cookbook_preview { target: String } => cookbook::cookbook_preview(target),
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
    daily_save { config: Config, paper: Option<String> } => daily::daily_save(config, paper),
    daily_setup {} => daily::daily_setup(),
    daily_state { paper: Option<String> } => Ok(daily::daily_state(paper)),
    device_backup { target: RunTarget, into: Option<String> } => kindle::device_backup(target, into),
    device_books { target: RunTarget } => kindle::device_books(target),
    device_put_back { target: RunTarget, file: String } => upgrades::device_put_back(target, file),
    device_rebuild { target: RunTarget, files: Vec<String> } => upgrades::device_rebuild(target, files),
    device_remove { target: RunTarget, file: String } => kindle::device_remove(target, file),
    device_remove_many { target: RunTarget, files: Vec<String> } =>
        kindle::device_remove_many(target, files),
    device_restore { target: RunTarget, dir: String } => kindle::device_restore(target, dir),
    device_upgrades { target: RunTarget } => upgrades::device_upgrades(target),
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
    highlights_set { folder: Option<String>, clippings: bool } => kindle::highlights_set(folder, clippings),
    highlights_status {} => Ok(kindle::highlights_status()),
    highlights_write { target: RunTarget } => kindle::highlights_write(target),
    import_articles { targets: Vec<String> } => Ok(commands::import_articles(targets)),
    inspect_books { paths: Vec<String> } => Ok(commands::inspect_books(paths)),
    interests_accept { key: String, paper: Option<String> } => readership::interests_accept(key, paper),
    interests_dismiss { key: String, paper: Option<String> } => readership::interests_dismiss(key, paper),
    koreader_install { folder: Option<String>, source: String } =>
        crate::koreader::koreader_install(folder, source),
    koreader_preview { folder: Option<String>, source: String } =>
        crate::koreader::koreader_preview(folder, source),
    koreader_remove { folder: Option<String>, name: String } =>
        crate::koreader::koreader_remove(folder, name),
    koreader_rollback { folder: Option<String>, name: String } =>
        crate::koreader::koreader_rollback(folder, name),
    koreader_set_enabled { folder: Option<String>, name: String, on: bool } =>
        crate::koreader::koreader_set_enabled(folder, name, on),
    koreader_set_patches { folder: Option<String>, on: bool } =>
        crate::koreader::koreader_set_patches(folder, on),
    koreader_status { folder: Option<String> } => Ok(crate::koreader::koreader_status(folder)),
    koreader_store_list {} => Ok(crate::koreader::koreader_store_list()),
    koreader_store_readme { repo: String } => crate::koreader::koreader_store_readme(repo),
    koreader_store_search { text: String, updated: bool } =>
        crate::koreader::koreader_store_search(text, updated),
    koreader_store_updates { folder: Option<String> } =>
        crate::koreader::koreader_store_updates(folder),
    koreader_update { folder: Option<String>, name: String } =>
        crate::koreader::koreader_update(folder, name),
    library_catalog { device: Option<String> } => Ok(books::library_catalog(device)),
    library_covers { paths: Vec<String> } => Ok(books::library_covers(paths)),
    library_fix_details { dir: String, details: Details } => books::library_fix_details(dir, details),
    library_read_details { dir: String } => books::library_read_details(dir),
    library_search { query: String } => Ok(books::library_search(query)),
    library_sent_kindles {} => Ok(books::library_sent_kindles()),
    library_set { root: Option<String>, auto_add: bool } => kindle::library_set(root, auto_add),
    library_sources {} => Ok(upgrades::library_sources()),
    library_sources_set { find_in_calibre: bool, calibre: Option<String>, folders: Vec<String> } =>
        upgrades::library_sources_set(find_in_calibre, calibre, folders),
    library_status {} => Ok(kindle::library_status()),
    library_suggest_details { dir: String, asked: bool } =>
        books::library_suggest_details(&ctx.state, dir, asked),
    library_watch_scan {} => books::library_watch_scan(),
    library_watch_set { dir: Option<String> } => books::library_watch_set(dir),
    list_devices {} => Ok(commands::list_devices(&ctx.state)),
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
        Ok(commands::plan_books(&ctx.state, books, target)),
    press_setup { paper: Option<String>, export_index: usize, enabled: bool, expected: Option<ExpectedExport> } =>
        press::press_setup(paper, export_index, enabled, expected, &ctx.exe),
    press_stop { on: bool, paper: Option<String> } => next::press_stop(on, paper),
    previewer_status {} => Ok(commands::previewer_status(&ctx.state)),
    reader_server_set_password { password: String } =>
        crate::reader_server::reader_server_set_password(password),
    reader_server_status {} => Ok(crate::reader_server::reader_server_status()),
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
        commands::run_books(ctx.events.clone(), &ctx.state, books, target),
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
    set_settings { settings: Settings } => if ctx.web {
        commands::set_settings_masked(&ctx.state, settings)
    } else {
        commands::set_settings(&ctx.state, settings)
    },
    shelf_forget {} => shelf::shelf_forget(),
    shelf_set { on: bool } => shelf::shelf_set(on),
    shelf_status {} => Ok(shelf::shelf_status()),
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
    wireless_queue { reader: String, paths: Vec<String> } =>
        crate::wireless::wireless_queue(reader, paths),
    wireless_remove { reader: String, lpaths: Vec<String> } =>
        crate::wireless::wireless_remove(reader, lpaths),
    wireless_set_password { password: Option<String> } =>
        crate::wireless::wireless_set_password(password),
    wireless_status {} => Ok(crate::wireless::wireless_status()),
    words_anki { target: RunTarget, to: String } => kindle::words_anki(target, to),
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
