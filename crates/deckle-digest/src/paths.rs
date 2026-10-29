// SPDX-License-Identifier: GPL-3.0-or-later
//! The daemon's start-up check of configured folders (Linux Docker design): a config carried
//! over from a Mac names folders that aren't in the container. A folder is fine when it exists
//! or its parent does (it's created on use); otherwise nothing could ever write there.
//!
//! The library root lives in `deckle-library`'s own config, not a digest `Config`, so it isn't
//! checked here.

use std::path::{Path, PathBuf};

use crate::Config;
use crate::inbox::expand_home;

/// A configured folder that doesn't exist and can't be created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathProblem {
    /// The paper's slug, `main` for `digest.toml`.
    pub paper: String,
    /// The key as written in the TOML, so the log names what to edit.
    pub key: &'static str,
    /// The folder, `~` expanded.
    pub path: PathBuf,
}

/// Configured folders that don't exist and can't be created (parent missing too).
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

fn usable(path: &Path) -> bool {
    path.exists()
        || path
            .parent()
            .is_none_or(|p| p.as_os_str().is_empty() || p.exists())
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
        let p = check(&[with_export("/Users/someone/iCloud/Deckle")]);
        assert_eq!(
            p,
            vec![PathProblem {
                paper: "main".into(),
                key: "export.dir",
                path: "/Users/someone/iCloud/Deckle".into(),
            }]
        );
    }

    #[test]
    fn missing_folder_with_existing_parent_is_fine() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("new");
        assert!(check(&[with_export(&dir.to_string_lossy())]).is_empty());
    }
}
