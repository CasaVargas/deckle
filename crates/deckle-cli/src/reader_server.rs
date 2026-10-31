// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle reader-server` and the watcher's start of it (KOReader hub design §A, §B): the catalog
//! lists every paper's Morgue and the books waiting in the EPUB outbox; KOReader's sync lands
//! in each paper's readership, or on the shelf for a book.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use deckle_digest::Config;
use deckle_readerserver::sync::Progress;
use deckle_readerserver::{Catalog, Hooks, Paper, Settings, Source, auth};

use crate::cli::ReaderServerCommand;
use crate::emit;

/// Papers and books for the catalog, read fresh each time.
pub(crate) fn source(main_path: std::path::PathBuf) -> Source {
    Arc::new(move || catalog(&main_path))
}

fn catalog(main_path: &Path) -> Catalog {
    let mut papers = Vec::new();
    let Ok(loaded) = deckle_digest::papers::load_all(main_path) else {
        return Catalog::default();
    };
    for (i, cfg) in loaded.papers.iter().enumerate() {
        let morgue = deckle_morgue::Morgue::from_config(cfg);
        for date in morgue.editions() {
            let (epub, _, _) = morgue.paths(date);
            if !epub.is_file() {
                continue;
            }
            let sidecar = morgue.sidecar(date);
            let title = sidecar
                .as_ref()
                .map(|s| s.paper.clone())
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| cfg.title.clone());
            let stem = sidecar
                .map(|s| s.stem)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("{}{date}", deckle_digest::EDITION_PREFIX));
            papers.push(Paper {
                date,
                title,
                epub,
                stem,
                main: i == 0,
            });
        }
    }
    papers.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.title.cmp(&b.title)));
    let mut books = crate::epub_queue().pending();
    books.retain(|p| {
        !p.file_name().is_some_and(|n| {
            n.to_string_lossy()
                .starts_with(deckle_digest::EDITION_PREFIX)
        })
    });
    books.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    Catalog { papers, books }
}

/// `koreader-documents.json`: the files Deckle handed to readers, by KOReader's names for them.
fn registry() -> std::path::PathBuf {
    deckle_readership::state_path().with_file_name("koreader-documents.json")
}

/// `koreader-sync.json`: each document's newest position.
fn sync_store() -> std::path::PathBuf {
    deckle_readership::state_path().with_file_name("koreader-sync.json")
}

/// Records files just handed to a reader (a drive, an export folder), so a later sync naming
/// one by its MD5 can be matched. A file that can't be read is skipped.
pub(crate) fn remember<'a>(files: impl Iterator<Item = &'a std::path::PathBuf>) {
    let reg = registry();
    for f in files {
        let (Some(name), Ok(bytes)) = (f.file_name(), std::fs::read(f)) else {
            continue;
        };
        if let Err(e) =
            deckle_core::koreader::documents::record(&reg, &name.to_string_lossy(), &bytes)
        {
            eprintln!("reader server: {}: {e}", reg.display());
            return;
        }
    }
}

fn hooks(main_path: &Path) -> Hooks {
    let main = main_path.to_path_buf();
    Hooks {
        source: source(main.clone()),
        served: Arc::new(|name, bytes| {
            if let Err(e) = deckle_core::koreader::documents::record(&registry(), name, bytes) {
                eprintln!("reader server: {e}");
            }
        }),
        progress: Arc::new(move |p| progress(&main, p)),
        sync_store: sync_store(),
    }
}

/// The file name KOReader's `document` stands for: what it sent with "Send document
/// metadata", else what Deckle recorded handing over.
fn file_of(p: &Progress) -> Option<String> {
    let sent = p
        .metadata
        .as_ref()
        .and_then(|m| m.filename.as_deref())
        .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f).to_owned())
        .filter(|f| !f.is_empty());
    sent.or_else(|| {
        deckle_core::koreader::documents::lookup(&registry(), &p.document).map(|d| d.file)
    })
}

fn iso(secs: i64) -> String {
    deckle_digest::chrono::DateTime::from_timestamp(secs, 0)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

/// A paper's position goes to its readership (§C); any other book's to From Your Shelf, as one
/// KOReader device's books in progress.
fn progress(main_path: &Path, p: &Progress) {
    let Ok(loaded) = deckle_digest::papers::load_all(main_path) else {
        return;
    };
    let device = format!("koreader/{}", p.device_id);
    if let Some(file) = file_of(p) {
        let at = [deckle_core::koreader::Position {
            book: file,
            xpointer: Some(p.progress.clone()),
            percent: Some(p.percentage),
            when: iso(p.timestamp),
        }];
        let placed: usize = loaded
            .papers
            .iter()
            .map(|cfg| crate::record_koreader(&crate::readership_path(cfg), &device, &at))
            .sum();
        if placed > 0 {
            return;
        }
    }
    shelf(&loaded.papers[0], &device, p);
}

/// Every book `p`'s device has open (not finished), from the sync records, as its shelf.
fn shelf(cfg: &Config, device: &str, p: &Progress) {
    let mut books: Vec<deckle_core::kobo::Book> = deckle_readerserver::sync::load(&sync_store())
        .into_values()
        .filter(|r| r.device_id == p.device_id && r.percentage > 0.0 && r.percentage < 1.0)
        .filter_map(|r| {
            let file = file_of(&r);
            let meta = r.metadata.clone().unwrap_or_default();
            let title = meta.title.filter(|t| !t.trim().is_empty()).or_else(|| {
                file.as_deref()
                    .map(|f| f.split('.').next().unwrap_or(f).to_owned())
            })?;
            // A paper isn't a book in progress, even when its readership missed it.
            if file
                .as_deref()
                .is_some_and(|f| f.starts_with(deckle_digest::EDITION_PREFIX))
            {
                return None;
            }
            Some(deckle_core::kobo::Book {
                content_id: file.unwrap_or_else(|| format!("koreader:{}", r.document)),
                title,
                authors: meta
                    .authors
                    .map(|a| {
                        a.split('\n')
                            .map(str::trim)
                            .filter(|a| !a.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                percent: r.percentage * 100.0,
                last_read: Some(iso(r.timestamp)),
            })
        })
        .collect();
    books.sort_by(|a, b| b.last_read.cmp(&a.last_read));
    let path = cfg.shelf_store();
    let previous = deckle_digest::shelf::store::Shelf::load(&path)
        .kindles
        .get(device)
        .cloned();
    let name = cfg
        .sync
        .device_name(device)
        .unwrap_or_else(|| p.device.clone());
    let snap = deckle_shelf::kobo_snapshot(
        books,
        None,
        true,
        &name,
        previous,
        deckle_digest::chrono::Utc::now(),
    );
    if let Err(e) = deckle_shelf::record(&path, device, snap) {
        eprintln!("shelf: {e}");
    }
}

/// The server's settings, when it's on and has a password.
pub(crate) fn settings(cfg: &Config, main_path: &Path) -> Result<Option<Settings>, String> {
    let rs = &cfg.reader_server;
    if !rs.enabled {
        return Ok(None);
    }
    let Some(key) = auth::load_key(&auth::key_path(main_path)) else {
        return Err("on, but no password is set: run `deckle reader-server password`".into());
    };
    Ok(Some(Settings {
        bind: deckle_readerserver::bind_address(),
        port: rs.port,
        username: rs.username.trim().to_owned(),
        key,
    }))
}

/// The running server, restarted when its settings change: turning it on in the app, or a new
/// password, applies on the watcher's next tick.
#[derive(Default)]
pub(crate) struct Supervisor {
    running: Option<(
        Settings,
        std::sync::mpsc::Sender<()>,
        std::thread::JoinHandle<()>,
    )>,
    said: Option<String>,
}

impl Supervisor {
    pub(crate) fn tick(&mut self, cfg: &Config, main_path: &Path) {
        let wanted = settings(cfg, main_path);
        if let (Ok(Some(w)), Some((now, _, handle))) = (&wanted, &self.running)
            && w == now
            && !handle.is_finished()
        {
            return;
        }
        if let Some((_, stop, handle)) = self.running.take() {
            let _ = stop.send(());
            let _ = handle.join();
            if !matches!(wanted, Ok(Some(_))) {
                eprintln!("reader server: stopped");
            }
        }
        match wanted {
            Ok(Some(s)) => {
                // A server that couldn't bind has finished: say so once, and try again only
                // when the settings change.
                let at = format!("{}:{}", s.bind, s.port);
                let hooks = hooks(main_path);
                let (stop, rx) = std::sync::mpsc::channel();
                let run = s.clone();
                let where_ = at.clone();
                let handle = std::thread::spawn(move || {
                    if let Err(e) = deckle_readerserver::serve_blocking(run, hooks, rx) {
                        eprintln!("reader server: can't listen on {where_}: {e}");
                    }
                });
                let (opds, _) = deckle_readerserver::urls(s.port);
                let note = format!("listening on {at}; KOReader's catalog is {opds}");
                if self.said.as_deref() != Some(note.as_str()) {
                    eprintln!("reader server: {note}");
                    self.said = Some(note);
                }
                self.running = Some((s, stop, handle));
            }
            Ok(None) => self.said = None,
            Err(why) => {
                if self.said.as_deref() != Some(why.as_str()) {
                    eprintln!("reader server: {why}");
                    self.said = Some(why);
                }
            }
        }
    }
}

pub(crate) fn run(json: bool, action: ReaderServerCommand) -> Result<ExitCode, String> {
    let main_path = deckle_digest::config::default_path();
    let key_path = auth::key_path(&main_path);
    match action {
        ReaderServerCommand::Password { stdin } => {
            let password = if stdin {
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|e| format!("stdin: {e}"))?;
                line.trim_end_matches(['\r', '\n']).to_owned()
            } else {
                rpassword::prompt_password("Reader server password: ")
                    .map_err(|e| format!("couldn't read the password: {e}"))?
            };
            auth::save_password(&key_path, &password).map_err(|e| e.to_string())?;
            emit(json, &serde_json::json!({ "saved": key_path }), || {
                format!("saved (its MD5, owner-only) in {}", key_path.display())
            })?;
            Ok(ExitCode::SUCCESS)
        }
        ReaderServerCommand::Status => {
            let cfg = Config::load(&main_path).map_err(|e| e.to_string())?;
            let rs = &cfg.reader_server;
            let has_password = auth::load_key(&key_path).is_some();
            let (opds, sync) = deckle_readerserver::urls(rs.port);
            let value = serde_json::json!({
                "enabled": rs.enabled,
                "password_set": has_password,
                "port": rs.port,
                "username": rs.username,
                "opds": opds,
                "sync": sync,
            });
            emit(json, &value, || {
                let state = match (rs.enabled, has_password) {
                    (true, true) => "on (the watcher serves it)",
                    (true, false) => "on, but no password: run `deckle reader-server password`",
                    (false, _) => "off: set [reader_server] enabled = true in digest.toml",
                };
                format!(
                    "reader server: {state}\nusername:     {}\ncatalog:      {opds}\nsync server:  {sync}",
                    rs.username
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
