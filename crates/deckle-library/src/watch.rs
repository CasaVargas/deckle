// SPDX-License-Identifier: GPL-3.0-or-later
//! Watch a folder (Books and Kindle design §8, off by default): EPUBs that land in it, your
//! Downloads say, are added to the library. Each file is tried once, by name, size and time.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Library;

/// `watched.json` beside `library.json`: the files already tried.
pub fn seen_path(config: &Path) -> PathBuf {
    config.with_file_name("watched.json")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Seen {
    files: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Watched {
    /// Titles added.
    pub added: Vec<String>,
    /// `(file, error)`
    pub failed: Vec<(String, String)>,
}

fn key(path: &Path) -> Option<String> {
    let m = std::fs::metadata(path).ok()?;
    let at = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("{}|{}|{at}", path.display(), m.len()))
}

/// Adds the EPUBs in `dir` (not its subfolders) that haven't been tried, remembering them in
/// `seen`. A file still being written (changed in the last few seconds) waits for the next look.
pub fn scan(lib: &Library, dir: &Path, seen: &Path) -> Result<Watched, String> {
    let mut state: Seen = std::fs::read(seen)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut out = Watched::default();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("epub"))
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    let settled = |p: &Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() >= 5)
    };
    for f in files {
        let Some(k) = key(&f) else { continue };
        if state.files.contains(&k) || !settled(&f) {
            continue;
        }
        match lib.add(&f) {
            Ok(a) => out.added.push(a.book.sidecar.metadata.title),
            Err(e) => out.failed.push((f.to_string_lossy().into_owned(), e)),
        }
        state.files.insert(k);
    }
    let json = serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?;
    crate::write_atomic(seen, &json)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_epub_is_added_once_and_other_files_are_left() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::new(tmp.path().join("Library"));
        let downloads = tmp.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let epub = downloads.join("mini.epub");
        std::fs::copy(deckle_kfx::fixtures::path("deckle-mini.epub"), &epub).unwrap();
        std::fs::write(downloads.join("notes.txt"), b"not a book").unwrap();
        // As if it landed a minute ago.
        let then = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&epub)
            .unwrap()
            .set_modified(then)
            .unwrap();
        let seen = tmp.path().join("watched.json");
        let first = scan(&lib, &downloads, &seen).unwrap();
        assert_eq!(first.added.len(), 1);
        assert_eq!(lib.list().len(), 1);
        let again = scan(&lib, &downloads, &seen).unwrap();
        assert!(again.added.is_empty() && again.failed.is_empty());
    }
}
