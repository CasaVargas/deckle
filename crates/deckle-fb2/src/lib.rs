// SPDX-License-Identifier: GPL-3.0-or-later
//! Import a FictionBook file (`.fb2`, or `.fb2.zip`) as a clean EPUB 3, so every later stage
//! (inspect, normalize, Previewer, repackage, deliver) runs on it unchanged.
//!
//! FB2 is XML with a fixed schema, so the structure is mapped rather than guessed: sections
//! become the contents, the notes body becomes pop-up footnotes, embedded binaries become images
//! and the cover, and `title-info` gives the authors, series and language.

mod render;

use std::io::{Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

use render::{Renderer, chapter_name, child, elements, escape, name, text_of};

/// EPUB files as (zip path, bytes), in zip order.
pub type Files = Vec<(String, Vec<u8>)>;

/// Larger than any real book; anything bigger is almost certainly not one.
const MAX_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Fb2Error {
    #[error("the FB2 file is empty")]
    Empty,
    #[error("the FB2 file isn't valid XML: {0}")]
    Xml(String),
    #[error("this isn't a FictionBook file: its root element is <{0}>")]
    NotFb2(String),
    #[error("the FB2 file has no body text")]
    NoBody,
    #[error("the FB2 file is {} MB; the limit is 50 MB", .0 / (1024 * 1024))]
    TooLarge(u64),
    #[error("the zip should hold exactly one .fb2 file; it holds {0}")]
    Zip(usize),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub title: String,
    /// XHTML files in the spine.
    pub chapters: usize,
    /// Entries in the contents.
    pub contents: usize,
    /// Images packaged, the cover included.
    pub images: usize,
    pub warnings: Vec<String>,
}

fn lower_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Inputs the importer takes: `.fb2` or `.fb2.zip`, any case.
pub fn is_fb2(path: &Path) -> bool {
    let n = lower_name(path);
    n.ends_with(".fb2") || n.ends_with(".fb2.zip")
}

/// Converts the FB2 file at `fb2` and writes the EPUB to `epub`.
pub fn convert(fb2: &Path, epub: &Path) -> Result<ImportReport, Fb2Error> {
    let size = std::fs::metadata(fb2)?.len();
    if size > MAX_BYTES {
        return Err(Fb2Error::TooLarge(size));
    }
    let mut bytes = std::fs::read(fb2)?;
    let file = fb2
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut stem = file.as_str();
    if lower_name(fb2).ends_with(".zip") {
        bytes = unzip(&bytes)?;
        stem = &stem[..stem.len() - ".zip".len()];
    }
    if stem.to_lowercase().ends_with(".fb2") {
        stem = &stem[..stem.len() - ".fb2".len()];
    }
    let (files, report) = to_epub(&bytes, stem)?;
    write_zip(epub, &files)?;
    Ok(report)
}

/// The one `.fb2` inside a `.fb2.zip`.
pub fn unzip(bytes: &[u8]) -> Result<Vec<u8>, Fb2Error> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| Fb2Error::Io(std::io::Error::other(e)))?;
    let inner: Vec<usize> = (0..zip.len())
        .filter(|&i| {
            zip.by_index(i).is_ok_and(|f| {
                let n = f.name().map(|n| n.to_lowercase()).unwrap_or_default();
                f.is_file() && n.ends_with(".fb2") && !n.starts_with("__macosx/")
            })
        })
        .collect();
    let [only] = inner[..] else {
        return Err(Fb2Error::Zip(inner.len()));
    };
    let entry = zip
        .by_index(only)
        .map_err(|e| Fb2Error::Io(std::io::Error::other(e)))?;
    let mut out = Vec::new();
    entry.take(MAX_BYTES + 1).read_to_end(&mut out)?;
    if out.len() as u64 > MAX_BYTES {
        return Err(Fb2Error::TooLarge(out.len() as u64));
    }
    Ok(out)
}

/// A BOM wins, then the XML declaration's encoding, then UTF-8. The declaration is dropped,
/// since the text is no longer in that encoding.
fn decode(bytes: &[u8], warnings: &mut Vec<String>) -> String {
    let text = if let Some((enc, bom)) = encoding_rs::Encoding::for_bom(bytes) {
        enc.decode_without_bom_handling(&bytes[bom..])
            .0
            .into_owned()
    } else {
        let encoding = declared_encoding(bytes).and_then(|label| {
            let found = encoding_rs::Encoding::for_label(label.as_bytes());
            if found.is_none() {
                warnings.push(format!("unknown encoding \"{label}\"; read as UTF-8"));
            }
            found
        });
        let (text, bad) = encoding
            .unwrap_or(encoding_rs::UTF_8)
            .decode_without_bom_handling(bytes);
        if bad {
            warnings.push("some characters couldn't be decoded and were replaced".into());
        }
        text.into_owned()
    };
    let trimmed = text.trim_start();
    match trimmed
        .strip_prefix("<?xml")
        .and_then(|r| r.find("?>").map(|e| &r[e + 2..]))
    {
        Some(rest) => rest.to_owned(),
        None => trimmed.to_owned(),
    }
}

/// `encoding="…"` from an `<?xml …?>` declaration at the start.
fn declared_encoding(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(1024)];
    let head = String::from_utf8_lossy(head);
    let decl = head.trim_start().strip_prefix("<?xml")?;
    let decl = &decl[..decl.find("?>")?];
    let after = &decl[decl.find("encoding")? + "encoding".len()..];
    let after = after.trim_start().strip_prefix('=')?.trim_start();
    let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &after[1..];
    Some(value[..value.find(quote)?].trim().to_owned())
}

struct Meta {
    title: String,
    authors: Vec<String>,
    language: String,
    description: Option<String>,
    subjects: Vec<String>,
    series: Option<(String, Option<u32>)>,
    cover: Option<String>,
}

fn metadata(root: roxmltree::Node, fallback_title: &str, warnings: &mut Vec<String>) -> Meta {
    let info = child(root, "description").and_then(|d| child(d, "title-info"));
    let text = |tag: &str| {
        info.and_then(|i| child(i, tag))
            .map(text_of)
            .filter(|t| !t.is_empty())
    };
    let authors = info
        .map(|i| {
            elements(i)
                .filter(|a| name(*a) == "author")
                .filter_map(|a| {
                    let full: Vec<String> = ["first-name", "middle-name", "last-name"]
                        .iter()
                        .filter_map(|t| child(a, t).map(text_of))
                        .filter(|t| !t.is_empty())
                        .collect();
                    if full.is_empty() {
                        child(a, "nickname").map(text_of).filter(|t| !t.is_empty())
                    } else {
                        Some(full.join(" "))
                    }
                })
                .fold(Vec::new(), |mut all: Vec<String>, a| {
                    if !all.contains(&a) {
                        all.push(a);
                    }
                    all
                })
        })
        .unwrap_or_default();
    let language = match text("lang") {
        Some(l) if is_language_tag(&l) => l,
        Some(l) => {
            warnings.push(format!("unknown language \"{l}\"; using English"));
            "en".into()
        }
        None => "en".into(),
    };
    let description = info.and_then(|i| child(i, "annotation")).map(|a| {
        let paras: Vec<String> = elements(a).map(text_of).filter(|t| !t.is_empty()).collect();
        if paras.is_empty() {
            text_of(a)
        } else {
            paras.join("\n\n")
        }
    });
    let subjects = info
        .map(|i| {
            elements(i)
                .filter(|g| name(*g) == "genre")
                .map(text_of)
                .filter(|g| !g.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let series = info.and_then(|i| child(i, "sequence")).and_then(|s| {
        let n = s.attribute("name")?.trim();
        (!n.is_empty()).then(|| {
            let number = s
                .attribute("number")
                .and_then(|v| v.trim().parse::<u32>().ok())
                .filter(|v| *v > 0);
            (n.to_owned(), number)
        })
    });
    let cover = info
        .and_then(|i| child(i, "coverpage"))
        .and_then(|c| child(c, "image"))
        .and_then(|img| img.attributes().find(|a| a.name() == "href"))
        .and_then(|a| a.value().strip_prefix('#').map(str::to_owned));
    Meta {
        title: text("book-title").unwrap_or_else(|| fallback_title.to_owned()),
        authors,
        language,
        description: description.filter(|d| !d.is_empty()),
        subjects,
        series,
        cover,
    }
}

/// A two- or three-letter language, then optional subtags: `ru`, `en-GB`, `zh-Hant`.
fn is_language_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    let primary = parts.next().unwrap_or("");
    (2..=3).contains(&primary.len())
        && primary.chars().all(|c| c.is_ascii_alphabetic())
        && parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// The EPUB's files in zip order (`mimetype` first). `fallback_title` is used when the book has
/// no `book-title`.
pub fn to_epub(bytes: &[u8], fallback_title: &str) -> Result<(Files, ImportReport), Fb2Error> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Fb2Error::TooLarge(bytes.len() as u64));
    }
    let mut warnings = Vec::new();
    let text = decode(bytes, &mut warnings);
    if text.trim().is_empty() {
        return Err(Fb2Error::Empty);
    }
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(&text, options)
        .map_err(|e| Fb2Error::Xml(e.to_string()))?;
    let root = doc.root_element();
    if name(root) != "FictionBook" {
        return Err(Fb2Error::NotFb2(name(root).to_owned()));
    }
    let meta = metadata(root, fallback_title, &mut warnings);

    let bodies: Vec<_> = elements(root).filter(|b| name(*b) == "body").collect();
    let main = *bodies
        .iter()
        .find(|b| b.attribute("name").is_none())
        .or(bodies.first())
        .ok_or(Fb2Error::NoBody)?;
    let has_text = main
        .descendants()
        .any(|d| d.is_text() && d.text().is_some_and(|t| !t.trim().is_empty()));
    if !has_text {
        return Err(Fb2Error::NoBody);
    }
    let is_notes = |b: &roxmltree::Node| {
        b.attribute("name")
            .is_some_and(|n| n.eq_ignore_ascii_case("notes") || n.eq_ignore_ascii_case("comments"))
    };
    let notes: Vec<_> = bodies
        .iter()
        .copied()
        .filter(|b| *b != main && is_notes(b))
        .collect();
    let extra: Vec<_> = bodies
        .iter()
        .copied()
        .filter(|b| *b != main && !is_notes(b))
        .collect();
    let mut book: Vec<_> = vec![main];
    book.extend(&extra);

    let mut r = Renderer::new(root, &book, &notes);
    let cover = meta.cover.as_deref().and_then(|id| r.image(id));
    for body in &book {
        if *body != main {
            let label = body.attribute("name").unwrap_or("");
            r.warnings.push(format!(
                "extra body \"{label}\" added at the end of the book"
            ));
        }
        r.body(*body);
    }
    r.notes(&notes);
    r.resolve_links();
    warnings.append(&mut r.warnings);

    let mut contents = std::mem::take(&mut r.contents);
    if contents.is_empty() {
        contents.push((
            1,
            format!(
                "<a href=\"{}\">{}</a>",
                chapter_name(0),
                escape(&meta.title)
            ),
        ));
    }

    let mut files: Files = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".into(),
            CONTAINER.as_bytes().to_vec(),
        ),
    ];
    for page in &r.pages {
        let doc_title = page.title.as_deref().unwrap_or(&meta.title);
        files.push((
            format!("OEBPS/{}", page.name),
            xhtml(doc_title, &meta.language, &page.html).into_bytes(),
        ));
    }
    let nav = format!(
        "<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1>{}</nav>",
        nest(&contents)
    );
    files.push((
        "OEBPS/nav.xhtml".into(),
        xhtml("Contents", &meta.language, &nav).into_bytes(),
    ));
    files.push(("OEBPS/style.css".into(), CSS.as_bytes().to_vec()));
    for img in &r.images {
        files.push((format!("OEBPS/{}", img.name), img.bytes.clone()));
    }

    let identifier = {
        let mut h = Sha256::new();
        h.update(meta.title.as_bytes());
        h.update(b"|");
        h.update(meta.authors.join("; ").as_bytes());
        let hex: String = h.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("urn:deckle:fb2:{hex}")
    };
    let mut manifest = String::new();
    let mut spine = String::new();
    for (i, page) in r.pages.iter().enumerate() {
        let i = i + 1;
        manifest.push_str(&format!(
            "<item id=\"c{i}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>",
            page.name
        ));
        spine.push_str(&format!("<itemref idref=\"c{i}\"/>"));
    }
    for (i, img) in r.images.iter().enumerate() {
        let props = if Some(i) == cover {
            " properties=\"cover-image\""
        } else {
            ""
        };
        manifest.push_str(&format!(
            "<item id=\"img{}\" href=\"{}\" media-type=\"{}\"{props}/>",
            i + 1,
            img.name,
            img.mime
        ));
    }
    let mut extra_meta = String::new();
    for a in &meta.authors {
        extra_meta.push_str(&format!("<dc:creator>{}</dc:creator>", escape(a)));
    }
    if let Some(d) = &meta.description {
        extra_meta.push_str(&format!("<dc:description>{}</dc:description>", escape(d)));
    }
    for s in &meta.subjects {
        extra_meta.push_str(&format!("<dc:subject>{}</dc:subject>", escape(s)));
    }
    if let Some((series, number)) = &meta.series {
        extra_meta.push_str(&format!(
            "<meta property=\"belongs-to-collection\" id=\"series\">{s}</meta>\
             <meta refines=\"#series\" property=\"collection-type\">series</meta>",
            s = escape(series)
        ));
        if let Some(n) = number {
            extra_meta.push_str(&format!(
                "<meta refines=\"#series\" property=\"group-position\">{n}</meta>"
            ));
        }
        extra_meta.push_str(&format!(
            "<meta name=\"calibre:series\" content=\"{}\"/>",
            escape(series)
        ));
        if let Some(n) = number {
            extra_meta.push_str(&format!(
                "<meta name=\"calibre:series_index\" content=\"{n}\"/>"
            ));
        }
    }
    if let Some(i) = cover {
        extra_meta.push_str(&format!("<meta name=\"cover\" content=\"img{}\"/>", i + 1));
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"{lang}\">\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:identifier id=\"uid\">{identifier}</dc:identifier>\
         <dc:title>{title}</dc:title>\
         <dc:language>{lang}</dc:language>{extra_meta}\
         <meta property=\"dcterms:modified\">2000-01-01T00:00:00Z</meta>\
         </metadata><manifest>\
         <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\
         {manifest}</manifest><spine>{spine}</spine></package>",
        lang = escape(&meta.language),
        title = escape(&meta.title),
    );
    files.insert(2, ("OEBPS/content.opf".into(), opf.into_bytes()));

    let report = ImportReport {
        title: meta.title,
        chapters: r.pages.len(),
        contents: contents.len(),
        images: r.images.len(),
        warnings,
    };
    Ok((files, report))
}

/// Nested `<ol>` lists from (level, item html), in order; level 1 is the top.
fn nest(items: &[(u8, String)]) -> String {
    let mut out = String::from("<ol>");
    let mut depth = 1;
    for (i, (level, html)) in items.iter().enumerate() {
        let level = (*level).max(1);
        if level > depth && i > 0 {
            while depth < level {
                out.push_str("<ol>");
                depth += 1;
            }
        } else {
            if i > 0 {
                out.push_str("</li>");
            }
            while depth > level {
                out.push_str("</ol></li>");
                depth -= 1;
            }
        }
        out.push_str("<li>");
        out.push_str(html);
    }
    out.push_str("</li>");
    while depth > 1 {
        out.push_str("</ol></li>");
        depth -= 1;
    }
    out.push_str("</ol>");
    out
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

pub fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> Result<(), Fb2Error> {
    let out = std::fs::File::create(path)?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| Fb2Error::Io(std::io::Error::other(e));
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
p { margin: 0; text-indent: 1.2em; }
h1, h2, h3, h4, h5, h6 { text-align: center; margin: 2em 0 1em 0; }
h1.book-title { margin-top: 30%; }
p.empty-line { text-indent: 0; }
p.subtitle, p.inner-title, p.poem-title { text-align: center; text-indent: 0; font-weight: bold; margin: 1em 0; }
blockquote { margin: 1em 1.5em; }
blockquote.epigraph { margin-left: 35%; font-style: italic; }
p.text-author { text-align: right; text-indent: 0; font-style: normal; font-weight: bold; }
p.date { text-align: right; text-indent: 0; }
div.poem { margin: 1em 0 1em 2em; }
p.stanza { text-indent: 0; margin: 0 0 1em 0; }
div.image { text-align: center; margin: 1em 0; }
div.image img { max-width: 100%; }
table { border-collapse: collapse; margin: 1em auto; }
td, th { border: 1px solid #888; padding: 0.2em 0.4em; }
aside p { text-indent: 0; }
p.note-title { margin-top: 1em; }
p.backlink { text-align: right; }
";

#[cfg(test)]
mod tests {
    use super::{declared_encoding, is_language_tag};

    #[test]
    fn reads_the_declared_encoding() {
        assert_eq!(
            declared_encoding(b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><a/>").as_deref(),
            Some("windows-1251")
        );
        assert_eq!(
            declared_encoding(b"<?xml version='1.0' encoding = 'koi8-r' ?>").as_deref(),
            Some("koi8-r")
        );
        assert_eq!(declared_encoding(b"<?xml version=\"1.0\"?><a/>"), None);
        assert_eq!(declared_encoding(b"<a/>"), None);
    }

    #[test]
    fn language_tags() {
        assert!(is_language_tag("ru"));
        assert!(is_language_tag("en-GB"));
        assert!(!is_language_tag("Russian language"));
        assert!(!is_language_tag("1ru"));
        assert!(!is_language_tag("Russian"));
        assert!(!is_language_tag("en-"));
    }
}
