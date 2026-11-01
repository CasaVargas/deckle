// SPDX-License-Identifier: GPL-3.0-or-later
//! The Deckle library (library design §3): one plain folder per book under a root,
//!
//! ```text
//! <root>/<Author sort>/<Title>/<source file>
//! <root>/<Author sort>/<Title>/deckle.json
//! ```
//!
//! No database, index or lock file: folders can be moved, synced or deleted with anything.
//! A book's identity is in its sidecar (the generated ASIN's seed), not its path.

pub mod calibre;
pub mod care;
pub mod catalog;
pub mod config;
pub mod details;
pub mod freshness;
pub mod search;
pub mod series;
pub mod shelf;
pub mod shelves;
pub mod sidecar;
pub mod sources;
pub mod upgrade;
pub mod watch;
pub mod writeback;

use std::path::{Path, PathBuf};

use deckle_core::deliver::{Target, author_sort, sanitize};
use deckle_core::devices::{now_iso, same_device, seen_mass_storage, seen_mtp};
use deckle_core::inspect::BookInfo;
use deckle_core::pipeline::{RunReport, asin_seed};
use sha2::{Digest, Sha256};

pub use config::LibraryConfig;
pub use sidecar::{FILE_NAME, Metadata, SCHEMA, SendRecord, SeriesRef, Sidecar};

const UNKNOWN_AUTHOR: &str = "Unknown Author";
/// Folder names stay well under every OS's limits, with room for ` (n)`.
const MAX_NAME: usize = 100;

/// A book in the library: its folder and its sidecar.
#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    pub dir: PathBuf,
    pub sidecar: Sidecar,
}

impl Book {
    pub fn source_path(&self) -> PathBuf {
        self.dir.join(&self.sidecar.source)
    }

    pub fn sidecar_path(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }
}

/// What [`Library::add`] did.
#[derive(Clone, Debug)]
pub struct Added {
    pub book: Book,
    /// A new folder (else an existing book was updated in place).
    pub new: bool,
    /// The source file was written (new, or different from the one kept).
    pub copied: bool,
}

/// What [`Library::scan`] found and did.
#[derive(Clone, Debug, Default)]
pub struct ScanReport {
    pub books: usize,
    /// Sources that got a sidecar (in place, or filed into their own folder).
    pub adopted: Vec<PathBuf>,
    /// Sidecars whose metadata changed with their source.
    pub refreshed: Vec<PathBuf>,
    /// Sidecars whose source file is gone; left alone.
    pub missing_source: Vec<PathBuf>,
    /// `(path, why)`: unreadable sidecars or sources, newer schemas, a second copy of a book.
    pub problems: Vec<(PathBuf, String)>,
}

/// A library rooted at `root` (created on first add).
#[derive(Clone, Debug)]
pub struct Library {
    pub root: PathBuf,
}

/// EPUB, Word, OpenDocument or RTF file, DRM-free Kindle book, plain text, FB2, PDF, CBZ, or
/// Markdown or a folder of page images (both kept as an EPUB snapshot).
pub fn is_source(path: &Path) -> bool {
    deckle_comic::is_comic(path)
        || deckle_docx::is_word(path)
        || deckle_txt::is_text(path)
        || deckle_fb2::is_fb2(path)
        || deckle_odt::is_odt(path)
        || deckle_rtf::is_rtf(path)
        || deckle_mobi::is_mobi_file(path)
        || deckle_markdown::is_markdown(path)
        || deckle_pdf::is_pdf(path)
        || path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
}

/// The book's metadata: the EPUB's, or the EPUB a Word document, text file, PDF or comic imports
/// as.
pub fn book_info(path: &Path) -> Result<BookInfo, String> {
    if deckle_comic::is_comic(path) {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = dir.path().join("imported.epub");
        deckle_comic::convert(path, &epub).map_err(|e| e.to_string())?;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    if deckle_markdown::is_markdown(path) {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = dir.path().join("imported.epub");
        deckle_markdown::convert(path, &epub).map_err(|e| e.to_string())?;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    if deckle_docx::is_word(path) {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = dir.path().join("imported.epub");
        deckle_docx::convert(path, &epub).map_err(|e| e.to_string())?;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    if deckle_pdf::is_pdf(path) {
        // Through the import stage, so a scan reads as the comic it's sent as.
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = deckle_core::import::import(path, dir.path(), "imported.epub")
            .ok_or("not a PDF Deckle can import")??
            .epub;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    if deckle_mobi::is_mobi_file(path) {
        // A fixed-layout Kindle book is never made an EPUB: its own header says what it is.
        if let Some(meta) = std::fs::read(path)
            .ok()
            .and_then(|b| deckle_mobi::read_meta(&b).ok())
            .filter(|m| m.fixed_layout && !m.drm)
        {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            return Ok(deckle_core::inspect::from_kindle(&meta, &stem));
        }
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = dir.path().join("imported.epub");
        deckle_mobi::convert(path, &epub).map_err(|e| e.to_string())?;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    if deckle_rtf::is_rtf(path) {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = dir.path().join("imported.epub");
        deckle_rtf::convert(path, &epub).map_err(|e| e.to_string())?;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    if deckle_odt::is_odt(path) {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = dir.path().join("imported.epub");
        deckle_odt::convert(path, &epub).map_err(|e| e.to_string())?;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    if deckle_fb2::is_fb2(path) {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = dir.path().join("imported.epub");
        deckle_fb2::convert(path, &epub).map_err(|e| e.to_string())?;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    if deckle_txt::is_text(path) {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let epub = dir.path().join("imported.epub");
        deckle_txt::convert(path, &epub).map_err(|e| e.to_string())?;
        return deckle_core::inspect::inspect(&epub).map_err(|e| e.to_string());
    }
    deckle_core::inspect::inspect(path).map_err(|e| e.to_string())
}

/// `bytes` to `path` by a temp file in the same folder and a rename: never half-written.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    tmp.write_all(bytes).map_err(|e| e.to_string())?;
    tmp.as_file().sync_all().map_err(|e| e.to_string())?;
    tmp.persist(path)
        .map_err(|e| format!("{}: {}", path.display(), e.error))?;
    Ok(())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn metadata_of(info: &BookInfo) -> Metadata {
    Metadata {
        title: info.title.clone(),
        authors: info.creators.iter().map(|c| c.name.clone()).collect(),
        author_sort: author_sort(info).filter(|a| !a.trim().is_empty()),
        series: info.series.as_ref().map(|s| SeriesRef {
            name: s.name.clone(),
            index: s.index,
        }),
        language: info.language.clone(),
        isbn: info.isbn.clone(),
        extra: Default::default(),
    }
}

/// A folder name: [`sanitize`]d, no leading dots (hidden), no trailing dots (Windows).
fn folder_name(s: &str, fallback: &str) -> String {
    let name: String = sanitize(s).chars().take(MAX_NAME).collect();
    let name = name.trim_start_matches('.').trim_end_matches(['.', ' ']);
    if name.is_empty() {
        fallback.to_owned()
    } else {
        name.to_owned()
    }
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn read_sidecar(path: &Path) -> Result<Sidecar, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// One folder under the root, as the walk found it.
enum Found {
    Book(Box<Book>),
    Unreadable(PathBuf, String),
    /// A folder (not the root) holding exactly one source and no sidecar.
    Bare(PathBuf),
    /// Sources at the root, or several in one folder without a sidecar.
    Loose(PathBuf),
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Library { root: root.into() }
    }

    /// The library `library.json` at `config` points at, when there is one.
    pub fn configured(config: &Path) -> Option<(Library, LibraryConfig)> {
        let cfg = LibraryConfig::load(config);
        let root = cfg.root.clone()?;
        Some((Library::new(root), cfg))
    }

    fn walk(&self) -> Vec<Found> {
        let mut out = Vec::new();
        self.walk_dir(&self.root, &mut out);
        out
    }

    fn walk_dir(&self, dir: &Path, out: &mut Vec<Found>) {
        let sidecar = dir.join(FILE_NAME);
        if sidecar.is_file() {
            out.push(match read_sidecar(&sidecar) {
                Ok(s) => Found::Book(Box::new(Book {
                    dir: dir.to_path_buf(),
                    sidecar: s,
                })),
                Err(e) => Found::Unreadable(sidecar, e),
            });
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| !is_hidden(&e.file_name().to_string_lossy()))
            .map(|e| e.path())
            .collect();
        entries.sort();
        let sources: Vec<&PathBuf> = entries
            .iter()
            .filter(|p| p.is_file() && is_source(p))
            .collect();
        if sources.len() == 1 && dir != self.root {
            out.push(Found::Bare(sources[0].clone()));
        } else {
            out.extend(sources.into_iter().cloned().map(Found::Loose));
        }
        for e in &entries {
            let sdr = e.extension().is_some_and(|x| x.eq_ignore_ascii_case("sdr"));
            if e.is_dir() && !sdr {
                self.walk_dir(e, out);
            }
        }
    }

    /// Every book with a readable sidecar, sorted by author then title.
    pub fn list(&self) -> Vec<Book> {
        let mut books: Vec<Book> = self
            .walk()
            .into_iter()
            .filter_map(|f| match f {
                Found::Book(b) => Some(*b),
                _ => None,
            })
            .collect();
        books.sort_by_cached_key(|b| {
            let m = &b.sidecar.metadata;
            (
                m.author_sort.clone().unwrap_or_default().to_lowercase(),
                m.title.to_lowercase(),
            )
        });
        books
    }

    pub fn find_by_asin(&self, asin: &str) -> Option<Book> {
        self.list().into_iter().find(|b| b.sidecar.asin == asin)
    }

    /// The book last sent to `device` as `file`.
    pub fn find_send(&self, device: &str, file: &str) -> Option<Book> {
        self.list().into_iter().find(|b| {
            b.sidecar
                .sends
                .iter()
                .any(|s| same_device(&s.device, device) && s.file == file)
        })
    }

    /// Writes the sidecar; refuses one from a newer Deckle.
    pub fn save(&self, book: &Book) -> Result<(), String> {
        if book.sidecar.schema > SCHEMA {
            return Err(format!(
                "{}: written by a newer Deckle (schema {}); left as it is",
                book.sidecar_path().display(),
                book.sidecar.schema
            ));
        }
        let json = serde_json::to_vec_pretty(&book.sidecar).map_err(|e| e.to_string())?;
        write_atomic(&book.sidecar_path(), &json)
    }

    /// `<root>/<Author sort>/<Title>`, or `<Title> (n)` when that holds something else.
    fn free_dir(&self, info: &BookInfo, fallback_title: &str) -> PathBuf {
        let author = folder_name(&author_sort(info).unwrap_or_default(), UNKNOWN_AUTHOR);
        let title = folder_name(&info.title, &folder_name(fallback_title, "Untitled"));
        let parent = self.root.join(author);
        let taken = |p: &Path| p.exists();
        let first = parent.join(&title);
        if !taken(&first) {
            return first;
        }
        (2..)
            .map(|n| parent.join(format!("{title} ({n})")))
            .find(|p| !taken(p))
            .expect("some name is free")
    }

    /// Adds `source` (Markdown is snapshotted as EPUB), or updates the book with the same identity in
    /// place: its source when the bytes differ, its metadata, keeping `added` and `sends`.
    pub fn add(&self, source: &Path) -> Result<Added, String> {
        // Markdown may depend on adjacent notes and images. Keep the self-contained EPUB,
        // so re-sends still work when the original folder moves or disappears.
        if deckle_markdown::is_markdown(source) {
            let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let epub = dir.path().join("notes.epub");
            deckle_markdown::convert(source, &epub).map_err(|e| e.to_string())?;
            return self.add(&epub);
        }
        // A folder of page images, likewise: the comic's own EPUB, named after the folder.
        if deckle_comic::is_image_folder(source) {
            let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let name = source.file_name().unwrap_or_default().to_string_lossy();
            let epub = dir.path().join(format!("{name}.epub"));
            deckle_comic::convert(source, &epub).map_err(|e| e.to_string())?;
            return self.add(&epub);
        }
        if !is_source(source) {
            return Err(format!(
                "{}: the library takes EPUBs, Word, OpenDocument and RTF files, DRM-free Kindle books, text, FB2, PDFs, comics (CBZ, CBR or a folder of page images) and Markdown",
                source.display()
            ));
        }
        let bytes = std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
        let info = book_info(source).map_err(|e| format!("{}: {e}", source.display()))?;
        let seed = asin_seed(&info);
        let id = String::from_utf8_lossy(&seed).into_owned();
        let asin = deckle_kfx::metadata::generate_asin(&seed);
        let file_name = source
            .file_name()
            .ok_or("no file name")?
            .to_string_lossy()
            .into_owned();
        let sha = sha256_hex(&bytes);
        if let Some(mut book) = self.list().into_iter().find(|b| b.sidecar.id == id) {
            if book.sidecar.schema > SCHEMA {
                return Err(format!(
                    "{}: written by a newer Deckle; not updated",
                    book.sidecar_path().display()
                ));
            }
            let old = book.source_path();
            let copied = book.sidecar.sha256 != sha || book.sidecar.source != file_name;
            if copied && !same_file(source, &book.dir.join(&file_name)) {
                write_atomic(&book.dir.join(&file_name), &bytes)?;
                if book.sidecar.source != file_name {
                    let _ = std::fs::remove_file(&old);
                }
            }
            book.sidecar.source = file_name;
            book.sidecar.sha256 = sha;
            book.sidecar.asin = asin;
            book.sidecar.metadata = Metadata {
                extra: std::mem::take(&mut book.sidecar.metadata.extra),
                ..metadata_of(&info)
            };
            self.save(&book)?;
            return Ok(Added {
                book,
                new: false,
                copied,
            });
        }
        let stem = source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = self.free_dir(&info, &stem);
        write_atomic(&dir.join(&file_name), &bytes)?;
        let book = Book {
            dir,
            sidecar: Sidecar {
                schema: SCHEMA,
                id,
                source: file_name,
                sha256: sha,
                added: now_iso(),
                asin,
                metadata: metadata_of(&info),
                sends: Vec::new(),
                extra: Default::default(),
            },
        };
        self.save(&book)?;
        Ok(Added {
            book,
            new: true,
            copied: true,
        })
    }

    /// Adopts sources without a sidecar, refreshes metadata from changed sources, and reports
    /// sidecars that can't be used. Never deletes a sidecar or a book.
    pub fn scan(&self) -> ScanReport {
        let mut report = ScanReport::default();
        let mut ids: Vec<(String, PathBuf)> = Vec::new();
        for found in self.walk() {
            match found {
                Found::Book(mut book) => {
                    if let Some((_, first)) = ids.iter().find(|(id, _)| *id == book.sidecar.id) {
                        report.problems.push((
                            book.dir.clone(),
                            format!("the same book as {}", first.display()),
                        ));
                        continue;
                    }
                    ids.push((book.sidecar.id.clone(), book.dir.clone()));
                    report.books += 1;
                    let source = book.source_path();
                    if !source.is_file() {
                        report.missing_source.push(book.dir.clone());
                        continue;
                    }
                    if book.sidecar.schema > SCHEMA {
                        report.problems.push((
                            book.sidecar_path(),
                            format!("schema {} is newer than this Deckle", book.sidecar.schema),
                        ));
                        continue;
                    }
                    match self.refresh(&mut book) {
                        Ok(true) => report.refreshed.push(book.dir.clone()),
                        Ok(false) => {}
                        Err(e) => report.problems.push((source, e)),
                    }
                }
                Found::Unreadable(path, why) => report.problems.push((path, why)),
                Found::Bare(source) => match self.adopt_in_place(&source) {
                    Ok(book) => {
                        if let Some((_, first)) = ids.iter().find(|(id, _)| *id == book.sidecar.id)
                        {
                            report.problems.push((
                                book.dir.clone(),
                                format!("the same book as {}", first.display()),
                            ));
                        } else {
                            ids.push((book.sidecar.id.clone(), book.dir.clone()));
                        }
                        report.books += 1;
                        report.adopted.push(source);
                    }
                    Err(e) => report.problems.push((source, e)),
                },
                Found::Loose(source) => match self.add(&source) {
                    Ok(added) => {
                        if added.book.source_path() != source {
                            let _ = std::fs::remove_file(&source);
                        }
                        if added.new {
                            report.books += 1;
                            ids.push((added.book.sidecar.id.clone(), added.book.dir.clone()));
                        }
                        report.adopted.push(source);
                    }
                    Err(e) => report.problems.push((source, e)),
                },
            }
        }
        report
    }

    /// Re-reads the source's metadata; rewrites the sidecar only when something changed.
    fn refresh(&self, book: &mut Book) -> Result<bool, String> {
        let source = book.source_path();
        let bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
        let sha = sha256_hex(&bytes);
        // Reading a PDF's metadata means reflowing all of it: an unchanged one is left be.
        if sha == book.sidecar.sha256 && deckle_pdf::is_pdf(&source) {
            return Ok(false);
        }
        let info = book_info(&source)?;
        let metadata = Metadata {
            extra: book.sidecar.metadata.extra.clone(),
            ..metadata_of(&info)
        };
        if sha == book.sidecar.sha256 && metadata == book.sidecar.metadata {
            return Ok(false);
        }
        book.sidecar.sha256 = sha;
        book.sidecar.metadata = metadata;
        self.save(book)?;
        Ok(true)
    }

    fn adopt_in_place(&self, source: &Path) -> Result<Book, String> {
        let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
        let info = book_info(source)?;
        let seed = asin_seed(&info);
        let book = Book {
            dir: source.parent().ok_or("no folder")?.to_path_buf(),
            sidecar: Sidecar {
                schema: SCHEMA,
                id: String::from_utf8_lossy(&seed).into_owned(),
                source: source
                    .file_name()
                    .ok_or("no file name")?
                    .to_string_lossy()
                    .into_owned(),
                sha256: sha256_hex(&bytes),
                added: now_iso(),
                asin: deckle_kfx::metadata::generate_asin(&seed),
                metadata: metadata_of(&info),
                sends: Vec::new(),
                extra: Default::default(),
            },
        };
        self.save(&book)?;
        Ok(book)
    }

    /// Records `send` on `book`'s sidecar.
    pub fn record_send(&self, book: &mut Book, send: SendRecord) -> Result<(), String> {
        book.sidecar.record(send);
        self.save(book)
    }

    /// Drops every book's record of `file` on `device` (it was removed there).
    pub fn forget_send(&self, device: &str, file: &str) -> Result<bool, String> {
        let mut any = false;
        for mut book in self.list() {
            if book.sidecar.forget(device, file) {
                self.save(&book)?;
                any = true;
            }
        }
        Ok(any)
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// How a send names its device in the library: the profile key `devices.json` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRef {
    pub key: String,
    pub name: String,
}

pub fn device_ref(target: &Target) -> DeviceRef {
    match target {
        Target::MassStorage(d) => {
            let s = seen_mass_storage(d);
            DeviceRef {
                key: s.key,
                name: s.model_hint,
            }
        }
        Target::Mtp(m) => {
            let s = seen_mtp(m);
            DeviceRef {
                key: s.key,
                name: s.model_hint,
            }
        }
        Target::Folder(dir) => DeviceRef {
            key: format!("folder:{}", dir.display()),
            name: dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "folder".into()),
        },
    }
}

/// What a finished run sent, as a send record; `None` when nothing was delivered.
pub fn send_record(report: &RunReport, device: &DeviceRef) -> Option<SendRecord> {
    let delivered = report.delivered.as_ref()?;
    let file = delivered.file_name()?.to_string_lossy().into_owned();
    Some(SendRecord {
        device: device.key.clone(),
        device_name: device.name.clone(),
        file,
        at: now_iso(),
        cde: report.cde_type.clone(),
        asin: report.asin.clone(),
        xray: report.xray.is_some(),
        word_wise: report.word_wise.is_some(),
        stamped: true,
        xray_engine: report.xray.as_ref().map(|x| x.engine),
        wordwise_db: report.word_wise.as_ref().map(|w| w.database.clone()),
        end: report.end_position,
        extra: Default::default(),
    })
}

/// After a successful send: adds `source` to the configured library and records the send,
/// when there is a library and `auto_add` is on. `Ok(None)` when it doesn't apply.
pub fn auto_add(
    config: &Path,
    source: &Path,
    report: &RunReport,
    target: &Target,
) -> Result<Option<Book>, String> {
    let Some((library, cfg)) = Library::configured(config) else {
        return Ok(None);
    };
    if !cfg.auto_add || report.error.is_some() || !is_source(source) {
        return Ok(None);
    }
    let Some(send) = send_record(report, &device_ref(target)) else {
        return Ok(None);
    };
    let mut book = library.add(source)?.book;
    library.record_send(&mut book, send)?;
    Ok(Some(book))
}

/// Where the library last put this book on `target`: the file a new send makes obsolete
/// (Kindle-updates design §4). Empty without a library, or when it was never sent there.
pub fn previous_copies(config: &Path, source: &Path, target: &Target) -> Vec<String> {
    let Some((library, _)) = Library::configured(config) else {
        return Vec::new();
    };
    let Ok(info) = book_info(source) else {
        return Vec::new();
    };
    let id = String::from_utf8_lossy(&asin_seed(&info)).into_owned();
    let device = device_ref(target);
    library
        .list()
        .into_iter()
        .find(|b| b.sidecar.id == id)
        .and_then(|b| b.sidecar.send_to(&device.key).map(|s| s.file.clone()))
        .into_iter()
        .collect()
}

/// Records a finished run's send on the library book for `source` (adding it when needed),
/// whatever `auto_add` says: an update of a library book always updates its record.
pub fn record_run(
    library: &Library,
    source: &Path,
    report: &RunReport,
    target: &Target,
) -> Result<Option<Book>, String> {
    let Some(send) = send_record(report, &device_ref(target)) else {
        return Ok(None);
    };
    let mut book = library.add(source)?.book;
    library.record_send(&mut book, send)?;
    Ok(Some(book))
}

/// A note for the run's warnings when adding to the library failed (the send still worked).
pub fn auto_add_warning(e: &str) -> String {
    format!("not added to the library: {e}")
}
