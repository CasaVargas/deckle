// SPDX-License-Identifier: GPL-3.0-or-later
//! A Kindle's books next to the library (library design §4): which library book each one is,
//! and on MTP (where books aren't read) the details the library's send record knows.

use deckle_core::inventory::DeviceBook;

use crate::{Book, Library};

/// A book on the Kindle and, when the library has it, the library's copy.
#[derive(Clone, Debug)]
pub struct Shelved {
    pub book: DeviceBook,
    pub library: Option<Book>,
}

/// The cover image of a library book's source (EPUB only), as stored.
pub fn source_cover(book: &Book) -> Option<Vec<u8>> {
    let path = book.source_path();
    let info = deckle_core::inspect::inspect(&path).ok()?;
    let mut reader = deckle_core::epub::EpubReader::open(&path).ok()?;
    let name = reader.resolve(info.cover_href.as_deref()?);
    reader.read_bytes(&name).ok()
}

/// Matches each of `books` (on the device `device`, a profile key) to the library: by the
/// KFX's ASIN, else by a send record naming the file. A match by record fills in what the
/// device listing couldn't read.
pub fn shelve(books: Vec<DeviceBook>, library: Option<&Library>, device: &str) -> Vec<Shelved> {
    let all = library.map(Library::list).unwrap_or_default();
    books
        .into_iter()
        .map(|mut book| {
            let by_asin = book
                .asin
                .as_deref()
                .and_then(|a| all.iter().find(|b| b.sidecar.asin == a));
            let by_send = all.iter().find(|b| {
                b.sidecar
                    .sends
                    .iter()
                    .any(|s| s.device == device && s.file == book.file)
            });
            let found = by_asin.or(by_send).cloned();
            if let Some(lib) = &found {
                book.from_deckle = true;
                let m = &lib.sidecar.metadata;
                if book.title.is_none() {
                    book.title = Some(m.title.clone()).filter(|t| !t.is_empty());
                }
                // A KFX keeps authors in sort form ("Le Guin, Ursula K."); show the names.
                if !m.authors.is_empty() {
                    book.authors = m.authors.clone();
                }
                if let Some(s) = lib.sidecar.send_to(device).filter(|s| s.file == book.file) {
                    book.cde = book.cde.take().or_else(|| s.cde.clone());
                    book.asin = book.asin.take().or_else(|| s.asin.clone());
                    // The device's own `.sdr` is the truth when it was listed.
                    book.xray |= s.xray;
                    book.word_wise |= s.word_wise;
                }
            }
            Shelved {
                book,
                library: found,
            }
        })
        .collect()
}
