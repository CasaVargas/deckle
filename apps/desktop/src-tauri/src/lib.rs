// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri shell: the one-window GUI over the pipeline.

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
            // A watcher still on the build this app replaced lacks its new routes (#234).
            std::thread::spawn(|| {
                if let Err(e) = deckle_digest::schedule::refresh_watcher() {
                    eprintln!("watcher: can't restart it on this build: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            wrap::app_version,
            wrap::capabilities,
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
            wrap::cookbook_preview,
            wrap::cookbook_add,
            wrap::cookbook_book,
            wrap::cookbook_update,
            wrap::cookbook_remove,
            wrap::cookbook_save_config,
            wrap::cookbook_send_now,
            wrap::cookbook_preview_book,
            wrap::cookbook_delivery,
            wrap::cookbook_set_kindle,
            wrap::cookbook_illustrate,
            wrap::cookbook_illustrate_stop,
            wrap::daily_state,
            wrap::papers_list,
            wrap::masthead_missing,
            wrap::paper_create,
            wrap::daily_setup,
            wrap::daily_save,
            wrap::press_setup,
            wrap::daily_day_preview,
            wrap::geocode,
            wrap::reading_list,
            wrap::reading_save,
            wrap::reading_remove,
            wrap::bookmarklet,
            wrap::recipe_bookmarklet,
            wrap::edition_preview,
            wrap::daily_run,
            wrap::schedule_get,
            wrap::schedule_set,
            wrap::email_status,
            wrap::email_set_password,
            wrap::reader_server_status,
            wrap::koreader_status,
            wrap::koreader_preview,
            wrap::koreader_install,
            wrap::koreader_update,
            wrap::koreader_remove,
            wrap::koreader_rollback,
            wrap::koreader_set_enabled,
            wrap::koreader_set_patches,
            wrap::koreader_store_list,
            wrap::koreader_store_search,
            wrap::koreader_store_readme,
            wrap::koreader_store_updates,
            wrap::reader_server_set_password,
            wrap::shelves_apply,
            wrap::shelves_kobo_ok,
            wrap::shelves_set,
            wrap::shelves_status,
            wrap::calibre_create_columns,
            wrap::calibre_set_password,
            wrap::calibre_writeback_confirm,
            wrap::calibre_writeback_set,
            wrap::calibre_writeback_status,
            wrap::calibre_writeback_write,
            wrap::wireless_cancel,
            wrap::wireless_queue,
            wrap::wireless_remove,
            wrap::wireless_set_password,
            wrap::wireless_status,
            wrap::email_test,
            wrap::subscriptions_status,
            wrap::subscription_set_password,
            wrap::subscription_test,
            wrap::subscription_forget,
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
