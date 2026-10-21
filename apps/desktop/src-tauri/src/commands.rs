// SPDX-License-Identifier: GPL-3.0-or-later
//! Tauri commands behind the GUI. Pure logic lives in plain functions so it can be unit-tested
//! without a running app.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use deckle_core::cover_rule::{Decision, DeviceClass, MatchResult, Override, decide, manual_asin};
use deckle_core::deliver::{Target, detect_mass_storage};
use deckle_core::devices::{
    DeviceProfile, DeviceRegistry, TransportKind, seen_mass_storage, seen_mtp,
};
use deckle_core::inspect::inspect;
use deckle_core::pipeline::{RunOptions, XRayMode, run_with_progress};
use deckle_core::previewer;
use deckle_core::wordwise::{WordWiseOptions, WordWiseSummary};
use deckle_core::xray::{LlmConfig, XRaySummary};
use deckle_kfx::convert::CdeType;
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
    /// `full` | `cover` | `error` (design §5), or `ready`: a finished KFX sent as is.
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
    /// Optional; empty falls back to ANTHROPIC_API_KEY / OPENAI_API_KEY in the environment.
    pub llm_api_key: String,
    pub xray: bool,
    /// Word Wise sidecar on EBOK books (M4); the first use downloads Deckle's database.
    pub word_wise: bool,
    /// 1 (rarest words only) … 5 (every hint; the Kindle's slider filters further).
    pub word_wise_level: u8,
    pub wikipedia: bool,
    pub amazon_match: bool,
    /// Fill empty title, author, series, description and subjects from Open Library (network).
    pub metadata_fill: bool,
    /// Curly quotes, dashes, ellipses and empty-paragraph cleanup (issue #30).
    pub typography: bool,
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
            llm_api_key: String::new(),
            xray: true,
            word_wise: true,
            word_wise_level: 5,
            wikipedia: false,
            amazon_match: false,
            metadata_fill: false,
            typography: false,
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
    /// The book's Amazon ASIN, typed by the owner (design §3).
    #[serde(default)]
    pub asin: Option<String>,
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
    /// What shipped and why (design §4 step 7), on the final event.
    #[serde(default)]
    pub cde: Option<String>,
    #[serde(default)]
    pub asin: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub matched: Option<String>,
    #[serde(default)]
    pub xray: Option<XRaySummary>,
    /// Older copies of the book removed from the Kindle (Kindle-updates design §4).
    #[serde(default)]
    pub replaced: Vec<String>,
    #[serde(default)]
    pub word_wise: Option<WordWiseSummary>,
}

/// One override chain for both the badge planner and the run: book, then device, then the
/// global setting unless it is `auto`.
pub fn effective_override<'a>(
    book: Option<&'a str>,
    device: Option<&'a str>,
    settings: &'a Settings,
) -> Option<&'a str> {
    book.or(device).or(if settings.cde == "auto" {
        None
    } else {
        Some(settings.cde.as_str())
    })
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
    /// Last known firmware (MTP: recorded by the first send).
    pub firmware: Option<String>,
    /// Known issues for this model, transport and firmware (`compat::RULES`).
    pub compat: Vec<String>,
}

fn compat_messages(p: &DeviceProfile) -> Vec<String> {
    deckle_core::compat::check_profile(p)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

/// All connected Kindles, recorded in the device registry.
pub fn device_rows(registry_file: &Path) -> Vec<DeviceRow> {
    let mut reg = DeviceRegistry::load(registry_file);
    let mut rows = Vec::new();
    for d in detect_mass_storage() {
        let p = reg.remember(&seen_mass_storage(&d));
        let compat = compat_messages(&p);
        rows.push(DeviceRow {
            compat,
            firmware: p.firmware,
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
        let compat = compat_messages(&p);
        rows.push(DeviceRow {
            compat,
            firmware: p.firmware,
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

/// `ids`: (book id, per-book cde override, typed ASIN).
pub fn plan_rows(
    ids: &[(String, Option<String>, Option<String>)],
    class: &DeviceClass,
    device_override: Option<&str>,
    verified: bool,
    settings: &Settings,
) -> Vec<PlanRow> {
    ids.iter()
        .map(|(id, book_override, asin)| {
            let user = match effective_override(book_override.as_deref(), device_override, settings)
            {
                Some("ebok") => Override::PreferFeatures,
                Some("pdoc") => Override::PreferCover,
                _ => Override::None,
            };
            // Open Library matching happens at run time; the badge assumes no match yet, but a
            // typed ASIN is known now.
            let matched = match asin.as_deref().map(manual_asin) {
                Some(Ok(asin)) => MatchResult::Candidate { asin, verified },
                _ => MatchResult::None,
            };
            let d: Decision = decide(class, &matched, user);
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
    /// `--daily` / `--kindle`: open on that tab (else `books`).
    pub start_tab: &'static str,
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
        if args[i] == "--send" {
            // "device:" = the first connected Kindle, resolved by the GUI once devices are listed
            target = Some("device:".into());
            i += 1;
            continue;
        }
        if !args[i].starts_with('-') {
            books.push(args[i].clone());
        }
        i += 1;
    }
    (books, target)
}

fn has_extension(p: &Path, ext: &str) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// A finished Kindle book: sent as is, never converted again.
pub fn is_kfx(p: &Path) -> bool {
    has_extension(p, "kfx")
}

fn is_book(p: &Path) -> bool {
    has_extension(p, "epub") || is_kfx(p)
}

/// EPUB and KFX files from a mix of file and folder paths (folders are walked, sorted by name).
pub fn expand_book_paths(paths: &[String]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            if e.is_dir() {
                walk(&e, out);
            } else if is_book(&e) {
                out.push(e);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        let p = PathBuf::from(p);
        if p.is_dir() {
            walk(&p, &mut out);
        } else if is_book(&p) {
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
const COVER_MODE_REASON: &str = "Enhanced Typesetting, cover and page numbers. X-Ray and Word Wise need the book to ship as EBOK.";

pub fn book_row(path: &Path) -> BookRow {
    if is_kfx(path) {
        return kfx_row(path);
    }
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

/// Web articles (addresses, `.webloc`/`.url` link files, saved pages) built into EPUBs in
/// `dir`, one row each (web-article design §3). A failed import is an error row saying why.
pub fn article_rows(targets: &[String], net: &dyn deckle_digest::Net, dir: &Path) -> Vec<BookRow> {
    targets
        .iter()
        .map(|t| match deckle_cli::import_article(t, net, dir) {
            Ok(book) => book_row(&book.path),
            Err(e) => {
                let title = Path::new(t)
                    .file_stem()
                    .filter(|_| !t.starts_with("http"))
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| t.clone());
                BookRow {
                    id: format!("{:x}", md5ish(t.as_bytes())),
                    path: t.clone(),
                    title,
                    authors: String::new(),
                    series: None,
                    cover: None,
                    mode: "error".into(),
                    reason: e.clone(),
                    warnings: Vec::new(),
                    error: Some(e),
                }
            }
        })
        .collect()
}

/// A finished KFX: listed from its own metadata and cover, sent unchanged under its file name.
pub fn kfx_row(path: &Path) -> BookRow {
    let id = format!("{:x}", md5ish(path.to_string_lossy().as_bytes()));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let read = std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| deckle_core::inspect::inspect_kfx(&b));
    match read {
        Ok(k) => {
            let cde = k.cde_type.clone().unwrap_or_else(|| "KFX".into());
            BookRow {
                id,
                path: path.to_string_lossy().into_owned(),
                title: if k.info.title.is_empty() {
                    stem
                } else {
                    k.info.title.clone()
                },
                authors: k
                    .info
                    .creators
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                series: None,
                cover: k.cover.as_ref().map(|jpeg| {
                    format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(jpeg)
                    )
                }),
                mode: "ready".into(),
                reason: format!(
                    "Already a Kindle book ({cde}): sent as is, without conversion. Its mode, X-Ray and Word Wise are what it was built with."
                ),
                warnings: Vec::new(),
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
            reason: format!("not a readable KFX: {e}"),
            warnings: Vec::new(),
            error: Some(format!("not a readable KFX: {e}")),
        },
    }
}

/// Sends a finished KFX unchanged under its own file name; the final progress event.
fn send_kfx(b: &RunRequest, target: &Target) -> ProgressEvent {
    let path = Path::new(&b.path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "book.kfx".into());
    let result = std::fs::read(path)
        .map_err(|e| format!("reading {}: {e}", path.display()))
        .and_then(|bytes| {
            let cde = deckle_core::inspect::inspect_kfx(&bytes)
                .ok()
                .and_then(|k| k.cde_type);
            deckle_core::deliver::deliver_named_on(
                &bytes,
                &name,
                &mut deckle_core::deliver::Conn::new(target),
                &Default::default(),
            )
            .map(|d| (d, cde))
            .map_err(|e| format!("deliver: {e}"))
        });
    let (status, delivered, cde, error) = match result {
        Ok((d, cde)) => ("done", Some(d.to_string_lossy().into_owned()), cde, None),
        Err(e) => ("failed", None, None, Some(e)),
    };
    ProgressEvent {
        id: b.id.clone(),
        status: status.into(),
        stage: None,
        delivered,
        pages: None,
        warnings: Vec::new(),
        error,
        cde,
        asin: None,
        reason: Some("Sent as is (already a Kindle book).".into()),
        matched: None,
        xray: None,
        word_wise: None,
        replaced: Vec::new(),
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

/// The LLM settings as a pipeline config; `off` (or anything unknown) disables it.
pub fn llm_config(settings: &Settings) -> Option<LlmConfig> {
    use deckle_xray::llm::Provider;
    let provider = match settings.llm_provider.as_str() {
        "anthropic" => Provider::Anthropic,
        "openai" => Provider::OpenAi,
        _ => return None,
    };
    Some(LlmConfig {
        provider,
        model: settings.llm_model.trim().to_owned(),
        endpoint: settings.llm_endpoint.trim().to_owned(),
        api_key: settings.llm_api_key.trim().to_owned(),
    })
}

fn cde_from(s: &str) -> CdeType {
    if s.eq_ignore_ascii_case("ebok") {
        CdeType::Ebok
    } else {
        CdeType::Pdoc
    }
}

/// Runs the books one after another, reporting through `emit`.
#[cfg(all(test, unix))]
pub fn run_books_inner(
    books: &[RunRequest],
    target: &RunTarget,
    settings: &Settings,
    emit: &mut dyn FnMut(ProgressEvent),
) {
    run_books_with(books, target, settings, None, false, emit);
}

/// `device_override` / `verified` come from the target's device profile. Returns the firmware
/// a send read off the Kindle, for its profile.
/// The connected Kindle (or folder) a GUI target names.
pub fn resolve_run_target(target: &RunTarget) -> Result<Target, String> {
    match target {
        RunTarget::Folder { path } => Ok(Target::Folder(PathBuf::from(path))),
        RunTarget::Mtp { bus, address } => deckle_transport::usb::detect()
            .into_iter()
            .find(|m| m.bus == *bus && m.address == *address)
            .map(Target::Mtp)
            .ok_or_else(|| "the MTP Kindle is no longer connected".into()),
        RunTarget::Device { root } => detect_mass_storage()
            .into_iter()
            .find(|d| d.root.to_string_lossy() == *root)
            .map(Target::MassStorage)
            .ok_or_else(|| format!("Kindle at {root} is no longer connected")),
    }
}

pub fn run_books_with(
    books: &[RunRequest],
    target: &RunTarget,
    settings: &Settings,
    device_override: Option<&str>,
    verified: bool,
    emit: &mut dyn FnMut(ProgressEvent),
) -> Option<String> {
    let target = match resolve_run_target(target) {
        Ok(t) => t,
        Err(e) => {
            for b in books {
                emit(ProgressEvent {
                    id: b.id.clone(),
                    status: "failed".into(),
                    stage: None,
                    delivered: None,
                    pages: None,
                    warnings: Vec::new(),
                    cde: None,
                    asin: None,
                    reason: None,
                    matched: None,
                    xray: None,
                    word_wise: None,
                    replaced: Vec::new(),
                    error: Some(e.clone()),
                });
            }
            return None;
        }
    };
    let mut firmware = None;
    for b in books {
        if is_kfx(Path::new(&b.path)) {
            emit(ProgressEvent {
                id: b.id.clone(),
                status: "running".into(),
                stage: Some("deliver".into()),
                delivered: None,
                pages: None,
                warnings: Vec::new(),
                error: None,
                cde: None,
                asin: None,
                reason: None,
                matched: None,
                xray: None,
                word_wise: None,
                replaced: Vec::new(),
            });
            emit(send_kfx(b, &target));
            continue;
        }
        let override_str = effective_override(b.cde.as_deref(), device_override, settings);
        let library = deckle_library::config::default_path();
        let opts = RunOptions {
            cde_type: cde_from(override_str.unwrap_or("pdoc")),
            cde_override: override_str.map(cde_from),
            amazon_match: settings.amazon_match,
            metadata_fill: settings.metadata_fill,
            typography: settings.typography,
            verified_real_asin: verified,
            asin: b.asin.clone(),
            pages: if settings.pages_auto {
                Some(None)
            } else {
                None
            },
            target: target.clone(),
            previewer: settings.previewer_path.as_ref().map(PathBuf::from),
            keep_work_dir: false,
            xray: if settings.xray {
                XRayMode::Auto
            } else {
                XRayMode::Off
            },
            llm: llm_config(settings),
            wikipedia: settings.wikipedia,
            word_wise: settings.word_wise.then(|| WordWiseOptions {
                level: settings.word_wise_level.clamp(1, 5),
                ..Default::default()
            }),
            // Kindle-updates design §4: the library's older copy under another name goes.
            replaces: deckle_library::previous_copies(&library, Path::new(&b.path), &target),
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
                cde: None,
                asin: None,
                reason: None,
                matched: None,
                xray: None,
                word_wise: None,
                replaced: Vec::new(),
            })
        });
        if report.firmware.is_some() {
            firmware = report.firmware.clone();
        }
        let mut report = report;
        // Every book sent goes into the library, when there is one (library design §3).
        if let Err(e) = deckle_library::auto_add(&library, Path::new(&b.path), &report, &target) {
            report.warnings.push(deckle_library::auto_add_warning(&e));
        }
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
            cde: report.cde_type,
            asin: report.asin,
            reason: report.decision.map(|d| d.reason),
            matched: report.matched,
            xray: report.xray,
            word_wise: report.word_wise,
            replaced: report.replaced,
        });
    }
    firmware
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
pub fn start_tab(state: State<'_, AppState>) -> &'static str {
    state.start_tab
}

#[tauri::command]
pub fn inspect_books(paths: Vec<String>) -> Vec<BookRow> {
    expand_book_paths(&paths)
        .iter()
        .map(|p| book_row(p))
        .collect()
}

/// Fetches and builds web articles off the main thread; see [`article_rows`].
#[tauri::command]
pub async fn import_articles(targets: Vec<String>) -> Result<Vec<BookRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        article_rows(&targets, &deckle_digest::Http, &deckle_cli::articles_dir())
    })
    .await
    .map_err(|e| e.to_string())
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
    let ids: Vec<(String, Option<String>, Option<String>)> = books
        .iter()
        .map(|b| (b.id.clone(), b.cde.clone(), b.asin.clone()))
        .collect();
    let settings = load_settings_from(&state.settings_file);
    plan_rows(
        &ids,
        &class,
        profile.as_ref().and_then(|p| p.cde_override.as_deref()),
        profile.as_ref().is_some_and(|p| p.verified_real_asin),
        &settings,
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
    let key = profile.map(|p| p.key);
    let devices_file = state.devices_file.clone();
    std::thread::spawn(move || {
        let firmware = run_books_with(
            &books,
            &target,
            &settings,
            device_override.as_deref(),
            verified,
            &mut |e| {
                let _ = app.emit("book-progress", e);
            },
        );
        // An MTP Kindle's firmware is only read by a send; keep it for the device card.
        if let (Some(key), Some(fw)) = (key, firmware) {
            let mut reg = DeviceRegistry::load(&devices_file);
            reg.set_firmware(&key, &fw);
            let _ = reg.save(&devices_file);
        }
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
    use deckle_transport::usb::MtpInfo;

    #[test]
    fn an_mtp_kindle_profile_carries_its_known_issues_and_a_recorded_firmware() {
        let m = MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: Some("G0X1".into()),
            product: Some("Kindle Scribe".into()),
            bus: 1,
            address: 2,
        };
        let mut reg = DeviceRegistry::default();
        let p = reg.remember(&seen_mtp(&m));
        assert_eq!(p.firmware, None, "the device poll never reads MTP firmware");
        assert_eq!(compat_messages(&p).len(), 3);
        reg.set_firmware(&p.key, "5.18.1.0.1");
        let p = reg.remember(&seen_mtp(&m));
        assert_eq!(p.firmware.as_deref(), Some("5.18.1.0.1"));
    }

    #[test]
    fn plan_rows_follow_the_cover_rule_and_overrides() {
        let auto = Settings::default();
        let ids = vec![
            ("a".to_string(), None, None),
            ("b".to_string(), Some("ebok".to_string()), None),
        ];
        let rows = plan_rows(&ids, &DeviceClass::Mtp, None, false, &auto);
        assert_eq!(rows[0].cde, "pdoc");
        assert_eq!(rows[1].cde, "ebok");
        // a typed ASIN with "Prefer features": the Kindle fetches the store cover
        let typed = [(
            "c".to_string(),
            Some("ebok".to_string()),
            Some("b0fxyz1234".to_string()),
        )];
        let rows = plan_rows(&typed, &DeviceClass::Mtp, None, false, &auto);
        assert_eq!(rows[0].cde, "ebok");
        assert!(rows[0].reason.contains("B0FXYZ1234") && !rows[0].reason.contains("grey"));
        let rows = plan_rows(
            &ids,
            &DeviceClass::MassStorage {
                thumbnails_writable: true,
            },
            None,
            false,
            &auto,
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
            &auto,
        );
        assert_eq!(rows[0].cde, "pdoc", "device override applies");
        assert_eq!(rows[1].cde, "ebok", "book override beats device override");
        // the global setting applies when nothing more specific does, exactly like the run
        let global = Settings {
            cde: "pdoc".into(),
            ..Settings::default()
        };
        let rows = plan_rows(
            &ids,
            &DeviceClass::MassStorage {
                thumbnails_writable: true,
            },
            None,
            false,
            &global,
        );
        assert_eq!(rows[0].cde, "pdoc");
        assert_eq!(effective_override(None, None, &global), Some("pdoc"));
        assert_eq!(
            effective_override(None, Some("ebok"), &global),
            Some("ebok")
        );
        assert_eq!(effective_override(None, None, &auto), None);
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
        let (books, target) = parse_cli_args(&["--send".into(), "d.epub".into()]);
        assert_eq!(books, ["d.epub"]);
        assert_eq!(target.as_deref(), Some("device:"));
    }

    #[test]
    fn expand_paths_finds_epubs_recursively_and_ignores_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub/deeper")).unwrap();
        std::fs::write(dir.path().join("a.epub"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/B.EPUB"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/deeper/c.epub"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/notes.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/deeper/d.kfx"), b"x").unwrap();
        let direct = dir.path().join("a.epub");
        let found = expand_book_paths(&[
            dir.path().join("sub").to_string_lossy().into_owned(),
            direct.to_string_lossy().into_owned(),
        ]);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["B.EPUB", "c.epub", "d.kfx", "a.epub"]);
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
    fn a_saved_page_imports_as_a_book_row_and_a_stub_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let para = "The council met again tonight to argue about the new library and its roof. ";
        let page = dir.path().join("essay.html");
        std::fs::write(
            &page,
            format!(
                "<html><head><title>The Roof Question</title></head><body><article>\
                 <h1>The Roof Question</h1>{}</article></body></html>",
                format!("<p>{para}</p>").repeat(40)
            ),
        )
        .unwrap();
        let stub = dir.path().join("paywall.html");
        std::fs::write(&stub, "<html><body><p>Subscribe.</p></body></html>").unwrap();
        let offline = |u: &str| -> Result<Vec<u8>, String> { Err(format!("offline: {u}")) };
        let rows = article_rows(
            &[
                page.to_string_lossy().into_owned(),
                stub.to_string_lossy().into_owned(),
                "https://ex.com/gone".into(),
            ],
            &offline,
            &dir.path().join("articles"),
        );
        assert_eq!(rows[0].title, "The Roof Question");
        assert!(rows[0].error.is_none());
        assert!(rows[0].path.ends_with(".epub"), "{}", rows[0].path);
        assert!(
            rows[0]
                .cover
                .as_deref()
                .unwrap()
                .starts_with("data:image/jpeg")
        );
        assert_eq!(rows[1].mode, "error");
        assert_eq!(rows[1].title, "paywall");
        assert!(
            rows[1].reason.contains("under 120 words"),
            "{}",
            rows[1].reason
        );
        assert_eq!(rows[2].title, "https://ex.com/gone");
        assert!(rows[2].reason.contains("offline"));
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
        assert_eq!(s.word_wise_level, 5);
        assert!(s.word_wise);
        assert!(
            !s.metadata_fill,
            "the Open Library metadata fill is off by default"
        );
        assert!(!s.amazon_match);
        assert!(!s.typography, "the typography pass is off by default");
        let mut changed = s.clone();
        changed.cde = "ebok".into();
        changed.metadata_fill = true;
        changed.typography = true;
        changed.previewer_path = Some("/x/KindlePreviewer4CLI".into());
        save_settings_to(&file, &changed).unwrap();
        assert_eq!(load_settings_from(&file), changed);
    }

    fn golden_kfx() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/mini/deckle-mini-pdoc.golden.kfx")
    }

    #[test]
    fn a_kfx_lists_as_ready_with_its_own_cover() {
        let row = book_row(&golden_kfx());
        assert_eq!(row.mode, "ready");
        assert!(row.error.is_none());
        assert!(!row.title.is_empty());
        assert!(row.reason.contains("PDOC"));
        assert!(row.cover.unwrap().starts_with("data:image/jpeg;base64,"));
        let bad = tempfile::tempdir().unwrap();
        let junk = bad.path().join("junk.kfx");
        std::fs::write(&junk, b"nope").unwrap();
        assert_eq!(book_row(&junk).mode, "error");
    }

    #[test]
    fn a_kfx_is_sent_unchanged_under_its_own_name_without_previewer() {
        let out = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let kfx = src.path().join("deckle-daily-2026-10-20.kfx");
        std::fs::copy(golden_kfx(), &kfx).unwrap();
        let settings = Settings {
            previewer_path: Some("/nonexistent/previewer".into()),
            ..Settings::default()
        };
        let mut events: Vec<ProgressEvent> = Vec::new();
        run_books_with(
            &[RunRequest {
                id: "k".into(),
                path: kfx.to_string_lossy().into_owned(),
                cde: Some("ebok".into()),
                asin: None,
            }],
            &RunTarget::Folder {
                path: out.path().to_string_lossy().into_owned(),
            },
            &settings,
            None,
            false,
            &mut |e| events.push(e),
        );
        let last = events.last().unwrap();
        assert_eq!(last.status, "done", "{:?}", last.error);
        assert_eq!(
            last.cde.as_deref(),
            Some("PDOC"),
            "the KFX's own type, not the override"
        );
        let sent = out.path().join("deckle-daily-2026-10-20.kfx");
        assert_eq!(std::fs::read(sent).unwrap(), std::fs::read(&kfx).unwrap());
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
                asin: None,
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
        assert_eq!(
            last.cde.as_deref(),
            Some("PDOC"),
            "folder target ships PDOC"
        );
        assert!(last.reason.as_deref().unwrap().contains("PDOC"));
        assert!(last.asin.as_deref().unwrap().ends_with("DECK"));
    }
}
