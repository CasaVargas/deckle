// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri shell: the one-window GUI over the pipeline.

mod commands;
mod daily;

use std::sync::Mutex;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // EPUBs passed on the command line (or dropped on the app icon) open in the list;
    // `--export-to DIR` converts them straight away.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (preload, preload_target) = commands::parse_cli_args(&args);
    let start_daily = args.iter().any(|a| a == "--daily");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let settings_file = commands::settings_file(app.handle());
            let devices_file = commands::devices_file(app.handle());
            app.manage(commands::AppState {
                settings_file,
                devices_file,
                preload: Mutex::new(preload.clone()),
                preload_target: Mutex::new(preload_target.clone()),
                start_daily,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::take_preloaded_books,
            commands::take_preloaded_target,
            commands::inspect_books,
            commands::list_devices,
            commands::set_device_options,
            commands::plan_books,
            commands::previewer_status,
            commands::get_settings,
            commands::set_settings,
            commands::run_books,
            commands::reveal,
            commands::start_tab,
            daily::daily_state,
            daily::daily_setup,
            daily::daily_save,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Deckle");
}
