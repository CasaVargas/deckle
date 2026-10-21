// SPDX-License-Identifier: GPL-3.0-or-later
//! Import a plain text file (.txt) as a clean EPUB 3, so every later stage (inspect, normalize,
//! Previewer, repackage, deliver) runs on it unchanged.
//!
//! Project Gutenberg licence text is dropped and its header sets the metadata; hard-wrapped lines
//! join into paragraphs; Chapter/Part lines and bare numerals become the contents, each starting a
//! new file; verse keeps its line breaks. The heuristics are conservative: when in doubt a line
//! break stays and no heading is invented.

mod text;

use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

use text::{Item, escape, gutenberg, items, language_tag};

/// EPUB files as (zip path, bytes), in zip order.
pub type Files = Vec<(String, Vec<u8>)>;

/// Larger than any real book; anything bigger is almost certainly not one.
const MAX_BYTES: u64 = 50 * 1024 * 1024;
/// A book with no headings is split into files of about this size, so Previewer gets
/// reasonably sized chapters.
const SPLIT_BYTES: usize = 200_000;

#[derive(Debug, thiserror::Error)]
pub enum TxtError {
    #[error("the text file is empty")]
    Empty,
    #[error("this doesn't look like a text file: it has binary data in it")]
    Binary,
    #[error("the text file is {} MB; the limit is 50 MB", .0 / (1024 * 1024))]
    TooLarge(u64),
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
    pub warnings: Vec<String>,
}

/// Inputs the importer takes: `.txt`, any case.
pub fn is_text(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
}

/// Converts the text file at `txt` and writes the EPUB to `epub`.
pub fn convert(txt: &Path, epub: &Path) -> Result<ImportReport, TxtError> {
    let size = std::fs::metadata(txt)?.len();
    if size > MAX_BYTES {
        return Err(TxtError::TooLarge(size));
    }
    let bytes = std::fs::read(txt)?;
    let stem = txt
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (files, report) = to_epub(&bytes, &stem)?;
    write_zip(epub, &files)?;
    Ok(report)
}

/// A BOM wins, then UTF-8, then Windows-1252 with a warning.
fn decode(bytes: &[u8], warnings: &mut Vec<String>) -> String {
    if let Some((enc, bom)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc
            .decode_without_bom_handling(&bytes[bom..])
            .0
            .into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => {
            warnings.push("decoded as Windows-1252".into());
            encoding_rs::WINDOWS_1252
                .decode_without_bom_handling(bytes)
                .0
                .into_owned()
        }
    }
}

/// More than 1% NUL or control characters (other than tab, line breaks and form feed).
fn is_binary(text: &str) -> bool {
    let mut total = 0usize;
    let mut bad = 0usize;
    for c in text.chars() {
        total += 1;
        if c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\x0c') {
            bad += 1;
        }
    }
    bad * 100 > total
}

/// The EPUB's files in zip order (`mimetype` first). `fallback_title` is used when the text has
/// no Gutenberg `Title:`.
pub fn to_epub(bytes: &[u8], fallback_title: &str) -> Result<(Files, ImportReport), TxtError> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(TxtError::TooLarge(bytes.len() as u64));
    }
    let mut warnings = Vec::new();
    let decoded = decode(bytes, &mut warnings);
    if decoded.trim().is_empty() {
        return Err(TxtError::Empty);
    }
    if is_binary(&decoded) {
        return Err(TxtError::Binary);
    }
    let decoded = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let (body, header) = gutenberg(&decoded);
    let items = items(body);
    if items.is_empty() {
        return Err(TxtError::Empty);
    }

    let title = header
        .title
        .clone()
        .unwrap_or_else(|| fallback_title.to_owned());
    let author = header.author.clone();
    let language = match header.language.as_deref() {
        Some(name) => language_tag(name).unwrap_or_else(|| {
            warnings.push(format!("unknown language \"{name}\"; using English"));
            "en".into()
        }),
        None => "en".into(),
    };

    // Files: a new one at each heading; with no headings, about every SPLIT_BYTES.
    let has_headings = items.iter().any(|i| matches!(i, Item::Heading { .. }));
    let mut files_html: Vec<String> = vec![String::new()];
    let mut file_titles: Vec<Option<String>> = vec![None];
    let mut contents: Vec<(u8, String)> = Vec::new();
    let mut heading_no = 0;
    for item in &items {
        let html = match item {
            Item::Heading {
                level,
                text,
                subtitle,
            } => {
                heading_no += 1;
                if !files_html.last().expect("one file").is_empty() {
                    files_html.push(String::new());
                    file_titles.push(None);
                }
                let label = match subtitle {
                    Some(s) => format!("{text} — {s}"),
                    None => text.clone(),
                };
                let name = chapter_name(files_html.len() - 1);
                contents.push((
                    *level,
                    format!("<a href=\"{name}#h{heading_no}\">{}</a>", escape(&label)),
                ));
                *file_titles.last_mut().expect("one file") = Some(label);
                let sub = subtitle
                    .as_deref()
                    .map(|s| format!("<br/><span class=\"subtitle\">{}</span>", escape(s)))
                    .unwrap_or_default();
                format!(
                    "<h{level} id=\"h{heading_no}\">{}{sub}</h{level}>",
                    escape(text)
                )
            }
            Item::Para(p) => format!("<p>{p}</p>"),
            Item::Verse(v) => format!("<p class=\"verse\">{v}</p>"),
            Item::Scene => "<hr class=\"scene\"/>".into(),
        };
        let current = files_html.last().expect("one file");
        if !has_headings && !current.is_empty() && current.len() + html.len() > SPLIT_BYTES {
            files_html.push(String::new());
            file_titles.push(None);
        }
        files_html.last_mut().expect("one file").push_str(&html);
    }
    if contents.is_empty() {
        contents.push((
            1,
            format!("<a href=\"{}\">{}</a>", chapter_name(0), escape(&title)),
        ));
    }

    let mut files: Files = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".into(),
            CONTAINER.as_bytes().to_vec(),
        ),
    ];
    for (i, (html, doc_title)) in files_html.iter().zip(&file_titles).enumerate() {
        let doc_title = doc_title.as_deref().unwrap_or(&title);
        files.push((
            format!("OEBPS/{}", chapter_name(i)),
            xhtml(doc_title, &language, html).into_bytes(),
        ));
    }
    let nav = format!(
        "<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1>{}</nav>",
        nest(&contents)
    );
    files.push((
        "OEBPS/nav.xhtml".into(),
        xhtml("Contents", &language, &nav).into_bytes(),
    ));
    files.push(("OEBPS/style.css".into(), CSS.as_bytes().to_vec()));

    let identifier = {
        let mut h = Sha256::new();
        h.update(title.as_bytes());
        h.update(b"|");
        h.update(author.as_deref().unwrap_or("").as_bytes());
        let hex: String = h.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("urn:deckle:txt:{hex}")
    };
    let manifest: String = (1..=files_html.len())
        .map(|i| {
            format!(
                "<item id=\"c{i}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>",
                chapter_name(i - 1)
            )
        })
        .collect();
    let spine: String = (1..=files_html.len())
        .map(|i| format!("<itemref idref=\"c{i}\"/>"))
        .collect();
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"{lang}\">\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:identifier id=\"uid\">{identifier}</dc:identifier>\
         <dc:title>{title}</dc:title>{creator}\
         <dc:language>{lang}</dc:language>\
         <meta property=\"dcterms:modified\">2000-01-01T00:00:00Z</meta>\
         </metadata><manifest>\
         <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\
         {manifest}</manifest><spine>{spine}</spine></package>",
        lang = escape(&language),
        title = escape(&title),
        creator = author
            .as_deref()
            .map(|c| format!("<dc:creator>{}</dc:creator>", escape(c)))
            .unwrap_or_default(),
    );
    files.insert(2, ("OEBPS/content.opf".into(), opf.into_bytes()));

    let report = ImportReport {
        title,
        chapters: files_html.len(),
        contents: contents.len(),
        warnings,
    };
    Ok((files, report))
}

fn chapter_name(i: usize) -> String {
    format!("chapter-{:03}.xhtml", i + 1)
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

pub fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> Result<(), TxtError> {
    let out = std::fs::File::create(path)?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| TxtError::Io(std::io::Error::other(e));
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
h1, h2 { text-align: center; margin: 2em 0 1em 0; }
h1 .subtitle, h2 .subtitle { font-size: 0.8em; font-weight: normal; }
p.verse { margin: 0 0 1em 1.5em; }
hr.scene { border: none; margin: 1em 0; }
hr.scene::after { content: \"* * *\"; display: block; text-align: center; }
";

#[cfg(test)]
mod tests {
    use super::nest;

    #[test]
    fn contents_nest_by_level() {
        let items = [
            (1, "a".to_owned()),
            (2, "a1".to_owned()),
            (2, "a2".to_owned()),
            (1, "b".to_owned()),
            (1, "c".to_owned()),
        ];
        assert_eq!(
            nest(&items),
            "<ol><li>a<ol><li>a1</li><li>a2</li></ol></li><li>b</li><li>c</li></ol>"
        );
    }
}
