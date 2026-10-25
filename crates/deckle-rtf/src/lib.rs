// SPDX-License-Identifier: GPL-3.0-or-later
//! Import a Rich Text Format file (`.rtf`) as a clean EPUB 3, so every later stage (inspect,
//! normalize, Previewer, repackage, deliver) runs on it unchanged.
//!
//! The output matches the Word and OpenDocument importers': headings 1–3 become the contents and
//! each level 1 starts a new file; character formatting becomes XHTML elements; lists, tables,
//! pictures and pop-up footnotes come through; `\info` gives the metadata.

mod html;
mod lex;
mod parse;

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

use html::{Block, escape, flatten, nest};
use lex::Node;
use parse::{CLOSE, Header, OPEN, Walker};

/// EPUB files as (zip path, bytes), in zip order.
pub type Files = Vec<(String, Vec<u8>)>;

/// Larger than any real document; anything bigger is almost certainly not one.
const MAX_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum RtfError {
    #[error("not an RTF file: it doesn't start with {{\\rtf")]
    NotRtf,
    #[error("the RTF document has no text")]
    Empty,
    #[error("the RTF file is {} MB; the limit is 50 MB", .0 / (1024 * 1024))]
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

/// Inputs the importer takes: `.rtf`, any case.
pub fn is_rtf(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("rtf"))
}

/// Converts the document at `rtf` and writes the EPUB to `epub`.
pub fn convert(rtf: &Path, epub: &Path) -> Result<ImportReport, RtfError> {
    let size = std::fs::metadata(rtf)?.len();
    if size > MAX_BYTES {
        return Err(RtfError::TooLarge(size));
    }
    let bytes = std::fs::read(rtf)?;
    let stem = rtf
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (files, report) = to_epub(&bytes, &stem)?;
    write_zip(epub, &files)?;
    Ok(report)
}

/// A Windows LCID as a BCP 47 tag, for the common ones.
fn lcid_tag(lcid: i32) -> Option<&'static str> {
    Some(match lcid {
        1025 => "ar-SA",
        1026 => "bg-BG",
        1027 => "ca-ES",
        1028 => "zh-TW",
        1029 => "cs-CZ",
        1030 => "da-DK",
        1031 => "de-DE",
        1032 => "el-GR",
        1033 => "en-US",
        1034 | 3082 => "es-ES",
        1035 => "fi-FI",
        1036 => "fr-FR",
        1037 => "he-IL",
        1038 => "hu-HU",
        1039 => "is-IS",
        1040 => "it-IT",
        1041 => "ja-JP",
        1042 => "ko-KR",
        1043 => "nl-NL",
        1044 => "nb-NO",
        1045 => "pl-PL",
        1046 => "pt-BR",
        1048 => "ro-RO",
        1049 => "ru-RU",
        1050 => "hr-HR",
        1051 => "sk-SK",
        1053 => "sv-SE",
        1054 => "th-TH",
        1055 => "tr-TR",
        1058 => "uk-UA",
        1060 => "sl-SI",
        1061 => "et-EE",
        1062 => "lv-LV",
        1063 => "lt-LT",
        1066 => "vi-VN",
        2052 => "zh-CN",
        2057 => "en-GB",
        2058 => "es-MX",
        2070 => "pt-PT",
        3081 => "en-AU",
        3084 => "fr-CA",
        4105 => "en-CA",
        _ => return None,
    })
}

/// The EPUB's files in zip order (`mimetype` first). `fallback_title` is used when the document
/// has neither an `\info` title nor a Title paragraph.
pub fn to_epub(bytes: &[u8], fallback_title: &str) -> Result<(Files, ImportReport), RtfError> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(RtfError::TooLarge(bytes.len() as u64));
    }
    let start = bytes
        .strip_prefix(b"\xef\xbb\xbf")
        .unwrap_or(bytes)
        .trim_ascii_start();
    if !start.starts_with(b"{\\rtf") {
        return Err(RtfError::NotRtf);
    }
    let (tree, missing, extra) = lex::parse(start);
    let mut warnings = Vec::new();
    if missing > 0 || extra > 0 {
        warnings.push("the RTF's braces don't balance; read as far as it goes".into());
    }
    let Some(Node::Group(rtf)) = tree.into_iter().next() else {
        return Err(RtfError::NotRtf);
    };
    let header = Header::parse(&rtf);
    let mut walker = Walker::new(&header);
    let blocks = walker.run(&rtf);
    if !blocks.iter().any(|b| match b {
        Block::Heading { text, .. } => !text.is_empty(),
        Block::Html(_) | Block::Item { .. } => true,
        Block::Notes(_) => false,
    }) {
        return Err(RtfError::Empty);
    }
    warnings.append(&mut walker.warnings);

    let info = &header.info;
    let title = info
        .title
        .clone()
        .or_else(|| walker.title.clone())
        .unwrap_or_else(|| fallback_title.to_owned());
    let language = match header.deflang {
        None | Some(1024) => "en".to_owned(),
        Some(l) => match lcid_tag(l) {
            Some(t) => t.to_owned(),
            None => {
                warnings.push(format!("unknown language code {l}; using English"));
                "en".to_owned()
            }
        },
    };

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
                        html.push_str(&walker.notes[n]);
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
    for (i, img) in walker.images.iter().enumerate() {
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
        h.update(info.author.as_deref().unwrap_or("").as_bytes());
        let hex: String = h.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("urn:deckle:rtf:{hex}")
    };
    let modified = info
        .revtim
        .clone()
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
    let mut extra_meta = String::new();
    if let Some(a) = &info.author {
        extra_meta.push_str(&format!("<dc:creator>{}</dc:creator>", escape(a)));
    }
    if let Some(d) = info.subject.as_ref().or(info.doccomm.as_ref()) {
        extra_meta.push_str(&format!("<dc:description>{}</dc:description>", escape(d)));
    }
    for k in info
        .keywords
        .iter()
        .flat_map(|k| k.split([',', ';']))
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        extra_meta.push_str(&format!("<dc:subject>{}</dc:subject>", escape(k)));
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"{lang}\">\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:identifier id=\"uid\">{identifier}</dc:identifier>\
         <dc:title>{title}</dc:title>\
         <dc:language>{lang}</dc:language>{extra_meta}\
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
        notes: walker.notes.len(),
        images: walker.images.len(),
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
                    let w = format!("bookmark {id} wasn't found; its link goes nowhere");
                    if !warnings.contains(&w) {
                        warnings.push(w);
                    }
                    out.push('#');
                }
            }
            rest = &after[end + CLOSE.len_utf8()..];
        }
        out.push_str(rest);
        *html = out;
    }
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

pub fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> Result<(), RtfError> {
    let out = std::fs::File::create(path)?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| RtfError::Io(std::io::Error::other(e));
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
