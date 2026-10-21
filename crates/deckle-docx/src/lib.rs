// SPDX-License-Identifier: GPL-3.0-or-later
//! Import a Word document (.docx) as a clean EPUB 3, so every later stage (inspect, normalize,
//! Previewer, repackage, deliver) runs on it unchanged.
//!
//! Heading 1–3 become the contents and each Heading 1 starts a new file; bold, italic, lists and
//! tables become their XHTML elements; images come over from `word/media`; footnotes and endnotes
//! become `epub:type` asides behind `noteref` links, which the Kindle shows as pop-up notes.

mod body;
mod package;
mod styles;

use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

use body::{Block, Body, Part, escape, flatten, media_type, nest};
use package::{Package, parse};
use styles::{Numbering, Styles};

/// EPUB files as (zip path, bytes), in zip order.
pub type Files = Vec<(String, Vec<u8>)>;

#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    #[error("not a Word document: the file is not a zip archive")]
    NotZip,
    #[error(
        "the Word document is password-protected; remove the password in Word (File → Info → \
         Protect Document) and try again"
    )]
    Encrypted,
    #[error("this is a Word 97-2003 .doc file; open it in Word and save it as .docx")]
    LegacyDoc,
    #[error("not a Word document: the archive has no word/document.xml")]
    NoDocument,
    #[error("corrupt Word document: {0}")]
    Corrupt(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub title: String,
    /// XHTML files in the spine.
    pub chapters: usize,
    /// Entries in the contents (Heading 1–3).
    pub contents: usize,
    pub notes: usize,
    pub images: usize,
    pub warnings: Vec<String>,
}

/// Inputs the importer takes: Word documents and templates, plus `.doc` so it gets a clear error.
pub fn is_word(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["docx", "docm", "dotx", "dotm", "doc"]
            .iter()
            .any(|w| e.eq_ignore_ascii_case(w))
    })
}

/// Converts the document at `docx` and writes the EPUB to `epub`.
pub fn convert(docx: &Path, epub: &Path) -> Result<ImportReport, DocxError> {
    let bytes = std::fs::read(docx)?;
    let stem = docx
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (files, report) = to_epub(&bytes, &stem)?;
    write_zip(epub, &files)?;
    Ok(report)
}

/// The EPUB's files in zip order (`mimetype` first). `fallback_title` is used when the document
/// has neither a title property nor a Title paragraph.
pub fn to_epub(bytes: &[u8], fallback_title: &str) -> Result<(Files, ImportReport), DocxError> {
    let mut pkg = Package::open(bytes)?;
    let root_rels = pkg.rels("")?;
    let main = root_rels
        .values()
        .find(|r| r.kind == "officeDocument" && !r.external)
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "word/document.xml".into());
    let Some(doc_xml) = pkg.read_string(&main)? else {
        return Err(DocxError::NoDocument);
    };
    let core = pkg.core(&root_rels)?;
    let doc_rels = pkg.rels(&main)?;
    let part_of = |kind: &str| {
        doc_rels
            .values()
            .find(|r| r.kind == kind && !r.external)
            .map(|r| r.target.clone())
    };
    let mut read_part = |path: Option<String>| -> Result<Option<(String, String)>, DocxError> {
        let Some(path) = path else { return Ok(None) };
        Ok(pkg.read_string(&path)?.map(|x| (path, x)))
    };
    let styles_xml = read_part(part_of("styles"))?;
    let numbering_xml = read_part(part_of("numbering"))?;
    let footnotes_xml = read_part(part_of("footnotes"))?;
    let endnotes_xml = read_part(part_of("endnotes"))?;

    let doc = parse(&doc_xml, &main)?;
    let styles_doc = styles_xml.as_ref().map(|(p, x)| parse(x, p)).transpose()?;
    let numbering_doc = numbering_xml
        .as_ref()
        .map(|(p, x)| parse(x, p))
        .transpose()?;
    let footnotes_doc = footnotes_xml
        .as_ref()
        .map(|(p, x)| parse(x, p))
        .transpose()?;
    let endnotes_doc = endnotes_xml
        .as_ref()
        .map(|(p, x)| parse(x, p))
        .transpose()?;

    let mut body = Body::new(
        Styles::parse(styles_doc.as_ref()),
        Numbering::parse(numbering_doc.as_ref()),
    );
    body.rels.insert(Part::Doc, doc_rels.clone());
    if let Some((path, _)) = &footnotes_xml {
        body.rels.insert(Part::Footnotes, pkg.rels(path)?);
    }
    if let Some((path, _)) = &endnotes_xml {
        body.rels.insert(Part::Endnotes, pkg.rels(path)?);
    }
    body.footnotes = notes_of(&footnotes_doc, "footnote");
    body.endnotes = notes_of(&endnotes_doc, "endnote");

    let Some(w_body) = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "body")
    else {
        return Err(DocxError::Corrupt(format!("{main} has no w:body")));
    };
    let blocks = body.body(w_body);

    let title = core
        .title
        .clone()
        .or_else(|| body.title.clone().filter(|t| !t.is_empty()))
        .unwrap_or_else(|| fallback_title.to_owned());
    let language = core
        .language
        .clone()
        .or_else(|| body.styles.language.clone())
        .unwrap_or_else(|| "en".into());
    let mut warnings = std::mem::take(&mut body.warnings);

    // Chapters: a new file at each Heading 1, so the Kindle starts each on a fresh page.
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
    if chapters.len() > 1 && chapters[0].is_empty() {
        chapters.remove(0);
    }
    if chapters.iter().all(|c| c.is_empty()) {
        warnings.push("the document has no text".into());
    }

    let chapter_name = |i: usize| format!("chapter-{:03}.xhtml", i + 1);
    let mut files: Files = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".into(),
            CONTAINER.as_bytes().to_vec(),
        ),
    ];
    let mut contents: Vec<(u8, bool, u32, String)> = Vec::new();
    let mut chapter_files = Vec::new();
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
                        html.push_str(&body.notes[n].html);
                    }
                }
                _ => {}
            }
        }
        let doc_title = chapter_title.unwrap_or_else(|| title.clone());
        files.push((
            format!("OEBPS/{name}"),
            xhtml(&doc_title, &language, &html).into_bytes(),
        ));
        chapter_files.push(name);
    }
    if contents.is_empty() {
        contents.push((
            1,
            true,
            1,
            format!("<a href=\"{}\">{}</a>", chapter_files[0], escape(&title)),
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

    // Every picture in the document's media folder comes along, placed or not.
    let mut images = body.images.clone();
    for name in pkg.names() {
        if name.starts_with("word/media/")
            && let Some(mt) = media_type(&name)
            && !images.iter().any(|i| i.source == name)
        {
            let file = name.rsplit('/').next().unwrap_or("image");
            let mut href = format!("images/{file}");
            if images.iter().any(|i| i.href == href) {
                href = format!("images/{}-{file}", images.len());
            }
            images.push(body::Image {
                source: name.clone(),
                href,
                media_type: mt,
            });
        }
    }
    let mut manifest_images = String::new();
    let mut packaged = 0;
    for (i, img) in images.iter().enumerate() {
        let Some(bytes) = pkg.read(&img.source)? else {
            warnings.push(format!("image {} is missing from the document", img.source));
            continue;
        };
        manifest_images.push_str(&format!(
            "<item id=\"img{}\" href=\"{}\" media-type=\"{}\"/>",
            i + 1,
            escape(&img.href),
            img.media_type
        ));
        files.push((format!("OEBPS/{}", img.href), bytes));
        packaged += 1;
    }

    let creator = core.creator.clone();
    let identifier = {
        let mut h = Sha256::new();
        h.update(title.as_bytes());
        h.update(b"|");
        h.update(creator.as_deref().unwrap_or("").as_bytes());
        let hex: String = h.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("urn:deckle:docx:{hex}")
    };
    let modified = core
        .modified
        .as_deref()
        .and_then(w3c_seconds)
        .unwrap_or_else(|| "2000-01-01T00:00:00Z".into());
    let manifest_chapters: String = chapter_files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            format!(
                "<item id=\"c{}\" href=\"{f}\" media-type=\"application/xhtml+xml\"/>",
                i + 1
            )
        })
        .collect();
    let spine: String = (1..=chapter_files.len())
        .map(|i| format!("<itemref idref=\"c{i}\"/>"))
        .collect();
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"{lang}\">\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:identifier id=\"uid\">{identifier}</dc:identifier>\
         <dc:title>{title}</dc:title>{creator}\
         <dc:language>{lang}</dc:language>\
         <meta property=\"dcterms:modified\">{modified}</meta>\
         </metadata><manifest>\
         <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\
         {manifest_chapters}{manifest_images}</manifest><spine>{spine}</spine></package>",
        lang = escape(&language),
        title = escape(&title),
        creator = creator
            .as_deref()
            .map(|c| format!("<dc:creator>{}</dc:creator>", escape(c)))
            .unwrap_or_default(),
    );
    files.insert(2, ("OEBPS/content.opf".into(), opf.into_bytes()));

    let report = ImportReport {
        title,
        chapters: chapter_files.len(),
        contents: contents.len(),
        notes: body.notes.len(),
        images: packaged,
        warnings,
    };
    Ok((files, report))
}

/// `2026-10-21T08:15:00.123Z` → `2026-10-21T08:15:00Z`; anything else → `None`.
fn w3c_seconds(s: &str) -> Option<String> {
    let head = s.get(..19)?;
    let shape = head.char_indices().all(|(i, c)| match i {
        4 | 7 => c == '-',
        10 => c == 'T',
        13 | 16 => c == ':',
        _ => c.is_ascii_digit(),
    });
    shape.then(|| format!("{head}Z"))
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

pub fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> Result<(), DocxError> {
    let out = std::fs::File::create(path)?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| DocxError::Io(std::io::Error::other(e));
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

/// Notes by id. Separator and continuation notes carry a `w:type`; real notes have none.
fn notes_of<'d, 'i>(
    d: &'d Option<roxmltree::Document<'i>>,
    tag: &str,
) -> std::collections::HashMap<String, roxmltree::Node<'d, 'i>> {
    let mut map = std::collections::HashMap::new();
    if let Some(d) = d {
        for n in d
            .root_element()
            .children()
            .filter(|n| n.tag_name().name() == tag)
            .filter(|n| package::attr(*n, "type").is_none_or(|t| t == "normal"))
        {
            if let Some(id) = package::attr(n, "id") {
                map.insert(id.to_owned(), n);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::w3c_seconds;

    #[test]
    fn modified_is_trimmed_to_seconds() {
        assert_eq!(
            w3c_seconds("2026-10-21T08:15:00.123Z").as_deref(),
            Some("2026-10-21T08:15:00Z")
        );
        assert_eq!(w3c_seconds("yesterday"), None);
    }
}
