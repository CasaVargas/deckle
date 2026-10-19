// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri shell: the one-window GUI over the pipeline.

mod commands;

use std::sync::Mutex;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // EPUBs passed on the command line (or dropped on the app icon) open in the list;
    // `--export-to DIR` converts them straight away.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (preload, preload_target) = commands::parse_cli_args(&args);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let settings_file = commands::settings_file(app.handle());
            app.manage(commands::AppState {
                settings_file,
                preload: Mutex::new(preload.clone()),
                preload_target: Mutex::new(preload_target.clone()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::take_preloaded_books,
            commands::take_preloaded_target,
            commands::inspect_books,
            commands::list_devices,
            commands::previewer_status,
            commands::get_settings,
            commands::set_settings,
            commands::run_books,
            commands::reveal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Deckle");
}
