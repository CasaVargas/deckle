// SPDX-License-Identifier: GPL-3.0-or-later
//! Import an OpenDocument text file (`.odt`, `.ott`, or flat `.fodt`) as a clean EPUB 3, so every
//! later stage (inspect, normalize, Previewer, repackage, deliver) runs on it unchanged.
//!
//! The output matches the Word importer's: headings 1–3 become the contents and each level 1
//! starts a new file; formatting from the style chain becomes XHTML elements; pictures come from
//! the package (or base64 in flat ODF); footnotes and endnotes become pop-up asides.

mod body;
mod styles;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

use roxmltree::{Document, Node};
use sha2::{Digest, Sha256};

use body::{Block, Body, CLOSE, OPEN, escape, flatten, nest};
use styles::{Styles, attr, child, local};

/// EPUB files as (zip path, bytes), in zip order.
pub type Files = Vec<(String, Vec<u8>)>;

/// Larger than any real document; anything bigger is almost certainly not one.
const MAX_BYTES: u64 = 50 * 1024 * 1024;
const TEXT: &str = "application/vnd.oasis.opendocument.text";
const TEMPLATE: &str = "application/vnd.oasis.opendocument.text-template";

#[derive(Debug, thiserror::Error)]
pub enum OdtError {
    #[error("not an OpenDocument text file: it is neither a zip archive nor ODF XML")]
    NotOdf,
    #[error(
        "this is an OpenDocument {0}, not a text document; only .odt, .ott and .fodt are books"
    )]
    WrongType(String),
    #[error(
        "the document is password-protected; open it in LibreOffice, remove the password \
         (File → Properties → Security) and try again"
    )]
    Encrypted,
    #[error("not an OpenDocument text file: the archive has no content.xml")]
    NoContent,
    #[error("corrupt OpenDocument file: {0}")]
    Corrupt(String),
    #[error("the document is {} MB; the limit is 50 MB", .0 / (1024 * 1024))]
    TooLarge(u64),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub title: String,
    /// XHTML files in the spine.
    pub chapters: usize,
    /// Entries in the contents (headings 1–3).
    pub contents: usize,
    pub notes: usize,
    pub images: usize,
    pub warnings: Vec<String>,
}

/// Inputs the importer takes: `.odt`, `.ott` and `.fodt`, plus the other ODF types so they get a
/// clear error.
pub fn is_odt(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["odt", "ott", "fodt"]
            .iter()
            .any(|w| e.eq_ignore_ascii_case(w))
    })
}

/// Converts the document at `odt` and writes the EPUB to `epub`.
pub fn convert(odt: &Path, epub: &Path) -> Result<ImportReport, OdtError> {
    let size = std::fs::metadata(odt)?.len();
    if size > MAX_BYTES {
        return Err(OdtError::TooLarge(size));
    }
    let bytes = std::fs::read(odt)?;
    let stem = odt
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (files, report) = to_epub(&bytes, &stem)?;
    write_zip(epub, &files)?;
    Ok(report)
}

fn mimetype_name(m: &str) -> String {
    m.rsplit('.').next().unwrap_or(m).replace('-', " ")
}

fn check_mimetype(m: &str) -> Result<(), OdtError> {
    let m = m.trim();
    if m == TEXT || m == TEMPLATE {
        Ok(())
    } else {
        Err(OdtError::WrongType(mimetype_name(m)))
    }
}

/// The package's XML parts as strings, and its other files (pictures) as bytes.
struct Package {
    content: String,
    styles: Option<String>,
    meta: Option<String>,
    files: HashMap<String, Vec<u8>>,
}

fn read_zip(bytes: &[u8]) -> Result<Package, OdtError> {
    let corrupt = |e: zip::result::ZipError| OdtError::Corrupt(e.to_string());
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(corrupt)?;
    let mut total = 0u64;
    let mut read = |zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
                    name: &str|
     -> Result<Option<Vec<u8>>, OdtError> {
        let entry = match zip.by_name(name) {
            Ok(e) => e,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(corrupt(e)),
        };
        let mut out = Vec::new();
        entry.take(MAX_BYTES + 1 - total).read_to_end(&mut out)?;
        total += out.len() as u64;
        if total > MAX_BYTES {
            return Err(OdtError::TooLarge(total));
        }
        Ok(Some(out))
    };
    let string = |b: Option<Vec<u8>>, name: &str| -> Result<Option<String>, OdtError> {
        b.map(|b| {
            String::from_utf8(b).map_err(|_| OdtError::Corrupt(format!("{name} isn't UTF-8")))
        })
        .transpose()
    };
    if let Some(m) = read(&mut zip, "mimetype")? {
        check_mimetype(&String::from_utf8_lossy(&m))?;
    }
    if let Some(manifest) = read(&mut zip, "META-INF/manifest.xml")?
        && String::from_utf8_lossy(&manifest).contains("encryption-data")
    {
        return Err(OdtError::Encrypted);
    }
    let Some(content) = string(read(&mut zip, "content.xml")?, "content.xml")? else {
        return Err(OdtError::NoContent);
    };
    let styles = string(read(&mut zip, "styles.xml")?, "styles.xml")?;
    let meta = string(read(&mut zip, "meta.xml")?, "meta.xml")?;
    let names: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .filter(|n| !n.ends_with(".xml") && !n.ends_with('/') && n != "mimetype")
        .collect();
    let mut files = HashMap::new();
    for n in names {
        if let Some(b) = read(&mut zip, &n)? {
            files.insert(n, b);
        }
    }
    Ok(Package {
        content,
        styles,
        meta,
        files,
    })
}

fn parse<'i>(xml: &'i str, part: &str) -> Result<Document<'i>, OdtError> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    Document::parse_with_options(xml, options)
        .map_err(|e| OdtError::Corrupt(format!("{part}: {e}")))
}

struct Meta {
    title: Option<String>,
    creator: Option<String>,
    language: Option<String>,
    description: Option<String>,
    keywords: Vec<String>,
    date: Option<String>,
}

fn text_of(n: Node) -> String {
    let raw: String = n
        .descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect();
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// From `office:meta`.
fn metadata(meta: Option<Node>) -> Meta {
    let get = |name: &str| {
        meta.and_then(|m| child(m, name))
            .map(text_of)
            .filter(|t| !t.is_empty())
    };
    Meta {
        title: get("title"),
        creator: get("creator").or_else(|| get("initial-creator")),
        language: get("language"),
        description: get("description"),
        keywords: meta
            .map(|m| {
                m.children()
                    .filter(|k| k.is_element() && local(*k) == "keyword")
                    .map(text_of)
                    .filter(|k| !k.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        date: get("date"),
    }
}

/// The EPUB's files in zip order (`mimetype` first). `fallback_title` is used when the document
/// has neither a title property nor a Title paragraph.
pub fn to_epub(bytes: &[u8], fallback_title: &str) -> Result<(Files, ImportReport), OdtError> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(OdtError::TooLarge(bytes.len() as u64));
    }
    let package;
    let flat;
    let (content_doc, styles_doc, meta_doc) = if bytes.starts_with(b"PK") {
        package = read_zip(bytes)?;
        (
            parse(&package.content, "content.xml")?,
            package
                .styles
                .as_deref()
                .map(|s| parse(s, "styles.xml"))
                .transpose()?,
            package
                .meta
                .as_deref()
                .map(|s| parse(s, "meta.xml"))
                .transpose()?,
        )
    } else {
        let text = std::str::from_utf8(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
            .map_err(|_| OdtError::NotOdf)?;
        if !text.trim_start().starts_with('<') {
            return Err(OdtError::NotOdf);
        }
        flat = text;
        package = Package {
            content: String::new(),
            styles: None,
            meta: None,
            files: HashMap::new(),
        };
        (
            parse(flat, "the flat document").map_err(|_| OdtError::NotOdf)?,
            None,
            None,
        )
    };

    let root = content_doc.root_element();
    if !matches!(local(root), "document" | "document-content") {
        return Err(OdtError::NotOdf);
    }
    if local(root) == "document" {
        check_mimetype(attr(root, "mimetype").unwrap_or(""))?;
    }
    let roots: Vec<Node> = [Some(root), styles_doc.as_ref().map(|d| d.root_element())]
        .into_iter()
        .flatten()
        .collect();
    let containers = |name: &str| -> Vec<Node> {
        roots
            .iter()
            .flat_map(|r| r.children().filter(|c| c.is_element() && local(*c) == name))
            .collect()
    };
    let styles = Styles::parse(&containers("automatic-styles"), &containers("styles"));
    let meta_node = meta_doc
        .as_ref()
        .and_then(|d| child(d.root_element(), "meta"))
        .or_else(|| child(root, "meta"));
    let meta = metadata(meta_node);

    let Some(text) = child(root, "body").and_then(|b| child(b, "text")) else {
        return Err(OdtError::Corrupt(
            "the document has no office:text body".into(),
        ));
    };
    let mut body = Body::new(&styles, &package.files, text);
    let blocks = body.body(text);

    let title = meta
        .title
        .clone()
        .or_else(|| body.title.clone())
        .unwrap_or_else(|| fallback_title.to_owned());
    let language = meta
        .language
        .clone()
        .or_else(|| styles.language.clone())
        .unwrap_or_else(|| "en".into());
    let mut warnings = std::mem::take(&mut body.warnings);

    // Chapters: a new file at each level 1 heading, so the Kindle starts each on a fresh page.
    let mut chapters: Vec<Vec<Block>> = vec![Vec::new()];
    for b in blocks {
        let starts_chapter = matches!(b, Block::Heading { level: 1, .. });
        let current = chapters.last_mut().expect("one chapter");
        let has_content = current.iter().any(|b| !matches!(b, Block::Notes(_)));
        if starts_chapter && has_content {
            chapters.push(Vec::new());
        }
        chapters.last_mut().expect("one chapter").push(b);
    }
    if chapters.iter().all(|c| c.is_empty()) {
        warnings.push("the document has no text".into());
    }

    let chapter_name = |i: usize| format!("chapter-{:03}.xhtml", i + 1);
    let mut contents: Vec<(u8, bool, u32, String)> = Vec::new();
    let mut pages: Vec<(String, String, String)> = Vec::new();
    for (i, blocks) in chapters.iter().enumerate() {
        let name = chapter_name(i);
        let mut html = flatten(blocks);
        let mut chapter_title = None;
        for b in blocks {
            match b {
                Block::Heading {
                    level, id, text, ..
                } => {
                    chapter_title.get_or_insert_with(|| text.clone());
                    if *level <= 3 {
                        contents.push((
                            *level,
                            true,
                            1,
                            format!("<a href=\"{name}#{id}\">{}</a>", escape(text)),
                        ));
                    }
                }
                Block::Notes(ix) => {
                    for &n in ix {
                        html.push_str(&body.notes[n]);
                    }
                }
                _ => {}
            }
        }
        pages.push((name, chapter_title.unwrap_or_else(|| title.clone()), html));
    }
    resolve_links(&mut pages, &mut warnings);
    if contents.is_empty() {
        contents.push((
            1,
            true,
            1,
            format!("<a href=\"{}\">{}</a>", pages[0].0, escape(&title)),
        ));
    }

    let mut files: Files = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".into(),
            CONTAINER.as_bytes().to_vec(),
        ),
    ];
    for (name, doc_title, html) in &pages {
        files.push((
            format!("OEBPS/{name}"),
            xhtml(doc_title, &language, html).into_bytes(),
        ));
    }
    let nav_items: Vec<(u8, bool, u32, &str)> = contents
        .iter()
        .map(|(l, o, s, h)| (*l, *o, *s, h.as_str()))
        .collect();
    let nav = format!(
        "<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1>{}</nav>",
        nest(&nav_items)
    );
    files.push((
        "OEBPS/nav.xhtml".into(),
        xhtml("Contents", &language, &nav).into_bytes(),
    ));
    files.push(("OEBPS/style.css".into(), CSS.as_bytes().to_vec()));
    let mut manifest_images = String::new();
    for (i, img) in body.images.iter().enumerate() {
        manifest_images.push_str(&format!(
            "<item id=\"img{}\" href=\"{}\" media-type=\"{}\"/>",
            i + 1,
            img.href,
            img.media_type
        ));
        files.push((format!("OEBPS/{}", img.href), img.bytes.clone()));
    }

    let identifier = {
        let mut h = Sha256::new();
        h.update(title.as_bytes());
        h.update(b"|");
        h.update(meta.creator.as_deref().unwrap_or("").as_bytes());
        let hex: String = h.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("urn:deckle:odt:{hex}")
    };
    let modified = meta
        .date
        .as_deref()
        .and_then(w3c_seconds)
        .unwrap_or_else(|| "2000-01-01T00:00:00Z".into());
    let manifest_chapters: String = pages
        .iter()
        .enumerate()
        .map(|(i, (f, _, _))| {
            format!(
                "<item id=\"c{}\" href=\"{f}\" media-type=\"application/xhtml+xml\"/>",
                i + 1
            )
        })
        .collect();
    let spine: String = (1..=pages.len())
        .map(|i| format!("<itemref idref=\"c{i}\"/>"))
        .collect();
    let mut extra = String::new();
    if let Some(c) = &meta.creator {
        extra.push_str(&format!("<dc:creator>{}</dc:creator>", escape(c)));
    }
    if let Some(d) = &meta.description {
        extra.push_str(&format!("<dc:description>{}</dc:description>", escape(d)));
    }
    for k in &meta.keywords {
        extra.push_str(&format!("<dc:subject>{}</dc:subject>", escape(k)));
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"{lang}\">\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:identifier id=\"uid\">{identifier}</dc:identifier>\
         <dc:title>{title}</dc:title>\
         <dc:language>{lang}</dc:language>{extra}\
         <meta property=\"dcterms:modified\">{modified}</meta>\
         </metadata><manifest>\
         <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\
         {manifest_chapters}{manifest_images}</manifest><spine>{spine}</spine></package>",
        lang = escape(&language),
        title = escape(&title),
    );
    files.insert(2, ("OEBPS/content.opf".into(), opf.into_bytes()));

    let report = ImportReport {
        title,
        chapters: pages.len(),
        contents: contents.len(),
        notes: body.notes.len(),
        images: body.images.len(),
        warnings,
    };
    Ok((files, report))
}

/// Replaces bookmark-link placeholders with `file#id`, from where each `id` landed.
fn resolve_links(pages: &mut [(String, String, String)], warnings: &mut Vec<String>) {
    let mut placed = HashMap::new();
    for (name, _, html) in pages.iter() {
        let mut rest = html.as_str();
        while let Some(i) = rest.find(" id=\"") {
            rest = &rest[i + 5..];
            if let Some(end) = rest.find('"') {
                placed.entry(rest[..end].to_owned()).or_insert(name.clone());
            }
        }
    }
    for (_, _, html) in pages.iter_mut() {
        if !html.contains(OPEN) {
            continue;
        }
        let mut out = String::with_capacity(html.len());
        let mut rest = html.as_str();
        while let Some(start) = rest.find(OPEN) {
            out.push_str(&rest[..start]);
            let after = &rest[start + OPEN.len_utf8()..];
            let end = after.find(CLOSE).expect("placeholders are closed");
            let id = &after[..end];
            match placed.get(id) {
                Some(file) => out.push_str(&format!("{file}#{id}")),
                None => {
                    warnings.push(format!("bookmark {id} wasn't found; its link goes nowhere"));
                    out.push('#');
                }
            }
            rest = &after[end + CLOSE.len_utf8()..];
        }
        out.push_str(rest);
        *html = out;
    }
}

/// `2026-10-21T08:15:00.123` → `2026-10-21T08:15:00Z`; anything else → `None`.
fn w3c_seconds(s: &str) -> Option<String> {
    let s = s.trim();
    let base = s.get(..19)?;
    let ok = base.char_indices().all(|(i, c)| match i {
        4 | 7 => c == '-',
        10 => c == 'T',
        13 | 16 => c == ':',
        _ => c.is_ascii_digit(),
    });
    ok.then(|| format!("{base}Z"))
}

fn xhtml(title: &str, lang: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{lang}\" lang=\"{lang}\"><head><title>{title}</title>\
         <link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/></head><body>{body}</body></html>\n",
        lang = escape(lang),
        title = escape(title),
    )
}

pub fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> Result<(), OdtError> {
    let out = std::fs::File::create(path)?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| OdtError::Io(std::io::Error::other(e));
    for (name, bytes) in files {
        let opts = if name == "mimetype" { stored } else { deflated };
        zw.start_file(name.as_str(), opts).map_err(err)?;
        zw.write_all(bytes)?;
    }
    zw.finish().map_err(err)?;
    Ok(())
}

const CONTAINER: &str = "<?xml version=\"1.0\"?>\n<container version=\"1.0\" \
    xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile \
    full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>\n";

const CSS: &str = "body { font-family: serif; }
p { margin: 0 0 0.5em 0; }
.title { font-size: 2em; font-weight: bold; text-align: center; margin: 2em 0 0.5em 0; }
.subtitle { font-style: italic; text-align: center; margin-bottom: 2em; }
.center { text-align: center; }
.right { text-align: right; }
.figure { text-align: center; }
.sc { font-variant: small-caps; }
img { max-width: 100%; }
blockquote { margin: 0.5em 1.5em; }
table { border-collapse: collapse; margin: 1em 0; }
td, th { border: 1px solid #888; padding: 0.2em 0.4em; vertical-align: top; }
td p, th p { margin: 0; }
aside.footnote, aside.endnote { font-size: 0.9em; }
";

#[cfg(test)]
mod tests {
    use super::w3c_seconds;

    #[test]
    fn dates_trim_to_seconds() {
        assert_eq!(
            w3c_seconds("2026-10-25T09:30:12.345678").as_deref(),
            Some("2026-10-25T09:30:12Z")
        );
        assert_eq!(w3c_seconds("2026-10-25"), None);
    }
}
