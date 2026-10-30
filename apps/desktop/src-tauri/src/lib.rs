// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri shell: the one-window GUI over the pipeline.

mod cookbook;
mod daily;
mod wrap;

use std::sync::Mutex;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // EPUBs passed on the command line (or dropped on the app icon) open in the list;
    // `--export-to DIR` converts them straight away.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (preload, preload_target) = deckle_app::commands::parse_cli_args(&args);
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
            let settings_file = wrap::settings_file(app.handle());
            let devices_file = wrap::devices_file(app.handle());
            app.manage(deckle_app::state::AppState {
                settings_file,
                devices_file,
                preload: Mutex::new(preload.clone()),
                preload_target: Mutex::new(preload_target.clone()),
                start_tab,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            wrap::app_version,
            wrap::take_preloaded_books,
            wrap::take_preloaded_target,
            wrap::inspect_books,
            wrap::bind_books,
            wrap::import_articles,
            wrap::list_devices,
            wrap::set_device_options,
            wrap::name_kindle,
            wrap::plan_books,
            wrap::previewer_status,
            wrap::get_settings,
            wrap::set_settings,
            wrap::set_illustrations_key,
            wrap::run_books,
            wrap::reveal,
            wrap::start_tab,
            cookbook::cookbook_preview,
            cookbook::cookbook_add,
            cookbook::cookbook_book,
            cookbook::cookbook_update,
            cookbook::cookbook_remove,
            cookbook::cookbook_save_config,
            cookbook::cookbook_send_now,
            cookbook::cookbook_preview_book,
            cookbook::cookbook_delivery,
            cookbook::cookbook_set_kindle,
            cookbook::cookbook_illustrate,
            cookbook::cookbook_illustrate_stop,
            daily::daily_state,
            daily::papers_list,
            daily::masthead_missing,
            daily::paper_create,
            daily::daily_setup,
            daily::daily_save,
            wrap::press_setup,
            daily::daily_day_preview,
            daily::geocode,
            daily::reading_list,
            daily::reading_save,
            daily::reading_remove,
            daily::bookmarklet,
            daily::recipe_bookmarklet,
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
            wrap::library_status,
            wrap::library_catalog,
            wrap::library_search,
            wrap::library_covers,
            wrap::book_preflight,
            wrap::cover_candidates,
            wrap::cover_fetch,
            wrap::library_fix_details,
            wrap::library_read_details,
            wrap::library_suggest_details,
            wrap::library_sent_kindles,
            wrap::library_watch_set,
            wrap::library_watch_scan,
            wrap::library_set,
            wrap::device_books,
            wrap::device_remove,
            wrap::device_remove_many,
            wrap::device_backup,
            wrap::device_restore,
            wrap::device_upgrades,
            wrap::device_put_back,
            wrap::device_rebuild,
            wrap::library_sources,
            wrap::library_sources_set,
            wrap::highlights_status,
            wrap::highlights_set,
            wrap::highlights_write,
            wrap::words_anki,
            wrap::readership_report,
            wrap::readership_apply,
            wrap::readership_dismiss,
            wrap::readership_set,
            wrap::interests_accept,
            wrap::interests_dismiss,
            wrap::morgue_month,
            wrap::morgue_search,
            wrap::morgue_show,
            wrap::morgue_send,
            wrap::morgue_open,
            wrap::shelf_status,
            wrap::shelf_forget,
            wrap::shelf_set,
            wrap::daily_next,
            wrap::daily_next_refresh,
            wrap::press_stop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Deckle");
}
