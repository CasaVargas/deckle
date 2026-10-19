// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri shell. M0: a window with the drop zone and bottom bar; no pipeline wiring yet.

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_version])
        .run(tauri::generate_context!())
        .expect("error while running Deckle");
}
