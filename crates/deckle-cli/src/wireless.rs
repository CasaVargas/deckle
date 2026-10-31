// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle wireless` and the watcher's start of it (calibre bridge design §4): calibre's
//! wireless connection, so KOReader's Tools → calibre → Connect takes books from Deckle.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use deckle_digest::Config;
use deckle_readerserver::wireless::{self, Library, Settings, Store};

use crate::cli::WirelessCommand;
use crate::emit;

/// The queue and `readers.json`, beside the readership state.
pub(crate) fn store() -> Store {
    Store::new(deckle_readership::state_path().with_file_name("wireless"))
}

/// The server's settings, when `[wireless]` is on.
pub(crate) fn settings(cfg: &Config, main_path: &Path) -> Option<Settings> {
    let w = &cfg.wireless;
    if !w.enabled {
        return None;
    }
    let secrets = wireless::load_secrets(&wireless::secrets_path(main_path));
    Some(Settings {
        bind: deckle_readerserver::bind_address(),
        port: w.port,
        discovery: w.discovery,
        library: Library {
            name: w.library_name.trim().to_owned(),
            uuid: secrets.library_uuid,
        },
        password: secrets.password.filter(|p| !p.is_empty()),
        content_port: if cfg.reader_server.enabled {
            cfg.reader_server.port
        } else {
            0
        },
        dir: store().dir,
        keepalive: Duration::from_secs(120),
        idle: Duration::from_secs(15 * 60),
        poll: Duration::from_secs(2),
    })
}

/// The address to type into a reader that can't find Deckle by itself.
fn address(port: u16) -> String {
    let host = deckle_readerserver::lan_address()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "<this computer's address>".into());
    format!("{host}:{port}")
}

/// The running server, restarted when its settings change.
#[derive(Default)]
pub(crate) struct Supervisor {
    running: Option<(
        Settings,
        std::sync::mpsc::Sender<()>,
        std::thread::JoinHandle<()>,
    )>,
    said: Option<String>,
    /// When the paper to send was last looked for.
    paper_checked: Option<std::time::Instant>,
}

/// The newest edition of the main paper, for a reader that connects (`[wireless] paper`).
fn newest_paper(main_path: &Path) -> Option<(PathBuf, String)> {
    crate::reader_server::catalog(main_path)
        .papers
        .into_iter()
        .find(|p| p.main)
        .map(|p| (p.epub, p.stem))
}

impl Supervisor {
    pub(crate) fn tick(&mut self, cfg: &Config, main_path: &Path) {
        if self
            .paper_checked
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
        {
            self.paper_checked = Some(std::time::Instant::now());
            let paper = (cfg.wireless.enabled && cfg.wireless.paper)
                .then(|| newest_paper(main_path))
                .flatten();
            if let Err(e) =
                store().set_paper(paper.as_ref().map(|(p, n)| (p.as_path(), n.as_str())))
            {
                eprintln!("wireless: {e}");
            }
        }
        let wanted = settings(cfg, main_path);
        if let (Some(w), Some((now, _, handle))) = (&wanted, &self.running)
            && w == now
            && !handle.is_finished()
        {
            return;
        }
        // A server that couldn't bind has finished: it's tried again only when the settings
        // change.
        if let (Some(w), Some((now, _, _))) = (&wanted, &self.running)
            && w == now
        {
            return;
        }
        if let Some((_, stop, handle)) = self.running.take() {
            let _ = stop.send(());
            let _ = handle.join();
            if wanted.is_none() {
                eprintln!("wireless: stopped");
            }
        }
        let Some(s) = wanted else {
            self.said = None;
            return;
        };
        let at = format!("{}:{}", s.bind, s.port);
        let (stop, rx) = std::sync::mpsc::channel();
        let run = s.clone();
        let where_ = at.clone();
        let handle = std::thread::spawn(move || {
            if let Err(e) = wireless::serve_blocking(run, rx) {
                eprintln!(
                    "wireless: can't listen on {where_}: {e} (is calibre's wireless server on? \
                     set [wireless] port to another)"
                );
            }
        });
        let note = format!(
            "listening on {at}; on the reader, Tools → calibre → Connect (or the address {})",
            address(s.port)
        );
        if self.said.as_deref() != Some(note.as_str()) {
            eprintln!("wireless: {note}");
            self.said = Some(note);
        }
        self.running = Some((s, stop, handle));
    }
}

/// `digest.toml`, or the defaults before there is one.
fn config(main_path: &Path) -> Result<Config, String> {
    if main_path.is_file() {
        Config::load(main_path).map_err(|e| e.to_string())
    } else {
        Ok(Config::default())
    }
}

fn read_password(stdin: bool) -> Result<String, String> {
    if stdin {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| format!("stdin: {e}"))?;
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    } else {
        rpassword::prompt_password("Wireless password: ")
            .map_err(|e| format!("couldn't read the password: {e}"))
    }
}

pub(crate) fn run(json: bool, action: WirelessCommand) -> Result<ExitCode, String> {
    let main_path = deckle_digest::config::default_path();
    let secrets_path = wireless::secrets_path(&main_path);
    let store = store();
    match action {
        WirelessCommand::Status => {
            let cfg = config(&main_path)?;
            let w = &cfg.wireless;
            let secrets = wireless::load_secrets(&secrets_path);
            let readers = store.readers();
            let jobs = store.jobs();
            let value = serde_json::json!({
                "enabled": w.enabled,
                "port": w.port,
                "address": address(w.port),
                "password_set": secrets.password.as_deref().is_some_and(|p| !p.is_empty()),
                "discovery": readers.discovery,
                "readers": readers.readers,
                "queue": jobs,
                "log": readers.log,
            });
            emit(json, &value, || {
                let mut out = vec![format!(
                    "wireless: {}\naddress:  {}",
                    if w.enabled {
                        "on (the watcher serves it)"
                    } else {
                        "off: set [wireless] enabled = true in digest.toml"
                    },
                    address(w.port)
                )];
                for (id, r) in &readers.readers {
                    out.push(format!(
                        "{} [{id}]: {}, {} books{}",
                        r.name,
                        if r.connected {
                            "connected".to_owned()
                        } else {
                            format!("last seen {}", r.last_seen)
                        },
                        r.books.len(),
                        r.free
                            .map(|f| format!(", {} MB free", f / 1_000_000))
                            .unwrap_or_default()
                    ));
                }
                for j in &jobs {
                    let what = j
                        .send
                        .as_ref()
                        .map(|p| format!("send {}", p.display()))
                        .or_else(|| j.remove.as_ref().map(|l| format!("remove {l}")))
                        .unwrap_or_default();
                    out.push(format!("queued for {}: {what}", j.reader));
                }
                if let Some(last) = readers.log.last() {
                    out.push(format!("last: {last}"));
                }
                out.join("\n")
            })?;
            Ok(ExitCode::SUCCESS)
        }
        WirelessCommand::Password { stdin, clear } => {
            let mut s = wireless::load_secrets(&secrets_path);
            s.password = if clear {
                None
            } else {
                let p = read_password(stdin)?;
                if p.is_empty() {
                    return Err("the password is empty (use --clear for none)".into());
                }
                Some(p)
            };
            wireless::save_secrets(&secrets_path, &s).map_err(|e| e.to_string())?;
            emit(
                json,
                &serde_json::json!({ "saved": secrets_path, "password_set": !clear }),
                || {
                    format!(
                        "{} (owner-only) in {}; on the reader, Tools → calibre → Wireless settings",
                        if clear { "cleared" } else { "saved" },
                        secrets_path.display()
                    )
                },
            )?;
            Ok(ExitCode::SUCCESS)
        }
        WirelessCommand::Queue {
            reader,
            books,
            as_is,
        } => {
            let mut added = Vec::new();
            for b in books {
                let path: PathBuf =
                    std::fs::canonicalize(&b).map_err(|e| format!("{}: {e}", b.display()))?;
                if !path.is_file() {
                    return Err(format!("{}: not a file", b.display()));
                }
                let job = if as_is {
                    store.add(&reader, Some(path), None)
                } else {
                    store.add_book(&reader, path)
                };
                added.push(job.map_err(|e| e.to_string())?);
            }
            emit(json, &added, || {
                format!(
                    "queued {} for {reader}: sent now if it's connected, else when it connects (Tools → \
                     calibre → Connect)",
                    if added.len() == 1 {
                        "1 book".to_owned()
                    } else {
                        format!("{} books", added.len())
                    }
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
        WirelessCommand::Remove { reader, lpaths } => {
            let mut added = Vec::new();
            for l in lpaths {
                added.push(
                    store
                        .add(&reader, None, Some(l))
                        .map_err(|e| e.to_string())?,
                );
            }
            emit(json, &added, || {
                format!(
                    "{} to take off {reader}: now if it's connected, else when it connects",
                    added.len()
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
        WirelessCommand::Serve => {
            let cfg = config(&main_path)?;
            let mut forced = cfg.clone();
            forced.wireless.enabled = true;
            let s = settings(&forced, &main_path).ok_or("no settings")?;
            eprintln!(
                "wireless: listening on {}:{}; on the reader, Tools → calibre → Connect (or the \
                 address {}). Ctrl-C to stop.",
                s.bind,
                s.port,
                address(s.port)
            );
            let (_stop, rx) = std::sync::mpsc::channel();
            wireless::serve_blocking(s, rx).map_err(|e| e.to_string())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
