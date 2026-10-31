// SPDX-License-Identifier: GPL-3.0-or-later
//! The Tauri commands over `deckle_app`: one line each, with the names and parameter names the
//! frontend already calls. Commands that were `async` stay so and run the blocking work off the
//! main thread; the rest run where Tauri puts a sync command.

use std::path::PathBuf;
use std::sync::Arc;

use deckle_app::books::{CatalogView, CoverOffer, PreflightView, SentKindle, Suggestions};
use deckle_app::commands::{
    AppState, BookRow, DeviceRow, PlanRow, PreviewerStatus, RunRequest, RunTarget, Settings,
};
use deckle_app::cookbook::{
    BookDto, ConfigDto, DeliveryDto, IllustrateDto, PatchDto, PreviewDto, SavedDto, SendDto,
};
use deckle_app::daily::{DailyState, DayPreview, EmailStatus, PapersView, PlaceRow, Preview};
use deckle_app::kindle::{DeviceShelf, HighlightsStatus, LibraryStatus};
use deckle_app::morgue::MonthView;
use deckle_app::press::{ExpectedExport, PressSetupStatus};
use deckle_app::readership::ReadershipView;
use deckle_app::shelf::ShelfStatus;
use deckle_app::upgrades::{Rebuild, SourcesStatus, UpgradesView};
use deckle_digest::Config;
use deckle_digest::cookbook::store::Picture;
use deckle_digest::next::plan::NextPaper;
use deckle_digest::reading::Entry;
use deckle_digest::schedule::Status;
use deckle_digest::session::{SessionState, SiteStatus};
use deckle_highlights::Outcome;
use deckle_library::care::{BackupReport, RestoreReport};
use deckle_library::details::Details;
use deckle_library::upgrade::PutBack;
use deckle_library::watch::Watched;
use deckle_morgue::detail::Detail;
use deckle_morgue::search::Hit;
use tauri::{AppHandle, Emitter, Manager, State};

/// The desktop window as the place `deckle_app`'s events go.
pub struct TauriEvents(pub AppHandle);

impl deckle_app::events::Events for TauriEvents {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        // As before: a closed window just misses the event.
        let _ = self.0.emit(event, payload);
    }
}

/// The app's own binary, which the commands that run the CLI start.
fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
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
    blocking(move || deckle_app::morgue::morgue_send(date, &current_exe()?)).await
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

// ---- core ---------------------------------------------------------------------------

/// `settings.json` in the app's config folder.
pub fn settings_file(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .map(|d| d.join("settings.json"))
        .unwrap_or_else(|_| PathBuf::from("deckle-settings.json"))
}

/// `devices.json` in the app's config folder.
pub fn devices_file(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .map(|d| d.join("devices.json"))
        .unwrap_or_else(|_| PathBuf::from("deckle-devices.json"))
}

#[tauri::command]
pub fn app_version() -> String {
    deckle_app::commands::app_version()
}

#[tauri::command]
pub fn capabilities(state: State<'_, AppState>) -> deckle_app::capabilities::Capabilities {
    deckle_app::capabilities::capabilities(false, &state)
}

#[tauri::command]
pub fn take_preloaded_books(state: State<'_, AppState>) -> Vec<String> {
    deckle_app::commands::take_preloaded_books(&state)
}

#[tauri::command]
pub fn take_preloaded_target(state: State<'_, AppState>) -> Option<String> {
    deckle_app::commands::take_preloaded_target(&state)
}

#[tauri::command]
pub fn start_tab(state: State<'_, AppState>) -> &'static str {
    deckle_app::commands::start_tab(&state)
}

#[tauri::command]
pub async fn inspect_books(paths: Vec<String>) -> Result<Vec<BookRow>, String> {
    blocking(move || Ok(deckle_app::commands::inspect_books(paths))).await
}

#[tauri::command]
pub async fn bind_books(
    paths: Vec<String>,
    title: String,
    author: Option<String>,
) -> Result<BookRow, String> {
    blocking(move || deckle_app::commands::bind_books(paths, title, author)).await
}

#[tauri::command]
pub async fn import_articles(targets: Vec<String>) -> Result<Vec<BookRow>, String> {
    blocking(move || Ok(deckle_app::commands::import_articles(targets))).await
}

#[tauri::command]
pub fn list_devices(state: State<'_, AppState>) -> Vec<DeviceRow> {
    deckle_app::commands::list_devices(&state)
}

#[tauri::command]
pub fn name_kindle(sync_id: String, name: String) -> Result<(), String> {
    deckle_app::commands::name_kindle(sync_id, name)
}

#[tauri::command]
pub fn set_device_options(
    state: State<'_, AppState>,
    key: String,
    cde_override: Option<String>,
    verified_real_asin: bool,
) -> Result<(), String> {
    deckle_app::commands::set_device_options(&state, key, cde_override, verified_real_asin)
}

#[tauri::command]
pub fn plan_books(
    state: State<'_, AppState>,
    books: Vec<RunRequest>,
    target: RunTarget,
) -> Vec<PlanRow> {
    deckle_app::commands::plan_books(&state, books, target)
}

#[tauri::command]
pub fn previewer_status(state: State<'_, AppState>) -> PreviewerStatus {
    deckle_app::commands::previewer_status(&state)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    deckle_app::commands::get_settings(&state)
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    deckle_app::commands::set_settings(&state, settings)
}

#[tauri::command]
pub fn set_illustrations_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    deckle_app::commands::set_illustrations_key(&state, key)
}

#[tauri::command]
pub fn run_books(
    app: AppHandle,
    state: State<'_, AppState>,
    books: Vec<RunRequest>,
    target: RunTarget,
) -> Result<(), String> {
    deckle_app::commands::run_books(Arc::new(TauriEvents(app)), &state, books, target)
}

#[tauri::command]
pub fn reveal(path: String) -> Result<(), String> {
    deckle_app::commands::reveal(path)
}

// ---- press --------------------------------------------------------------------------

#[tauri::command]
pub fn press_setup(
    paper: Option<String>,
    export_index: usize,
    enabled: bool,
    expected: Option<ExpectedExport>,
) -> Result<PressSetupStatus, String> {
    let exe = std::env::current_exe().map_err(|_| "App executable is unavailable.".to_owned())?;
    deckle_app::press::press_setup(paper, export_index, enabled, expected, &exe)
}

// ---- books --------------------------------------------------------------------------
// The ones that read settings take the app handle rather than `State`, so the state can be
// reached from the blocking thread.

#[tauri::command]
pub async fn library_catalog(device: Option<String>) -> Result<CatalogView, String> {
    blocking(move || Ok(deckle_app::books::library_catalog(device))).await
}

#[tauri::command]
pub async fn library_covers(paths: Vec<String>) -> Result<Vec<(String, Option<String>)>, String> {
    blocking(move || Ok(deckle_app::books::library_covers(paths))).await
}

#[tauri::command]
pub async fn book_preflight(
    app: AppHandle,
    path: String,
    cover: Option<deckle_app::commands::CoverChoice>,
) -> Result<PreflightView, String> {
    blocking(move || deckle_app::books::book_preflight(&app.state::<AppState>(), path, cover)).await
}

#[tauri::command]
pub async fn cover_candidates(
    app: AppHandle,
    title: String,
    author: String,
    isbn: Option<String>,
) -> Result<Vec<CoverOffer>, String> {
    blocking(move || {
        deckle_app::books::cover_candidates(&app.state::<AppState>(), title, author, isbn)
    })
    .await
}

#[tauri::command]
pub async fn cover_fetch(large: String) -> Result<String, String> {
    blocking(move || deckle_app::books::cover_fetch(large)).await
}

#[tauri::command]
pub fn library_fix_details(dir: String, details: Details) -> Result<(), String> {
    deckle_app::books::library_fix_details(dir, details)
}

#[tauri::command]
pub fn library_read_details(dir: String) -> Result<Details, String> {
    deckle_app::books::library_read_details(dir)
}

#[tauri::command]
pub async fn library_suggest_details(
    app: AppHandle,
    dir: String,
    asked: bool,
) -> Result<Option<Suggestions>, String> {
    blocking(move || {
        deckle_app::books::library_suggest_details(&app.state::<AppState>(), dir, asked)
    })
    .await
}

#[tauri::command]
pub fn library_sent_kindles() -> Vec<SentKindle> {
    deckle_app::books::library_sent_kindles()
}

#[tauri::command]
pub fn library_watch_set(dir: Option<String>) -> Result<Option<String>, String> {
    deckle_app::books::library_watch_set(dir)
}

#[tauri::command]
pub async fn library_watch_scan() -> Result<Watched, String> {
    blocking(deckle_app::books::library_watch_scan).await
}

#[tauri::command]
pub async fn library_search(query: String) -> Result<deckle_library::search::Report, String> {
    blocking(move || Ok(deckle_app::books::library_search(query))).await
}

// ---- kindle -------------------------------------------------------------------------

#[tauri::command]
pub fn library_status() -> LibraryStatus {
    deckle_app::kindle::library_status()
}

#[tauri::command]
pub fn library_set(root: Option<String>, auto_add: bool) -> Result<LibraryStatus, String> {
    deckle_app::kindle::library_set(root, auto_add)
}

#[tauri::command]
pub async fn device_books(target: RunTarget) -> Result<DeviceShelf, String> {
    blocking(move || deckle_app::kindle::device_books(target)).await
}

#[tauri::command]
pub fn highlights_status() -> HighlightsStatus {
    deckle_app::kindle::highlights_status()
}

#[tauri::command]
pub fn highlights_set(folder: Option<String>, clippings: bool) -> Result<HighlightsStatus, String> {
    deckle_app::kindle::highlights_set(folder, clippings)
}

#[tauri::command]
pub async fn highlights_write(target: RunTarget) -> Result<Outcome, String> {
    blocking(move || deckle_app::kindle::highlights_write(target)).await
}

#[tauri::command]
pub async fn words_anki(target: RunTarget, to: String) -> Result<usize, String> {
    blocking(move || deckle_app::kindle::words_anki(target, to)).await
}

#[tauri::command]
pub async fn device_remove(target: RunTarget, file: String) -> Result<Vec<String>, String> {
    blocking(move || deckle_app::kindle::device_remove(target, file)).await
}

#[tauri::command]
pub async fn device_remove_many(
    target: RunTarget,
    files: Vec<String>,
) -> Result<Vec<(String, String)>, String> {
    blocking(move || deckle_app::kindle::device_remove_many(target, files)).await
}

#[tauri::command]
pub async fn device_backup(
    target: RunTarget,
    into: Option<String>,
) -> Result<BackupReport, String> {
    blocking(move || deckle_app::kindle::device_backup(target, into)).await
}

#[tauri::command]
pub async fn device_restore(target: RunTarget, dir: String) -> Result<RestoreReport, String> {
    blocking(move || deckle_app::kindle::device_restore(target, dir)).await
}

// ---- upgrades -----------------------------------------------------------------------

#[tauri::command]
pub async fn device_upgrades(target: RunTarget) -> Result<UpgradesView, String> {
    blocking(move || deckle_app::upgrades::device_upgrades(target)).await
}

#[tauri::command]
pub async fn device_rebuild(target: RunTarget, files: Vec<String>) -> Result<Vec<Rebuild>, String> {
    blocking(move || deckle_app::upgrades::device_rebuild(target, files)).await
}

#[tauri::command]
pub async fn device_put_back(target: RunTarget, file: String) -> Result<PutBack, String> {
    blocking(move || deckle_app::upgrades::device_put_back(target, file)).await
}

#[tauri::command]
pub fn library_sources() -> SourcesStatus {
    deckle_app::upgrades::library_sources()
}

#[tauri::command]
pub fn library_sources_set(
    find_in_calibre: bool,
    calibre: Option<String>,
    folders: Vec<String>,
) -> Result<SourcesStatus, String> {
    deckle_app::upgrades::library_sources_set(find_in_calibre, calibre, folders)
}

// ---- daily --------------------------------------------------------------------------

#[tauri::command]
pub fn daily_state(paper: Option<String>) -> DailyState {
    deckle_app::daily::daily_state(paper)
}

#[tauri::command]
pub fn masthead_missing(title: String) -> Vec<String> {
    deckle_app::daily::masthead_missing(title)
}

#[tauri::command]
pub fn papers_list() -> Result<PapersView, String> {
    deckle_app::daily::papers_list()
}

#[tauri::command]
pub fn paper_create(title: String, reader: String) -> Result<String, String> {
    deckle_app::daily::paper_create(title, reader)
}

#[tauri::command]
pub fn daily_setup() -> Result<DailyState, String> {
    deckle_app::daily::daily_setup()
}

#[tauri::command]
pub fn daily_save(config: Config, paper: Option<String>) -> Result<DailyState, String> {
    deckle_app::daily::daily_save(config, paper)
}

#[tauri::command]
pub async fn daily_day_preview(
    day: deckle_digest::config::DayConfig,
) -> Result<DayPreview, String> {
    blocking(move || Ok(deckle_app::daily::daily_day_preview(day))).await
}

#[tauri::command]
pub async fn geocode(query: String) -> Result<Vec<PlaceRow>, String> {
    blocking(move || deckle_app::daily::geocode(query)).await
}

#[tauri::command]
pub fn reading_list() -> Vec<Entry> {
    deckle_app::daily::reading_list()
}

#[tauri::command]
pub fn reading_save(url: String) -> Result<String, String> {
    deckle_app::daily::reading_save(url)
}

#[tauri::command]
pub fn reading_remove(id: String) -> Result<bool, String> {
    deckle_app::daily::reading_remove(id)
}

#[tauri::command]
pub fn bookmarklet() -> Result<String, String> {
    deckle_app::daily::bookmarklet()
}

#[tauri::command]
pub fn recipe_bookmarklet() -> Result<String, String> {
    deckle_app::daily::recipe_bookmarklet()
}

#[tauri::command]
pub fn edition_preview(paper: Option<String>) -> Option<Preview> {
    deckle_app::daily::edition_preview(paper)
}

#[tauri::command]
pub fn daily_run(app: AppHandle, send: bool, paper: Option<String>) -> Result<(), String> {
    let exe = current_exe()?;
    deckle_app::daily::daily_run(Arc::new(TauriEvents(app)), exe, send, paper)
}

#[tauri::command]
pub fn schedule_get() -> Status {
    deckle_app::daily::schedule_get()
}

#[tauri::command]
pub fn schedule_set(at: Option<String>, paper: Option<String>) -> Result<Status, String> {
    deckle_app::daily::schedule_set(at, paper)
}

#[tauri::command]
pub fn email_status(paper: Option<String>) -> EmailStatus {
    deckle_app::daily::email_status(paper)
}

#[tauri::command]
pub fn email_set_password(password: String) -> Result<EmailStatus, String> {
    deckle_app::daily::email_set_password(password)
}

#[tauri::command]
pub fn reader_server_status() -> deckle_app::reader_server::ReaderServerStatus {
    deckle_app::reader_server::reader_server_status()
}

#[tauri::command]
pub fn reader_server_set_password(
    password: String,
) -> Result<deckle_app::reader_server::ReaderServerStatus, String> {
    deckle_app::reader_server::reader_server_set_password(password)
}

#[tauri::command]
pub fn wireless_status() -> deckle_app::wireless::WirelessView {
    deckle_app::wireless::wireless_status()
}

#[tauri::command]
pub fn wireless_set_password(
    password: Option<String>,
) -> Result<deckle_app::wireless::WirelessView, String> {
    deckle_app::wireless::wireless_set_password(password)
}

#[tauri::command]
pub fn wireless_queue(
    reader: String,
    paths: Vec<String>,
) -> Result<deckle_app::wireless::WirelessView, String> {
    deckle_app::wireless::wireless_queue(reader, paths)
}

#[tauri::command]
pub fn wireless_remove(
    reader: String,
    lpaths: Vec<String>,
) -> Result<deckle_app::wireless::WirelessView, String> {
    deckle_app::wireless::wireless_remove(reader, lpaths)
}

#[tauri::command]
pub fn wireless_cancel(id: String) -> Result<deckle_app::wireless::WirelessView, String> {
    deckle_app::wireless::wireless_cancel(id)
}

#[tauri::command]
pub async fn email_test(paper: Option<String>) -> Result<String, String> {
    blocking(move || deckle_app::daily::email_test(paper)).await
}

#[tauri::command]
pub fn subscriptions_status() -> Result<Vec<SiteStatus>, String> {
    deckle_app::daily::subscriptions_status()
}

#[tauri::command]
pub fn subscription_set_password(
    site: String,
    password: String,
) -> Result<Vec<SiteStatus>, String> {
    deckle_app::daily::subscription_set_password(site, password)
}

#[tauri::command]
pub async fn subscription_test(site: String) -> Result<SessionState, String> {
    blocking(move || deckle_app::daily::subscription_test(site)).await
}

#[tauri::command]
pub fn subscription_forget(site: String) -> Result<Vec<SiteStatus>, String> {
    deckle_app::daily::subscription_forget(site)
}

// ---- cookbook -----------------------------------------------------------------------

#[tauri::command]
pub async fn cookbook_preview(target: String) -> Result<PreviewDto, String> {
    blocking(move || deckle_app::cookbook::cookbook_preview(target)).await
}

#[tauri::command]
pub async fn cookbook_add(
    target: String,
    chapter: Option<String>,
    note: Option<String>,
    picture: Option<Picture>,
) -> Result<SavedDto, String> {
    blocking(move || deckle_app::cookbook::cookbook_add(target, chapter, note, picture)).await
}

#[tauri::command]
pub async fn cookbook_book() -> Result<BookDto, String> {
    blocking(deckle_app::cookbook::cookbook_book).await
}

#[tauri::command]
pub fn cookbook_update(id: String, patch: PatchDto) -> Result<SavedDto, String> {
    deckle_app::cookbook::cookbook_update(id, patch)
}

#[tauri::command]
pub fn cookbook_remove(id: String) -> Result<(), String> {
    deckle_app::cookbook::cookbook_remove(id)
}

#[tauri::command]
pub fn cookbook_save_config(cfg: ConfigDto) -> Result<(), String> {
    deckle_app::cookbook::cookbook_save_config(cfg)
}

#[tauri::command]
pub async fn cookbook_send_now() -> Result<SendDto, String> {
    blocking(deckle_app::cookbook::cookbook_send_now).await
}

#[tauri::command]
pub async fn cookbook_preview_book() -> Result<String, String> {
    blocking(deckle_app::cookbook::cookbook_preview_book).await
}

#[tauri::command]
pub fn cookbook_delivery() -> DeliveryDto {
    deckle_app::cookbook::cookbook_delivery()
}

#[tauri::command]
pub fn cookbook_set_kindle(id: String, on: bool) -> Result<(), String> {
    deckle_app::cookbook::cookbook_set_kindle(id, on)
}

/// Reaches the state through the handle, as `book_preflight` does, on the blocking thread.
#[tauri::command]
pub async fn cookbook_illustrate(
    app: AppHandle,
    id: Option<String>,
    all: bool,
    redraw: bool,
) -> Result<IllustrateDto, String> {
    blocking(move || {
        let events = Arc::new(TauriEvents(app.clone()));
        deckle_app::cookbook::cookbook_illustrate(events, &app.state::<AppState>(), id, all, redraw)
    })
    .await
}

#[tauri::command]
pub fn cookbook_illustrate_stop() {
    deckle_app::cookbook::cookbook_illustrate_stop()
}

use deckle_app::koreader::{KoreaderPreview, KoreaderView};

#[tauri::command]
pub async fn koreader_status(folder: Option<String>) -> KoreaderView {
    tauri::async_runtime::spawn_blocking(move || deckle_app::koreader::koreader_status(folder))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn koreader_preview(
    folder: Option<String>,
    source: String,
) -> Result<KoreaderPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        deckle_app::koreader::koreader_preview(folder, source)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn koreader_install(
    folder: Option<String>,
    source: String,
) -> Result<KoreaderView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        deckle_app::koreader::koreader_install(folder, source)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn koreader_update(folder: Option<String>, name: String) -> Result<KoreaderView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        deckle_app::koreader::koreader_update(folder, name)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn koreader_remove(folder: Option<String>, name: String) -> Result<KoreaderView, String> {
    deckle_app::koreader::koreader_remove(folder, name)
}

#[tauri::command]
pub fn koreader_rollback(folder: Option<String>, name: String) -> Result<KoreaderView, String> {
    deckle_app::koreader::koreader_rollback(folder, name)
}

#[tauri::command]
pub fn koreader_set_enabled(
    folder: Option<String>,
    name: String,
    on: bool,
) -> Result<KoreaderView, String> {
    deckle_app::koreader::koreader_set_enabled(folder, name, on)
}

#[tauri::command]
pub fn koreader_set_patches(folder: Option<String>, on: bool) -> Result<KoreaderView, String> {
    deckle_app::koreader::koreader_set_patches(folder, on)
}

#[tauri::command]
pub async fn koreader_store_list() -> deckle_app::koreader::StoreList {
    tauri::async_runtime::spawn_blocking(deckle_app::koreader::koreader_store_list)
        .await
        .expect("the store list never panics")
}

#[tauri::command]
pub async fn koreader_store_search(
    text: String,
    updated: bool,
) -> Result<Vec<deckle_app::koreader::Found>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        deckle_app::koreader::koreader_store_search(text, updated)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn koreader_store_readme(
    repo: String,
) -> Result<deckle_app::koreader::StoreReadme, String> {
    tauri::async_runtime::spawn_blocking(move || deckle_app::koreader::koreader_store_readme(repo))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn koreader_store_updates(
    folder: Option<String>,
) -> Result<Vec<deckle_app::koreader::Update>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        deckle_app::koreader::koreader_store_updates(folder)
    })
    .await
    .map_err(|e| e.to_string())?
}
