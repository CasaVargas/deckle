// SPDX-License-Identifier: GPL-3.0-or-later
//! What's on a Kindle (library design §4): the books in `documents/`, their X-Ray and Word
//! Wise sidecars, and free space, over the caller's connection (one MTP session). Remove takes
//! a book and its `.sdr` folder off again.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use deckle_transport::session::{Mtp, MtpError};

use crate::deliver::{Conn, DeliverError, Target, is_xray_sidecar};

/// Book files the Kindle library shows.
pub const BOOK_FORMATS: [&str; 9] = [
    "kfx", "azw3", "azw", "mobi", "prc", "pdf", "txt", "epub", "kpf",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    pub total: u64,
    pub free: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceBook {
    /// The path under `documents/`: a name, or `<folder>/<name>`.
    pub file: String,
    pub size: u64,
    /// The extension, lowercase: `kfx`, `azw3`, `pdf`, …
    pub format: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    /// `PDOC` | `EBOK` (KFX).
    pub cde: Option<String>,
    pub asin: Option<String>,
    /// Deckle built it: its ASIN carries the `DECK` marker (the caller also matches library
    /// send records).
    pub from_deckle: bool,
    pub xray: bool,
    pub word_wise: bool,
    /// The KFX's cover, JPEG.
    #[serde(skip)]
    pub cover: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Inventory {
    pub books: Vec<DeviceBook>,
    pub space: Option<Space>,
}

fn format_of(name: &str) -> Option<String> {
    let ext = Path::new(name)
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();
    BOOK_FORMATS.contains(&ext.as_str()).then_some(ext)
}

fn is_sdr(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("sdr"))
}

fn is_word_wise(name: &str) -> bool {
    name.starts_with("LanguageLayer.") && name.ends_with(".kll")
}

/// `<stem>.sdr` for a book file.
fn sdr_name(file: &str) -> String {
    Path::new(file)
        .with_extension("sdr")
        .to_string_lossy()
        .into_owned()
}

/// A Deckle-made ASIN: `B0` + four + `DECK` (design §5).
pub fn is_generated_asin(asin: &str) -> bool {
    asin.len() == 10 && asin.starts_with("B0") && asin.ends_with("DECK")
}

/// The KFX's own details: title, authors, CDE type, ASIN, cover.
fn read_details(book: &mut DeviceBook, kfx: &[u8]) {
    let Ok(k) = crate::inspect::inspect_kfx(kfx) else {
        return;
    };
    book.title = Some(k.info.title).filter(|t| !t.is_empty());
    book.authors = k.info.creators.into_iter().map(|c| c.name).collect();
    book.cde = k.cde_type;
    book.from_deckle = k.asin.as_deref().is_some_and(is_generated_asin);
    book.asin = k.asin;
    book.cover = k.cover;
}

/// A KFX's details, cached under `cache` by path, size and modified time, so a listing
/// doesn't re-read every book.
fn details_cached(book: &mut DeviceBook, path: &Path, cache: Option<&Path>) {
    #[derive(Serialize, Deserialize)]
    struct Cached {
        title: Option<String>,
        authors: Vec<String>,
        cde: Option<String>,
        asin: Option<String>,
    }
    let stamp = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key = format!(
        "{:016x}",
        fnv(&format!("{}|{}|{stamp}", path.display(), book.size))
    );
    let files = cache.map(|c| (c.join(format!("{key}.json")), c.join(format!("{key}.jpg"))));
    if let Some((json, jpg)) = &files
        && let Some(c) = std::fs::read(json)
            .ok()
            .and_then(|b| serde_json::from_slice::<Cached>(&b).ok())
    {
        book.title = c.title;
        book.authors = c.authors;
        book.cde = c.cde;
        book.from_deckle = c.asin.as_deref().is_some_and(is_generated_asin);
        book.asin = c.asin;
        book.cover = std::fs::read(jpg).ok();
        return;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    read_details(book, &bytes);
    if let Some((json, jpg)) = files {
        let c = Cached {
            title: book.title.clone(),
            authors: book.authors.clone(),
            cde: book.cde.clone(),
            asin: book.asin.clone(),
        };
        if let Some(dir) = json.parent()
            && std::fs::create_dir_all(dir).is_ok()
        {
            let _ = std::fs::write(&json, serde_json::to_vec(&c).unwrap_or_default());
            if let Some(cover) = &book.cover {
                let _ = std::fs::write(&jpg, cover);
            }
        }
    }
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn sidecar_flags(book: &mut DeviceBook, names: impl IntoIterator<Item = String>) {
    for n in names {
        book.xray |= is_xray_sidecar(&n);
        book.word_wise |= is_word_wise(&n);
    }
}

fn names_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Books in a `documents` folder on disk and one level of subfolders (not `.sdr`).
fn list_dir(docs: &Path, cache: Option<&Path>) -> Result<Vec<DeviceBook>, DeliverError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| DeliverError::Io { path, source }
    };
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(docs)
        .map_err(io(docs))?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            if is_sdr(&name) {
                continue;
            }
            for (file, path) in names_in(&path)
                .into_iter()
                .filter(|n| format_of(n).is_some())
                .map(|n| (format!("{name}/{n}"), path.join(n)))
            {
                out.push(book_on_disk(docs, &file, &path, cache));
            }
        } else if format_of(&name).is_some() {
            out.push(book_on_disk(docs, &name, &path, cache));
        }
    }
    Ok(out)
}

fn book_on_disk(docs: &Path, file: &str, path: &Path, cache: Option<&Path>) -> DeviceBook {
    let mut book = DeviceBook {
        file: file.to_owned(),
        size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        format: format_of(file).unwrap_or_default(),
        ..Default::default()
    };
    sidecar_flags(&mut book, names_in(&docs.join(sdr_name(file))));
    if book.format == "kfx" {
        details_cached(&mut book, path, cache);
    }
    book
}

fn space_of(root: &Path) -> Option<Space> {
    let s = fs4::statvfs(root).ok()?;
    Some(Space {
        total: s.total_space(),
        free: s.available_space(),
    })
}

fn mtp_err(e: MtpError) -> DeliverError {
    DeliverError::Mtp(e.to_string())
}

fn list_mtp(dev: &mut Mtp) -> Result<Inventory, DeliverError> {
    let storage = *dev
        .storages()
        .map_err(mtp_err)?
        .first()
        .ok_or_else(|| DeliverError::Mtp("device reports no storage".into()))?;
    let docs = dev.find_documents(storage).map_err(mtp_err)?;
    let space = dev.storage_info(storage).ok().map(|s| Space {
        total: s.max_capacity,
        free: s.free_bytes,
    });
    let mut books = Vec::new();
    let top = dev.list(storage, docs).map_err(mtp_err)?;
    // `<folder>/` prefix, the folder's children.
    let mut levels = vec![(String::new(), top)];
    let mut i = 0;
    while i < levels.len() {
        let (prefix, entries) = levels[i].clone();
        for (h, info) in &entries {
            let name = &info.filename;
            if info.association {
                if prefix.is_empty() && !is_sdr(name) && !name.starts_with('.') {
                    let children = dev.list(storage, *h).map_err(mtp_err)?;
                    levels.push((format!("{name}/"), children));
                }
                continue;
            }
            let Some(format) = format_of(name) else {
                continue;
            };
            let mut book = DeviceBook {
                file: format!("{prefix}{name}"),
                size: u64::from(info.size),
                format,
                ..Default::default()
            };
            let sdr = sdr_name(name);
            if let Some((sh, _)) = entries
                .iter()
                .find(|(_, i)| i.association && i.filename.eq_ignore_ascii_case(&sdr))
            {
                let names = dev
                    .list(storage, *sh)
                    .map_err(mtp_err)?
                    .into_iter()
                    .map(|(_, i)| i.filename);
                sidecar_flags(&mut book, names);
            }
            books.push(book);
        }
        i += 1;
    }
    books.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(Inventory { books, space })
}

/// What's on the connection's target. A folder export lists like a `documents` folder.
/// `cache` keeps KFX details between listings (mass storage and folders).
pub fn list_on(conn: &mut Conn<'_>, cache: Option<&Path>) -> Result<Inventory, DeliverError> {
    match conn.target {
        Target::Folder(dir) => Ok(Inventory {
            books: list_dir(dir, cache)?,
            space: space_of(dir),
        }),
        Target::MassStorage(dev) => Ok(Inventory {
            books: list_dir(&dev.root.join("documents"), cache)?,
            space: space_of(&dev.root),
        }),
        Target::Mtp(_) => match conn.mtp() {
            Some(Ok(dev)) => list_mtp(dev),
            Some(Err(e)) => Err(DeliverError::Mtp(e.to_string())),
            None => unreachable!("an MTP target always has a connection"),
        },
    }
}

/// `file` as an inventory names it: a book format, a name or `<folder>/<name>`, nothing that
/// climbs out of `documents/`.
fn check_file(file: &str) -> Result<Vec<&str>, DeliverError> {
    let parts: Vec<&str> = file.split('/').collect();
    let ok = !file.is_empty()
        && parts.len() <= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains('\\'))
        && !parts.first().is_some_and(|p| is_sdr(p))
        && format_of(file).is_some();
    if ok {
        Ok(parts)
    } else {
        Err(DeliverError::Refused(format!(
            "{file}: not a book in documents/"
        )))
    }
}

/// Removes `documents/<file>` and its `.sdr` folder; on mass storage also the thumbnail
/// Deckle wrote for a `…DECK` ASIN. Returns what was removed (paths under the target).
pub fn remove_on(conn: &mut Conn<'_>, file: &str) -> Result<Vec<String>, DeliverError> {
    let parts = check_file(file)?;
    let (docs, root) = match conn.target {
        Target::Folder(dir) => (dir.clone(), None),
        Target::MassStorage(dev) => (dev.root.join("documents"), Some(dev.root.clone())),
        Target::Mtp(_) => {
            return match conn.mtp() {
                Some(Ok(dev)) => remove_mtp(dev, &parts),
                Some(Err(e)) => Err(DeliverError::Mtp(e.to_string())),
                None => unreachable!("an MTP target always has a connection"),
            };
        }
    };
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| DeliverError::Io { path, source }
    };
    let path: PathBuf = parts.iter().fold(docs.clone(), |p, s| p.join(s));
    if !path.is_file() {
        return Err(DeliverError::Refused(format!(
            "documents/{file}: not found"
        )));
    }
    let asin = (format_of(file).as_deref() == Some("kfx"))
        .then(|| std::fs::read(&path).ok())
        .flatten()
        .and_then(|b| crate::inspect::inspect_kfx(&b).ok())
        .and_then(|k| k.asin);
    let mut removed = vec![format!("documents/{file}")];
    std::fs::remove_file(&path).map_err(io(&path))?;
    let sdr = path.with_extension("sdr");
    if sdr.is_dir() {
        std::fs::remove_dir_all(&sdr).map_err(io(&sdr))?;
        removed.push(format!("documents/{}", sdr_name(file)));
    }
    if let (Some(root), Some(asin)) = (root, asin.filter(|a| is_generated_asin(a))) {
        for cde in [
            deckle_kfx::convert::CdeType::Ebok,
            deckle_kfx::convert::CdeType::Pdoc,
        ] {
            let name = crate::thumbnail::thumbnail_name(&asin, cde);
            // The thumbnail and Deckle's restore copy, which every send would put back.
            for dir in [crate::thumbnail::THUMBNAIL_DIR, crate::thumbnail::CACHE_DIR] {
                let thumb = root.join(dir).join(&name);
                if thumb.is_file() && std::fs::remove_file(&thumb).is_ok() {
                    removed.push(format!("{dir}/{name}"));
                }
            }
        }
    }
    Ok(removed)
}

fn remove_mtp(dev: &mut Mtp, parts: &[&str]) -> Result<Vec<String>, DeliverError> {
    let storage = *dev
        .storages()
        .map_err(mtp_err)?
        .first()
        .ok_or_else(|| DeliverError::Mtp("device reports no storage".into()))?;
    let mut parent = dev.find_documents(storage).map_err(mtp_err)?;
    let (name, folders) = parts.split_last().expect("checked: at least one part");
    for f in folders {
        parent = dev
            .find_child(storage, parent, f)
            .map_err(mtp_err)?
            .ok_or_else(|| DeliverError::Refused(format!("documents/{f}: not found")))?;
    }
    let file = parts.join("/");
    let handle = dev
        .find_child(storage, parent, name)
        .map_err(mtp_err)?
        .ok_or_else(|| DeliverError::Refused(format!("documents/{file}: not found")))?;
    dev.delete(handle).map_err(mtp_err)?;
    let mut removed = vec![format!("documents/{file}")];
    let sdr = sdr_name(name);
    if let Some(h) = dev.find_child(storage, parent, &sdr).map_err(mtp_err)? {
        // The folder's files first: not every firmware deletes a folder's contents with it.
        for (c, _) in dev.list(storage, h).map_err(mtp_err)? {
            dev.delete(c).map_err(mtp_err)?;
        }
        dev.delete(h).map_err(mtp_err)?;
        removed.push(format!("documents/{}", sdr_name(&file)));
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_books_inside_documents_can_be_removed() {
        for ok in ["a.kfx", "Sub/a.azw3", "x.PDF"] {
            assert!(check_file(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "../a.kfx",
            "a/../../b.kfx",
            "/a.kfx",
            "a/b/c.kfx",
            "notes.txt.sdr",
            "a.sdr/x.kfx",
            "photo.jpg",
            "a\\b.kfx",
        ] {
            assert!(check_file(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn generated_asins_are_recognized() {
        assert!(is_generated_asin("B0Q7ZKDECK"));
        assert!(!is_generated_asin("B00ABCDEFG"));
        assert!(!is_generated_asin("DECK"));
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;
    use crate::deliver::Device;
    use crate::fake_mtp as fake;
    use deckle_kfx::convert::{CdeType, ConvertOptions, kpf_to_kfx};

    /// The mini book as Deckle builds it: EBOK with a generated ASIN.
    fn deckle_kfx() -> (Vec<u8>, String) {
        let asin = deckle_kfx::metadata::generate_asin(b"urn:test:mini");
        let out = kpf_to_kfx(
            &deckle_kfx::fixtures::path("deckle-mini.kpf"),
            &ConvertOptions {
                cde_type: CdeType::Ebok,
                asin: Some(asin.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        (out.bytes, asin)
    }

    fn kindle(root: &Path) -> Target {
        Target::MassStorage(Device {
            root: root.to_path_buf(),
            model_hint: "Kindle Paperwhite".into(),
            thumbnails_writable: true,
            firmware: None,
        })
    }

    #[test]
    fn a_mass_storage_kindle_lists_books_with_details_and_space() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("documents");
        let (kfx, asin) = deckle_kfx();
        std::fs::create_dir_all(docs.join("Mine - Book.sdr")).unwrap();
        std::fs::write(docs.join("Mine - Book.kfx"), &kfx).unwrap();
        std::fs::write(
            docs.join("Mine - Book.sdr")
                .join(format!("XRAY.entities.{asin}.asc")),
            b"x",
        )
        .unwrap();
        std::fs::write(
            docs.join("Mine - Book.sdr")
                .join(format!("LanguageLayer.en.{asin}.kll")),
            b"x",
        )
        .unwrap();
        std::fs::write(
            docs.join("Store.kfx"),
            std::fs::read(deckle_kfx::fixtures::path("deckle-mini-pdoc.golden.kfx")).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(docs.join("Papers")).unwrap();
        std::fs::write(docs.join("Papers/paper.pdf"), b"%PDF").unwrap();
        std::fs::write(docs.join("notes.jpg"), b"x").unwrap();
        std::fs::write(docs.join(".hidden.kfx"), b"x").unwrap();
        let target = kindle(tmp.path());
        let cache = tmp.path().join("cache");
        let mut conn = Conn::new(&target);
        let inv = list_on(&mut conn, Some(&cache)).unwrap();
        let files: Vec<&str> = inv.books.iter().map(|b| b.file.as_str()).collect();
        assert_eq!(files, ["Mine - Book.kfx", "Papers/paper.pdf", "Store.kfx"]);
        let mine = &inv.books[0];
        assert_eq!(mine.title.as_deref(), Some("Deckle Mini Fixture"));
        assert_eq!(mine.cde.as_deref(), Some("EBOK"));
        assert_eq!(mine.asin.as_deref(), Some(asin.as_str()));
        assert!(mine.from_deckle && mine.xray && mine.word_wise);
        assert!(mine.cover.is_some());
        assert_eq!(mine.size, kfx.len() as u64);
        let store = &inv.books[2];
        assert_eq!(store.cde.as_deref(), Some("PDOC"));
        assert!(!store.from_deckle && !store.xray);
        assert_eq!(inv.books[1].format, "pdf");
        assert!(inv.books[1].title.is_none());
        let space = inv.space.unwrap();
        assert!(space.total > 0 && space.free <= space.total);
        // The second listing comes from the cache, cover included.
        assert!(std::fs::read_dir(&cache).unwrap().count() >= 2);
        let again = list_on(&mut conn, Some(&cache)).unwrap();
        assert_eq!(again.books[0].title, mine.title);
        assert_eq!(again.books[0].cover, mine.cover);
    }

    #[test]
    fn remove_takes_the_book_its_sdr_and_its_thumbnail() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("documents");
        let thumbs = tmp.path().join("system/thumbnails");
        let (kfx, asin) = deckle_kfx();
        std::fs::create_dir_all(docs.join("Mine - Book.sdr")).unwrap();
        std::fs::create_dir_all(&thumbs).unwrap();
        std::fs::write(docs.join("Mine - Book.kfx"), &kfx).unwrap();
        std::fs::write(docs.join("Mine - Book.sdr/x.asc"), b"x").unwrap();
        let thumb = format!("thumbnail_{asin}_EBOK_portrait.jpg");
        std::fs::write(thumbs.join(&thumb), b"jpg").unwrap();
        let copies = tmp.path().join(crate::thumbnail::CACHE_DIR);
        std::fs::create_dir_all(&copies).unwrap();
        std::fs::write(copies.join(&thumb), b"jpg").unwrap();
        std::fs::write(
            thumbs.join("thumbnail_B00STORE00_EBOK_portrait.jpg"),
            b"jpg",
        )
        .unwrap();
        let target = kindle(tmp.path());
        let mut conn = Conn::new(&target);
        let removed = remove_on(&mut conn, "Mine - Book.kfx").unwrap();
        assert_eq!(
            removed,
            [
                "documents/Mine - Book.kfx".to_owned(),
                "documents/Mine - Book.sdr".to_owned(),
                format!("system/thumbnails/{thumb}"),
                format!("deckle-covers/{thumb}"),
            ]
        );
        // The next send's restore pass has nothing to bring back.
        assert_eq!(crate::thumbnail::sync(tmp.path()).unwrap(), 0);
        assert!(!thumbs.join(&thumb).exists());
        assert_eq!(std::fs::read_dir(&docs).unwrap().count(), 0);
        assert!(
            thumbs
                .join("thumbnail_B00STORE00_EBOK_portrait.jpg")
                .exists()
        );
        assert!(list_on(&mut conn, None).unwrap().books.is_empty());
        assert!(matches!(
            remove_on(&mut conn, "Mine - Book.kfx"),
            Err(DeliverError::Refused(_))
        ));
        assert!(matches!(
            remove_on(&mut conn, "../system/thumbnails/x.kfx"),
            Err(DeliverError::Refused(_))
        ));
    }

    #[test]
    fn an_mtp_kindle_lists_space_and_removes_over_one_session() {
        fake::reset(&[]);
        fake::put("Le Guin, Ursula - The Dispossessed.kfx", 5_000_000);
        fake::put(
            "Le Guin, Ursula - The Dispossessed.sdr/XRAY.entities.B0Q7ZKDECK.asc",
            10,
        );
        fake::put("Le Guin, Ursula - The Dispossessed.sdr/x.sidecar", 10);
        fake::put("Papers/paper.pdf", 300);
        fake::put("photo.jpg", 1);
        let target = Target::Mtp(deckle_transport::usb::MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: Some("G0X1".into()),
            product: Some("Kindle Scribe".into()),
            bus: 1,
            address: 2,
        });
        let mut conn = Conn::with_opener(&target, fake::open);
        let inv = list_on(&mut conn, None).unwrap();
        let files: Vec<&str> = inv.books.iter().map(|b| b.file.as_str()).collect();
        assert_eq!(
            files,
            ["Le Guin, Ursula - The Dispossessed.kfx", "Papers/paper.pdf"]
        );
        assert_eq!(inv.books[0].size, 5_000_000);
        assert!(inv.books[0].xray && !inv.books[0].word_wise);
        assert!(inv.books[0].title.is_none(), "MTP books aren't read");
        assert_eq!(
            inv.space,
            Some(Space {
                total: fake::CAPACITY,
                free: fake::FREE
            })
        );
        let removed = remove_on(&mut conn, "Le Guin, Ursula - The Dispossessed.kfx").unwrap();
        assert_eq!(removed.len(), 2);
        remove_on(&mut conn, "Papers/paper.pdf").unwrap();
        assert_eq!(fake::paths(), ["Papers/", "photo.jpg"]);
        drop(conn);
        assert_eq!(fake::opens(), 1, "one session for all of it");
    }
}
