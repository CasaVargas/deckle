// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri shell: the one-window GUI over the pipeline.

mod commands;

use std::sync::Mutex;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // EPUBs passed on the command line (or dropped on the app icon) open in the list.
    let preload: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .collect();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let settings_file = commands::settings_file(app.handle());
            app.manage(commands::AppState {
                settings_file,
                preload: Mutex::new(preload.clone()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::take_preloaded_books,
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
