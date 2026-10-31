// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle calibre` and the watcher's write-back (calibre bridge design §5): what the readers
//! say goes into the calibre columns the owner chose, through `calibredb`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use deckle_core::reading::Record;
use deckle_digest::secret::{Keyring, SystemKeyring};
use deckle_library::writeback::{self, Paths, ReadLog, Writeback};

use crate::cli::CalibreCommand;
use crate::emit;

fn paths() -> Paths {
    Paths::new(
        &deckle_library::config::default_path(),
        deckle_library::sources::home().as_deref(),
    )
}

/// The content server's password: `DECKLE_CALIBRE_PASSWORD`, else the keychain's.
fn password(wb: &Writeback) -> Option<String> {
    if let Ok(p) = std::env::var("DECKLE_CALIBRE_PASSWORD")
        && !p.is_empty()
    {
        return Some(p);
    }
    let user = wb.username.as_deref().filter(|u| !u.is_empty())?;
    SystemKeyring
        .get(writeback::KEYCHAIN_SERVICE, user)
        .ok()
        .flatten()
        .map(|p| p.to_string())
}

/// Adds what `reader` said to the reading log.
pub(crate) fn record(reader: &str, records: &[Record]) {
    if records.is_empty() {
        return;
    }
    let p = paths();
    let mut log = ReadLog::load(&p.log);
    if log.record(reader, records) > 0
        && let Err(e) = log.save(&p.log)
    {
        eprintln!("calibre: {}: {e}", p.log.display());
    }
}

/// The watcher's write-back: at most once a minute, when it's on.
#[derive(Default)]
pub(crate) struct Writer {
    last: Option<Instant>,
    said: Option<String>,
}

impl Writer {
    pub(crate) fn tick(&mut self) {
        if self
            .last
            .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
        {
            return;
        }
        self.last = Some(Instant::now());
        let p = paths();
        let wb = Writeback::load(&p.config);
        if !wb.enabled {
            return;
        }
        let note =
            match writeback::write_now(&p, password(&wb).as_deref(), &mut writeback::system_runner)
            {
                Ok(None) => return,
                Ok(Some(r)) if r.stopped.is_none() && r.failed.is_empty() => {
                    eprintln!("calibre: {} written", r.written);
                    self.said = None;
                    return;
                }
                Ok(Some(r)) => format!(
                    "{} written; {}",
                    r.written,
                    r.stopped
                        .into_iter()
                        .chain(r.failed)
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
                Err(e) => e,
            };
        if self.said.as_deref() != Some(note.as_str()) {
            eprintln!("calibre: {note}");
            self.said = Some(note);
        }
    }
}

fn save(p: &Paths, wb: &Writeback) -> Result<(), String> {
    wb.save(&p.config)
        .map_err(|e| format!("{}: {e}", p.config.display()))
}

fn plan_lines(s: &writeback::Status) -> Vec<String> {
    let mut out = Vec::new();
    let Some(plan) = &s.plan else { return out };
    for c in &plan.changes {
        out.push(format!(
            "  {} · #{}: {} → {}",
            c.title,
            c.column,
            c.old.as_deref().unwrap_or("(empty)"),
            c.new
        ));
    }
    for c in &plan.conflicts {
        out.push(format!(
            "  left alone: {} · #{} is {} in calibre (the reader says {})",
            c.title, c.column, c.calibre, c.reader
        ));
    }
    for (i, t) in plan.to_confirm.iter().enumerate() {
        out.push(format!(
            "  to confirm [{}]: {} ({}) on {} is calibre's “{}”?  deckle calibre confirm {}",
            i + 1,
            t.title,
            t.authors.join(", "),
            t.reader,
            t.calibre_title,
            i + 1
        ));
    }
    for a in &plan.annotations {
        out.push(format!(
            "  {} · {} highlights into calibre's annotations",
            a.title, a.count
        ));
    }
    if !plan.unmatched.is_empty() {
        out.push(format!("  not in calibre: {}", plan.unmatched.join(", ")));
    }
    out
}

pub(crate) fn run(json: bool, action: CalibreCommand) -> Result<ExitCode, String> {
    let p = paths();
    let mut wb = Writeback::load(&p.config);
    match action {
        CalibreCommand::Status => {
            let s = writeback::status(&p);
            emit(json, &s, || {
                let c = &s.writeback.columns;
                let col = |x: &Option<String>| {
                    x.as_ref()
                        .map(|l| format!("#{l}"))
                        .unwrap_or_else(|| "—".into())
                };
                let mut out = vec![
                    format!(
                        "write-back: {}",
                        if s.writeback.enabled {
                            "on (the watcher writes)"
                        } else {
                            "off: deckle calibre on"
                        }
                    ),
                    format!(
                        "library:    {}",
                        s.library
                            .as_ref()
                            .map(|l| l.display().to_string())
                            .unwrap_or_else(|| "not found".into())
                    ),
                    format!(
                        "calibredb:  {}",
                        s.calibredb
                            .as_ref()
                            .map(|l| l.display().to_string())
                            .unwrap_or_else(|| "not found".into())
                    ),
                    format!(
                        "columns:    progress {}, read {}, last read {}, rating {}, highlights {}",
                        col(&c.progress),
                        col(&c.read),
                        col(&c.last_read),
                        col(&c.rating),
                        col(&c.highlights)
                    ),
                ];
                out.push(format!(
                    "annotations: {}",
                    match (s.writeback.annotations, s.annotations_supported) {
                        (true, true) => "on: highlights also go into calibre's own annotations",
                        (true, false) => "on, but this calibredb can't import annotations (calibre 9.18 or later)",
                        (false, true) => "off (deckle calibre annotations on)",
                        (false, false) => "off; needs calibre 9.18 or later",
                    }
                ));
                if let Some(server) = &s.writeback.server {
                    out.push(format!("server:     {server}"));
                }
                if let Some(e) = &s.error {
                    out.push(format!("note:       {e}"));
                }
                if let Some(pl) = &s.plan {
                    out.push(format!("next batch: {} to write", pl.changes.len()));
                    out.extend(plan_lines(&s));
                }
                if let Some(t) = &s.writeback.last_run {
                    out.push(format!(
                        "last run:   {t}: {}",
                        s.writeback.last_result.join("; ")
                    ));
                }
                out.join("\n")
            })?;
        }
        CalibreCommand::On | CalibreCommand::Off => {
            wb.enabled = matches!(action, CalibreCommand::On);
            if wb.enabled && wb.columns.is_empty() {
                return Err("no columns chosen yet: deckle calibre map progress <column>, or deckle calibre create-columns".into());
            }
            save(&p, &wb)?;
            emit(json, &serde_json::json!({ "enabled": wb.enabled }), || {
                format!("write-back {}", if wb.enabled { "on" } else { "off" })
            })?;
        }
        CalibreCommand::Map { field, column } => {
            let col = column.trim().trim_start_matches('#').to_owned();
            let col = (!col.is_empty() && col != "none").then_some(col);
            if let (Some(c), Some(lib)) = (&col, &p.library) {
                let fits = writeback::choices(&deckle_library::calibre::columns(lib)?);
                let ok = fits.get(field.as_str()).is_some_and(|v| v.contains(c));
                if !ok {
                    return Err(format!(
                        "#{c} can't hold {field}; it could go in: {}",
                        fits.get(field.as_str())
                            .map(|v| v
                                .iter()
                                .map(|l| format!("#{l}"))
                                .collect::<Vec<_>>()
                                .join(", "))
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "no column yet".into())
                    ));
                }
            }
            match field.as_str() {
                "progress" => wb.columns.progress = col,
                "read" => wb.columns.read = col,
                "last_read" => wb.columns.last_read = col,
                "rating" => wb.columns.rating = col,
                "highlights" => wb.columns.highlights = col,
                other => {
                    return Err(format!(
                        "{other}: one of progress, read, last_read, rating, highlights"
                    ));
                }
            }
            save(&p, &wb)?;
            emit(json, &wb.columns, || "saved".into())?;
        }
        CalibreCommand::Server { url, username } => {
            wb.server = url.filter(|u| !u.trim().is_empty() && u != "none");
            wb.username = username.filter(|u| !u.is_empty());
            save(&p, &wb)?;
            emit(
                json,
                &serde_json::json!({ "server": wb.server, "username": wb.username }),
                || match &wb.server {
                    Some(s) => format!(
                        "writes go through {s} (deckle calibre password, if it asks for one)"
                    ),
                    None => "writes go to the library folder (calibre must be closed)".into(),
                },
            )?;
        }
        CalibreCommand::Password { stdin } => {
            let user = wb.username.clone().ok_or("set the content server's username first: deckle calibre server <url> --username <name>")?;
            let pw = if stdin {
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|e| e.to_string())?;
                line.trim_end_matches(['\r', '\n']).to_owned()
            } else {
                rpassword::prompt_password("calibre content server password: ")
                    .map_err(|e| e.to_string())?
            };
            SystemKeyring
                .set(writeback::KEYCHAIN_SERVICE, &user, &pw)
                .map_err(|e| {
                    format!("the keychain said no ({e}); set DECKLE_CALIBRE_PASSWORD instead")
                })?;
            emit(json, &serde_json::json!({ "saved": true }), || {
                "saved in the system keychain".into()
            })?;
        }
        CalibreCommand::Preview => {
            let s = writeback::status(&p);
            emit(json, &s.plan, || {
                let mut out = vec![match &s.plan {
                    Some(pl) => format!("{} to write", pl.changes.len()),
                    None => s.error.clone().unwrap_or_default(),
                }];
                out.extend(plan_lines(&s));
                out.join("\n")
            })?;
        }
        CalibreCommand::Write => {
            let r =
                writeback::write_now(&p, password(&wb).as_deref(), &mut writeback::system_runner)?;
            emit(json, &r, || match &r {
                None => "nothing to write".into(),
                Some(r) => std::iter::once(format!("{} written", r.written))
                    .chain((r.annotations > 0).then(|| {
                        format!("{} highlights into calibre's annotations", r.annotations)
                    }))
                    .chain(r.stopped.clone())
                    .chain(r.failed.clone())
                    .collect::<Vec<_>>()
                    .join("\n"),
            })?;
            if r.as_ref()
                .is_some_and(|r| r.stopped.is_some() || !r.failed.is_empty())
            {
                return Ok(ExitCode::FAILURE);
            }
        }
        CalibreCommand::Confirm { n } => {
            let s = writeback::status(&p);
            let list = s.plan.map(|pl| pl.to_confirm).unwrap_or_default();
            let picked: Vec<_> = if n == "all" {
                list
            } else {
                let i: usize = n.parse().map_err(|_| "a number from the preview, or all")?;
                list.into_iter()
                    .nth(i.wrapping_sub(1))
                    .into_iter()
                    .collect()
            };
            if picked.is_empty() {
                return Err("nothing to confirm by that number (deckle calibre preview)".into());
            }
            for t in &picked {
                wb.confirmed.insert(t.key.clone(), t.uuid.clone());
            }
            save(&p, &wb)?;
            emit(json, &picked, || format!("{} confirmed", picked.len()))?;
        }
        CalibreCommand::CreateColumns => {
            let lib: PathBuf = p.library.clone().ok_or("no calibre library found")?;
            let exe = writeback::find_calibredb(wb.calibredb.as_deref())
                .ok_or("calibredb wasn't found")?;
            let added = writeback::create_usual(&mut wb, &exe, &lib, &mut writeback::system_runner)
                .map_err(|e| e.to_string())?;
            save(&p, &wb)?;
            emit(json, &added, || {
                format!(
                    "{}; mapped all four. Restart calibre to see them.",
                    if added.is_empty() {
                        "the columns were there already".to_owned()
                    } else {
                        format!("added #{}", added.join(", #"))
                    }
                )
            })?;
        }
        CalibreCommand::Annotations { on } => {
            wb.annotations = match on.as_str() {
                "on" => true,
                "off" => false,
                _ => return Err("on or off".into()),
            };
            save(&p, &wb)?;
            emit(
                json,
                &serde_json::json!({ "annotations": wb.annotations }),
                || format!("annotations {on}"),
            )?;
        }
        CalibreCommand::Calibredb { path } => {
            wb.calibredb = path.filter(|p| p != Path::new("none"));
            save(&p, &wb)?;
            emit(json, &wb.calibredb, || "saved".into())?;
        }
    }
    Ok(ExitCode::SUCCESS)
}
