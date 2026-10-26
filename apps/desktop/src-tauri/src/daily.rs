// SPDX-License-Identifier: GPL-3.0-or-later
//! The Daily tab (Daily design §5c): the paper's settings, Your Reading, the schedule, and
//! building or sending an edition. Builds run the app's own binary as the CLI in a child
//! process, so the window never blocks and the result is the CLI's JSON report.

use std::path::{Path, PathBuf};

use base64::Engine;
use deckle_digest::Config;
use deckle_digest::chrono::{DateTime, Utc};
use deckle_digest::config::EmailConfig;
use deckle_digest::reading::{Added, Entry, ReadingQueue, State};
use deckle_digest::secret::{self, Keyring, SystemKeyring};
use deckle_digest::zeroize::Zeroizing;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Serialize)]
pub struct DailyState {
    pub config_path: String,
    pub exists: bool,
    pub config: Option<Config>,
    /// Why the config couldn't be read.
    pub error: Option<String>,
}

pub fn state_at(path: &Path) -> DailyState {
    paper_state_at(path, None)
}

/// The Daily tab's state for one paper (`None`: the main paper, `digest.toml`).
pub fn paper_state_at(main: &Path, paper: Option<&str>) -> DailyState {
    let exists = main.exists();
    let file = |slug: &str| match slug {
        deckle_digest::papers::MAIN => main.to_path_buf(),
        s => deckle_digest::papers::paper_path(main, s),
    };
    let slug = paper.unwrap_or(deckle_digest::papers::MAIN);
    let (config, error) = if exists {
        match deckle_digest::papers::load_one(main, slug) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };
    DailyState {
        config_path: file(slug).to_string_lossy().into_owned(),
        exists,
        config,
        error,
    }
}

fn config_path() -> PathBuf {
    deckle_digest::config::default_path()
}

/// One paper's config (`None`: the main paper).
pub fn paper_config_at(main: &Path, paper: Option<&str>) -> Result<Config, String> {
    deckle_digest::papers::load_one(main, paper.unwrap_or(deckle_digest::papers::MAIN))
}

fn paper_config(paper: Option<&str>) -> Result<Config, String> {
    paper_config_at(&config_path(), paper)
}

/// Saves `config` as paper `paper`'s file. The window's copy doesn't carry which paper it is,
/// so that's taken from the file on disk.
pub fn save_paper_at(main: &Path, paper: Option<&str>, config: Config) -> Result<(), String> {
    let old = paper_config_at(main, paper)?;
    let state = old
        .state_file("readership.json")
        .unwrap_or_else(deckle_readership::state_path);
    let learning = deckle_readership::Learning::path_for(&old);
    save_paper_with_history(main, paper, config, &state, &learning)
}

/// Saves a paper's config and, for every interest whose name changed (paired by position with
/// the file's current entries), moves its readership and learning history to the new key
/// (interests design §5.4, call 11).
pub fn save_paper_with_history(
    main: &Path,
    paper: Option<&str>,
    mut config: Config,
    state: &Path,
    learning: &Path,
) -> Result<(), String> {
    let old = paper_config_at(main, paper)?;
    config.paper = old.paper.clone();
    // A rename is a name that changed in place: same count, the new name unknown before, the
    // old one gone after. A removal or a reorder shifts positions and moves no history.
    let key = |s: &str| s.trim().to_lowercase();
    let before: Vec<String> = old.interests.iter().map(|i| key(&i.name)).collect();
    let after: Vec<String> = config.interests.iter().map(|i| key(&i.name)).collect();
    let renamed: Vec<(String, String)> = if before.len() == after.len() {
        old.interests
            .iter()
            .zip(&config.interests)
            .filter(|(a, b)| {
                let (from, to) = (key(&a.name), key(&b.name));
                from != to && !to.is_empty() && !before.contains(&to) && !after.contains(&from)
            })
            .map(|(a, b)| (a.name.trim().to_owned(), b.name.trim().to_owned()))
            .collect()
    } else {
        Vec::new()
    };
    deckle_digest::papers::save(main, &config)?;
    if renamed.is_empty() {
        return Ok(());
    }
    let mut st = deckle_readership::State::load(state);
    let mut lr = deckle_readership::Learning::load(learning);
    let (mut moved_state, mut moved_learning) = (0, false);
    for (from, to) in &renamed {
        moved_state +=
            st.rename_section_key(&format!("interests:{from}"), &format!("interests:{to}"));
        moved_learning |= lr.rename(&from.to_lowercase(), &to.to_lowercase());
    }
    if moved_state > 0 {
        st.save(state)?;
    }
    if moved_learning {
        lr.save(learning)?;
    }
    Ok(())
}

/// A card in Your papers (many papers §7).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PaperCard {
    pub slug: String,
    pub title: String,
    pub reader: String,
    /// Section titles, in order.
    pub sections: Vec<String>,
    /// `kindle =`: a Kindle's name or sync id; empty for any Kindle.
    pub kindle: String,
    /// The Kindle address, when the paper is emailed.
    pub email_to: Option<String>,
    /// `[email] usb_too`.
    pub usb_too: bool,
    /// `[schedule] at`.
    pub at: Option<String>,
    pub days: Vec<deckle_digest::config::Day>,
    pub large_print: bool,
    /// Its edition job is installed.
    pub installed: bool,
}

/// Every paper's card, main first, then by title; and notes on paper files that were skipped.
pub fn cards_at(
    main: &Path,
    status: &deckle_digest::schedule::Status,
) -> Result<(Vec<PaperCard>, Vec<String>), String> {
    let loaded = deckle_digest::papers::load_all(main)?;
    let mut cards: Vec<PaperCard> = loaded
        .papers
        .iter()
        .map(|c| {
            let slug = c.slug().to_owned();
            PaperCard {
                installed: status.papers.iter().any(|p| p.slug == slug),
                sections: section_names(c),
                email_to: (c.email.enabled && !c.email.to.trim().is_empty())
                    .then(|| c.email.to.trim().to_owned()),
                usb_too: c.email.usb_too,
                at: c.schedule.at.clone(),
                days: c.schedule.days.clone(),
                large_print: c.large_print,
                kindle: c.kindle.trim().to_owned(),
                reader: c.reader.clone(),
                title: c.title.clone(),
                slug,
            }
        })
        .collect();
    cards[1..].sort_by_key(|c| c.title.to_lowercase());
    Ok((cards, loaded.notes))
}

/// A paper's section names in order, as its page plan names them: each feed and each other
/// place is a section of its own.
pub fn section_names(cfg: &Config) -> Vec<String> {
    use deckle_digest::config::SectionKind as K;
    let mut out = Vec::new();
    for kind in &cfg.sections {
        match kind {
            K::Reading => out.push("Your Reading".into()),
            K::Weather => out.push("Weather".into()),
            K::Local => {
                out.push("Local".into());
                out.extend(cfg.local.places.iter().map(|p| p.name.clone()));
            }
            K::Interests => out.extend(cfg.interests.iter().map(|i| i.name.clone())),
            K::Feeds => out.extend(cfg.feeds.iter().map(|f| f.name.clone())),
            K::Lottery => out.push("Lottery".into()),
            K::Horoscope => out.push("Horoscope".into()),
            K::Puzzle => out.push("Puzzle".into()),
            K::Shelf => out.push("From Your Shelf".into()),
            K::Onthisday => out.push("On This Day".into()),
        }
    }
    out.retain(|n| !n.trim().is_empty());
    out
}

fn queue_for(cfg: Option<&Config>) -> ReadingQueue {
    ReadingQueue::new(
        cfg.and_then(|c| c.reading.queue_path.clone())
            .unwrap_or_else(ReadingQueue::default_path),
    )
}

/// The queue as the tab lists it: pending first, then the last 7 days, newest first.
pub fn reading_rows(queue: &ReadingQueue, now: DateTime<Utc>) -> Vec<Entry> {
    let week_ago = now - deckle_digest::chrono::Duration::days(7);
    let mut rows: Vec<Entry> = queue
        .load()
        .into_iter()
        .filter(|e| e.state == State::Pending || e.saved_at > week_ago)
        .collect();
    rows.sort_by_key(|e| (e.state != State::Pending, std::cmp::Reverse(e.saved_at)));
    rows
}

#[derive(Clone, Debug, Serialize)]
pub struct Preview {
    pub title: String,
    pub file: String,
    /// `data:image/jpeg;base64,…` of the edition's cover.
    pub cover: Option<String>,
    /// Editions waiting in the outbox for a Kindle.
    pub outbox: usize,
}

/// The newest `<prefix>YYYY-MM-DD.epub` in `dir`: one paper's, never another's.
pub fn newest_edition_of(dir: &Path, prefix: &str) -> Option<PathBuf> {
    let mut eds: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                deckle_digest::is_edition_name(&n, prefix) && n.ends_with(".epub")
            })
        })
        .collect();
    eds.sort();
    eds.pop()
}

pub fn preview_of(epub: &Path, outbox: usize) -> Preview {
    let info = deckle_core::inspect::inspect(epub).ok();
    let cover = deckle_core::epub::EpubReader::open(epub)
        .ok()
        .and_then(|mut r| r.read_bytes("OEBPS/cover.jpg").ok())
        .map(|b| {
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(b)
            )
        });
    Preview {
        title: info.map(|i| i.title).unwrap_or_else(|| {
            epub.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into()
        }),
        file: epub.to_string_lossy().into_owned(),
        cover,
        outbox,
    }
}

fn digest_cache() -> PathBuf {
    deckle_core::wordwise::default_cache_dir().join("digest")
}

#[derive(Clone, Debug, Serialize)]
pub struct RunResult {
    pub ok: bool,
    /// The CLI's JSON report (`digest build` / `digest run`), when it printed one.
    pub report: Option<serde_json::Value>,
    /// The last lines of stderr (inbox notes, errors).
    pub log: String,
}

/// The arguments for the app's own binary acting as the CLI, for paper `paper` (`None`: main).
pub fn run_args(send: bool, preview_dir: &Path, paper: Option<&str>) -> Vec<String> {
    let mut a = vec!["digest".to_owned(), "--json".to_owned()];
    if let Some(slug) = paper.filter(|s| *s != deckle_digest::papers::MAIN) {
        a.extend(["--paper".into(), slug.to_owned()]);
    }
    if send {
        // Send now is an explicit ask: it runs past a stopped press
        a.extend([
            "run".into(),
            "--to".into(),
            "device".into(),
            "--force".into(),
        ]);
    } else {
        a.extend([
            "build".into(),
            "-o".into(),
            preview_dir.to_string_lossy().into_owned(),
        ]);
    }
    a
}

pub fn parse_run(status_ok: bool, stdout: &[u8], stderr: &[u8]) -> RunResult {
    let report = serde_json::from_slice(stdout).ok();
    let err = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
    RunResult {
        ok: status_ok,
        report,
        log: lines[lines.len().saturating_sub(6)..].join("\n"),
    }
}

/// The "Email to Kindle" card's state (email design §8): whether a password is there, never
/// the password.
#[derive(Clone, Debug, Serialize)]
pub struct EmailStatus {
    /// `[email]` has every address and the host it needs.
    pub configured: bool,
    pub has_password: bool,
}

pub fn email_status_with(
    cfg: &EmailConfig,
    keys: &dyn Keyring,
    env: &dyn Fn(&str) -> Option<String>,
) -> EmailStatus {
    EmailStatus {
        configured: cfg.missing().is_empty() && cfg.validate().is_ok(),
        has_password: secret::has_password(cfg, keys, env),
    }
}

/// Saves the password for `cfg`'s account; the plain copy is zeroed when it's dropped here.
pub fn set_password_with(
    cfg: &EmailConfig,
    keys: &dyn Keyring,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    secret::set(cfg, keys, &password).map_err(|e| e.to_string())
}

/// A paper's email settings: its own Kindle address, the household's account.
fn email_config(paper: Option<&str>) -> Result<EmailConfig, String> {
    Ok(paper_config(paper)?.email)
}

fn env_var(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

// ---- Tauri commands -----------------------------------------------------------------

#[tauri::command]
pub fn daily_state(paper: Option<String>) -> DailyState {
    paper_state_at(&config_path(), paper.as_deref())
}

#[derive(Clone, Debug, Serialize)]
pub struct PapersView {
    pub papers: Vec<PaperCard>,
    /// Paper files that were skipped, and why.
    pub notes: Vec<String>,
    /// The household's named Kindles, for a paper's Kindle picker.
    pub kindles: Vec<KnownKindle>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KnownKindle {
    pub sync_id: String,
    pub name: String,
}

pub fn papers_at(
    main: &Path,
    status: &deckle_digest::schedule::Status,
) -> Result<PapersView, String> {
    let (papers, notes) = cards_at(main, status)?;
    let kindles = Config::load(main)?
        .sync
        .named()
        .map(|(id, name)| KnownKindle {
            sync_id: id.to_owned(),
            name: name.to_owned(),
        })
        .collect();
    Ok(PapersView {
        papers,
        notes,
        kindles,
    })
}

#[tauri::command]
pub fn papers_list() -> Result<PapersView, String> {
    papers_at(&config_path(), &deckle_digest::schedule::status())
}

/// Writes `papers/<slug>.toml` for a new paper; returns its slug.
pub fn create_at(main: &Path, title: &str, reader: &str) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Give the paper a name.".into());
    }
    let slug = deckle_digest::papers::slug_from_title(title);
    if !deckle_digest::papers::valid_slug(&slug) {
        return Err(format!(
            "\"{title}\" doesn't make a file name; start it with a letter"
        ));
    }
    deckle_digest::papers::create(main, &slug, title, reader.trim())?;
    Ok(slug)
}

#[tauri::command]
pub fn paper_create(title: String, reader: String) -> Result<String, String> {
    create_at(&config_path(), &title, &reader)
}

/// First run: write the starter config.
#[tauri::command]
pub fn daily_setup() -> Result<DailyState, String> {
    let path = config_path();
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, deckle_digest::config::STARTER).map_err(|e| e.to_string())?;
    }
    Ok(state_at(&path))
}

#[tauri::command]
pub fn daily_save(config: Config, paper: Option<String>) -> Result<DailyState, String> {
    let path = config_path();
    save_paper_at(&path, paper.as_deref(), config)?;
    Ok(paper_state_at(&path, paper.as_deref()))
}

#[derive(Clone, Debug, Serialize)]
pub struct PlaceRow {
    pub label: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// What the Daily tab's Your Day card shows: today's count, each calendar as masked for
/// display, and the calendars that couldn't be read (almanac design §5).
#[derive(Clone, Debug, Serialize)]
pub struct DayPreview {
    pub events: usize,
    pub labels: Vec<String>,
    pub errors: Vec<String>,
}

pub fn day_preview_with(
    day: &deckle_digest::config::DayConfig,
    date: deckle_digest::chrono::NaiveDate,
    tz: deckle_digest::chrono_tz::Tz,
    net: &dyn deckle_digest::fetch::Net,
) -> DayPreview {
    let agenda = deckle_digest::day::agenda(day, date, tz, net);
    DayPreview {
        events: agenda.entries.len(),
        labels: day
            .calendars
            .iter()
            .map(|c| deckle_digest::day::mask(c))
            .collect(),
        errors: agenda.errors,
    }
}

#[tauri::command]
pub async fn daily_day_preview(
    day: deckle_digest::config::DayConfig,
) -> Result<DayPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let tz = deckle_digest::tz::local_tz();
        let date = Utc::now().with_timezone(&tz).date_naive();
        day_preview_with(&day, date, tz, &deckle_digest::Http)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn geocode(query: String) -> Result<Vec<PlaceRow>, String> {
    let places = tauri::async_runtime::spawn_blocking(move || {
        deckle_digest::geocode::search(&query, &deckle_digest::Http)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(places
        .into_iter()
        .map(|p| PlaceRow {
            label: p.label(),
            latitude: p.latitude,
            longitude: p.longitude,
        })
        .collect())
}

#[tauri::command]
pub fn reading_list() -> Vec<Entry> {
    let cfg = Config::load(&config_path()).ok();
    reading_rows(&queue_for(cfg.as_ref()), deckle_digest::chrono::Utc::now())
}

#[tauri::command]
pub fn reading_save(url: String) -> Result<String, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("Paste a web address (https://…)".into());
    }
    let cfg = Config::load(&config_path()).ok();
    match queue_for(cfg.as_ref()).add_url(url, None, "app", deckle_digest::chrono::Utc::now())? {
        Added::New(_) => Ok("Saved for tomorrow's paper".into()),
        Added::Duplicate(_) => Ok("Already saved".into()),
    }
}

#[tauri::command]
pub fn reading_remove(id: String) -> Result<bool, String> {
    let cfg = Config::load(&config_path()).ok();
    queue_for(cfg.as_ref()).remove(&id)
}

#[tauri::command]
pub fn bookmarklet() -> Result<String, String> {
    let cfg = Config::load(&config_path()).ok();
    let q = queue_for(cfg.as_ref());
    let dir = q.path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let token = deckle_digest::bookmarklet::token(&dir).map_err(|e| e.to_string())?;
    Ok(deckle_digest::bookmarklet::javascript(&token))
}

#[tauri::command]
pub fn edition_preview(paper: Option<String>) -> Option<Preview> {
    let outbox = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir())
        .pending()
        .len();
    let cfg = paper_config(paper.as_deref()).ok()?;
    let prefix = deckle_digest::edition_prefix(&cfg);
    let cache = deckle_core::wordwise::default_cache_dir();
    let dirs = [digest_cache().join("preview"), cfg.work_dir(&cache)];
    let newest = dirs
        .iter()
        .filter_map(|d| newest_edition_of(d, &prefix))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())?;
    Some(preview_of(&newest, outbox))
}

/// Builds (preview) or runs (send) the edition in a child process; emits `daily-finished`
/// with a [`RunResult`].
#[tauri::command]
pub fn daily_run(app: AppHandle, send: bool, paper: Option<String>) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let preview = digest_cache().join("preview");
    std::fs::create_dir_all(&preview).map_err(|e| e.to_string())?;
    let args = run_args(send, &preview, paper.as_deref());
    std::thread::spawn(move || {
        let result = match std::process::Command::new(&exe).args(&args).output() {
            Ok(out) => parse_run(out.status.success(), &out.stdout, &out.stderr),
            Err(e) => RunResult {
                ok: false,
                report: None,
                log: format!("couldn't start {}: {e}", exe.display()),
            },
        };
        let _ = app.emit("daily-finished", result);
    });
    Ok(())
}

#[tauri::command]
pub fn schedule_get() -> deckle_digest::schedule::Status {
    deckle_digest::schedule::status()
}

/// Writes paper `paper`'s `[schedule] at` (`None` clears it), as `digest schedule` does.
pub fn set_at_in(main: &Path, paper: Option<&str>, at: Option<&str>) -> Result<Config, String> {
    let mut cfg = paper_config_at(main, paper)?;
    if let Some(at) = at {
        deckle_digest::schedule::parse_at(at)?;
    }
    let at = at.map(str::to_owned);
    if cfg.schedule.at != at {
        cfg.schedule.at = at;
        deckle_digest::papers::save(main, &cfg)?;
    }
    Ok(cfg)
}

/// `Some("05:30")` saves the paper's time and installs (or moves) its job with the app's own
/// binary, on its days; `None` removes that paper's job. Other papers' jobs, and the watcher
/// they need, stay.
#[tauri::command]
pub fn schedule_set(
    at: Option<String>,
    paper: Option<String>,
) -> Result<deckle_digest::schedule::Status, String> {
    use deckle_digest::schedule;
    let cfg = set_at_in(&config_path(), paper.as_deref(), at.as_deref())?;
    match at {
        None => {
            schedule::remove_paper(cfg.slug())?;
        }
        Some(at) => {
            let (hour, minute) = schedule::parse_at(&at)?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let mut days = cfg.schedule.days.clone();
            days.sort();
            schedule::install(&schedule::Schedule {
                hour,
                minute,
                days,
                slug: cfg.slug().to_owned(),
                exe: exe.canonicalize().unwrap_or(exe),
                config: None,
            })?;
        }
    }
    Ok(schedule::status())
}

#[tauri::command]
pub fn email_status(paper: Option<String>) -> EmailStatus {
    let cfg = email_config(paper.as_deref()).unwrap_or_default();
    email_status_with(&cfg, &SystemKeyring, &env_var)
}

/// Writes the keychain for the saved `[email]` account. Nothing reads it back to the window.
#[tauri::command]
pub fn email_set_password(password: String) -> Result<EmailStatus, String> {
    // The account is the household's: the main paper's.
    let cfg = email_config(None)?;
    set_password_with(&cfg, &SystemKeyring, password)?;
    Ok(email_status_with(&cfg, &SystemKeyring, &env_var))
}

/// `deckle digest email test` from the card: a one-page EPUB to the Kindle address.
#[tauri::command]
pub async fn email_test(paper: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = email_config(paper.as_deref())?;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        deckle_digest::mail::send_test(
            &cfg,
            dir.path(),
            deckle_digest::chrono::Local::now().date_naive(),
            &SystemKeyring,
            &env_var,
            |pw| deckle_digest::mail::SmtpMailer::new(&cfg, pw),
        )
        .map_err(|e| e.to_string())?;
        Ok(format!(
            "Test sent to {}. It should reach your Kindle within a few minutes.",
            cfg.to.trim()
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_a_renamed_interest_migrates_its_history() {
        use deckle_readership::layout::{Layout, SectionStart};
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("digest.toml");
        std::fs::write(
            &main,
            "sections = [\"interests\"]\n[[interests]]\nname = \"Rust\"\n",
        )
        .unwrap();
        let mut st = deckle_readership::State::default();
        st.record_layout(Layout {
            date: "2026-10-21".parse().unwrap(),
            title: "T".into(),
            file: "deckle-daily-2026-10-21".into(),
            end: 100,
            sections: vec![
                SectionStart {
                    key: "front".into(),
                    name: "Front page".into(),
                    start: 0,
                    items: None,
                    stories: Vec::new(),
                },
                SectionStart {
                    key: "interests:Rust".into(),
                    name: "Rust".into(),
                    start: 10,
                    items: None,
                    stories: Vec::new(),
                },
            ],
        });
        let state = main.with_file_name("readership.json");
        st.save(&state).unwrap();
        let mut learning = deckle_readership::Learning::default();
        learning
            .accepted
            .insert("rust".into(), "2026-10-21".parse().unwrap());
        let interests = main.with_file_name("interests.json");
        learning.save(&interests).unwrap();
        let mut cfg = Config::load(&main).unwrap();
        cfg.interests[0].name = "Rust lang".into();
        save_paper_with_history(&main, None, cfg, &state, &interests).unwrap();
        let st = deckle_readership::State::load(&state);
        let keys: Vec<String> = st
            .editions
            .values()
            .flat_map(|l| l.sections.iter().map(|s| s.key.clone()))
            .collect();
        assert!(keys.contains(&"interests:Rust lang".to_owned()), "{keys:?}");
        assert!(!keys.contains(&"interests:Rust".to_owned()));
        let learning = deckle_readership::Learning::load(&interests);
        assert!(
            learning.accepted.contains_key("rust lang"),
            "{:?}",
            learning.accepted
        );
        // a rename by case only saves without a duplicate error
        let mut cfg = Config::load(&main).unwrap();
        cfg.interests[0].name = "RUST LANG".into();
        save_paper_with_history(&main, None, cfg, &state, &interests).unwrap();
        assert!(
            std::fs::read_to_string(&main)
                .unwrap()
                .contains("RUST LANG")
        );
    }

    #[test]
    fn removing_or_reordering_interests_moves_no_history() {
        use deckle_readership::layout::{Layout, SectionStart};
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("digest.toml");
        std::fs::write(
            &main,
            "sections = [\"interests\"]\n[[interests]]\nname = \"A\"\n[[interests]]\nname = \"B\"\n[[interests]]\nname = \"C\"\n",
        )
        .unwrap();
        let sec = |key: &str| SectionStart {
            key: key.into(),
            name: key.into(),
            start: 0,
            items: None,
            stories: Vec::new(),
        };
        let mut st = deckle_readership::State::default();
        st.record_layout(Layout {
            date: "2026-10-21".parse().unwrap(),
            title: "T".into(),
            file: "deckle-daily-2026-10-21".into(),
            end: 100,
            sections: vec![
                sec("front"),
                sec("interests:A"),
                sec("interests:B"),
                sec("interests:C"),
            ],
        });
        let state = main.with_file_name("readership.json");
        st.save(&state).unwrap();
        let mut learning = deckle_readership::Learning::default();
        learning
            .accepted
            .insert("b".into(), "2026-10-21".parse().unwrap());
        learning
            .accepted
            .insert("c".into(), "2026-10-22".parse().unwrap());
        let interests = main.with_file_name("interests.json");
        learning.save(&interests).unwrap();
        let keys = |state: &std::path::Path| -> Vec<String> {
            deckle_readership::State::load(state)
                .editions
                .values()
                .flat_map(|l| l.sections.iter().map(|s| s.key.clone()))
                .collect()
        };
        // remove B from the middle: [A, B, C] → [A, C]
        let mut cfg = Config::load(&main).unwrap();
        cfg.interests.remove(1);
        save_paper_with_history(&main, None, cfg, &state, &interests).unwrap();
        assert_eq!(
            keys(&state),
            ["front", "interests:A", "interests:B", "interests:C"]
        );
        let l = deckle_readership::Learning::load(&interests);
        assert!(l.accepted.contains_key("b") && l.accepted.contains_key("c"));
        // swap: [A, C] → [C, A]
        let mut cfg = Config::load(&main).unwrap();
        cfg.interests.swap(0, 1);
        save_paper_with_history(&main, None, cfg, &state, &interests).unwrap();
        assert_eq!(
            keys(&state),
            ["front", "interests:A", "interests:B", "interests:C"]
        );
    }

    #[test]
    fn section_names_list_each_interest_like_each_feed() {
        let cfg = Config::parse(
            "sections = [\"local\", \"interests\", \"feeds\"]\n[[interests]]\nname = \"Rust\"\n\
             [[interests]]\nname = \"Carolina Panthers\"\n[[feeds]]\nname = \"HN\"\nurl = \"https://x/f\"\n",
        )
        .unwrap();
        assert_eq!(
            section_names(&cfg),
            ["Local", "Rust", "Carolina Panthers", "HN"]
        );
    }

    #[test]
    fn day_preview_counts_events_and_masks_addresses() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Home.ics");
        std::fs::write(
            &file,
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261022T090000\r\n\
             SUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        )
        .unwrap();
        let day = deckle_digest::config::DayConfig {
            calendars: vec![
                file.display().to_string(),
                "webcal://cal.example/s3cr3t/x.ics".into(),
            ],
            ..Default::default()
        };
        let net = |_: &str| -> Result<Vec<u8>, String> { Err("offline".into()) };
        let tz: deckle_digest::chrono_tz::Tz = "America/New_York".parse().unwrap();
        let date = deckle_digest::chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let p = day_preview_with(&day, date, tz, &net);
        assert_eq!(p.events, 1);
        assert_eq!(p.labels, ["Home.ics", "https://cal.example/…"]);
        assert_eq!(p.errors, ["https://cal.example/…: offline"]);
    }

    #[test]
    fn a_missing_config_is_a_first_run_and_a_broken_one_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let missing = state_at(&dir.path().join("digest.toml"));
        assert!(!missing.exists && missing.config.is_none() && missing.error.is_none());
        std::fs::write(dir.path().join("bad.toml"), "titel = 1").unwrap();
        let bad = state_at(&dir.path().join("bad.toml"));
        assert!(bad.exists && bad.config.is_none() && bad.error.is_some());
    }

    #[test]
    fn preview_finds_the_newest_edition() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["2026-10-18", "2026-10-20", "2026-10-19"] {
            std::fs::write(dir.path().join(format!("deckle-daily-{d}.epub")), b"x").unwrap();
        }
        std::fs::write(dir.path().join("deckle-reads-2026-10-21-x.epub"), b"x").unwrap();
        std::fs::write(dir.path().join("deckle-daily-kid-2026-10-22.epub"), b"x").unwrap();
        let main = deckle_digest::EDITION_PREFIX;
        assert!(
            newest_edition_of(dir.path(), main)
                .unwrap()
                .ends_with("deckle-daily-2026-10-20.epub")
        );
        assert!(
            newest_edition_of(dir.path(), "deckle-daily-kid-")
                .unwrap()
                .ends_with("deckle-daily-kid-2026-10-22.epub")
        );
        assert!(newest_edition_of(&dir.path().join("nope"), main).is_none());
    }

    #[test]
    fn runs_use_the_cli_with_json_and_read_its_report() {
        let a = run_args(true, Path::new("/p"), None);
        // Send now is an explicit ask: it runs past a stopped press
        assert_eq!(a, ["digest", "--json", "run", "--to", "device", "--force"]);
        let b = run_args(false, Path::new("/p"), Some("main"));
        assert_eq!(b, ["digest", "--json", "build", "-o", "/p"]);
        let c = run_args(false, Path::new("/p"), Some("kid-courier"));
        assert_eq!(
            c,
            [
                "digest",
                "--json",
                "--paper",
                "kid-courier",
                "build",
                "-o",
                "/p"
            ]
        );
        let r = parse_run(true, br#"{"title":"T"}"#, b"inbox: saved 1\n\nlast\n");
        assert!(r.ok);
        assert_eq!(r.report.unwrap()["title"], "T");
        assert_eq!(r.log, "inbox: saved 1\nlast");
        assert!(parse_run(false, b"oops", b"").report.is_none());
    }

    #[test]
    fn email_status_never_returns_the_password() {
        use deckle_digest::secret::MemoryKeyring;
        const PW: &str = "hunter2-Secret!";
        let cfg = deckle_digest::config::EmailConfig {
            enabled: true,
            to: "me_1@kindle.com".into(),
            from: "me@example.com".into(),
            smtp_host: "smtp.example.com".into(),
            username: "me@example.com".into(),
            ..Default::default()
        };
        let keys = MemoryKeyring::default();
        let no_env = |_: &str| None;
        let before = email_status_with(&cfg, &keys, &no_env);
        assert!(before.configured && !before.has_password);
        set_password_with(&cfg, &keys, PW.to_owned()).unwrap();
        let after = email_status_with(&cfg, &keys, &no_env);
        assert!(after.configured && after.has_password);
        let shown = format!("{after:?} {}", serde_json::to_string(&after).unwrap());
        assert!(!shown.contains(PW), "{shown}");
        assert_eq!(
            serde_json::to_value(&after).unwrap(),
            serde_json::json!({"configured": true, "has_password": true})
        );
        let unfinished = email_status_with(&Default::default(), &keys, &no_env);
        assert!(!unfinished.configured && !unfinished.has_password);
        let e = set_password_with(&Default::default(), &keys, PW.to_owned()).unwrap_err();
        assert!(!e.contains(PW), "{e}");
    }

    #[test]
    fn reading_rows_put_pending_first() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        let now: DateTime<Utc> = "2026-10-20T12:00:00Z".parse().unwrap();
        let id = |a: Added| match a {
            Added::New(i) | Added::Duplicate(i) => i,
        };
        let printed = id(q.add_url("https://ex.com/old", None, "app", now).unwrap());
        q.add_url("https://ex.com/new", None, "app", now).unwrap();
        q.commit(
            deckle_digest::chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
            &[(printed, deckle_digest::reading::Outcome::Printed)],
        )
        .unwrap();
        let rows = reading_rows(&q, now);
        assert_eq!(rows[0].url.as_deref(), Some("https://ex.com/new"));
        assert_eq!(rows[1].state, State::Printed);
    }

    /// `digest.toml` with a second paper beside it.
    fn household() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("digest.toml");
        std::fs::write(
            &main,
            "title = \"The Morning Deckle\"\nsections = [\"weather\", \"feeds\"]\n\
             [[feeds]]\nname = \"Tech\"\nurl = \"https://ex.com/rss\"\n\
             [schedule]\nat = \"05:30\"\n",
        )
        .unwrap();
        (dir, main)
    }

    #[test]
    fn a_new_paper_gets_a_slug_a_file_and_a_card() {
        let (_dir, main) = household();
        assert_eq!(
            create_at(&main, "The Lakeside Gazette", "Dad").unwrap(),
            "lakeside-gazette"
        );
        assert_eq!(
            create_at(&main, "Kid Courier", "Maya").unwrap(),
            "kid-courier"
        );
        assert!(create_at(&main, "Kid Courier", "Maya").is_err());
        assert!(create_at(&main, "  ", "").is_err());
        assert!(create_at(&main, "2026", "").is_err());

        let status = deckle_digest::schedule::Status {
            papers: vec![deckle_digest::schedule::PaperStatus {
                slug: "main".into(),
                at: Some("05:30".into()),
                days: Vec::new(),
            }],
            ..Default::default()
        };
        let view = papers_at(&main, &status).unwrap();
        assert!(view.kindles.is_empty());
        let (cards, notes) = (view.papers, view.notes);
        assert!(notes.is_empty(), "{notes:?}");
        let titles: Vec<&str> = cards.iter().map(|c| c.title.as_str()).collect();
        // main first, then by title
        assert_eq!(
            titles,
            ["The Morning Deckle", "Kid Courier", "The Lakeside Gazette"]
        );
        assert_eq!(cards[0].slug, "main");
        assert_eq!(cards[0].sections, ["Weather", "Tech"]);
        assert_eq!(cards[0].at.as_deref(), Some("05:30"));
        assert!(cards[0].installed && !cards[1].installed);
        assert_eq!(cards[1].reader, "Maya");
        assert_eq!(cards[1].kindle, "");
        assert!(!cards[1].large_print && cards[1].email_to.is_none());
    }

    #[test]
    fn each_paper_is_read_and_saved_in_its_own_file() {
        let (_dir, main) = household();
        let slug = create_at(&main, "Kid Courier", "Maya").unwrap();
        let before = std::fs::read_to_string(&main).unwrap();

        let st = paper_state_at(&main, Some(&slug));
        assert_eq!(
            Path::new(&st.config_path),
            deckle_digest::papers::paper_path(&main, &slug)
        );
        let mut cfg = st.config.unwrap();
        assert_eq!(cfg.title, "Kid Courier");
        // The window's copy doesn't say which paper it is.
        cfg.paper = None;
        cfg.large_print = true;
        cfg.kindle = "Mom's Kindle".into();
        cfg.schedule.days = vec![deckle_digest::config::Day::Sat];
        save_paper_at(&main, Some(&slug), cfg).unwrap();

        assert_eq!(std::fs::read_to_string(&main).unwrap(), before);
        let file =
            std::fs::read_to_string(deckle_digest::papers::paper_path(&main, &slug)).unwrap();
        assert!(file.contains("large_print = true"), "{file}");
        assert!(file.contains("kindle = \"Mom's Kindle\""), "{file}");
        // The household tables stay in digest.toml (the file's own comment names them).
        assert!(
            !file.lines().any(|l| l == "[sync]" || l == "[reading]"),
            "{file}"
        );

        let main_state = paper_state_at(&main, None);
        assert_eq!(main_state.config.unwrap().title, "The Morning Deckle");
        assert!(paper_state_at(&main, Some("nope")).error.is_some());
    }

    #[test]
    fn a_papers_time_is_saved_in_its_file() {
        let (_dir, main) = household();
        let slug = create_at(&main, "Kid Courier", "Maya").unwrap();
        let cfg = set_at_in(&main, Some(&slug), Some("07:00")).unwrap();
        assert_eq!(cfg.slug(), "kid-courier");
        assert_eq!(
            paper_config_at(&main, Some(&slug))
                .unwrap()
                .schedule
                .at
                .as_deref(),
            Some("07:00")
        );
        assert!(set_at_in(&main, Some(&slug), Some("7 o'clock")).is_err());
        set_at_in(&main, Some(&slug), None).unwrap();
        assert_eq!(
            paper_config_at(&main, Some(&slug)).unwrap().schedule.at,
            None
        );
        // The main paper's own time is untouched.
        assert_eq!(
            paper_config_at(&main, None).unwrap().schedule.at.as_deref(),
            Some("05:30")
        );
    }
}
