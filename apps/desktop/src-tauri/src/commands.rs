// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri commands behind the GUI. Pure logic lives in plain functions so it can be unit-tested
//! without a running app.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use deckle_core::cover_rule::{Decision, DeviceClass, MatchResult, Override, decide};
use deckle_core::deliver::{Target, detect_mass_storage};
use deckle_core::devices::{DeviceRegistry, Seen, TransportKind, seen_mass_storage};
use deckle_core::inspect::inspect;
use deckle_core::pipeline::{RunOptions, run_with_progress};
use deckle_core::previewer;
use deckle_kfx::convert::CdeType;
use deckle_transport::usb::MtpInfo;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

/// Everything the book list needs to render one row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookRow {
    pub id: String,
    pub path: String,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    /// `data:image/...;base64,...` of the EPUB's cover, when it has one.
    pub cover: Option<String>,
    /// `full` | `cover` | `error` (design §5).
    pub mode: String,
    pub reason: String,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub previewer_path: Option<String>,
    /// `auto` (cover rule) | `pdoc` | `ebok`
    pub cde: String,
    pub pages_auto: bool,
    /// `off` | `anthropic` | `openai` (M3)
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_endpoint: String,
    pub word_wise_level: u8,
    pub wikipedia: bool,
    pub amazon_match: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            previewer_path: None,
            cde: "auto".into(),
            pages_auto: true,
            llm_provider: "off".into(),
            llm_model: String::new(),
            llm_endpoint: String::new(),
            word_wise_level: 3,
            wikipedia: false,
            amazon_match: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RunTarget {
    Device { root: String },
    Mtp { bus: u8, address: u8 },
    Folder { path: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRequest {
    pub id: String,
    pub path: String,
    /// Per-book override of the settings' cde (`pdoc` | `ebok`).
    pub cde: Option<String>,
}

/// Emitted as `book-progress` once per stage and once at the end of each book.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub id: String,
    /// `running` | `done` | `failed`
    pub status: String,
    pub stage: Option<String>,
    pub delivered: Option<String>,
    pub pages: Option<u32>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreviewerStatus {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub download_url: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceRow {
    /// Profile key (`ms:…` / `mtp:…`), also the value the GUI selects by.
    pub key: String,
    pub kind: TransportKind,
    pub root: Option<String>,
    pub bus: u8,
    pub address: u8,
    pub model_hint: String,
    pub thumbnails_writable: bool,
    pub cde_override: Option<String>,
    pub verified_real_asin: bool,
}

fn seen_mtp(m: &MtpInfo) -> Seen {
    let serial = m
        .serial
        .clone()
        .unwrap_or_else(|| format!("{}:{}", m.bus, m.address));
    Seen {
        key: format!("mtp:{serial}"),
        root: None,
        model_hint: m.product.clone().unwrap_or_else(|| "Kindle (MTP)".into()),
        transport: TransportKind::Mtp,
        thumbnails_writable: false,
        serial: Some(serial),
    }
}

/// All connected Kindles, recorded in the device registry.
pub fn device_rows(registry_file: &Path) -> Vec<DeviceRow> {
    let mut reg = DeviceRegistry::load(registry_file);
    let mut rows = Vec::new();
    for d in detect_mass_storage() {
        let p = reg.remember(&seen_mass_storage(&d));
        rows.push(DeviceRow {
            key: p.key,
            kind: TransportKind::MassStorage,
            root: Some(d.root.to_string_lossy().into_owned()),
            bus: 0,
            address: 0,
            model_hint: p.model_hint,
            thumbnails_writable: p.thumbnails_writable,
            cde_override: p.cde_override,
            verified_real_asin: p.verified_real_asin,
        });
    }
    for m in deckle_transport::usb::detect() {
        let p = reg.remember(&seen_mtp(&m));
        rows.push(DeviceRow {
            key: p.key,
            kind: TransportKind::Mtp,
            root: None,
            bus: m.bus,
            address: m.address,
            model_hint: p.model_hint,
            thumbnails_writable: false,
            cde_override: p.cde_override,
            verified_real_asin: p.verified_real_asin,
        });
    }
    let _ = reg.save(registry_file);
    rows
}

/// What the cover rule will do for a book on a target (for the row badge).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanRow {
    pub id: String,
    pub cde: String,
    pub write_thumbnail: bool,
    pub reason: String,
}

pub fn plan_rows(
    ids: &[(String, Option<String>)],
    class: &DeviceClass,
    device_override: Option<&str>,
    verified: bool,
) -> Vec<PlanRow> {
    ids.iter()
        .map(|(id, book_override)| {
            let user = match book_override.as_deref().or(device_override) {
                Some("ebok") => Override::PreferFeatures,
                Some("pdoc") => Override::PreferCover,
                _ => Override::None,
            };
            // Matching happens at run time; the badge assumes no match yet.
            let _ = verified;
            let d: Decision = decide(class, &MatchResult::None, user);
            PlanRow {
                id: id.clone(),
                cde: d.cde.as_str().to_lowercase(),
                write_thumbnail: d.write_thumbnail,
                reason: d.reason,
            }
        })
        .collect()
}

/// App state: the settings file location and what the command line asked for.
pub struct AppState {
    pub settings_file: PathBuf,
    pub devices_file: PathBuf,
    pub preload: Mutex<Vec<String>>,
    /// `--export-to <dir>`: run the preloaded books into this folder as soon as they load.
    pub preload_target: Mutex<Option<String>>,
}

/// `deckle-desktop [--export-to DIR] [BOOK...]`: books to open and an optional auto-export dir.
pub fn parse_cli_args(args: &[String]) -> (Vec<String>, Option<String>) {
    let mut books = Vec::new();
    let mut target = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--export-to" && i + 1 < args.len() {
            target = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        if !args[i].starts_with('-') {
            books.push(args[i].clone());
        }
        i += 1;
    }
    (books, target)
}

fn is_epub(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
}

/// EPUB files from a mix of file and folder paths (folders are walked, sorted by name).
pub fn expand_epub_paths(paths: &[String]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            if e.is_dir() {
                walk(&e, out);
            } else if is_epub(&e) {
                out.push(e);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        let p = PathBuf::from(p);
        if p.is_dir() {
            walk(&p, &mut out);
        } else if is_epub(&p) {
            out.push(p);
        }
    }
    out
}

fn cover_data_url(epub: &Path, href: &str) -> Option<String> {
    let mut reader = deckle_core::epub::EpubReader::open(epub).ok()?;
    let name = reader.resolve(href);
    let bytes = reader.read_bytes(&name).ok()?;
    let mime = match name
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// The mode badge for a book today: everything ships as a cover-first PDOC until the M3 features
/// exist (design §5 keeps covers non-negotiable).
const COVER_MODE_REASON: &str =
    "Enhanced Typesetting, cover and page numbers. X-Ray and Word Wise arrive with M3.";

pub fn book_row(path: &Path) -> BookRow {
    let id = format!("{:x}", md5ish(path.to_string_lossy().as_bytes()));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match inspect(path) {
        Ok(info) => {
            let authors = info
                .creators
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let series = info.series.as_ref().map(|s| match s.index {
                Some(i) if i.fract() == 0.0 => format!("{} #{}", s.name, i as i64),
                Some(i) => format!("{} #{i}", s.name),
                None => s.name.clone(),
            });
            let cover = info
                .cover_href
                .as_deref()
                .and_then(|h| cover_data_url(path, h));
            BookRow {
                id,
                path: path.to_string_lossy().into_owned(),
                title: info.title,
                authors,
                series,
                cover,
                mode: "cover".into(),
                reason: COVER_MODE_REASON.into(),
                warnings: info.warnings,
                error: None,
            }
        }
        Err(e) => BookRow {
            id,
            path: path.to_string_lossy().into_owned(),
            title: stem,
            authors: String::new(),
            series: None,
            cover: None,
            mode: "error".into(),
            reason: e.to_string(),
            warnings: Vec::new(),
            error: Some(e.to_string()),
        },
    }
}

/// Small stable hash for row ids (not security relevant).
fn md5ish(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn load_settings_from(file: &Path) -> Settings {
    std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_settings_to(file: &Path, s: &Settings) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        file,
        serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn cde_from(s: &str) -> CdeType {
    if s.eq_ignore_ascii_case("ebok") {
        CdeType::Ebok
    } else {
        CdeType::Pdoc
    }
}

/// Runs the books one after another, reporting through `emit`.
#[cfg(test)]
pub fn run_books_inner(
    books: &[RunRequest],
    target: &RunTarget,
    settings: &Settings,
    emit: &mut dyn FnMut(ProgressEvent),
) {
    run_books_with(books, target, settings, None, false, emit)
}

/// `device_override` / `verified` come from the target's device profile.
pub fn run_books_with(
    books: &[RunRequest],
    target: &RunTarget,
    settings: &Settings,
    device_override: Option<&str>,
    verified: bool,
    emit: &mut dyn FnMut(ProgressEvent),
) {
    let target = match target {
        RunTarget::Folder { path } => Target::Folder(PathBuf::from(path)),
        RunTarget::Mtp { bus, address } => match deckle_transport::usb::detect()
            .into_iter()
            .find(|m| m.bus == *bus && m.address == *address)
        {
            Some(m) => Target::Mtp(m),
            None => {
                for b in books {
                    emit(ProgressEvent {
                        id: b.id.clone(),
                        status: "failed".into(),
                        stage: None,
                        delivered: None,
                        pages: None,
                        warnings: Vec::new(),
                        error: Some("the MTP Kindle is no longer connected".into()),
                    });
                }
                return;
            }
        },
        RunTarget::Device { root } => match detect_mass_storage()
            .into_iter()
            .find(|d| d.root.to_string_lossy() == *root)
        {
            Some(d) => Target::MassStorage(d),
            None => {
                for b in books {
                    emit(ProgressEvent {
                        id: b.id.clone(),
                        status: "failed".into(),
                        stage: None,
                        delivered: None,
                        pages: None,
                        warnings: Vec::new(),
                        error: Some(format!("Kindle at {root} is no longer connected")),
                    });
                }
                return;
            }
        },
    };
    for b in books {
        let override_str = b
            .cde
            .as_deref()
            .or(device_override)
            .or(if settings.cde == "auto" {
                None
            } else {
                Some(settings.cde.as_str())
            });
        let opts = RunOptions {
            cde_type: cde_from(override_str.unwrap_or("pdoc")),
            cde_override: override_str.map(cde_from),
            amazon_match: settings.amazon_match,
            verified_real_asin: verified,
            asin: None,
            pages: if settings.pages_auto {
                Some(None)
            } else {
                None
            },
            target: target.clone(),
            previewer: settings.previewer_path.as_ref().map(PathBuf::from),
            keep_work_dir: false,
        };
        let id = b.id.clone();
        let report = run_with_progress(Path::new(&b.path), &opts, &mut |stage| {
            emit(ProgressEvent {
                id: id.clone(),
                status: "running".into(),
                stage: Some(stage.to_owned()),
                delivered: None,
                pages: None,
                warnings: Vec::new(),
                error: None,
            })
        });
        emit(ProgressEvent {
            id: b.id.clone(),
            status: if report.error.is_some() {
                "failed".into()
            } else {
                "done".into()
            },
            stage: None,
            delivered: report.delivered.map(|p| p.to_string_lossy().into_owned()),
            pages: report.pages,
            warnings: report.warnings,
            error: report.error,
        });
    }
}

// ---- Tauri command wrappers -----------------------------------------------------------------

#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[tauri::command]
pub fn take_preloaded_books(state: State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.preload.lock().unwrap())
}

#[tauri::command]
pub fn take_preloaded_target(state: State<'_, AppState>) -> Option<String> {
    state.preload_target.lock().unwrap().take()
}

#[tauri::command]
pub fn inspect_books(paths: Vec<String>) -> Vec<BookRow> {
    expand_epub_paths(&paths)
        .iter()
        .map(|p| book_row(p))
        .collect()
}

#[tauri::command]
pub fn list_devices(state: State<'_, AppState>) -> Vec<DeviceRow> {
    device_rows(&state.devices_file)
}

#[tauri::command]
pub fn set_device_options(
    state: State<'_, AppState>,
    key: String,
    cde_override: Option<String>,
    verified_real_asin: bool,
) -> Result<(), String> {
    let mut reg = DeviceRegistry::load(&state.devices_file);
    reg.set_override(&key, cde_override.as_deref());
    reg.set_verified_real_asin(&key, verified_real_asin);
    reg.save(&state.devices_file)
}

#[tauri::command]
pub fn plan_books(
    state: State<'_, AppState>,
    books: Vec<RunRequest>,
    target: RunTarget,
) -> Vec<PlanRow> {
    let reg = DeviceRegistry::load(&state.devices_file);
    let (class, profile) = match &target {
        RunTarget::Folder { .. } => (DeviceClass::Folder, None),
        RunTarget::Mtp { bus, address } => {
            let p = deckle_transport::usb::detect()
                .into_iter()
                .find(|m| m.bus == *bus && m.address == *address)
                .and_then(|m| reg.get(&seen_mtp(&m).key).cloned());
            (DeviceClass::Mtp, p)
        }
        RunTarget::Device { root } => {
            let d = detect_mass_storage()
                .into_iter()
                .find(|d| d.root.to_string_lossy() == *root);
            let class = DeviceClass::MassStorage {
                thumbnails_writable: d.as_ref().is_some_and(|d| d.thumbnails_writable),
            };
            (
                class,
                d.and_then(|d| reg.get(&seen_mass_storage(&d).key).cloned()),
            )
        }
    };
    let ids: Vec<(String, Option<String>)> = books
        .iter()
        .map(|b| (b.id.clone(), b.cde.clone()))
        .collect();
    plan_rows(
        &ids,
        &class,
        profile.as_ref().and_then(|p| p.cde_override.as_deref()),
        profile.as_ref().is_some_and(|p| p.verified_real_asin),
    )
}

#[tauri::command]
pub fn previewer_status(state: State<'_, AppState>) -> PreviewerStatus {
    let settings = load_settings_from(&state.settings_file);
    match previewer::locate(settings.previewer_path.as_deref().map(Path::new)) {
        Ok(p) => PreviewerStatus {
            found: true,
            path: Some(p.path.to_string_lossy().into_owned()),
            version: Some(p.version),
            download_url: previewer::PREVIEWER_DOWNLOAD_URL,
        },
        Err(_) => PreviewerStatus {
            found: false,
            path: None,
            version: None,
            download_url: previewer::PREVIEWER_DOWNLOAD_URL,
        },
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    load_settings_from(&state.settings_file)
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    save_settings_to(&state.settings_file, &settings)
}

#[tauri::command]
pub fn run_books(
    app: AppHandle,
    state: State<'_, AppState>,
    books: Vec<RunRequest>,
    target: RunTarget,
) -> Result<(), String> {
    let settings = load_settings_from(&state.settings_file);
    let reg = DeviceRegistry::load(&state.devices_file);
    let profile = match &target {
        RunTarget::Device { root } => detect_mass_storage()
            .into_iter()
            .find(|d| d.root.to_string_lossy() == *root)
            .and_then(|d| reg.get(&seen_mass_storage(&d).key).cloned()),
        RunTarget::Mtp { bus, address } => deckle_transport::usb::detect()
            .into_iter()
            .find(|m| m.bus == *bus && m.address == *address)
            .and_then(|m| reg.get(&seen_mtp(&m).key).cloned()),
        RunTarget::Folder { .. } => None,
    };
    let (device_override, verified) = (
        profile.as_ref().and_then(|p| p.cde_override.clone()),
        profile.as_ref().is_some_and(|p| p.verified_real_asin),
    );
    std::thread::spawn(move || {
        run_books_with(
            &books,
            &target,
            &settings,
            device_override.as_deref(),
            verified,
            &mut |e| {
                let _ = app.emit("book-progress", e);
            },
        );
        let _ = app.emit("run-finished", ());
    });
    Ok(())
}

/// Shows a file in Finder / Explorer / the file manager.
#[tauri::command]
pub fn reveal(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer")
        .arg(format!("/select,{path}"))
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let r = std::process::Command::new("xdg-open")
        .arg(Path::new(&path).parent().unwrap_or(Path::new("/")))
        .spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

pub fn settings_file(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .map(|d| d.join("settings.json"))
        .unwrap_or_else(|_| PathBuf::from("deckle-settings.json"))
}

pub fn devices_file(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .map(|d| d.join("devices.json"))
        .unwrap_or_else(|_| PathBuf::from("deckle-devices.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_rows_follow_the_cover_rule_and_overrides() {
        let ids = vec![
            ("a".to_string(), None),
            ("b".to_string(), Some("ebok".to_string())),
        ];
        let rows = plan_rows(&ids, &DeviceClass::Mtp, None, false);
        assert_eq!(rows[0].cde, "pdoc");
        assert_eq!(rows[1].cde, "ebok");
        let rows = plan_rows(
            &ids,
            &DeviceClass::MassStorage {
                thumbnails_writable: true,
            },
            None,
            false,
        );
        assert_eq!(rows[0].cde, "ebok");
        assert!(rows[0].write_thumbnail);
        let rows = plan_rows(
            &ids,
            &DeviceClass::MassStorage {
                thumbnails_writable: true,
            },
            Some("pdoc"),
            false,
        );
        assert_eq!(rows[0].cde, "pdoc", "device override applies");
        assert_eq!(rows[1].cde, "ebok", "book override beats device override");
    }

    #[test]
    fn cli_args_separate_books_from_the_export_target() {
        let (books, target) = parse_cli_args(&[
            "--export-to".into(),
            "/tmp/out".into(),
            "a.epub".into(),
            "b.epub".into(),
        ]);
        assert_eq!(books, ["a.epub", "b.epub"]);
        assert_eq!(target.as_deref(), Some("/tmp/out"));
        let (books, target) = parse_cli_args(&["c.epub".into(), "--verbose".into()]);
        assert_eq!(books, ["c.epub"]);
        assert_eq!(target, None);
    }

    #[test]
    fn expand_paths_finds_epubs_recursively_and_ignores_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub/deeper")).unwrap();
        std::fs::write(dir.path().join("a.epub"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/B.EPUB"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/deeper/c.epub"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/notes.txt"), b"x").unwrap();
        let direct = dir.path().join("a.epub");
        let found = expand_epub_paths(&[
            dir.path().join("sub").to_string_lossy().into_owned(),
            direct.to_string_lossy().into_owned(),
        ]);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["B.EPUB", "c.epub", "a.epub"]);
    }

    #[test]
    fn book_row_carries_cover_data_url_and_cover_mode() {
        let row = book_row(&deckle_kfx::fixtures::path("deckle-mini.epub"));
        assert_eq!(row.title, "Deckle Mini Fixture");
        assert_eq!(row.authors, "Jane Testwright");
        assert_eq!(row.series.as_deref(), Some("Fixture Series #2"));
        assert!(
            row.cover
                .as_deref()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(row.mode, "cover");
        assert!(row.reason.contains("X-Ray"));
        assert!(row.error.is_none());
    }

    #[test]
    fn book_row_reports_drm_as_an_error_row() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("broken.epub");
        std::fs::write(&p, b"not a zip").unwrap();
        let row = book_row(&p);
        assert_eq!(row.mode, "error");
        assert!(row.error.as_deref().unwrap().contains("zip"));
        assert_eq!(row.title, "broken");
    }

    #[test]
    fn settings_round_trip_through_json_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let s = load_settings_from(&file);
        assert_eq!(s.cde, "auto");
        assert!(s.pages_auto);
        assert_eq!(s.llm_provider, "off");
        assert_eq!(s.word_wise_level, 3);
        let mut changed = s.clone();
        changed.cde = "ebok".into();
        changed.previewer_path = Some("/x/KindlePreviewer4CLI".into());
        save_settings_to(&file, &changed).unwrap();
        assert_eq!(load_settings_from(&file), changed);
    }

    #[test]
    #[cfg(unix)]
    fn run_books_emits_stage_events_and_a_final_result_per_book() {
        let out = tempfile::tempdir().unwrap();
        let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../crates/deckle-core/tests/fake_previewer/ok.sh");
        let settings = Settings {
            previewer_path: Some(fake.to_string_lossy().into_owned()),
            pages_auto: false,
            ..Settings::default()
        };
        let mut events: Vec<ProgressEvent> = Vec::new();
        let target = RunTarget::Folder {
            path: out.path().to_string_lossy().into_owned(),
        };
        run_books_inner(
            &[RunRequest {
                id: "b1".into(),
                path: deckle_kfx::fixtures::path("deckle-mini.epub")
                    .to_string_lossy()
                    .into_owned(),
                cde: None,
            }],
            &target,
            &settings,
            &mut |e| events.push(e),
        );
        let stages: Vec<String> = events.iter().filter_map(|e| e.stage.clone()).collect();
        assert_eq!(
            stages,
            ["inspect", "normalize", "convert", "repackage", "deliver"]
        );
        let last = events.last().unwrap();
        assert_eq!(last.id, "b1");
        assert_eq!(last.status, "done");
        assert!(
            last.delivered
                .as_deref()
                .unwrap()
                .ends_with("Deckle Mini Fixture.kfx")
        );
        assert!(last.error.is_none());
    }
}
