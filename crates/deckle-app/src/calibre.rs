// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings → Library → Reading data into calibre (calibre bridge design §5.4): the columns,
//! the content server, a preview of the next batch, conflicts and books to confirm. Each change
//! answers with the section as it now is. The watcher writes once a minute while it's on.

use std::path::PathBuf;

use deckle_digest::secret::{Keyring, SystemKeyring};
use deckle_library::writeback::{self, Columns, Paths, Status, Writeback};
use serde::Deserialize;

fn paths() -> Paths {
    Paths::new(
        &deckle_library::config::default_path(),
        deckle_library::sources::home().as_deref(),
    )
}

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

pub fn calibre_writeback_status() -> Status {
    writeback::status(&paths())
}

/// The owner's choices; a field left out stays as it is.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct WritebackPatch {
    pub enabled: Option<bool>,
    pub columns: Option<Columns>,
    /// `""` for none.
    pub server: Option<String>,
    pub username: Option<String>,
    /// `""` to look for it again.
    pub calibredb: Option<String>,
}

fn update(f: impl FnOnce(&mut Writeback) -> Result<(), String>) -> Result<Status, String> {
    let p = paths();
    let mut wb = Writeback::load(&p.config);
    f(&mut wb)?;
    wb.save(&p.config).map_err(|e| e.to_string())?;
    Ok(writeback::status(&p))
}

pub fn calibre_writeback_set(patch: WritebackPatch) -> Result<Status, String> {
    update(|wb| {
        if let Some(c) = patch.columns {
            wb.columns = c;
        }
        if let Some(s) = patch.server {
            wb.server = Some(s.trim().to_owned()).filter(|s| !s.is_empty());
        }
        if let Some(u) = patch.username {
            wb.username = Some(u.trim().to_owned()).filter(|u| !u.is_empty());
        }
        if let Some(c) = patch.calibredb {
            wb.calibredb = Some(PathBuf::from(c.trim())).filter(|p| !p.as_os_str().is_empty());
        }
        if let Some(on) = patch.enabled {
            let c = &wb.columns;
            if on
                && [&c.progress, &c.read, &c.last_read, &c.rating]
                    .iter()
                    .all(|x| x.is_none())
            {
                return Err("choose a column first, or create the usual ones".into());
            }
            wb.enabled = on;
        }
        Ok(())
    })
}

/// Books matched by title and author are the same books: `keys` from the plan's `to_confirm`.
pub fn calibre_writeback_confirm(keys: Vec<String>) -> Result<Status, String> {
    let p = paths();
    let plan = writeback::status(&p).plan.unwrap_or_default();
    update(|wb| {
        for t in plan.to_confirm.iter().filter(|t| keys.contains(&t.key)) {
            wb.confirmed.insert(t.key.clone(), t.uuid.clone());
        }
        Ok(())
    })
}

/// Writes the next batch now; what it did is in the answer's `last_result`.
pub fn calibre_writeback_write() -> Result<Status, String> {
    let p = paths();
    let wb = Writeback::load(&p.config);
    writeback::write_now(&p, password(&wb).as_deref(), &mut writeback::system_runner)?;
    Ok(writeback::status(&p))
}

/// Adds the usual columns (calibre closed) and uses them.
pub fn calibre_create_columns() -> Result<Status, String> {
    let p = paths();
    let lib = p.library.clone().ok_or("no calibre library found")?;
    update(|wb| {
        let exe = writeback::find_calibredb(wb.calibredb.as_deref())
            .ok_or("calibredb wasn't found: install calibre, or say where calibredb is")?;
        writeback::create_usual(wb, &exe, &lib, &mut writeback::system_runner)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Keeps the content server's password in the system keychain.
pub fn calibre_set_password(password: String) -> Result<Status, String> {
    let p = paths();
    let wb = Writeback::load(&p.config);
    let user = wb.username.ok_or("set the content server's user first")?;
    SystemKeyring
        .set(writeback::KEYCHAIN_SERVICE, &user, &password)
        .map_err(|e| format!("the keychain said no ({e}); set DECKLE_CALIBRE_PASSWORD instead"))?;
    Ok(writeback::status(&p))
}
