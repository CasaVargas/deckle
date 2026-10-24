// SPDX-License-Identifier: GPL-3.0-or-later
//! Metadata search across the library, live inventories and recorded sends.
use std::path::Path;

use deckle_core::deliver::{Conn, Target};
use deckle_core::devices::{same_device, seen_mass_storage, seen_mtp};
use deckle_core::inventory::DeviceBook;
use serde::Serialize;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use crate::{Book, Library, SeriesRef};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    Library,
    Connected,
    RecordedSend,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Location {
    pub kind: LocationKind,
    pub device: Option<String>,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Hit {
    pub title: String,
    pub authors: Vec<String>,
    pub series: Option<SeriesRef>,
    pub locations: Vec<Location>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub results: Vec<Hit>,
    pub warnings: Vec<String>,
}

pub struct DeviceInventory {
    pub key: String,
    pub name: String,
    pub books: Vec<DeviceBook>,
}

fn folded(s: &str) -> String {
    s.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Every whitespace-separated term must occur somewhere in the metadata.
pub fn matches(title: &str, authors: &[String], series: Option<&str>, query: &str) -> bool {
    let haystack = folded(&format!(
        "{title} {} {}",
        authors.join(" "),
        series.unwrap_or_default()
    ));
    folded(query)
        .split_whitespace()
        .all(|term| haystack.contains(term))
}

/// Only strong identifiers merge device rows with a library book, never title alone.
fn identifies(book: &Book, device: &DeviceInventory, row: &DeviceBook) -> bool {
    book.sidecar
        .sends
        .iter()
        .any(|s| same_device(&s.device, &device.key) && s.file == row.file)
        || row.asin.as_ref().is_some_and(|asin| {
            !asin.is_empty()
                && (asin == &book.sidecar.asin
                    || book
                        .sidecar
                        .sends
                        .iter()
                        .any(|s| s.asin.as_ref() == Some(asin)))
        })
}

/// Pure search: a successfully read live inventory supersedes historical sends to that device.
pub fn search(books: &[Book], devices: &[DeviceInventory], query: &str) -> Vec<Hit> {
    let mut hits: Vec<Hit> = books
        .iter()
        .map(|b| {
            let m = &b.sidecar.metadata;
            let mut locations = vec![Location {
                kind: LocationKind::Library,
                device: None,
                name: "Library".into(),
                path: b.source_path().to_string_lossy().into_owned(),
            }];
            for s in &b.sidecar.sends {
                if !(s.device.starts_with("ms:") || s.device.starts_with("mtp:"))
                    || devices.iter().any(|d| same_device(&s.device, &d.key))
                {
                    continue;
                }
                locations.push(Location {
                    kind: LocationKind::RecordedSend,
                    device: Some(s.device.clone()),
                    name: if s.device_name.is_empty() {
                        s.device.clone()
                    } else {
                        s.device_name.clone()
                    },
                    path: s.file.clone(),
                });
            }
            Hit {
                title: m.title.clone(),
                authors: m.authors.clone(),
                series: m.series.clone(),
                locations,
            }
        })
        .collect();
    for d in devices {
        for row in &d.books {
            let location = Location {
                kind: LocationKind::Connected,
                device: Some(d.key.clone()),
                name: d.name.clone(),
                path: row.file.clone(),
            };
            if let Some(i) = books.iter().position(|b| identifies(b, d, row)) {
                if !hits[i].locations.contains(&location) {
                    hits[i].locations.push(location);
                }
            } else {
                hits.push(Hit {
                    title: row
                        .title
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| row.file.clone()),
                    authors: row.authors.clone(),
                    series: None,
                    locations: vec![location],
                });
            }
        }
    }
    hits.retain(|h| {
        matches(
            &h.title,
            &h.authors,
            h.series.as_ref().map(|s| s.name.as_str()),
            query,
        )
    });
    hits.sort_by_key(|h| (folded(&h.title), h.authors.join(", ")));
    hits
}

/// Read each target once. Failed devices produce warnings while other results remain usable.
pub fn collect(config: &Path, targets: &[Target], query: &str) -> Report {
    let books = Library::configured(config)
        .map(|(l, _)| l.list())
        .unwrap_or_default();
    let mut report = Report::default();
    let mut devices = Vec::new();
    let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
    for target in targets {
        let seen = match target {
            Target::MassStorage(d) => seen_mass_storage(d),
            Target::Mtp(d) => seen_mtp(d),
            Target::Folder(_) => continue,
        };
        match deckle_core::inventory::list_on(&mut Conn::new(target), Some(&cache)) {
            Ok(inv) => devices.push(DeviceInventory {
                key: seen.key,
                name: seen.model_hint,
                books: inv.books,
            }),
            Err(e) => report.warnings.push(format!("{}: {e}", seen.model_hint)),
        }
    }
    report.results = search(&books, &devices, query);
    report
}
