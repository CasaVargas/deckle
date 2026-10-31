// SPDX-License-Identifier: GPL-3.0-or-later
//! Shelves on a reader that follow calibre (calibre bridge design §6): which books on a drive
//! are which calibre books, and the changes that make a reader's shelves (a Kobo's) or
//! collections (KOReader's) what the rules say. Only shelves Deckle made lose books or go;
//! a shelf the owner made with the same name is joined, never emptied.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Folders deep the walk for books goes.
const DEPTH: usize = 6;
/// Books looked at before the walk stops.
const MAX_BOOKS: usize = 20_000;
/// Folders never walked into.
const SKIP: &[&str] = &[
    ".kobo",
    ".adds",
    ".Trashes",
    ".Spotlight-V100",
    ".fseventsd",
    "System Volume Information",
    "system",
    "koreader",
];

/// A book on a drive calibre knows: its path from the drive's root (`/`-separated) and its
/// calibre id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnDrive {
    pub path: String,
    pub calibre_id: String,
}

/// Every EPUB on the drive at `root` that carries a calibre id.
pub fn books_on(root: &Path) -> Vec<OnDrive> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    walk(root, root, 0, &mut out, &mut seen);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<OnDrive>, seen: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        if *seen >= MAX_BOOKS {
            return;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            if depth < DEPTH && !SKIP.contains(&name.as_str()) && !name.ends_with(".sdr") {
                walk(root, &path, depth + 1, out, seen);
            }
        } else if name.to_ascii_lowercase().ends_with(".epub") && !name.starts_with('.') {
            *seen += 1;
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if let Some(id) = crate::reading::calibre_id_of(root, &rel) {
                out.push(OnDrive {
                    path: rel,
                    calibre_id: id,
                });
            }
        }
    }
}

/// Shelf name → the books on it (paths as the reader names them).
pub type Shelves = BTreeMap<String, BTreeSet<String>>;

/// A rule worked out against calibre: the shelf it makes, the readers it's for (`all`, or
/// reader keys), and the calibre ids of its books.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSet {
    pub name: String,
    pub readers: Vec<String>,
    pub books: BTreeSet<String>,
}

impl RuleSet {
    pub fn for_reader(&self, key: &str) -> bool {
        self.readers
            .iter()
            .any(|r| r == "all" || r.eq_ignore_ascii_case(key))
    }
}

/// The shelves `sets` want on reader `key`, which holds `books`: each shelf the paths of its
/// books there. A shelf with none of the reader's books isn't made.
pub fn wanted(sets: &[RuleSet], key: &str, books: &[OnDrive]) -> Shelves {
    let mut out = Shelves::new();
    for set in sets {
        if !set.for_reader(key) || set.name.trim().is_empty() {
            continue;
        }
        let paths: BTreeSet<String> = books
            .iter()
            .filter(|b| set.books.contains(&b.calibre_id))
            .map(|b| b.path.clone())
            .collect();
        if !paths.is_empty() {
            out.entry(set.name.trim().to_owned())
                .or_default()
                .extend(paths);
        }
    }
    out
}

/// What to change on a reader.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    /// New shelves to make.
    pub create: Vec<String>,
    /// Deckle's shelves no rule makes any more.
    pub delete: Vec<String>,
    pub add: Shelves,
    /// Only ever from Deckle's own shelves.
    pub remove: Shelves,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.create.is_empty()
            && self.delete.is_empty()
            && self.add.is_empty()
            && self.remove.is_empty()
    }

    /// Books added and taken off, for a line in the app.
    pub fn counts(&self) -> (usize, usize) {
        (
            self.add.values().map(BTreeSet::len).sum(),
            self.remove.values().map(BTreeSet::len).sum(),
        )
    }
}

/// The changes that take `current` to `wanted`. `ours` are the shelves Deckle made: only they
/// lose books or go. A wanted shelf the owner already has is joined (books added, none
/// taken off) and doesn't become Deckle's.
pub fn diff(wanted: &Shelves, current: &Shelves, ours: &BTreeSet<String>) -> Diff {
    let mut d = Diff::default();
    for (name, books) in wanted {
        let have = current.get(name);
        if have.is_none() {
            d.create.push(name.clone());
        }
        let add: BTreeSet<String> = books
            .iter()
            .filter(|b| have.is_none_or(|h| !h.contains(*b)))
            .cloned()
            .collect();
        if !add.is_empty() {
            d.add.insert(name.clone(), add);
        }
        if let Some(h) = have
            && ours.contains(name)
        {
            let gone: BTreeSet<String> = h.difference(books).cloned().collect();
            if !gone.is_empty() {
                d.remove.insert(name.clone(), gone);
            }
        }
    }
    for name in ours {
        if !wanted.contains_key(name) && current.contains_key(name) {
            d.delete.push(name.clone());
        }
    }
    d
}

/// The shelves Deckle made on a reader, kept beside the reader's own data
/// (`.kobo/deckle-shelves.json`, `<KOReader>/deckle-shelves.json`).
pub fn ours(file: &Path) -> BTreeSet<String> {
    std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Records that Deckle's shelves are now `before` with `d` applied.
pub fn record_ours(
    file: &Path,
    before: &BTreeSet<String>,
    d: &Diff,
    wanted: &Shelves,
) -> std::io::Result<()> {
    let mut now = before.clone();
    // A shelf Deckle created is Deckle's; one it joined stays the owner's.
    now.extend(d.create.iter().cloned());
    for gone in &d.delete {
        now.remove(gone);
    }
    now.retain(|n| wanted.contains_key(n));
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&now)?)?;
    std::fs::rename(tmp, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shelves(list: &[(&str, &[&str])]) -> Shelves {
        list.iter()
            .map(|(n, b)| (n.to_string(), b.iter().map(|x| x.to_string()).collect()))
            .collect()
    }

    #[test]
    fn deckles_shelves_follow_the_rules_and_the_owners_are_only_joined() {
        let wanted = shelves(&[
            ("Sci-Fi", &["a", "b"]),
            ("Favorites", &["c"]),
            ("New", &["d"]),
        ]);
        let current = shelves(&[
            ("Sci-Fi", &["a", "old"]),
            ("Favorites", &["x"]),
            ("Gone", &["e"]),
            ("Mine", &["f"]),
        ]);
        let ours: BTreeSet<String> = ["Sci-Fi", "Gone"].into_iter().map(String::from).collect();
        let d = diff(&wanted, &current, &ours);
        assert_eq!(d.create, ["New"]);
        assert_eq!(d.delete, ["Gone"]);
        assert_eq!(
            d.add,
            shelves(&[("Sci-Fi", &["b"]), ("Favorites", &["c"]), ("New", &["d"])])
        );
        // "Favorites" is the owner's: joined, x stays. "Mine" isn't touched.
        assert_eq!(d.remove, shelves(&[("Sci-Fi", &["old"])]));
        assert_eq!(d.counts(), (3, 1));
        assert!(
            diff(&wanted, &wanted, &ours).is_empty(),
            "nothing to do when they match"
        );

        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("deckle-shelves.json");
        record_ours(&f, &ours, &d, &wanted).unwrap();
        let now = super::ours(&f);
        assert_eq!(now.into_iter().collect::<Vec<_>>(), ["New", "Sci-Fi"]);
    }
}
