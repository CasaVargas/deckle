// SPDX-License-Identifier: GPL-3.0-or-later
//! The daemon's start-up check of configured folders (Linux Docker design): a config carried
//! over from a Mac names folders that aren't in the container. A missing folder is created on
//! use, so it's fine while its nearest existing ancestor is a folder other than the filesystem
//! root. `/Users/…` from a Mac, where only `/` exists, is reported.
//!
//! The library root lives in `deckle-library`'s own config, not a digest `Config`, so it isn't
//! checked here.

use std::path::{Path, PathBuf};

use crate::Config;
use crate::inbox::expand_home;

/// A configured folder with nothing to grow from: its nearest existing ancestor is the
/// filesystem root, or isn't a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathProblem {
    /// The paper's slug, `main` for `digest.toml`.
    pub paper: String,
    /// The key as written in the TOML, so the log names what to edit.
    pub key: &'static str,
    /// The folder, `~` expanded.
    pub path: PathBuf,
}

/// Configured folders with nothing to grow from: the nearest existing ancestor is the root (or a
/// bare drive root) or not a folder.
pub fn check(papers: &[Config]) -> Vec<PathProblem> {
    let mut out = Vec::new();
    for cfg in papers {
        let paper = cfg
            .paper
            .as_ref()
            .map_or_else(|| "main".to_owned(), |p| p.slug.clone());
        let mut each = |key: &'static str, raw: &str| {
            if raw.trim().is_empty() {
                return;
            }
            let path = expand_home(raw);
            if !usable(&path) {
                out.push(PathProblem {
                    paper: paper.clone(),
                    key,
                    path,
                });
            }
        };
        for e in &cfg.export {
            each("export.dir", &e.dir);
        }
        each("reading.inbox", &cfg.reading.inbox);
        each("morgue.folder", &cfg.morgue.folder);
    }
    out
}

/// Fine unless the nearest existing ancestor is the filesystem root (or a bare drive root) or
/// isn't a folder: a fresh `$HOME` lacks `~/Deckle`, which is created on use, but `/Users/…`
/// or `C:\Users\…` copied from a Mac or PC has nothing to grow from.
fn usable(path: &Path) -> bool {
    if !path.is_absolute() {
        // A Windows drive path on Linux, or a rooted one without a drive on Windows, is a name
        // that can never be made.
        let b = path.to_string_lossy();
        return !(b.starts_with(['/', '\\']) || b.as_bytes().get(1) == Some(&b':'));
    }
    match path.ancestors().find(|a| a.exists()) {
        Some(a) => a.is_dir() && a.parent().is_some(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ExportConfig;

    fn with_export(dir: &str) -> Config {
        let mut cfg = Config::default();
        cfg.export.push(ExportConfig {
            dir: dir.into(),
            ..Default::default()
        });
        // Defaults under `~` would add noise: point them somewhere real.
        let here = std::env::temp_dir().to_string_lossy().into_owned();
        cfg.reading.inbox = here.clone();
        cfg.morgue.folder = here;
        cfg
    }

    #[test]
    fn check_names_missing_mac_export_dir() {
        let p = check(&[with_export("/no-such-root/someone/iCloud/Deckle")]);
        assert_eq!(
            p,
            vec![PathProblem {
                paper: "main".into(),
                key: "export.dir",
                path: "/no-such-root/someone/iCloud/Deckle".into(),
            }]
        );
    }

    #[test]
    fn default_paths_under_fresh_home_are_fine() {
        // What `~/Deckle/Inbox` and `~/Deckle/Morgue` expand to under a new `$HOME`.
        let home = tempfile::tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.reading.inbox = home
            .path()
            .join("Deckle/Inbox")
            .to_string_lossy()
            .into_owned();
        cfg.morgue.folder = home
            .path()
            .join("Deckle/Morgue")
            .to_string_lossy()
            .into_owned();
        assert!(check(&[cfg]).is_empty());
    }

    #[test]
    fn windows_and_volume_paths_are_flagged() {
        for p in ["/no-such-volumes/Backup/Deckle", "Q:\\no-such\\Deckle"] {
            assert_eq!(check(&[with_export(p)]).len(), 1, "{p}");
        }
    }

    #[test]
    fn under_a_file_is_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("file");
        std::fs::write(&f, "").unwrap();
        assert_eq!(
            check(&[with_export(&f.join("x/y").to_string_lossy())]).len(),
            1
        );
    }

    #[test]
    fn missing_folder_with_existing_parent_is_fine() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("new");
        assert!(check(&[with_export(&dir.to_string_lossy())]).is_empty());
    }
}
