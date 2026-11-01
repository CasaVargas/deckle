// SPDX-License-Identifier: GPL-3.0-or-later
//! E-readers that mount as a drive and take EPUB (docs/superpowers/specs/
//! 2026-10-23-other-readers-design.md §8, phase 3): a Kobo, sent KEPUBs, and a PocketBook, sent
//! EPUBs as they are. Deckle writes only into a `Deckle/` folder of its own on the drive, and
//! prunes only there.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::kobo::{self, Kobo};

/// The folder on the reader that Deckle writes to and prunes: nothing outside it is touched.
pub const FOLDER: &str = "Deckle";

/// A PocketBook mounted as a drive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PocketBook {
    pub root: PathBuf,
}

/// A reader found on a drive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Reader {
    Kobo(Kobo),
    PocketBook(PocketBook),
}

impl Reader {
    /// The key `[sync.devices."<id>"]` uses: `kobo/<serial>`, `pocketbook/<volume>`.
    pub fn id(&self) -> String {
        match self {
            Reader::Kobo(k) => k.id(),
            Reader::PocketBook(p) => format!("pocketbook/{}", volume(&p.root)),
        }
    }

    /// What the reports call it.
    pub fn name(&self) -> String {
        match self {
            Reader::Kobo(_) => "Kobo",
            Reader::PocketBook(_) => "PocketBook",
        }
        .to_owned()
    }

    pub fn root(&self) -> &Path {
        match self {
            Reader::Kobo(k) => &k.root,
            Reader::PocketBook(p) => &p.root,
        }
    }

    /// The extension its books get: a Kobo reads `.kepub.epub` as a KEPUB.
    pub fn extension(&self) -> &'static str {
        match self {
            Reader::Kobo(_) => ".kepub.epub",
            Reader::PocketBook(_) => ".epub",
        }
    }

    /// The file name a book called `stem` gets on this reader.
    pub fn file_name(&self, stem: &str) -> String {
        format!("{stem}{}", self.extension())
    }

    /// An EPUB as this reader takes it: a KEPUB for a Kobo, as it is for a PocketBook (once it
    /// opens as a zip, so a broken file isn't sent).
    ///
    /// A fixed-layout book never gets KEPUB markup: its pages are laid out on purpose and have
    /// no text to count (fixed-layout spec §5). It keeps the Kobo's `.kepub.epub` name all the
    /// same, so the Kobo opens it in its own renderer, the one that handles fixed layout, not
    /// Adobe's. Deckle's own comics are fitted to the reader's screen first.
    pub fn prepare(&self, epub: &[u8]) -> Result<Vec<u8>, String> {
        if let Some(layout) = fixed_layout_of(epub) {
            if !layout.deckle_comic {
                return Ok(epub.to_vec());
            }
            let (fitted, failed) =
                deckle_comic::pages::fit_epub(epub, crate::screens::for_reader(self))?;
            if failed > 0 {
                eprintln!(
                    "{failed} page image(s) couldn't be fitted to the {} and went as they were",
                    self.name()
                );
            }
            return Ok(fitted);
        }
        match self {
            Reader::Kobo(_) => crate::kepub::convert(epub),
            Reader::PocketBook(_) => zip::ZipArchive::new(std::io::Cursor::new(epub))
                .map(|_| epub.to_vec())
                .map_err(|e| e.to_string()),
        }
    }

    fn folder(&self) -> PathBuf {
        self.root().join(FOLDER)
    }
}

/// What the OPF of a fixed-layout EPUB says about it.
struct FixedLayout {
    /// Deckle made it from a CBZ or page folder (`urn:deckle:comic:` identifier).
    deckle_comic: bool,
}

/// `Some` when the EPUB in `bytes` is fixed layout: `rendition:layout` `pre-paginated` for the
/// whole book or on every spine item, as `inspect` decides.
fn fixed_layout_of(bytes: &[u8]) -> Option<FixedLayout> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut read = |name: &str| -> Option<String> {
        let mut s = String::new();
        zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
        Some(s)
    };
    let container = read("META-INF/container.xml")?;
    let doc = roxmltree::Document::parse(&container).ok()?;
    let opf_path = doc
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))?
        .attribute("full-path")?
        .to_owned();
    let opf = read(&opf_path)?;
    let doc = roxmltree::Document::parse(&opf).ok()?;
    let whole = doc.descendants().any(|n| {
        n.has_tag_name("meta")
            && n.attribute("property") == Some("rendition:layout")
            && n.text().is_some_and(|t| t.trim() == "pre-paginated")
    });
    let itemrefs: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("itemref"))
        .collect();
    let every = !itemrefs.is_empty()
        && itemrefs.iter().all(|r| {
            r.attribute("properties").is_some_and(|p| {
                p.split_whitespace()
                    .any(|p| p == "rendition:layout-pre-paginated")
            })
        });
    (whole || every).then(|| FixedLayout {
        deckle_comic: doc.descendants().any(|n| {
            n.has_tag_name("identifier")
                && n.text()
                    .is_some_and(|t| t.trim().starts_with("urn:deckle:comic:"))
        }),
    })
}

fn volume(root: &Path) -> String {
    root.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Readers among explicit mount roots: a Kobo by its `.kobo/` database, a PocketBook by its
/// `system/` folders (spike 3 confirms the marker). A Kindle (`documents/` and `system/`) is
/// neither.
pub fn detect_in(roots: &[PathBuf]) -> Vec<Reader> {
    let mut out: Vec<Reader> = kobo::detect_in(roots)
        .into_iter()
        .map(Reader::Kobo)
        .collect();
    for root in roots {
        if out.iter().any(|r| r.root() == root) {
            continue;
        }
        let system = root.join("system");
        let marked = system.join("explorer-3").is_dir()
            || (system.join("config").is_dir() && system.join("profiles").is_dir());
        if marked && !root.join("documents").is_dir() {
            out.push(Reader::PocketBook(PocketBook { root: root.clone() }));
        }
    }
    out
}

/// Connected readers, on the same mount points as a Kindle ([`crate::deliver::mount_roots`]).
pub fn detect() -> Vec<Reader> {
    detect_in(&crate::deliver::mount_roots())
}

/// `readers.json` beside the app's `devices.json`: the readers Deckle has seen, by id, with
/// when. While it names one, `digest run` also queues the paper as an EPUB.
pub fn seen_file() -> PathBuf {
    crate::devices::default_registry_file().with_file_name("readers.json")
}

/// Records a sighting of `reader` in `file`, at `now`.
pub fn remember(file: &Path, reader: &Reader, now: &str) -> Result<(), String> {
    let mut seen: std::collections::BTreeMap<String, String> = std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    seen.insert(reader.id(), now.to_owned());
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = file.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(&seen).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, file).map_err(|e| format!("{}: {e}", file.display()))
}

/// Whether `file` names any reader.
pub fn any_seen(file: &Path) -> bool {
    std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice::<std::collections::BTreeMap<String, String>>(&b).ok())
        .is_some_and(|m| !m.is_empty())
}

/// Writes `bytes` to `Deckle/<name>` through a hidden temporary file, flushed to the drive
/// before it's renamed, so an unplugged reader never holds half a book.
pub fn deliver(reader: &Reader, bytes: &[u8], name: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let dir = reader.folder();
    std::fs::create_dir_all(&dir)?;
    let to = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &to).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(to)
}

/// Removes all but the newest `keep` books in `Deckle/` whose name starts with `prefix` (the
/// editions, named by date so the name sorts them), sparing `exempt`, each with its KOReader
/// sidecar. Returns the names removed.
pub fn prune(
    reader: &Reader,
    prefix: &str,
    keep: usize,
    exempt: &[String],
) -> std::io::Result<Vec<String>> {
    let Ok(rd) = std::fs::read_dir(reader.folder()) else {
        return Ok(Vec::new());
    };
    let ext = reader.extension();
    let mut names: Vec<String> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix) && n.ends_with(ext))
        // On a PocketBook `.epub` would also match a `.kepub.epub` someone put there.
        .filter(|n| ext == ".kepub.epub" || !n.ends_with(".kepub.epub"))
        .filter(|n| !exempt.iter().any(|x| stem(x) == stem(n)))
        .collect();
    names.sort_unstable_by(|a, b| b.cmp(a));
    let mut gone = Vec::new();
    for n in names.into_iter().skip(keep) {
        let book = reader.folder().join(&n);
        std::fs::remove_file(&book)?;
        crate::koreader::remove_sidecar(&book)?;
        gone.push(n);
    }
    Ok(gone)
}

/// A file name without `.kepub.epub`, `.epub` or `.kfx`: `deckle-daily-2026-10-01.kfx` and
/// `deckle-daily-2026-10-01.kepub.epub` are one edition.
pub fn stem(name: &str) -> &str {
    [".kepub.epub", ".epub", ".kfx"]
        .iter()
        .find_map(|x| name.strip_suffix(x))
        .unwrap_or(name)
}

/// What [`book_epub`] adds to a book besides importing and cleaning it.
#[derive(Clone, Debug, Default)]
pub struct BookOptions {
    /// X-Ray's people and terms as a Who's who page (Who's who design §3); `None` = no page.
    pub whos_who: Option<crate::xray::XRayOptions>,
    /// A recap at the end of each chapter (AI extras design §4); `None` = none.
    pub recaps: Option<crate::recaps::RecapOptions>,
}

/// A book made ready for the EPUB outbox: a Word document, PDF or text file imported first,
/// then cleaned as for a Kindle (no Previewer). Each reader takes it through [`Reader::prepare`].
/// Chapter recaps, then the Who's who page, go in as `opts` asks; one that can't be made is a
/// warning on stderr, never a lost book. Real HTTP.
pub fn book_epub(source: &Path, opts: &BookOptions) -> Result<Vec<u8>, String> {
    book_epub_with(
        source,
        opts,
        &mut deckle_xray::llm::http_post,
        &mut deckle_xray::wiki::http_get,
    )
}

/// [`book_epub`] with the LLM and Wikipedia calls injected (tests).
pub fn book_epub_with(
    source: &Path,
    opts: &BookOptions,
    post: &mut crate::xray::Post<'_>,
    fetch: &mut crate::xray::Fetch<'_>,
) -> Result<Vec<u8>, String> {
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let epub = if crate::import::needs_import(source) {
        crate::import::import(source, work.path(), "imported.epub")
            .ok_or("not a book Deckle can import")??
            .epub
    } else {
        source.to_path_buf()
    };
    let normalized = work.path().join("normalized.epub");
    crate::normalize::normalize(
        &epub,
        &normalized,
        &crate::normalize::NormalizeOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut bytes = std::fs::read(&normalized).map_err(|e| e.to_string())?;
    if let Some(recaps) = &opts.recaps {
        match crate::recaps::add_with(&bytes, recaps, post) {
            Ok((with, summary)) => {
                for w in summary.iter().flat_map(|s| &s.warnings) {
                    eprintln!("{}: {w}", source.display());
                }
                bytes = with;
            }
            Err(e) => eprintln!("{}: Chapter recaps skipped: {e}", source.display()),
        }
    }
    let Some(whos_who) = &opts.whos_who else {
        return Ok(bytes);
    };
    match crate::whoswho::add_with(&bytes, whos_who, post, fetch) {
        Ok((with, summary)) => {
            for w in summary.iter().flat_map(|s| &s.warnings) {
                eprintln!("{}: {w}", source.display());
            }
            Ok(with)
        }
        Err(e) => {
            eprintln!("{}: Who's who skipped: {e}", source.display());
            Ok(bytes)
        }
    }
}

/// [`book_epub`], prepared for `reader`.
pub fn book(reader: &Reader, source: &Path, opts: &BookOptions) -> Result<Vec<u8>, String> {
    reader.prepare(&book_epub(source, opts)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kobo::tests::fake_kobo;

    pub fn fake_pocketbook(root: &Path) -> Reader {
        std::fs::create_dir_all(root.join("system/explorer-3")).unwrap();
        std::fs::create_dir_all(root.join("system/config")).unwrap();
        detect_in(&[root.to_path_buf()]).remove(0)
    }

    #[test]
    fn kobos_and_pocketbooks_are_found_and_kindles_are_not() {
        let tmp = tempfile::tempdir().unwrap();
        let kobo = tmp.path().join("KOBOeReader");
        fake_kobo(&kobo, "");
        let pb = tmp.path().join("PB632");
        std::fs::create_dir_all(pb.join("system/config")).unwrap();
        std::fs::create_dir_all(pb.join("system/profiles")).unwrap();
        let kindle = tmp.path().join("Kindle");
        std::fs::create_dir_all(kindle.join("documents")).unwrap();
        std::fs::create_dir_all(kindle.join("system/config")).unwrap();
        std::fs::create_dir_all(kindle.join("system/profiles")).unwrap();
        let other = tmp.path().join("USB");
        std::fs::create_dir_all(other.join("system/config")).unwrap();
        let found = detect_in(&[kobo, pb.clone(), kindle, other]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name(), "Kobo");
        assert_eq!(found[1], Reader::PocketBook(PocketBook { root: pb }));
        assert_eq!(found[1].id(), "pocketbook/PB632");
    }

    #[test]
    fn a_pocketbook_takes_epub_as_it_is_and_a_kobo_a_kepub() {
        let tmp = tempfile::tempdir().unwrap();
        let pb = fake_pocketbook(&tmp.path().join("PB"));
        assert_eq!(
            pb.file_name("deckle-daily-2026-10-01"),
            "deckle-daily-2026-10-01.epub"
        );
        assert!(pb.prepare(b"not a zip").is_err());
        let kobo = Reader::Kobo(fake_kobo(&tmp.path().join("K"), ""));
        assert_eq!(
            kobo.file_name("deckle-daily-2026-10-01"),
            "deckle-daily-2026-10-01.kepub.epub"
        );
    }

    /// A Deckle comic EPUB with one oversized colour page (1600 × 2400).
    fn comic_epub(dir: &Path) -> Vec<u8> {
        use std::io::Write;
        let img = image::RgbImage::from_pixel(1600, 2400, image::Rgb([200, 40, 40]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let cbz = dir.join("c.cbz");
        let mut zw = zip::ZipWriter::new(std::fs::File::create(&cbz).unwrap());
        zw.start_file("1.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(png.get_ref()).unwrap();
        zw.finish().unwrap();
        let epub = dir.join("c.epub");
        deckle_comic::convert(&cbz, &epub).unwrap();
        std::fs::read(epub).unwrap()
    }

    fn entry(epub: &[u8], name: &str) -> Vec<u8> {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(epub)).unwrap();
        let mut out = Vec::new();
        z.by_name(name).unwrap().read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn a_fixed_layout_book_gets_no_kepub_markup_and_deckles_comics_are_fitted() {
        let tmp = tempfile::tempdir().unwrap();
        let kobo = Reader::Kobo(fake_kobo(&tmp.path().join("K"), ""));
        let pb = fake_pocketbook(&tmp.path().join("PB"));
        let comic = comic_epub(tmp.path());
        for (reader, (w, h)) in [(&kobo, (1280, 1920)), (&pb, (1248, 1872))] {
            let out = reader.prepare(&comic).unwrap();
            let page = String::from_utf8(entry(&out, "OEBPS/pages/p0001.xhtml")).unwrap();
            assert!(!page.contains("koboSpan") && !page.contains("book-columns"));
            let img = image::load_from_memory(&entry(&out, "OEBPS/images/p0001.png")).unwrap();
            // Scaled down to the screen, colour kept.
            assert_eq!((img.width(), img.height()), (w, h), "{}", reader.name());
            assert_eq!(img.color(), image::ColorType::Rgb8);
            assert_eq!(entry(&out, "mimetype"), b"application/epub+zip");
        }
        // The Kobo still calls it a KEPUB: its own renderer is the one that does fixed layout.
        assert!(kobo.file_name("Night Train").ends_with(".kepub.epub"));

        // A publisher's fixed-layout book goes exactly as it came.
        let publisher = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/comic/fixed-layout-rtl.epub"),
        )
        .unwrap();
        assert_eq!(kobo.prepare(&publisher).unwrap(), publisher);

        // A reflowable book is a KEPUB as before.
        let mini = std::fs::read(deckle_kfx::fixtures::path("deckle-mini.epub")).unwrap();
        let kepub = kobo.prepare(&mini).unwrap();
        assert!(String::from_utf8_lossy(&entry(&kepub, "OEBPS/c0.xhtml")).contains("koboSpan"));
    }

    #[test]
    fn delivery_writes_into_deckles_folder_and_prune_keeps_the_newest() {
        let tmp = tempfile::tempdir().unwrap();
        let k = Reader::Kobo(fake_kobo(tmp.path(), ""));
        for d in ["01", "02", "03", "04"] {
            let name = k.file_name(&format!("deckle-daily-2026-10-{d}"));
            deliver(&k, b"KEPUB", &name).unwrap();
        }
        deliver(&k, b"KEPUB", "deckle-reads-a.kepub.epub").unwrap();
        std::fs::write(
            tmp.path().join("deckle-daily-2026-09-01.kepub.epub"),
            b"mine",
        )
        .unwrap();
        let exempt = vec!["deckle-daily-2026-10-01.kfx".to_owned()];
        let gone = prune(&k, "deckle-daily-", 2, &exempt).unwrap();
        assert_eq!(gone, ["deckle-daily-2026-10-02.kepub.epub"]);
        let mut left: Vec<String> = std::fs::read_dir(tmp.path().join(FOLDER))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "deckle-daily-2026-10-01.kepub.epub",
                "deckle-daily-2026-10-03.kepub.epub",
                "deckle-daily-2026-10-04.kepub.epub",
                "deckle-reads-a.kepub.epub"
            ]
        );
        // Outside Deckle's folder nothing is touched.
        assert!(
            tmp.path()
                .join("deckle-daily-2026-09-01.kepub.epub")
                .is_file()
        );
    }

    #[test]
    fn a_pocketbook_prunes_its_epubs_only() {
        let tmp = tempfile::tempdir().unwrap();
        let pb = fake_pocketbook(tmp.path());
        for n in [
            "deckle-daily-2026-10-01.epub",
            "deckle-daily-2026-10-02.epub",
            "deckle-daily-2026-10-03.kepub.epub",
        ] {
            deliver(&pb, b"EPUB", n).unwrap();
        }
        assert_eq!(
            prune(&pb, "deckle-daily-", 1, &[]).unwrap(),
            ["deckle-daily-2026-10-01.epub"]
        );
        assert!(
            tmp.path()
                .join("Deckle/deckle-daily-2026-10-03.kepub.epub")
                .is_file()
        );
    }

    #[test]
    fn a_pruned_edition_takes_its_sidecar_and_only_its_own() {
        let tmp = tempfile::tempdir().unwrap();
        let k = Reader::Kobo(fake_kobo(tmp.path(), ""));
        for n in [
            "deckle-daily-2026-10-01.kepub.epub",
            "deckle-daily-2026-10-02.kepub.epub",
            "walden.kepub.epub",
        ] {
            let book = deliver(&k, b"KEPUB", n).unwrap();
            crate::koreader::tune_paper(&book).unwrap();
        }
        let gone = prune(&k, "deckle-daily-", 1, &[]).unwrap();
        assert_eq!(gone, ["deckle-daily-2026-10-01.kepub.epub"]);
        let dir = tmp.path().join(FOLDER);
        assert!(!dir.join("deckle-daily-2026-10-01.kepub.sdr").exists());
        assert!(dir.join("deckle-daily-2026-10-02.kepub.sdr").is_dir());
        assert!(dir.join("walden.kepub.sdr").is_dir());
    }

    #[test]
    fn a_reader_once_seen_is_remembered() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("cfg/readers.json");
        assert!(!any_seen(&file));
        let pb = fake_pocketbook(&tmp.path().join("PB"));
        remember(&file, &pb, "2026-10-23T05:30:00Z").unwrap();
        assert!(any_seen(&file));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("pocketbook/PB"), "{text}");
    }

    #[test]
    fn stems() {
        assert_eq!(
            stem("deckle-daily-2026-10-01.kepub.epub"),
            "deckle-daily-2026-10-01"
        );
        assert_eq!(
            stem("deckle-daily-2026-10-01.epub"),
            "deckle-daily-2026-10-01"
        );
        assert_eq!(
            stem("deckle-daily-2026-10-01.kfx"),
            "deckle-daily-2026-10-01"
        );
    }
}
