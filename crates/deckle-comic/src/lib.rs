// SPDX-License-Identifier: GPL-3.0-or-later
//! Import a comic — a CBZ, or a folder of page images — as a fixed-layout EPUB 3, so every later
//! stage (inspect, normalize, Previewer, deliver) runs on it like any other book (fixed-layout
//! spec §6).
//!
//! One page per image, in natural filename order, each sized to its own image; the images are
//! copied as they are, except that a landscape spread after the cover becomes two pages (spec
//! §7.1). `ComicInfo.xml` gives the metadata and, for manga, the right-to-left reading order.
//! Fitting pages to a device's screen happens at finish ([`pages::fit`]), not here.

mod cbr;
mod comicinfo;
mod order;
pub mod pages;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use comicinfo::ComicInfo;
use pages::Half;

#[derive(Debug, thiserror::Error)]
pub enum ComicError {
    #[error("this comic has no pages: there are no JPEG, PNG or GIF images in it")]
    NoPages,
    #[error("page {0} can't be read as an image")]
    BadPage(String),
    #[error("page {page} is {format}, which Kindles can't show; convert it to JPEG or PNG first")]
    UnsupportedPage { page: String, format: String },
    #[error("page {0} is password-protected; only unprotected comics can be converted")]
    Protected(String),
    #[error("this isn't a CBZ file: {0}")]
    NotZip(String),
    #[error("this CBR couldn't be opened: {0}")]
    NotArchive(String),
    #[error(
        "opening a CBR needs bsdtar (libarchive): macOS and Windows have it as `tar`; on Linux, \
         install libarchive-tools, or set DECKLE_BSDTAR to it"
    )]
    NoBsdtar,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub title: String,
    pub pages: usize,
    /// Landscape spreads split into two pages each.
    pub spreads: usize,
    /// The spine reads right to left (manga).
    pub right_to_left: bool,
    pub warnings: Vec<String>,
}

/// `.cbz`, any case.
pub fn is_cbz(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cbz"))
}

/// A folder of page images: images at its top level or one folder down (chapter folders), and
/// no Markdown (that's a notes book) or other books (that's a shelf to walk) at its top level.
pub fn is_image_folder(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    let mut images = false;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            subdirs.push(e.path());
        } else if t.is_file() {
            if NOT_A_PAGE_FOLDER.contains(&extension(&name).as_str()) {
                return false;
            }
            images |= is_image_name(&name);
        }
    }
    images
        || subdirs.iter().any(|d| {
            std::fs::read_dir(d).is_ok_and(|es| {
                es.flatten().any(|e| {
                    e.file_type().is_ok_and(|t| t.is_file())
                        && is_image_name(&e.file_name().to_string_lossy())
                })
            })
        })
}

/// `.cbr`, any case: a RAR (or, often enough, a zip) of page images.
pub fn is_cbr(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cbr"))
}

/// A comic archive: a CBZ or a CBR.
pub fn is_comic_archive(path: &Path) -> bool {
    is_cbz(path) || is_cbr(path)
}

/// A CBZ, a CBR or a folder of page images.
pub fn is_comic(path: &Path) -> bool {
    is_comic_archive(path) || is_image_folder(path)
}

/// Converts the comic at `source` (a CBZ, a CBR or a folder) and writes the EPUB to `epub`.
pub fn convert(source: &Path, epub: &Path) -> Result<ImportReport, ComicError> {
    convert_with(source, epub, None)
}

/// Reading order, when the owner sets it for a book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
}

/// [`convert`], reading in `direction` when given, whatever `ComicInfo.xml` says (fixed-layout
/// spec §3: the owner's choice comes first).
pub fn convert_with(
    source: &Path,
    epub: &Path,
    direction: Option<Direction>,
) -> Result<ImportReport, ComicError> {
    let mut src = Source::open(source)?;
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut names = src.names();
    let mut info = match names.iter().position(|n| is_comicinfo(n)) {
        Some(i) => {
            let name = names.remove(i);
            ComicInfo::parse(&src.read(&name)?)
        }
        None => ComicInfo::default(),
    };
    if let Some(d) = direction {
        info.right_to_left = d == Direction::RightToLeft;
    }
    names.retain(|n| is_image_name(n) || is_unsupported_image_name(n));
    order::natural_sort(&mut names);

    let mut pages = Vec::with_capacity(names.len());
    let mut spreads = 0;
    for name in names {
        if is_unsupported_image_name(&name) {
            return Err(ComicError::UnsupportedPage {
                format: extension(&name).to_uppercase(),
                page: name,
            });
        }
        let bytes = src.read(&name)?;
        let page = Page::probe(name, &bytes)?;
        // A landscape page after the cover is a spread: two pages on a portrait screen. The
        // cover isn't split, since a wraparound one would put the back cover first.
        if !pages.is_empty() && page.width > page.height {
            spreads += 1;
            let halves = if info.right_to_left {
                [Half::Right, Half::Left]
            } else {
                [Half::Left, Half::Right]
            };
            pages.extend(halves.map(|h| page.half(h)));
        } else {
            pages.push(page);
        }
    }
    if pages.is_empty() {
        return Err(ComicError::NoPages);
    }

    let book = Book::new(&info, &stem, &pages);
    write_epub(epub, &book, &pages, &mut src)?;
    Ok(ImportReport {
        title: book.title,
        pages: pages.len(),
        spreads,
        right_to_left: book.right_to_left,
        warnings: book.warnings,
    })
}

/// Where the pages come from.
enum Source {
    Zip(zip::ZipArchive<std::fs::File>),
    Folder(PathBuf),
    /// A CBR unpacked into a temp folder, removed with the source.
    Unpacked(tempfile::TempDir),
}

fn starts_with(path: &Path, magic: &[u8]) -> std::io::Result<bool> {
    use std::io::Read;
    let mut head = vec![0u8; magic.len()];
    let n = std::fs::File::open(path)?.read(&mut head)?;
    Ok(n == magic.len() && head == magic)
}

impl Source {
    fn open(path: &Path) -> Result<Self, ComicError> {
        if path.is_dir() {
            return Ok(Source::Folder(path.to_path_buf()));
        }
        // A CBR is often a zip under another name; a real one (RAR, 7z, tar) is unpacked by
        // the system's bsdtar into a temp folder (fixed-layout spec §10: no bundled unRAR).
        if is_cbr(path) && !starts_with(path, b"PK\x03\x04")? {
            let dir = tempfile::Builder::new().prefix("deckle-cbr-").tempdir()?;
            cbr::extract(&cbr::bsdtar()?, path, dir.path())?;
            return Ok(Source::Unpacked(dir));
        }
        let file = std::fs::File::open(path)?;
        zip::ZipArchive::new(file)
            .map(Source::Zip)
            .map_err(|e| ComicError::NotZip(e.to_string()))
    }

    /// Every file, as a `/`-separated path, skipping hidden files and macOS resource forks.
    fn names(&self) -> Vec<String> {
        let names = match self {
            Source::Zip(z) => z
                .file_names()
                .flatten()
                .filter(|n| !n.ends_with('/'))
                .map(|n| n.into_owned())
                .collect(),
            Source::Folder(_) | Source::Unpacked(_) => {
                let mut out = Vec::new();
                walk(self.root(), "", 0, &mut out);
                out
            }
        };
        names
            .into_iter()
            .filter(|n| {
                !n.split('/')
                    .any(|seg| seg.starts_with('.') || seg == "__MACOSX")
            })
            .collect()
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>, ComicError> {
        match self {
            Source::Zip(z) => {
                let mut f = z.by_name(name).map_err(|e| match e {
                    zip::result::ZipError::UnsupportedArchive(m) if m.contains("Password") => {
                        ComicError::Protected(name.to_owned())
                    }
                    e => ComicError::NotZip(e.to_string()),
                })?;
                let mut bytes = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut bytes)?;
                Ok(bytes)
            }
            Source::Folder(_) | Source::Unpacked(_) => Ok(std::fs::read(self.root().join(name))?),
        }
    }

    /// The folder pages are read from (not a zip's).
    fn root(&self) -> &Path {
        match self {
            Source::Folder(root) => root,
            Source::Unpacked(dir) => dir.path(),
            Source::Zip(_) => Path::new(""),
        }
    }
}

/// Files under `dir`, as paths relative to the folder being imported; three levels at most.
fn walk(dir: &Path, prefix: &str, depth: u8, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = format!("{prefix}{name}");
        match e.file_type() {
            Ok(t) if t.is_dir() && depth < 2 => walk(&e.path(), &format!("{rel}/"), depth + 1, out),
            Ok(t) if t.is_file() => out.push(rel),
            _ => {}
        }
    }
}

/// Files that make a folder something other than a comic: Markdown notes, or books of their own.
const NOT_A_PAGE_FOLDER: [&str; 17] = [
    "md", "markdown", "epub", "kfx", "cbz", "cbr", "pdf", "docx", "doc", "odt", "rtf", "mobi",
    "azw", "azw3", "prc", "fb2", "djvu",
];

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_image_name(name: &str) -> bool {
    matches!(extension(name).as_str(), "jpg" | "jpeg" | "png" | "gif")
}

/// Image formats a page might be in that Kindles can't show (spec §6: refused, not dropped).
fn is_unsupported_image_name(name: &str) -> bool {
    matches!(
        extension(name).as_str(),
        "webp" | "avif" | "jxl" | "heic" | "bmp" | "tif" | "tiff"
    )
}

fn is_comicinfo(name: &str) -> bool {
    name.rsplit('/')
        .next()
        .is_some_and(|f| f.eq_ignore_ascii_case("comicinfo.xml"))
}

/// One page: where it came from, what it is, and its size in pixels.
struct Page {
    source: String,
    ext: &'static str,
    media_type: &'static str,
    width: u32,
    height: u32,
    /// The half of a spread this page is, cut at write time.
    half: Option<Half>,
}

impl Page {
    /// Reads the image's header: its real format (not its file name's) and its size.
    fn probe(source: String, bytes: &[u8]) -> Result<Self, ComicError> {
        let bad = || ComicError::BadPage(source.clone());
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| bad())?;
        let (ext, media_type) = match reader.format() {
            Some(image::ImageFormat::Jpeg) => ("jpg", "image/jpeg"),
            Some(image::ImageFormat::Png) => ("png", "image/png"),
            Some(image::ImageFormat::Gif) => ("gif", "image/gif"),
            Some(other) => {
                return Err(ComicError::UnsupportedPage {
                    format: format!("{other:?}").to_uppercase(),
                    page: source,
                });
            }
            None => return Err(bad()),
        };
        let (width, height) = reader.into_dimensions().map_err(|_| bad())?;
        if width == 0 || height == 0 {
            return Err(bad());
        }
        Ok(Page {
            source,
            ext,
            media_type,
            width,
            height,
            half: None,
        })
    }

    /// One half of this spread (a GIF's halves are PNG: see [`pages::split`]).
    fn half(&self, half: Half) -> Page {
        let (ext, media_type) = if self.ext == "gif" {
            ("png", "image/png")
        } else {
            (self.ext, self.media_type)
        };
        Page {
            source: self.source.clone(),
            ext,
            media_type,
            width: pages::half_width(self.width, half),
            height: self.height,
            half: Some(half),
        }
    }
}

/// The book-level facts the OPF needs.
struct Book {
    title: String,
    writer: Option<String>,
    series: Option<(String, Option<String>)>,
    language: String,
    description: Option<String>,
    right_to_left: bool,
    identifier: String,
    warnings: Vec<String>,
}

impl Book {
    fn new(info: &ComicInfo, stem: &str, pages: &[Page]) -> Self {
        let mut warnings = Vec::new();
        let title = info
            .title
            .clone()
            .or_else(|| {
                info.series.as_ref().map(|s| match &info.number {
                    Some(n) => format!("{s} {n}"),
                    None => s.clone(),
                })
            })
            .unwrap_or_else(|| stem.to_owned());
        let language = match info.language.as_deref() {
            Some(l) if is_language_tag(l) => l.to_owned(),
            Some(l) => {
                warnings.push(format!("unknown language \"{l}\"; using English"));
                "en".into()
            }
            None => "en".into(),
        };
        let identifier = {
            let mut h = Sha256::new();
            h.update(title.as_bytes());
            h.update(b"|");
            h.update(info.writer.as_deref().unwrap_or("").as_bytes());
            h.update(b"|");
            h.update(pages.len().to_string().as_bytes());
            let hex: String = h.finalize()[..8]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            format!("urn:deckle:comic:{hex}")
        };
        Book {
            title,
            writer: info.writer.clone(),
            series: info.series.clone().map(|s| (s, info.number.clone())),
            language,
            description: info.summary.clone(),
            right_to_left: info.right_to_left,
            identifier,
            warnings,
        }
    }
}

/// `en`, `ja`, `pt-BR`: letters and digits in hyphen-separated parts, the first 2–3 letters.
fn is_language_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    parts
        .next()
        .is_some_and(|p| (2..=3).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphabetic()))
        && parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn page_name(i: usize) -> String {
    format!("p{:04}", i + 1)
}

fn write_epub(
    path: &Path,
    book: &Book,
    pages: &[Page],
    src: &mut Source,
) -> Result<(), ComicError> {
    let out = std::fs::File::create(path)?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| ComicError::Io(std::io::Error::other(e));
    let put = |zw: &mut zip::ZipWriter<std::fs::File>,
               name: &str,
               bytes: &[u8],
               opts: zip::write::SimpleFileOptions|
     -> Result<(), ComicError> {
        zw.start_file(name, opts).map_err(err)?;
        zw.write_all(bytes)?;
        Ok(())
    };
    put(&mut zw, "mimetype", b"application/epub+zip", stored)?;
    put(
        &mut zw,
        "META-INF/container.xml",
        CONTAINER.as_bytes(),
        deflated,
    )?;
    put(
        &mut zw,
        "OEBPS/content.opf",
        opf(book, pages).as_bytes(),
        deflated,
    )?;
    put(&mut zw, "OEBPS/nav.xhtml", nav(book).as_bytes(), deflated)?;
    for (i, page) in pages.iter().enumerate() {
        put(
            &mut zw,
            &format!("OEBPS/pages/{}.xhtml", page_name(i)),
            page_xhtml(book, page, i).as_bytes(),
            deflated,
        )?;
    }
    // Images are already compressed; one at a time, so a large CBZ isn't held in memory.
    for (i, page) in pages.iter().enumerate() {
        let mut bytes = src.read(&page.source)?;
        if let Some(half) = page.half {
            bytes = pages::split(&bytes, half)
                .map_err(|_| ComicError::BadPage(page.source.clone()))?
                .0;
        }
        put(
            &mut zw,
            &format!("OEBPS/images/{}.{}", page_name(i), page.ext),
            &bytes,
            stored,
        )?;
    }
    zw.finish().map_err(err)?;
    Ok(())
}

fn opf(book: &Book, pages: &[Page]) -> String {
    let first = &pages[0];
    let mut meta = String::new();
    if let Some(w) = &book.writer {
        meta.push_str(&format!("<dc:creator>{}</dc:creator>", escape(w)));
    }
    if let Some(d) = &book.description {
        meta.push_str(&format!("<dc:description>{}</dc:description>", escape(d)));
    }
    if let Some((series, number)) = &book.series {
        meta.push_str(&format!(
            "<meta property=\"belongs-to-collection\" id=\"series\">{}</meta>\
             <meta refines=\"#series\" property=\"collection-type\">series</meta>\
             <meta name=\"calibre:series\" content=\"{}\"/>",
            escape(series),
            escape(series)
        ));
        if let Some(n) = number
            .as_deref()
            .filter(|n| n.trim().parse::<f32>().is_ok())
        {
            meta.push_str(&format!(
                "<meta refines=\"#series\" property=\"group-position\">{n}</meta>\
                 <meta name=\"calibre:series_index\" content=\"{n}\"/>",
                n = escape(n.trim())
            ));
        }
    }
    if book.right_to_left {
        meta.push_str("<meta name=\"primary-writing-mode\" content=\"horizontal-rl\"/>");
    }
    let manifest: String = pages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let n = page_name(i);
            let cover = if i == 0 {
                " properties=\"cover-image\""
            } else {
                ""
            };
            format!(
                "<item id=\"{n}\" href=\"pages/{n}.xhtml\" media-type=\"application/xhtml+xml\"/>\
                 <item id=\"img-{n}\" href=\"images/{n}.{ext}\" media-type=\"{mt}\"{cover}/>",
                ext = p.ext,
                mt = p.media_type,
            )
        })
        .collect();
    let spine: String = (0..pages.len())
        .map(|i| format!("<itemref idref=\"{}\"/>", page_name(i)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"{lang}\">\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:identifier id=\"uid\">{id}</dc:identifier>\
         <dc:title>{title}</dc:title>\
         <dc:language>{lang}</dc:language>{meta}\
         <meta property=\"dcterms:modified\">2000-01-01T00:00:00Z</meta>\
         <meta property=\"rendition:layout\">pre-paginated</meta>\
         <meta property=\"rendition:spread\">landscape</meta>\
         <meta property=\"rendition:orientation\">auto</meta>\
         <meta name=\"cover\" content=\"img-{first_name}\"/>\
         <meta name=\"book-type\" content=\"comic\"/>\
         <meta name=\"fixed-layout\" content=\"true\"/>\
         <meta name=\"original-resolution\" content=\"{w}x{h}\"/>\
         <meta name=\"zero-gutter\" content=\"true\"/>\
         <meta name=\"zero-margin\" content=\"true\"/>\
         </metadata><manifest>\
         <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
         {manifest}</manifest><spine{ppd}>{spine}</spine></package>",
        lang = escape(&book.language),
        id = escape(&book.identifier),
        title = escape(&book.title),
        first_name = page_name(0),
        w = first.width,
        h = first.height,
        ppd = if book.right_to_left {
            " page-progression-direction=\"rtl\""
        } else {
            ""
        },
    )
}

fn page_xhtml(book: &Book, page: &Page, i: usize) -> String {
    let (w, h) = (page.width, page.height);
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{lang}\" lang=\"{lang}\"><head><title>{title} – page {n}</title>\
         <meta name=\"viewport\" content=\"width={w}, height={h}\"/>\
         <style>html, body {{ margin: 0; padding: 0; }} \
         body {{ width: {w}px; height: {h}px; }} \
         img {{ display: block; width: {w}px; height: {h}px; }}</style></head>\
         <body><img src=\"../images/{name}.{ext}\" alt=\"Page {n}\"/></body></html>\n",
        lang = escape(&book.language),
        title = escape(&book.title),
        n = i + 1,
        name = page_name(i),
        ext = page.ext,
    )
}

fn nav(book: &Book) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{lang}\" lang=\"{lang}\"><head><title>Contents</title></head><body>\
         <nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1><ol><li>\
         <a href=\"pages/{first}.xhtml\">{title}</a></li></ol></nav></body></html>\n",
        lang = escape(&book.language),
        title = escape(&book.title),
        first = page_name(0),
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const CONTAINER: &str = "<?xml version=\"1.0\"?>\n<container version=\"1.0\" \
    xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile \
    full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>\n";

#[cfg(test)]
mod tests {
    use super::is_language_tag;

    #[test]
    fn language_tags() {
        for ok in ["en", "ja", "pt-BR", "zh-Hant-TW"] {
            assert!(is_language_tag(ok), "{ok}");
        }
        for bad in ["", "English", "e", "en_US", "en-"] {
            assert!(!is_language_tag(bad), "{bad}");
        }
    }
}
