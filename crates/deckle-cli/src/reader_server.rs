// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle reader-server` and the watcher's start of it (KOReader hub design §A): the catalog
//! lists every paper's Morgue and the books waiting in the EPUB outbox.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use deckle_digest::Config;
use deckle_readerserver::{Catalog, Paper, Settings, Source, auth};

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
    for cfg in &loaded.papers {
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
                let source = source(main_path.to_path_buf());
                let (stop, rx) = std::sync::mpsc::channel();
                let run = s.clone();
                let where_ = at.clone();
                let handle = std::thread::spawn(move || {
                    if let Err(e) = deckle_readerserver::serve_blocking(run, source, rx) {
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
