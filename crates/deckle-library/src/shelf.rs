// SPDX-License-Identifier: GPL-3.0-or-later
//! A Kindle's books next to the library (library design §4): which library book each one is,
//! and on MTP (where books aren't read) the details the library's send record knows.

use deckle_core::devices::same_device;
use deckle_core::inventory::DeviceBook;

use crate::freshness::{Current, Reason, freshness};
use crate::{Book, Library};

/// A book on the Kindle and, when the library has it, the library's copy.
#[derive(Clone, Debug)]
pub struct Shelved {
    pub book: DeviceBook,
    pub library: Option<Book>,
    /// Why Deckle could build it better now (Kindle-updates design §2); empty: current, or
    /// not in the library.
    pub updates: Vec<Reason>,
    /// Another book on this Kindle has the same ASIN, or the same title and first author.
    pub duplicate: bool,
}

/// What makes two books on a Kindle the same: the ASIN, else title and first author.
fn identity(b: &DeviceBook) -> Option<String> {
    if let Some(a) = &b.asin {
        return Some(format!("asin:{a}"));
    }
    let norm = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let title = norm(b.title.as_deref()?);
    (!title.is_empty()).then(|| {
        format!(
            "book:{title}|{}",
            norm(b.authors.first().map(String::as_str).unwrap_or(""))
        )
    })
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
/// device listing couldn't read. Then judges updates against `current`, and duplicates.
pub fn shelve(
    books: Vec<DeviceBook>,
    library: Option<&Library>,
    device: &str,
    current: &Current,
) -> Vec<Shelved> {
    let all = library.map(Library::list).unwrap_or_default();
    let mut shelf: Vec<Shelved> = books
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
                    .any(|s| same_device(&s.device, device) && s.file == book.file)
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
            }
            // The record of the send that put this very file here; the listing's `.sdr` flags
            // say what is on the Kindle now.
            let send = found
                .as_ref()
                .and_then(|lib| lib.sidecar.send_to(device))
                .filter(|s| s.file == book.file);
            let updates = match send {
                Some(s) => {
                    book.cde = book.cde.take().or_else(|| s.cde.clone());
                    book.asin = book.asin.take().or_else(|| s.asin.clone());
                    // Over MTP the KFX isn't read: the send knew where the book ends.
                    book.end = book.end.or(s.end);
                    freshness(s, book.xray, book.word_wise, current)
                }
                None => Vec::new(),
            };
            Shelved {
                book,
                library: found,
                updates,
                duplicate: false,
            }
        })
        .collect();
    let ids: Vec<Option<String>> = shelf.iter().map(|s| identity(&s.book)).collect();
    for (i, s) in shelf.iter_mut().enumerate() {
        s.duplicate = ids[i]
            .as_ref()
            .is_some_and(|id| ids.iter().filter(|x| x.as_ref() == Some(id)).count() > 1);
    }
    shelf
}
