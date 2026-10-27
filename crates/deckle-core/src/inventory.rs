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
    /// DRM-protected: a KFX that is a DRMION container or has a `.voucher`, or an AZW/MOBI whose
    /// record 0 says it's encrypted. Such a file is never opened past its metadata.
    #[serde(default)]
    pub drm: bool,
    /// A fixed-layout book (EXTH 122): comics, children's books. Left as it is.
    #[serde(default)]
    pub fixed_layout: bool,
    /// An older MOBI with no KF8 part: it can't be rebuilt as an EPUB. False until read.
    #[serde(default)]
    pub no_kf8: bool,
    /// calibre's id for the book, when the file says (EXTH 112 `calibre:<uuid>`).
    #[serde(default)]
    pub calibre_id: Option<String>,
    #[serde(default)]
    pub isbn: Option<String>,
    /// Whether the book's details were read: false over MTP unless asked for (see
    /// [`list_detailed`]), and for formats that carry none.
    #[serde(default)]
    pub details: bool,
    /// The KFX's end position (`erl`), the denominator of a reading position.
    #[serde(skip)]
    pub end: Option<i64>,
    /// The reading-position sidecar's bytes (KFX `.yjf`, AZW3 `.azw3f`), when
    /// [`list_with_positions`] read it.
    #[serde(skip)]
    pub yjf: Option<Vec<u8>>,
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

/// Over MTP, at most this many `.yjf` are read per connection; the rest are counted.
pub const MAX_YJF: usize = 300;

/// Asks the walk to also read each book's `.yjf`. Books whose file name starts with
/// `skip_prefix` (the editions) are left out.
struct Positions<'a> {
    skip_prefix: &'a str,
    read: usize,
    over: usize,
}

/// Formats whose `.yjf` holds a reading position: the Kindle's own, not a PDF's or a text file's.
const POSITION_FORMATS: [&str; 5] = ["kfx", "azw3", "azw", "mobi", "prc"];

/// How many folders down the shelf's walk looks for books; the Kindle tab's listing is one.
const SHELF_DEPTH: usize = 3;

impl Positions<'_> {
    fn wanted(&self, file: &str) -> bool {
        let name = file.rsplit('/').next().unwrap_or(file);
        format_of(name).is_some_and(|f| POSITION_FORMATS.contains(&f.as_str()))
            && !name.starts_with(self.skip_prefix)
    }
}

/// A reading-position file in a `.sdr`: KFX `.yjf`, AZW3 `.azw3f` (the same KRDS container; its
/// positions aren't KFX ids, but the Kindle's own fraction is there).
fn is_position_file(name: &str) -> bool {
    crate::yjf::is_yjf(name) || name.to_ascii_lowercase().ends_with(".azw3f")
}

/// How many folders below `documents/` a walk looks into.
fn depth_for(want: &Option<Positions<'_>>) -> usize {
    if want.is_some() { SHELF_DEPTH } else { 1 }
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

/// Formats whose files carry details Deckle reads: KFX, and the PalmDB ones.
const DETAIL_FORMATS: [&str; 5] = ["kfx", "azw3", "azw", "mobi", "prc"];

fn has_details(format: &str) -> bool {
    DETAIL_FORMATS.contains(&format)
}

/// The book's own details, from its whole file (KFX) or its first records (PalmDB).
fn read_details(book: &mut DeviceBook, bytes: &[u8]) {
    book.details = true;
    if book.format == "kfx" {
        read_kfx_details(book, bytes);
    } else {
        read_mobi_details(book, deckle_mobi::read_meta(bytes).ok());
    }
}

fn read_mobi_details(book: &mut DeviceBook, meta: Option<deckle_mobi::Meta>) {
    book.details = true;
    let Some(m) = meta else { return };
    book.title = m.title;
    book.authors = m.authors;
    book.cde = m.cde;
    book.from_deckle = m.asin.as_deref().is_some_and(is_generated_asin);
    book.asin = m.asin;
    book.isbn = m.isbn;
    book.calibre_id = m.calibre_id;
    book.drm = m.drm;
    book.fixed_layout = m.fixed_layout;
    book.no_kf8 = !m.kf8;
    book.cover = m.cover;
}

/// The KFX's own details: title, authors, CDE type, ASIN, cover. A DRMION container is a
/// store (or loaned) book: only that much is known.
fn read_kfx_details(book: &mut DeviceBook, kfx: &[u8]) {
    if kfx.starts_with(deckle_kfx::kpf::DRMION_SIGNATURE) {
        book.drm = true;
        return;
    }
    let Ok(k) = crate::inspect::inspect_kfx(kfx) else {
        return;
    };
    book.title = Some(k.info.title).filter(|t| !t.is_empty());
    book.authors = k.info.creators.into_iter().map(|c| c.name).collect();
    book.cde = k.cde_type;
    book.from_deckle = k.asin.as_deref().is_some_and(is_generated_asin);
    book.asin = k.asin;
    book.cover = k.cover;
    book.end = deckle_kfx::text::content_runs(kfx).ok().map(|(_, e)| e);
}

/// A KFX's details, cached under `cache` by path, size and modified time, so a listing
/// doesn't re-read every book.
fn details_cached(book: &mut DeviceBook, path: &Path, cache: Option<&Path>) {
    let stamp = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key = format!("{}|{}|{stamp}", path.display(), book.size);
    if load_cached(book, cache, &key) {
        return;
    }
    if book.format == "kfx" {
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        read_details(book, &bytes);
    } else {
        // Only the header, record 0 and the cover: a partial read.
        let meta = std::fs::File::open(path)
            .ok()
            .and_then(|mut f| deckle_mobi::read_meta_from(&mut f).ok());
        read_mobi_details(book, meta);
    }
    save_cached(book, cache, &key);
}

/// What a listing keeps of a book's details between listings. Every field is required, so a
/// cache file written by an older Deckle is re-read once.
#[derive(Serialize, Deserialize)]
struct Cached {
    title: Option<String>,
    authors: Vec<String>,
    cde: Option<String>,
    asin: Option<String>,
    /// `-1` when unknown.
    end: i64,
    drm: bool,
    fixed_layout: bool,
    no_kf8: bool,
    calibre_id: Option<String>,
    isbn: Option<String>,
}

fn cache_files(cache: Option<&Path>, key: &str) -> Option<(PathBuf, PathBuf)> {
    let key = format!("{:016x}", fnv(key));
    cache.map(|c| (c.join(format!("{key}.json")), c.join(format!("{key}.jpg"))))
}

/// Fills `book` from the cache entry for `key`; false when there is none.
fn load_cached(book: &mut DeviceBook, cache: Option<&Path>, key: &str) -> bool {
    let Some((json, jpg)) = cache_files(cache, key) else {
        return false;
    };
    let Some(c) = std::fs::read(json)
        .ok()
        .and_then(|b| serde_json::from_slice::<Cached>(&b).ok())
    else {
        return false;
    };
    book.title = c.title;
    book.authors = c.authors;
    book.cde = c.cde;
    book.from_deckle = c.asin.as_deref().is_some_and(is_generated_asin);
    book.asin = c.asin;
    book.end = Some(c.end).filter(|e| *e >= 0);
    book.drm |= c.drm;
    book.fixed_layout = c.fixed_layout;
    book.no_kf8 = c.no_kf8;
    book.calibre_id = c.calibre_id;
    book.isbn = c.isbn;
    book.cover = std::fs::read(jpg).ok();
    book.details = true;
    true
}

fn save_cached(book: &DeviceBook, cache: Option<&Path>, key: &str) {
    let Some((json, jpg)) = cache_files(cache, key) else {
        return;
    };
    let c = Cached {
        title: book.title.clone(),
        authors: book.authors.clone(),
        cde: book.cde.clone(),
        asin: book.asin.clone(),
        end: book.end.unwrap_or(-1),
        drm: book.drm,
        fixed_layout: book.fixed_layout,
        no_kf8: book.no_kf8,
        calibre_id: book.calibre_id.clone(),
        isbn: book.isbn.clone(),
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
        // A store KFX's key travels beside it.
        book.drm |= n.to_ascii_lowercase().ends_with(".voucher");
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
fn list_dir(
    docs: &Path,
    cache: Option<&Path>,
    want: &Option<Positions<'_>>,
) -> Result<Vec<DeviceBook>, DeliverError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| DeliverError::Io { path, source }
    };
    let mut out = Vec::new();
    let mut dirs = vec![(String::new(), 0usize)];
    let max = depth_for(want);
    while let Some((prefix, depth)) = dirs.pop() {
        let dir = docs.join(&prefix);
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .map_err(io(&dir))?
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
                if !is_sdr(&name) && depth < max {
                    dirs.push((format!("{prefix}{name}/"), depth + 1));
                }
            } else if format_of(&name).is_some() {
                out.push(book_on_disk(
                    docs,
                    &format!("{prefix}{name}"),
                    &path,
                    cache,
                    want,
                ));
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(out)
}

fn book_on_disk(
    docs: &Path,
    file: &str,
    path: &Path,
    cache: Option<&Path>,
    want: &Option<Positions<'_>>,
) -> DeviceBook {
    let mut book = DeviceBook {
        file: file.to_owned(),
        size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        format: format_of(file).unwrap_or_default(),
        ..Default::default()
    };
    let sdr = docs.join(sdr_name(file));
    sidecar_flags(&mut book, names_in(&sdr));
    if has_details(&book.format) {
        details_cached(&mut book, path, cache);
    }
    if want.as_ref().is_some_and(|w| w.wanted(file))
        && let Some(name) = names_in(&sdr).into_iter().find(|n| is_position_file(n))
    {
        book.yjf = std::fs::read(sdr.join(name)).ok();
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

fn list_mtp(
    dev: &mut Mtp,
    want: &mut Option<Positions<'_>>,
    cache: Option<&Path>,
    deep: bool,
) -> Result<Inventory, DeliverError> {
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
    let max = depth_for(want);
    let mut levels = vec![(String::new(), top, 0usize)];
    let mut i = 0;
    while i < levels.len() {
        let (prefix, entries, depth) = levels[i].clone();
        for (h, info) in &entries {
            let name = &info.filename;
            if info.association {
                if depth < max && !is_sdr(name) && !name.starts_with('.') {
                    let children = dev.list(storage, *h).map_err(mtp_err)?;
                    levels.push((format!("{prefix}{name}/"), children, depth + 1));
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
                let inside = dev.list(storage, *sh).map_err(mtp_err)?;
                sidecar_flags(&mut book, inside.iter().map(|(_, i)| i.filename.clone()));
                if let Some(w) = want.as_mut()
                    && w.wanted(&book.file)
                    && let Some((fh, _)) = inside
                        .iter()
                        .find(|(_, i)| !i.association && is_position_file(&i.filename))
                {
                    if w.read < MAX_YJF {
                        w.read += 1;
                        // One sidecar that won't download costs that book, not the listing.
                        book.yjf = dev.get_object(*fh).ok();
                    } else {
                        w.over += 1;
                    }
                }
            }
            // MTP has no partial reads: a book is read whole once, then known by its name and
            // size.
            // A plain listing takes what an earlier detailed one cached, and reads nothing more.
            if has_details(&book.format) && cache.is_some() {
                let key = format!("mtp|{}|{}", book.file, book.size);
                if !load_cached(&mut book, cache, &key)
                    && deep
                    && let Ok(bytes) = dev.get_object(*h)
                {
                    read_details(&mut book, &bytes);
                    save_cached(&book, cache, &key);
                }
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
    walk(conn, cache, &mut None, false)
}

/// [`list_with_positions`] with every book's details, over MTP too: each book not in `cache`
/// is read whole once (slow the first time on a big Kindle). For sorting books into upgrade
/// classes, and seeing which ones are being read.
pub fn list_detailed(
    conn: &mut Conn<'_>,
    cache: Option<&Path>,
    skip_prefix: &str,
) -> Result<(Inventory, usize), DeliverError> {
    let mut want = Some(Positions {
        skip_prefix,
        read: 0,
        over: 0,
    });
    let inv = walk(conn, cache, &mut want, true)?;
    Ok((inv, want.map_or(0, |w| w.over)))
}

/// [`list_on`], also reading each KFX book's `.yjf` in the same walk (and the same MTP
/// session). Books whose file name starts with `skip_prefix` get none. The count is how many
/// `.yjf` went unread past [`MAX_YJF`].
pub fn list_with_positions(
    conn: &mut Conn<'_>,
    cache: Option<&Path>,
    skip_prefix: &str,
) -> Result<(Inventory, usize), DeliverError> {
    let mut want = Some(Positions {
        skip_prefix,
        read: 0,
        over: 0,
    });
    let inv = walk(conn, cache, &mut want, false)?;
    Ok((inv, want.map_or(0, |w| w.over)))
}

fn walk(
    conn: &mut Conn<'_>,
    cache: Option<&Path>,
    want: &mut Option<Positions<'_>>,
    deep: bool,
) -> Result<Inventory, DeliverError> {
    match conn.target {
        Target::Folder(dir) => Ok(Inventory {
            books: list_dir(dir, cache, want)?,
            space: space_of(dir),
        }),
        Target::MassStorage(dev) => Ok(Inventory {
            books: list_dir(&dev.root.join("documents"), cache, want)?,
            space: space_of(&dev.root),
        }),
        Target::Mtp(_) => match conn.mtp() {
            Some(Ok(dev)) => list_mtp(dev, want, cache, deep),
            Some(Err(e)) => Err(DeliverError::Mtp(e.to_string())),
            None => unreachable!("an MTP target always has a connection"),
        },
    }
}

/// `file` as an inventory names it: a book format, a name under at most [`SHELF_DEPTH`] folders
/// (a Kindle files downloads in `Downloads/Items01/`), nothing that climbs out of `documents/`.
fn check_file(file: &str) -> Result<Vec<&str>, DeliverError> {
    let parts: Vec<&str> = file.split('/').collect();
    let ok = !file.is_empty()
        && parts.len() <= SHELF_DEPTH + 1
        && parts
            .iter()
            .all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains('\\'))
        && !parts[..parts.len() - 1].iter().any(|p| is_sdr(p))
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
    let (name, folders) = parts.split_last().expect("checked: at least one part");
    let (storage, parent) = mtp_parent(dev, folders)?;
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

/// A book file and its `.sdr` folder's files (name, bytes), as read off the target.
pub struct BookFiles {
    pub book: Vec<u8>,
    pub sdr: Vec<(String, Vec<u8>)>,
}

/// Reads `documents/<file>` and the files in its `.sdr` folder (not deeper), over the caller's
/// connection.
pub fn read_book_on(conn: &mut Conn<'_>, file: &str) -> Result<BookFiles, DeliverError> {
    let parts = check_file(file)?;
    let docs = match conn.target {
        Target::Folder(dir) => dir.clone(),
        Target::MassStorage(dev) => dev.root.join("documents"),
        Target::Mtp(_) => {
            return match conn.mtp() {
                Some(Ok(dev)) => read_book_mtp(dev, &parts),
                Some(Err(e)) => Err(DeliverError::Mtp(e.to_string())),
                None => unreachable!("an MTP target always has a connection"),
            };
        }
    };
    let path: PathBuf = parts.iter().fold(docs, |p, s| p.join(s));
    let book = std::fs::read(&path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => {
            DeliverError::Refused(format!("documents/{file}: not found"))
        }
        _ => DeliverError::Io {
            path: path.clone(),
            source,
        },
    })?;
    let sdr_dir = path.with_extension("sdr");
    let mut sdr = Vec::new();
    for name in names_in(&sdr_dir) {
        let p = sdr_dir.join(&name);
        if p.is_file() {
            let bytes = std::fs::read(&p).map_err(|source| DeliverError::Io { path: p, source })?;
            sdr.push((name, bytes));
        }
    }
    sdr.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(BookFiles { book, sdr })
}

fn mtp_parent(dev: &mut Mtp, folders: &[&str]) -> Result<(u32, u32), DeliverError> {
    let storage = *dev
        .storages()
        .map_err(mtp_err)?
        .first()
        .ok_or_else(|| DeliverError::Mtp("device reports no storage".into()))?;
    let mut parent = dev.find_documents(storage).map_err(mtp_err)?;
    for f in folders {
        parent = dev
            .find_child(storage, parent, f)
            .map_err(mtp_err)?
            .ok_or_else(|| DeliverError::Refused(format!("documents/{f}: not found")))?;
    }
    Ok((storage, parent))
}

fn read_book_mtp(dev: &mut Mtp, parts: &[&str]) -> Result<BookFiles, DeliverError> {
    let (name, folders) = parts.split_last().expect("checked: at least one part");
    let (storage, parent) = mtp_parent(dev, folders)?;
    let file = parts.join("/");
    let handle = dev
        .find_child(storage, parent, name)
        .map_err(mtp_err)?
        .ok_or_else(|| DeliverError::Refused(format!("documents/{file}: not found")))?;
    let book = dev.get_object(handle).map_err(mtp_err)?;
    let mut sdr = Vec::new();
    if let Some(h) = dev
        .find_child(storage, parent, &sdr_name(name))
        .map_err(mtp_err)?
    {
        for (c, info) in dev.list(storage, h).map_err(mtp_err)? {
            if !info.association {
                sdr.push((info.filename.clone(), dev.get_object(c).map_err(mtp_err)?));
            }
        }
    }
    sdr.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(BookFiles { book, sdr })
}

/// Copies `documents/<file>` and its `.sdr` folder into `dest`, as `dest/<name>` and
/// `dest/<stem>.sdr/` (Books and Kindle design §6: an upgrade keeps the old copy).
pub fn copy_out(conn: &mut Conn<'_>, file: &str, dest: &Path) -> Result<(), DeliverError> {
    let files = read_book_on(conn, file)?;
    let name = file.rsplit('/').next().unwrap_or(file);
    let io = |path: PathBuf| move |source| DeliverError::Io { path, source };
    std::fs::create_dir_all(dest).map_err(io(dest.to_path_buf()))?;
    let book = dest.join(name);
    std::fs::write(&book, &files.book).map_err(io(book.clone()))?;
    if !files.sdr.is_empty() {
        let sdr = dest.join(sdr_name(name));
        std::fs::create_dir_all(&sdr).map_err(io(sdr.clone()))?;
        for (n, bytes) in &files.sdr {
            let p = sdr.join(n);
            std::fs::write(&p, bytes).map_err(io(p.clone()))?;
        }
    }
    Ok(())
}

/// Puts a book [`copy_out`] kept back as `documents/<file>`, with its `.sdr` files. An existing
/// file of that name is overwritten.
pub fn copy_in(conn: &mut Conn<'_>, src: &Path, file: &str) -> Result<(), DeliverError> {
    let parts = check_file(file)?;
    let name = *parts.last().expect("checked: at least one part");
    let io = |path: PathBuf| move |source| DeliverError::Io { path, source };
    let book_path = src.join(name);
    let book = std::fs::read(&book_path).map_err(io(book_path.clone()))?;
    let sdr_src = src.join(sdr_name(name));
    let mut sdr = Vec::new();
    for n in names_in(&sdr_src) {
        let p = sdr_src.join(&n);
        if p.is_file() {
            sdr.push((n, std::fs::read(&p).map_err(io(p.clone()))?));
        }
    }
    let docs = match conn.target {
        Target::Folder(dir) => dir.clone(),
        Target::MassStorage(dev) => dev.root.join("documents"),
        Target::Mtp(_) => {
            let dev = match conn.mtp() {
                Some(Ok(dev)) => dev,
                Some(Err(e)) => return Err(DeliverError::Mtp(e.to_string())),
                None => unreachable!("an MTP target always has a connection"),
            };
            let (_, folders) = parts.split_last().expect("checked");
            let (storage, parent) = mtp_parent(dev, folders)?;
            if let Some(h) = dev.find_child(storage, parent, name).map_err(mtp_err)? {
                dev.delete(h).map_err(mtp_err)?;
            }
            dev.send_file(storage, parent, name, &book)
                .map_err(mtp_err)?;
            if !sdr.is_empty() {
                let folder = dev
                    .ensure_folder(storage, parent, &sdr_name(name))
                    .map_err(mtp_err)?;
                for (n, bytes) in &sdr {
                    if let Some(h) = dev.find_child(storage, folder, n).map_err(mtp_err)? {
                        dev.delete(h).map_err(mtp_err)?;
                    }
                    dev.send_file(storage, folder, n, bytes).map_err(mtp_err)?;
                }
            }
            return Ok(());
        }
    };
    let path: PathBuf = parts.iter().fold(docs, |p, s| p.join(s));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io(dir.to_path_buf()))?;
    }
    std::fs::write(&path, &book).map_err(io(path.clone()))?;
    if !sdr.is_empty() {
        let dir = path.with_extension("sdr");
        std::fs::create_dir_all(&dir).map_err(io(dir.clone()))?;
        for (n, bytes) in &sdr {
            let p = dir.join(n);
            std::fs::write(&p, bytes).map_err(io(p.clone()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_books_inside_documents_can_be_removed() {
        for ok in [
            "a.kfx",
            "Sub/a.azw3",
            "x.PDF",
            // Where a Kindle files what it downloads, as deep as the shelf looks.
            "Downloads/Items01/a.azw3",
            "a/b/c/d.kfx",
        ] {
            assert!(check_file(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "../a.kfx",
            "a/../../b.kfx",
            "/a.kfx",
            "a/b/c/d/e.kfx",
            "notes.txt.sdr",
            "a.sdr/x.kfx",
            "Sub/a.sdr/x.kfx",
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
            serial: None,
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
    fn listing_reads_end_from_kfx() {
        let tmp = tempfile::tempdir().unwrap();
        let (kfx, _) = deckle_kfx();
        let want = deckle_kfx::text::content_runs(&kfx).unwrap().1;
        let books = tmp.path().join("books");
        std::fs::create_dir_all(&books).unwrap();
        std::fs::write(books.join("Mini.kfx"), &kfx).unwrap();
        let cache = tmp.path().join("cache");
        let target = Target::Folder(books);
        let mut conn = Conn::new(&target);
        let inv = list_on(&mut conn, Some(&cache)).unwrap();
        assert_eq!(inv.books[0].end, Some(want));
        // The second listing comes from the cache, which now holds the end.
        let json: String = std::fs::read_dir(&cache)
            .unwrap()
            .filter_map(|e| std::fs::read_to_string(e.unwrap().path()).ok())
            .collect();
        assert!(json.contains("\"end\""), "{json}");
        let again = list_on(&mut conn, Some(&cache)).unwrap();
        assert_eq!(again.books[0].end, Some(want));
    }

    fn mtp_target() -> Target {
        Target::Mtp(deckle_transport::usb::MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: Some("G0001".into()),
            product: Some("Kindle".into()),
            bus: 1,
            address: 2,
        })
    }

    const EDITION: &str = "deckle-daily-";
    const E: &str = "deckle-daily-2026-10-21";

    fn assert_positions(inv: &Inventory) {
        let by = |f: &str| inv.books.iter().find(|b| b.file == f);
        assert_eq!(by("Book.kfx").unwrap().yjf.as_deref(), Some(&b"BOOK"[..]));
        assert_eq!(
            by("deckle-longread-walden.kfx").unwrap().yjf.as_deref(),
            Some(&b"LONG"[..]),
            "a long read is a book"
        );
        assert_eq!(
            by("Sub/Inner.kfx").unwrap().yjf.as_deref(),
            Some(&b"INNER"[..])
        );
        assert_eq!(by("Other.kfx").unwrap().yjf, None);
        let edition = format!("{E}.kfx");
        assert!(
            by(&edition).is_none_or(|b| b.yjf.is_none()),
            "editions get no .yjf"
        );
    }

    #[test]
    fn positions_ride_in_the_listing() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("documents");
        std::fs::create_dir_all(docs.join("Sub/Inner.sdr")).unwrap();
        for (kfx, sdr, bytes) in [
            ("Book", "Book.sdr", "BOOK"),
            (E, &format!("{E}.sdr"), "EDITION"),
            (
                "deckle-longread-walden",
                "deckle-longread-walden.sdr",
                "LONG",
            ),
        ] {
            std::fs::write(docs.join(format!("{kfx}.kfx")), b"x").unwrap();
            std::fs::create_dir_all(docs.join(sdr)).unwrap();
            std::fs::write(docs.join(sdr).join("p9d6e.yjf"), bytes).unwrap();
            std::fs::write(docs.join(sdr).join("p9d6e.yjr"), b"YJR").unwrap();
            std::fs::write(docs.join(sdr).join("p9d6e.yjr.bad_file"), b"BAD").unwrap();
        }
        std::fs::write(docs.join("Other.kfx"), b"x").unwrap();
        std::fs::write(docs.join("Sub/Inner.kfx"), b"x").unwrap();
        std::fs::write(docs.join("Sub/Inner.sdr/i9d6e.yjf"), b"INNER").unwrap();
        let target = kindle(tmp.path());
        let mut conn = Conn::new(&target);
        let (inv, over) = list_with_positions(&mut conn, None, EDITION).unwrap();
        assert_eq!(over, 0);
        assert_positions(&inv);
    }

    #[test]
    fn positions_over_mtp_in_one_session() {
        fake::reset(&[
            "Book.kfx",
            "Other.kfx",
            &format!("{E}.kfx"),
            "deckle-longread-walden.kfx",
        ]);
        fake::put("Sub/Inner.kfx", 1);
        fake::put_bytes("Book.sdr/Book9d6e.yjf", b"BOOK");
        fake::put_bytes("Book.sdr/Book9d6e.yjr", b"YJR");
        fake::put_bytes(&format!("{E}.sdr/{E}9d6e.yjf"), b"EDITION");
        fake::put_bytes("deckle-longread-walden.sdr/w9d6e.yjf", b"LONG");
        fake::put_bytes("Sub/Inner.sdr/Inner9d6e.yjf", b"INNER");
        let target = mtp_target();
        let mut conn = Conn::with_opener(&target, fake::open);
        let (inv, over) = list_with_positions(&mut conn, None, EDITION).unwrap();
        assert_eq!(over, 0);
        assert_positions(&inv);
        drop(conn);
        assert_eq!(fake::opens(), 1);
    }

    #[test]
    fn positions_cover_other_formats_and_deeper_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("documents");
        for (book, sdr, bytes) in [
            ("Plain.mobi", "Plain.sdr", "MOBI"),
            ("Books/Items/Deep.azw3", "Books/Items/Deep.sdr", "DEEP"),
            ("Doc.pdf", "Doc.sdr", "PDF"),
        ] {
            let b = docs.join(book);
            std::fs::create_dir_all(b.parent().unwrap()).unwrap();
            std::fs::write(&b, b"x").unwrap();
            std::fs::create_dir_all(docs.join(sdr)).unwrap();
            std::fs::write(docs.join(sdr).join("p9d6e.yjf"), bytes).unwrap();
        }
        let target = kindle(tmp.path());
        let mut conn = Conn::new(&target);
        let (inv, _) = list_with_positions(&mut conn, None, EDITION).unwrap();
        let yjf = |f: &str| {
            inv.books
                .iter()
                .find(|b| b.file == f)
                .map(|b| b.yjf.clone())
        };
        assert_eq!(yjf("Plain.mobi"), Some(Some(b"MOBI".to_vec())));
        assert_eq!(yjf("Books/Items/Deep.azw3"), Some(Some(b"DEEP".to_vec())));
        assert_eq!(
            yjf("Doc.pdf"),
            Some(None),
            "a PDF's position isn't a reading position"
        );
        // The Kindle tab's own listing stays one folder deep.
        let plain = list_on(&mut conn, None).unwrap();
        assert!(
            plain
                .books
                .iter()
                .all(|b| b.file != "Books/Items/Deep.azw3")
        );
    }

    #[test]
    fn positions_over_mtp_cover_other_formats_and_deeper_folders() {
        fake::reset(&["Plain.mobi"]);
        fake::put_bytes("Plain.sdr/p9d6e.yjf", b"MOBI");
        fake::put("Books/Items/Deep.azw3", 1);
        fake::put_bytes("Books/Items/Deep.sdr/d9d6e.yjf", b"DEEP");
        let target = mtp_target();
        let mut conn = Conn::with_opener(&target, fake::open);
        let (inv, _) = list_with_positions(&mut conn, None, EDITION).unwrap();
        let yjf = |f: &str| {
            inv.books
                .iter()
                .find(|b| b.file == f)
                .and_then(|b| b.yjf.clone())
        };
        assert_eq!(yjf("Plain.mobi"), Some(b"MOBI".to_vec()));
        assert_eq!(yjf("Books/Items/Deep.azw3"), Some(b"DEEP".to_vec()));
        drop(conn);
        assert_eq!(fake::opens(), 1);
    }

    #[test]
    fn cap_counts_the_rest() {
        let names: Vec<String> = (0..302).map(|i| format!("B{i:03}.kfx")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        fake::reset(&refs);
        for i in 0..302 {
            fake::put_bytes(&format!("B{i:03}.sdr/B{i:03}x.yjf"), b"Y");
        }
        let target = mtp_target();
        let mut conn = Conn::with_opener(&target, fake::open);
        let (inv, over) = list_with_positions(&mut conn, None, EDITION).unwrap();
        assert_eq!(
            inv.books.iter().filter(|b| b.yjf.is_some()).count(),
            MAX_YJF
        );
        assert_eq!(over, 2);
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
