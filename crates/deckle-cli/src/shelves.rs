// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle shelves` and the watcher's shelves (calibre bridge design §6): rules that make a
//! Kobo's shelves and KOReader's collections follow calibre.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use deckle_library::shelves::{self, Config, Rule, Target};

use crate::cli::ShelvesCommand;
use crate::emit;

fn library_json() -> PathBuf {
    deckle_library::config::default_path()
}

fn config_path() -> PathBuf {
    shelves::config_path(&library_json())
}

fn sets() -> Result<Vec<deckle_core::shelves::RuleSet>, String> {
    shelves::sets(&library_json(), deckle_library::sources::home().as_deref())
}

fn now() -> String {
    deckle_core::devices::now_iso()
}

/// The rules on a plugged-in Kobo or PocketBook (or a folder standing in for one).
fn on_drive(folder: Option<&Path>, apply: bool) -> Result<Vec<Target>, String> {
    let cfg = Config::load(&config_path());
    let sets = sets()?;
    match folder {
        Some(f) => {
            let reader = deckle_core::reader::detect_in(&[f.to_path_buf()])
                .into_iter()
                .next();
            let key = reader.as_ref().map(|r| r.id()).unwrap_or_else(|| {
                format!(
                    "folder/{}",
                    f.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                )
            });
            let kobo = match &reader {
                Some(deckle_core::reader::Reader::Kobo(k)) => Some(k.clone()),
                _ => None,
            };
            Ok(shelves::for_drive(
                &cfg,
                &sets,
                f,
                &key,
                kobo.as_ref(),
                apply,
                &now(),
            ))
        }
        None => {
            let reader = crate::detect_reader()
                .ok_or("no Kobo or PocketBook plugged in (or give --folder)")?;
            Ok(for_reader(&cfg, &sets, &reader, apply))
        }
    }
}

fn for_reader(
    cfg: &Config,
    sets: &[deckle_core::shelves::RuleSet],
    reader: &deckle_core::reader::Reader,
    apply: bool,
) -> Vec<Target> {
    let kobo = match reader {
        deckle_core::reader::Reader::Kobo(k) => Some(k),
        _ => None,
    };
    shelves::for_drive(cfg, sets, reader.root(), &reader.id(), kobo, apply, &now())
}

/// When a reader is plugged in: the rules on it, when they're on automatically.
pub(crate) fn on_sync(reader: &deckle_core::reader::Reader) {
    let cfg = Config::load(&config_path());
    if !cfg.auto || cfg.rules.is_empty() {
        return;
    }
    let sets = match sets() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("shelves: {e}");
            return;
        }
    };
    for t in for_reader(&cfg, &sets, reader, true) {
        if t.applied || t.note.is_some() {
            let (added, removed) = t.diff.counts();
            eprintln!(
                "shelves: {} {}: {added} added, {removed} taken off{}",
                t.key,
                t.kind,
                t.note.map(|n| format!(" ({n})")).unwrap_or_default()
            );
        }
    }
}

/// The rules for wireless sessions, worked out again once a minute.
#[derive(Default)]
pub(crate) struct Publisher {
    last: Option<Instant>,
    said: Option<String>,
}

impl Publisher {
    pub(crate) fn tick(&mut self) {
        if self
            .last
            .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
        {
            return;
        }
        self.last = Some(Instant::now());
        let store = crate::wireless::store();
        // Off: sessions leave collections alone.
        let on = Config::load(&config_path()).auto;
        let r = if on { sets() } else { Ok(Vec::new()) }
            .and_then(|s| store.set_shelves(&s).map_err(|e| e.to_string()));
        match r {
            Ok(()) => self.said = None,
            Err(e) if self.said.as_deref() != Some(e.as_str()) => {
                eprintln!("shelves: {e}");
                self.said = Some(e);
            }
            Err(_) => {}
        }
    }
}

fn target_lines(targets: &[Target]) -> String {
    if targets.is_empty() {
        return "no Kobo shelves or KOReader there".into();
    }
    targets
        .iter()
        .map(|t| {
            let (added, removed) = t.diff.counts();
            let mut line = format!(
                "{} ({}): {} new, {added} added, {removed} taken off, {} deleted{}",
                t.key,
                if t.kind == "kobo" {
                    "Kobo shelves"
                } else {
                    "KOReader collections"
                },
                t.diff.create.len(),
                t.diff.delete.len(),
                if t.applied { " — done" } else { "" }
            );
            for (name, books) in &t.diff.add {
                line.push_str(&format!(
                    "\n  + {name}: {}",
                    books.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
            for (name, books) in &t.diff.remove {
                line.push_str(&format!(
                    "\n  − {name}: {}",
                    books.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
            if let Some(n) = &t.note {
                line.push_str(&format!("\n  {n}"));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn run(json: bool, action: ShelvesCommand) -> Result<ExitCode, String> {
    let path = config_path();
    let mut cfg = Config::load(&path);
    let save = |cfg: &Config| {
        cfg.save(&path)
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    match action {
        ShelvesCommand::List => {
            emit(json, &cfg, || {
                let mut out: Vec<String> = cfg
                    .rules
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        format!(
                            "{}. {} ← {} ({})",
                            i + 1,
                            if r.name.is_empty() {
                                "(each series)"
                            } else {
                                &r.name
                            },
                            r.from,
                            r.readers.join(", ")
                        )
                    })
                    .collect();
                if out.is_empty() {
                    out.push("no rules yet: deckle shelves add Sci-Fi tag:Sci-Fi".into());
                }
                out.push(format!(
                    "automatic: {}",
                    if cfg.auto {
                        "on (when a reader is plugged in, and over Wi-Fi)"
                    } else {
                        "off (deckle shelves auto on)"
                    }
                ));
                out.join("\n")
            })?;
        }
        ShelvesCommand::Add { name, from, reader } => {
            if let Some(why) = shelves::check(&from) {
                return Err(why);
            }
            cfg.rules.push(Rule {
                name: if from == "series:*" {
                    String::new()
                } else {
                    name
                },
                from,
                readers: if reader.is_empty() {
                    vec!["all".into()]
                } else {
                    reader
                },
            });
            save(&cfg)?;
            emit(json, &cfg.rules, || {
                format!(
                    "added; {} {}",
                    cfg.rules.len(),
                    if cfg.rules.len() == 1 {
                        "rule"
                    } else {
                        "rules"
                    }
                )
            })?;
        }
        ShelvesCommand::Remove { n } => {
            if n == 0 || n > cfg.rules.len() {
                return Err("a number from deckle shelves list".into());
            }
            let gone = cfg.rules.remove(n - 1);
            save(&cfg)?;
            emit(json, &gone, || {
                format!(
                    "removed {} ← {}; its shelves go the next time",
                    gone.name, gone.from
                )
            })?;
        }
        ShelvesCommand::Auto { on } => {
            cfg.auto = match on.as_str() {
                "on" => true,
                "off" => false,
                _ => return Err("on or off".into()),
            };
            save(&cfg)?;
            emit(json, &serde_json::json!({ "auto": cfg.auto }), || {
                format!("automatic {on}")
            })?;
        }
        ShelvesCommand::KoboOk { key } => {
            if !key.starts_with("kobo/") {
                return Err("a Kobo's key, kobo/<serial> (deckle shelves preview shows it)".into());
            }
            if !cfg.kobo_ok.contains(&key) {
                cfg.kobo_ok.push(key.clone());
            }
            save(&cfg)?;
            emit(json, &cfg.kobo_ok, || {
                format!("Deckle may write shelves on {key}")
            })?;
        }
        ShelvesCommand::Preview { folder } => {
            let t = on_drive(folder.as_deref(), false)?;
            emit(json, &t, || target_lines(&t))?;
        }
        ShelvesCommand::Apply { folder } => {
            let t = on_drive(folder.as_deref(), true)?;
            emit(json, &t, || target_lines(&t))?;
        }
    }
    Ok(ExitCode::SUCCESS)
}
