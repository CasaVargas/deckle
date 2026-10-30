// SPDX-License-Identifier: GPL-3.0-or-later
//! The Tauri commands over `deckle_app`: one line each, with the names and parameter names the
//! frontend already calls. Commands that were `async` stay so and run the blocking work off the
//! main thread; the rest run where Tauri puts a sync command.

use deckle_app::morgue::MonthView;
use deckle_app::readership::ReadershipView;
use deckle_app::shelf::ShelfStatus;
use deckle_digest::next::plan::NextPaper;
use deckle_morgue::detail::Detail;
use deckle_morgue::search::Hit;
use tauri::{AppHandle, Emitter};

/// The desktop window as the place `deckle_app`'s events go.
#[expect(dead_code, reason = "the commands that emit move into deckle-app next")]
pub struct TauriEvents(pub AppHandle);

impl deckle_app::events::Events for TauriEvents {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        // As before: a closed window just misses the event.
        let _ = self.0.emit(event, payload);
    }
}

/// Runs the blocking `f` off the main thread, as the async commands always have.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

// ---- morgue -------------------------------------------------------------------------

#[tauri::command]
pub async fn morgue_month(year: i32, month: u32) -> Result<MonthView, String> {
    blocking(move || deckle_app::morgue::morgue_month(year, month)).await
}

#[tauri::command]
pub async fn morgue_search(query: String) -> Result<Vec<Hit>, String> {
    blocking(move || deckle_app::morgue::morgue_search(query)).await
}

#[tauri::command]
pub async fn morgue_show(date: String) -> Result<Option<Detail>, String> {
    blocking(move || deckle_app::morgue::morgue_show(date)).await
}

#[tauri::command]
pub async fn morgue_send(date: String) -> Result<String, String> {
    blocking(move || deckle_app::morgue::morgue_send(date)).await
}

#[tauri::command]
pub fn morgue_open(date: String) -> Result<(), String> {
    deckle_app::morgue::morgue_open(date)
}

// ---- next ---------------------------------------------------------------------------

#[tauri::command]
pub fn daily_next(paper: Option<String>) -> Result<NextPaper, String> {
    deckle_app::next::daily_next(paper)
}

#[tauri::command]
pub async fn daily_next_refresh(paper: Option<String>) -> Result<NextPaper, String> {
    blocking(move || deckle_app::next::daily_next_refresh(paper)).await
}

#[tauri::command]
pub fn press_stop(on: bool, paper: Option<String>) -> Result<NextPaper, String> {
    deckle_app::next::press_stop(on, paper)
}

// ---- readership ---------------------------------------------------------------------

#[tauri::command]
pub fn readership_report(paper: Option<String>) -> Result<ReadershipView, String> {
    deckle_app::readership::readership_report(paper)
}

#[tauri::command]
pub fn readership_apply(id: String, paper: Option<String>) -> Result<ReadershipView, String> {
    deckle_app::readership::readership_apply(id, paper)
}

#[tauri::command]
pub fn readership_dismiss(id: String, paper: Option<String>) -> Result<ReadershipView, String> {
    deckle_app::readership::readership_dismiss(id, paper)
}

#[tauri::command]
pub fn interests_accept(key: String, paper: Option<String>) -> Result<ReadershipView, String> {
    deckle_app::readership::interests_accept(key, paper)
}

#[tauri::command]
pub fn interests_dismiss(key: String, paper: Option<String>) -> Result<ReadershipView, String> {
    deckle_app::readership::interests_dismiss(key, paper)
}

#[tauri::command]
pub fn readership_set(on: bool) -> Result<ReadershipView, String> {
    deckle_app::readership::readership_set(on)
}

// ---- shelf --------------------------------------------------------------------------

#[tauri::command]
pub fn shelf_status() -> ShelfStatus {
    deckle_app::shelf::shelf_status()
}

#[tauri::command]
pub fn shelf_forget() -> Result<ShelfStatus, String> {
    deckle_app::shelf::shelf_forget()
}

#[tauri::command]
pub fn shelf_set(on: bool) -> Result<ShelfStatus, String> {
    deckle_app::shelf::shelf_set(on)
}
