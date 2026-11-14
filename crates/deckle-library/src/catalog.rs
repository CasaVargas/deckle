// SPDX-License-Identifier: GPL-3.0-or-later
//! The Books tab's library (Books and Kindle design §8): every book Deckle can see, from its
//! own library, calibre and your folders, one entry a book, with its series and the Kindles it
//! has been sent to.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::sources::{Origin, Sources};
use crate::upgrade::Upgrades;
use crate::{Library, SeriesRef};

/// Where an entry's file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum From {
    /// Deckle's own library: its details can be fixed.
    Library,
    Calibre,
    Folder,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    /// The file a send reads.
    pub path: PathBuf,
    pub from: From,
    pub title: String,
    pub authors: Vec<String>,
    pub series: Option<SeriesRef>,
    pub isbn: Option<String>,
    /// The book's folder in Deckle's library.
    pub library_dir: Option<PathBuf>,
    /// The Kindles (profile keys) it has been sent to, or upgraded on.
    pub sent_to: Vec<String>,
    /// When it was added to Deckle's library, RFC 3339.
    pub added: Option<String>,
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The entries kept so far, by what makes two the same book.
#[derive(Default)]
struct Seen {
    paths: HashSet<PathBuf>,
    isbns: HashSet<String>,
    names: HashSet<(String, String)>,
}

impl Seen {
    fn add(&mut self, e: &Entry) {
        self.paths.insert(e.path.clone());
        if let Some(i) = &e.isbn {
            self.isbns.insert(norm(i));
        }
        if !e.title.is_empty() {
            self.names.insert(name_key(e));
        }
    }

    fn has(&self, e: &Entry) -> bool {
        self.paths.contains(&e.path)
            || e.isbn
                .as_ref()
                .is_some_and(|i| self.isbns.contains(&norm(i)))
            || self.names.contains(&name_key(e))
    }
}

/// Title and first author, compared without case or punctuation. Two entries are the same book
/// when their ISBNs match, else when these do (and the kept one has a title).
fn name_key(e: &Entry) -> (String, String) {
    let first = e.authors.first().map(|s| norm(s)).unwrap_or_default();
    (norm(&e.title), first)
}

/// Deckle's library first, then calibre's and your folders' books that aren't in it. `upgrades`
/// counts a calibre or folder book as on each Kindle it upgraded a copy on.
pub fn catalog(library: Option<&Library>, sources: &Sources, upgrades: &Upgrades) -> Vec<Entry> {
    let mut out: Vec<Entry> = library
        .map(Library::list)
        .unwrap_or_default()
        .into_iter()
        .map(|b| {
            let m = &b.sidecar.metadata;
            Entry {
                path: b.source_path(),
                from: From::Library,
                title: m.title.clone(),
                authors: m.authors.clone(),
                series: m.series.clone().filter(|s| !s.name.trim().is_empty()),
                isbn: m.isbn.clone(),
                sent_to: b
                    .sidecar
                    .sends
                    .iter()
                    .filter(|s| !s.device.starts_with("folder:"))
                    .map(|s| s.device.clone())
                    .collect(),
                added: Some(b.sidecar.added.clone()),
                library_dir: Some(b.dir),
            }
        })
        .collect();
    // Each kept entry's path, ISBN and name, so each source is looked up rather than compared
    // with every entry so far (#85: quadratic in the books).
    let mut seen = Seen::default();
    out.iter().for_each(|e| seen.add(e));
    for s in &sources.list {
        let entry = Entry {
            path: s.path.clone(),
            from: match s.origin {
                Origin::Calibre => From::Calibre,
                _ => From::Folder,
            },
            title: s.title.clone(),
            authors: s.authors.clone(),
            series: s.series.clone().filter(|x| !x.name.trim().is_empty()),
            isbn: s.isbn.clone(),
            library_dir: None,
            sent_to: Vec::new(),
            added: None,
        };
        if !seen.has(&entry) {
            seen.add(&entry);
            out.push(entry);
        }
    }
    let mut at: HashMap<PathBuf, usize> = HashMap::new();
    for (i, e) in out.iter().enumerate() {
        at.entry(e.path.clone()).or_insert(i);
    }
    for u in &upgrades.upgrades {
        if let Some(&i) = at.get(&u.source)
            && !out[i].sent_to.contains(&u.device)
        {
            out[i].sent_to.push(u.device.clone());
        }
    }
    out
}

/// The cover to show for an entry: calibre's `cover.jpg` beside the book, else the EPUB's own.
pub fn cover_of(path: &Path) -> Option<Vec<u8>> {
    if let Some(dir) = path.parent() {
        let calibre = dir.join("cover.jpg");
        if dir.join("metadata.opf").is_file()
            && let Ok(b) = std::fs::read(&calibre)
        {
            return Some(b);
        }
    }
    let epub = path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("epub"));
    epub.then(|| deckle_core::preflight::cover_of(path))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::Source;

    fn source(title: &str, origin: Origin, isbn: Option<&str>) -> Source {
        Source {
            path: PathBuf::from(format!("/books/{title}.epub")),
            origin,
            title: title.into(),
            authors: vec!["A. Writer".into()],
            isbn: isbn.map(Into::into),
            asin: None,
            calibre_id: None,
            series: Some(SeriesRef {
                name: "The Lamps".into(),
                index: Some(2.0),
            }),
        }
    }

    #[test]
    fn calibre_and_folders_join_the_library_once_each_and_upgrades_count_as_sent() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::new(tmp.path().join("Library"));
        let added = lib
            .add(&deckle_kfx::fixtures::path("deckle-mini.epub"))
            .unwrap()
            .book;
        let mine = &added.sidecar.metadata;
        let mut twin = source(&mine.title, Origin::Calibre, None);
        twin.authors = mine.authors.clone();
        let sources = Sources {
            list: vec![
                twin,
                source("Wick", Origin::Calibre, Some("978-1")),
                source("Wick Again", Origin::Folder, Some("9781")),
                source("Flame", Origin::Folder, None),
            ],
            warnings: Vec::new(),
        };
        let upgrades = Upgrades {
            version: 1,
            upgrades: vec![crate::upgrade::UpgradeRecord {
                device: "mtp:scribe".into(),
                old_file: "Flame.azw3".into(),
                new_file: None,
                title: "Flame".into(),
                source: "/books/Flame.epub".into(),
                origin: Some(Origin::Folder),
                kept: "/kept".into(),
                at: "2026-10-23T00:00:00Z".into(),
            }],
        };
        let all = catalog(Some(&lib), &sources, &upgrades);
        let titles: Vec<(&str, From)> = all.iter().map(|e| (e.title.as_str(), e.from)).collect();
        assert_eq!(
            titles,
            [
                (mine.title.as_str(), From::Library),
                ("Wick", From::Calibre),
                ("Flame", From::Folder)
            ]
        );
        assert!(all[0].library_dir.is_some());
        assert_eq!(all[2].sent_to, ["mtp:scribe"]);
        assert_eq!(all[1].series.as_ref().unwrap().index, Some(2.0));
    }

    #[test]
    fn a_large_calibre_library_is_catalogued_quickly() {
        // #85: each book was compared with every entry so far, rebuilding its keys each time.
        let list: Vec<Source> = (0..20_000)
            .map(|i| source(&format!("Book {i}"), Origin::Calibre, None))
            .chain([source("Book 7", Origin::Folder, None)])
            .collect();
        let sources = Sources {
            list,
            warnings: Vec::new(),
        };
        let started = std::time::Instant::now();
        let all = catalog(None, &sources, &Upgrades::default());
        assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
        assert_eq!(all.len(), 20_000, "the folder's Book 7 is calibre's");
    }
}
