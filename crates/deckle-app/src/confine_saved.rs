// SPDX-License-Identifier: GPL-3.0-or-later
//! Paths a save from the web UI writes into a config file, checked against the server's
//! folders. Only values that changed are checked: the window sends the whole file, and a path
//! saved before this check (or edited by hand) would otherwise block every later save. Each
//! field is compared only with that same field's old values, so an old path can't be copied
//! into another setting. Whatever is saved is checked again where it is used
//! (`deckle_core::confine::check_saved`).

use deckle_digest::Config;
use deckle_digest::masthead::MastheadFont;

use crate::dispatch::Ctx;

/// A path field's new values, against its old ones: each new one is checked unless the field
/// already held it.
fn changed<'a>(
    ctx: &Ctx,
    setting: &str,
    new: impl IntoIterator<Item = &'a str>,
    old: &[&str],
    write: bool,
) -> Result<(), String> {
    for n in new {
        let n = n.trim();
        if n.is_empty() || old.contains(&n) {
            continue;
        }
        let r = if write {
            ctx.write_path(n)
        } else {
            ctx.path(n)
        };
        r.map_err(|e| format!("{setting}: {e}"))?;
    }
    Ok(())
}

/// Calendars are files or addresses; only files are paths.
fn files(calendars: &[String]) -> impl Iterator<Item = &str> {
    calendars
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.contains("://"))
}

fn font(m: &MastheadFont) -> Option<&str> {
    match m {
        MastheadFont::File(p) => Some(p.as_str()),
        _ => None,
    }
}

/// The paper's `[reading] inbox`, `[morgue] folder`, `[[exports]] dir`, calendar files and
/// masthead font, as `daily_save` is about to write them.
pub fn daily(ctx: &Ctx, new: &Config, paper: Option<&str>) -> Result<(), String> {
    if ctx.roots.is_none() {
        return Ok(());
    }
    let old = crate::daily::paper_config_at(&deckle_digest::config::default_path(), paper)
        .unwrap_or_default();
    changed(
        ctx,
        "[reading] inbox",
        [new.reading.inbox.as_str()],
        &[old.reading.inbox.trim()],
        true,
    )?;
    changed(
        ctx,
        "[morgue] folder",
        [new.morgue.folder.as_str()],
        &[old.morgue.folder.trim()],
        true,
    )?;
    let old_exports: Vec<&str> = old.export.iter().map(|e| e.dir.trim()).collect();
    changed(
        ctx,
        "[[exports]] dir",
        new.export.iter().map(|e| e.dir.as_str()),
        &old_exports,
        true,
    )?;
    let old_cals: Vec<&str> = files(&old.day.calendars).collect();
    changed(
        ctx,
        "[day] calendars",
        files(&new.day.calendars),
        &old_cals,
        false,
    )?;
    let old_font: Vec<&str> = font(&old.masthead).into_iter().collect();
    changed(ctx, "masthead", font(&new.masthead), &old_font, false)
}
