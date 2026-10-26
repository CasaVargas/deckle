// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri shell: the one-window GUI over the pipeline.

mod books;
mod commands;
mod daily;
mod kindle;
mod morgue;
mod next;
mod readership;
mod shelf;
mod upgrades;

use std::sync::Mutex;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // EPUBs passed on the command line (or dropped on the app icon) open in the list;
    // `--export-to DIR` converts them straight away.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (preload, preload_target) = commands::parse_cli_args(&args);
    let start_tab = if args.iter().any(|a| a == "--daily") {
        "daily"
    } else if args.iter().any(|a| a == "--kindle") {
        "kindle"
    } else {
        "books"
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let settings_file = commands::settings_file(app.handle());
            let devices_file = commands::devices_file(app.handle());
            app.manage(commands::AppState {
                settings_file,
                devices_file,
                preload: Mutex::new(preload.clone()),
                preload_target: Mutex::new(preload_target.clone()),
                start_tab,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::take_preloaded_books,
            commands::take_preloaded_target,
            commands::inspect_books,
            commands::bind_books,
            commands::import_articles,
            commands::list_devices,
            commands::set_device_options,
            commands::name_kindle,
            commands::plan_books,
            commands::previewer_status,
            commands::get_settings,
            commands::set_settings,
            commands::run_books,
            commands::reveal,
            commands::start_tab,
            daily::daily_state,
            daily::papers_list,
            daily::paper_create,
            daily::daily_setup,
            daily::daily_save,
            daily::daily_day_preview,
            daily::geocode,
            daily::reading_list,
            daily::reading_save,
            daily::reading_remove,
            daily::bookmarklet,
            daily::edition_preview,
            daily::daily_run,
            daily::schedule_get,
            daily::schedule_set,
            daily::email_status,
            daily::email_set_password,
            daily::email_test,
            daily::subscriptions_status,
            daily::subscription_set_password,
            daily::subscription_test,
            daily::subscription_forget,
            kindle::library_status,
            books::library_catalog,
            books::library_search,
            books::library_covers,
            books::book_preflight,
            books::cover_candidates,
            books::cover_fetch,
            books::library_fix_details,
            books::library_watch_set,
            books::library_watch_scan,
            kindle::library_set,
            kindle::device_books,
            kindle::device_remove,
            kindle::device_remove_many,
            kindle::device_backup,
            kindle::device_restore,
            upgrades::device_upgrades,
            upgrades::device_put_back,
            upgrades::device_rebuild,
            upgrades::library_sources,
            upgrades::library_sources_set,
            kindle::highlights_status,
            kindle::highlights_set,
            kindle::highlights_write,
            kindle::words_anki,
            readership::readership_report,
            readership::readership_apply,
            readership::readership_dismiss,
            readership::readership_set,
            morgue::morgue_month,
            morgue::morgue_search,
            morgue::morgue_show,
            morgue::morgue_send,
            morgue::morgue_open,
            shelf::shelf_status,
            shelf::shelf_forget,
            shelf::shelf_set,
            next::daily_next,
            next::daily_next_refresh,
            next::press_stop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Deckle");
}
