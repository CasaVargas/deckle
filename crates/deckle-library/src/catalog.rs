// SPDX-License-Identifier: GPL-3.0-or-later
//! The Books tab's library (Books and Kindle design §8): every book Deckle can see, from its
//! own library, calibre and your folders, one entry a book, with its series and the Kindles it
//! has been sent to.

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

/// What makes two entries the same book: the ISBN, else title and first author.
fn same(a: &Entry, b: &Entry) -> bool {
    if let (Some(x), Some(y)) = (&a.isbn, &b.isbn)
        && norm(x) == norm(y)
    {
        return true;
    }
    let first = |e: &Entry| e.authors.first().map(|s| norm(s)).unwrap_or_default();
    !a.title.is_empty() && norm(&a.title) == norm(&b.title) && first(a) == first(b)
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
        if !out.iter().any(|e| e.path == entry.path || same(e, &entry)) {
            out.push(entry);
        }
    }
    for u in &upgrades.upgrades {
        if let Some(e) = out.iter_mut().find(|e| e.path == u.source)
            && !e.sent_to.contains(&u.device)
        {
            e.sent_to.push(u.device.clone());
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
}
