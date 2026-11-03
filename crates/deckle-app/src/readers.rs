// SPDX-License-Identifier: GPL-3.0-or-later
//! The Devices tab's other readers: a Kobo or PocketBook plugged in as a drive, and a
//! reMarkable over its USB web interface or over SSH. Each lists its library, removes books
//! (where the reader allows it) and backs everything up to this computer
//! (`deckle_core::reader_library` does the reading and writing).

use std::path::{Path, PathBuf};

use deckle_core::reader::Reader;
use deckle_core::reader_library;
pub use deckle_core::reader_library::{Backup, Library, Removed};
use deckle_digest::config::RemarkableTransport;
use serde::Serialize;

/// A connected reader that isn't a Kindle.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReaderRow {
    /// `kobo/<serial>`, `pocketbook/<volume>`, `remarkable/usb` or `remarkable/ssh`.
    pub key: String,
    /// `kobo`, `pocketbook` or `remarkable`.
    pub kind: &'static str,
    /// The owner's name for it, else the kind.
    pub name: String,
    /// "USB drive", "USB web interface", "SSH to 10.11.99.1".
    pub connection: String,
    /// Firmware or model, when known.
    pub detail: Option<String>,
    pub can_remove: bool,
    /// Why it can't, when it can't.
    pub remove_note: Option<String>,
}

fn config() -> deckle_digest::Config {
    deckle_digest::Config::load(&deckle_digest::config::default_path()).unwrap_or_default()
}

/// Every connected Kobo and PocketBook, and the reMarkable when its web interface answers or
/// SSH is set up (SSH is tried only when its library is opened).
pub fn readers_list() -> Vec<ReaderRow> {
    let cfg = config();
    let mut out = Vec::new();
    for r in deckle_core::reader::detect() {
        let id = r.id();
        let (kind, detail) = match &r {
            Reader::Kobo(k) => ("kobo", k.firmware.as_ref().map(|f| format!("Firmware {f}"))),
            Reader::PocketBook(_) => ("pocketbook", None),
        };
        out.push(ReaderRow {
            name: cfg.sync.device_name(&id).unwrap_or_else(|| r.name()),
            key: id,
            kind,
            connection: "USB drive".into(),
            detail,
            can_remove: true,
            remove_note: None,
        });
    }
    let rm = &cfg.remarkable;
    if rm.transport == RemarkableTransport::Ssh {
        out.push(ReaderRow {
            key: "remarkable/ssh".into(),
            kind: "remarkable",
            name: "reMarkable".into(),
            connection: format!("SSH to {}", rm.ssh_host()),
            detail: None,
            can_remove: true,
            remove_note: None,
        });
    } else if deckle_core::remarkable::detect_with(rm.address()).is_some() {
        out.push(ReaderRow {
            key: "remarkable/usb".into(),
            kind: "remarkable",
            name: "reMarkable".into(),
            connection: "USB web interface".into(),
            detail: None,
            can_remove: false,
            remove_note: Some(
                "The tablet's USB web interface can't delete. Remove documents on the tablet, or \
                 set up SSH (Daily → Delivery → reMarkable) to remove them here."
                    .into(),
            ),
        });
    }
    out
}

/// One of the readers `key` names, connected now.
enum Found {
    Drive(Reader),
    Web(deckle_core::remarkable::Remarkable),
    Ssh(deckle_core::remarkable_ssh::SshShell),
}

fn find(key: &str) -> Result<Found, String> {
    match key {
        "remarkable/usb" => deckle_core::remarkable::detect_with(config().remarkable.address())
            .map(Found::Web)
            .ok_or_else(|| {
                "the reMarkable isn't answering: plug it in with Settings → Storage → USB web \
                 interface on"
                    .into()
            }),
        "remarkable/ssh" => {
            let cfg = config();
            deckle_digest::remarkable_login::connect(
                &cfg.remarkable,
                &deckle_digest::config::default_path(),
            )
            .map(Found::Ssh)
        }
        _ => deckle_core::reader::detect()
            .into_iter()
            .find(|r| r.id() == key)
            .map(Found::Drive)
            .ok_or_else(|| "that reader isn't plugged in any more".into()),
    }
}

/// The library of the reader `key`.
pub fn readers_library(key: String) -> Result<Library, String> {
    Ok(match find(&key)? {
        Found::Drive(r) => reader_library::on_drive(&r),
        Found::Web(r) => reader_library::on_remarkable_web(&r),
        Found::Ssh(mut sh) => reader_library::on_remarkable_ssh(&mut sh),
    })
}

/// Where backups of other readers go: `reader-backups/` beside `digest.toml`.
pub fn backups_root() -> PathBuf {
    deckle_digest::config::default_path().with_file_name("reader-backups")
}

fn stamp() -> String {
    deckle_digest::chrono::Utc::now()
        .format("%Y-%m-%dT%H-%M-%SZ")
        .to_string()
}

/// A folder name from a reader's key: `kobo/N418…` → `kobo-N418…`.
fn folder_of(key: &str) -> String {
    key.chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() || c == '-' || c == '.' => c,
            _ => '-',
        })
        .collect()
}

/// Removes `ids` from the reader `key`. A Kobo's database is copied into its backups folder
/// first.
pub fn readers_remove(key: String, ids: Vec<String>) -> Result<Removed, String> {
    Ok(match find(&key)? {
        Found::Drive(r) => {
            let aside = backups_root().join(folder_of(&key)).join("databases");
            reader_library::remove_on_drive(&r, &ids, &aside, &stamp())
        }
        Found::Web(_) => {
            return Err("the reMarkable's USB web interface can't delete documents".into());
        }
        Found::Ssh(mut sh) => reader_library::remove_remarkable_ssh(&mut sh, &ids),
    })
}

/// What [`readers_edit`] did, for the notice.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Edited {
    pub id: String,
    /// What else the owner should know (the Kobo shows it after ejecting, …).
    pub note: Option<String>,
}

/// Writes a file atomically beside itself: a half-written book never replaces the old one.
fn write_beside(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.deckle-tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

/// Changes the title, authors and series of `id` on the reader `key`: an EPUB or KEPUB's own
/// details on a Kobo or PocketBook (and the Kobo's library entry), or a reMarkable document's
/// name over SSH.
pub fn readers_edit(
    key: String,
    id: String,
    edit: crate::commands::BookEdit,
) -> Result<Edited, String> {
    let details = edit.details();
    deckle_library::details::check(&details)?;
    match find(&key)? {
        Found::Drive(r) => {
            let path = reader_library::book_path(r.root(), &id)?;
            let lower = id.to_ascii_lowercase();
            if !lower.ends_with(".epub") {
                return Err(
                    "only an EPUB's details can be changed on the reader; a PDF or other file \
                     keeps the details it was made with"
                        .into(),
                );
            }
            let bytes = deckle_library::details::rewrite_epub(&path, &details)?;
            write_beside(&path, &bytes)?;
            let mut note = None;
            if let Reader::Kobo(k) = &r {
                let now = deckle_digest::chrono::Utc::now().to_rfc3339();
                let series = details.series.as_ref().map(|s| (s.name.as_str(), s.index));
                match deckle_core::kobo_shelves::set_book(
                    k,
                    &id,
                    &details.title,
                    &details.authors,
                    series,
                    &now,
                ) {
                    Ok(true) => {
                        note = Some("The Kobo shows it once you eject it.".into());
                    }
                    Ok(false) => {
                        note = Some(
                            "The Kobo hasn't added this book to its library yet; it reads the new \
                             details when it does."
                                .into(),
                        )
                    }
                    Err(e) => {
                        note = Some(format!(
                            "The book's file has the new details, but the Kobo's library entry \
                             couldn't be changed ({e}); it may show the old ones until the book \
                             is added again."
                        ))
                    }
                }
            }
            Ok(Edited { id, note })
        }
        Found::Web(_) => Err(
            "the reMarkable's USB web interface can't rename documents: rename it on the tablet, \
             or set up SSH"
                .into(),
        ),
        Found::Ssh(mut sh) => {
            reader_library::rename_remarkable_ssh(&mut sh, &id, &details.title)?;
            Ok(Edited {
                id,
                note: Some("The tablet restarted its reader to show the new name.".into()),
            })
        }
    }
}

/// Backs up everything on the reader `key` into `into` (else its folder under
/// [`backups_root`]), in a dated folder.
pub fn readers_backup(key: String, into: Option<String>) -> Result<Backup, String> {
    let root = into.map_or_else(|| backups_root().join(folder_of(&key)), PathBuf::from);
    let dir = root.join(stamp());
    Ok(match find(&key)? {
        Found::Drive(r) => reader_library::backup_drive(&r, &dir),
        Found::Web(r) => {
            let lib = reader_library::on_remarkable_web(&r);
            reader_library::backup_remarkable_web(&r, &lib, &dir)
        }
        Found::Ssh(mut sh) => {
            let lib = reader_library::on_remarkable_ssh(&mut sh);
            reader_library::backup_remarkable_ssh(&mut sh, &lib, &dir)
        }
    })
}

/// The newest backup of the reader `key` under [`backups_root`]: its folder.
pub fn readers_last_backup(key: String) -> Option<String> {
    last_backup_in(&backups_root().join(folder_of(&key)))
}

fn last_backup_in(dir: &Path) -> Option<String> {
    let mut dated: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("backup.json").is_file())
        .collect();
    dated.sort();
    dated.pop().map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_becomes_a_folder_name_and_the_newest_backup_is_found() {
        assert_eq!(folder_of("kobo/N418170012345"), "kobo-N418170012345");
        assert_eq!(folder_of("pocketbook/My Reader"), "pocketbook-My-Reader");
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(last_backup_in(tmp.path()), None);
        for d in [
            "2026-11-01T08-00-00Z",
            "2026-11-03T08-00-00Z",
            "2026-11-04T08-00-00Z",
        ] {
            std::fs::create_dir_all(tmp.path().join(d)).unwrap();
        }
        // only a finished backup (with its manifest) counts
        for d in ["2026-11-01T08-00-00Z", "2026-11-03T08-00-00Z"] {
            std::fs::write(tmp.path().join(d).join("backup.json"), b"{}").unwrap();
        }
        assert!(
            last_backup_in(tmp.path())
                .unwrap()
                .ends_with("2026-11-03T08-00-00Z")
        );
    }
}
