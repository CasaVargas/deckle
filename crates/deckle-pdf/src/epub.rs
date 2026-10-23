// SPDX-License-Identifier: GPL-3.0-or-later
//! The EPUB: the blocks as XHTML chapters, the pictures beside them, a contents page and the
//! package (PDF spec §5). The same shape as `deckle-txt`'s.

use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::ImportReport;
use crate::engine::Info;
use crate::layout::{Block, Figure};

/// EPUB files as (zip path, bytes), in zip order.
pub type Files = Vec<(String, Vec<u8>)>;

/// With no headings to split at, a new file about every this many bytes.
const SPLIT_BYTES: usize = 200_000;

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn chapter_name(i: usize) -> String {
    format!("chapter-{:03}.xhtml", i + 1)
}

fn picture_name(i: usize) -> String {
    format!("images/figure-{:03}.png", i + 1)
}

/// The book's files and the report. `pictures[n]` is the PNG for the n-th figure block.
pub fn write(
    blocks: &[Block],
    pictures: &[Vec<u8>],
    info: &Info,
    stem: &str,
) -> (Files, ImportReport) {
    let mut warnings = Vec::new();
    // Chapters start at the highest heading level whose headings, at least two of them, all
    // open a page and lead into real text: a label in a figure doesn't start a chapter.
    let starts_chapter = |i: usize| {
        let Block::Heading { level, top, .. } = &blocks[i] else {
            return false;
        };
        let text: usize = blocks[i + 1..]
            .iter()
            .take_while(|b| !matches!(b, Block::Heading { level: l, .. } if l <= level))
            .map(|b| match b {
                Block::Para { text, .. } => text.len(),
                _ => 0,
            })
            .sum();
        *top && text >= 200
    };
    let split_level = (1..=3u8).find(|lv| {
        let at: Vec<usize> = (0..blocks.len())
            .filter(|&i| matches!(&blocks[i], Block::Heading { level, .. } if level == lv))
            .collect();
        at.len() >= 2 && at.iter().all(|&i| starts_chapter(i))
    });
    // The title: the document's own, else the largest heading near the start.
    let title = info
        .title
        .clone()
        .or_else(|| {
            blocks
                .iter()
                .take(12)
                .filter_map(|b| match b {
                    Block::Heading { level, text, .. } => Some((*level, text.clone())),
                    _ => None,
                })
                .min_by_key(|(level, _)| *level)
                .map(|(_, text)| text)
        })
        .unwrap_or_else(|| stem.to_owned());
    let language = "en";

    let mut files_html: Vec<String> = vec![String::new()];
    let mut file_titles: Vec<Option<String>> = vec![None];
    let mut contents: Vec<(u8, String)> = Vec::new();
    let (mut heading_no, mut figure_no) = (0, 0);
    for b in blocks {
        let html = match b {
            Block::Heading { level, text, .. } => {
                heading_no += 1;
                if split_level == Some(*level) && !files_html.last().expect("one file").is_empty() {
                    files_html.push(String::new());
                    file_titles.push(None);
                }
                let name = chapter_name(files_html.len() - 1);
                if split_level.is_some_and(|s| *level <= s + 1) {
                    let depth = 1 + level.saturating_sub(split_level.unwrap_or(1));
                    contents.push((
                        depth,
                        format!("<a href=\"{name}#h{heading_no}\">{}</a>", escape(text)),
                    ));
                }
                let doc_title = file_titles.last_mut().expect("one file");
                if doc_title.is_none() {
                    *doc_title = Some(text.clone());
                }
                format!("<h{level} id=\"h{heading_no}\">{}</h{level}>", escape(text))
            }
            Block::Para { text, small } => {
                let class = if *small { " class=\"small\"" } else { "" };
                format!("<p{class}>{}</p>", escape(text))
            }
            Block::Figure { kind, page, .. } => {
                let what = match kind {
                    Figure::Image => "Figure",
                    Figure::Table => "Table",
                    Figure::Equation => "Equation",
                };
                let class = match kind {
                    Figure::Equation => "equation",
                    _ => "figure",
                };
                let html = format!(
                    "<div class=\"{class}\"><img src=\"{}\" alt=\"{what} from page {} of the PDF\"/></div>",
                    picture_name(figure_no),
                    page + 1
                );
                figure_no += 1;
                html
            }
        };
        let current = files_html.last().expect("one file");
        if split_level.is_none() && !current.is_empty() && current.len() + html.len() > SPLIT_BYTES
        {
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
    if figure_no != pictures.len() {
        warnings.push(format!(
            "{} pictures for {figure_no} figures",
            pictures.len()
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
            xhtml(doc_title, language, html).into_bytes(),
        ));
    }
    for (i, png) in pictures.iter().enumerate() {
        files.push((format!("OEBPS/{}", picture_name(i)), png.clone()));
    }
    let nav = format!(
        "<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1>{}</nav>",
        nest(&contents)
    );
    files.push((
        "OEBPS/nav.xhtml".into(),
        xhtml("Contents", language, &nav).into_bytes(),
    ));
    files.push(("OEBPS/style.css".into(), CSS.as_bytes().to_vec()));

    let identifier = {
        let mut h = Sha256::new();
        h.update(title.as_bytes());
        h.update(b"|");
        h.update(info.author.as_deref().unwrap_or("").as_bytes());
        let hex: String = h.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("urn:deckle:pdf:{hex}")
    };
    let manifest: String = (1..=files_html.len())
        .map(|i| {
            format!(
                "<item id=\"c{i}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>",
                chapter_name(i - 1)
            )
        })
        .chain((0..pictures.len()).map(|i| {
            format!(
                "<item id=\"f{}\" href=\"{}\" media-type=\"image/png\"/>",
                i + 1,
                picture_name(i)
            )
        }))
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
        lang = language,
        title = escape(&title),
        creator = info
            .author
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
        ..Default::default()
    };
    (files, report)
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

pub fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> Result<(), std::io::Error> {
    let out = std::fs::File::create(path)?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        // Pictures are already compressed.
        let opts = if name == "mimetype" || name.ends_with(".png") {
            stored
        } else {
            deflated
        };
        zw.start_file(name.as_str(), opts)
            .map_err(std::io::Error::other)?;
        zw.write_all(bytes)?;
    }
    zw.finish().map_err(std::io::Error::other)?;
    Ok(())
}

const CONTAINER: &str = "<?xml version=\"1.0\"?>\n<container version=\"1.0\" \
    xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile \
    full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>\n";

const CSS: &str = "body { font-family: serif; }
p { margin: 0 0 0.5em 0; }
p.small { font-size: 0.85em; }
h1, h2 { margin: 1.5em 0 0.75em 0; }
div.figure, div.equation { margin: 1em 0; text-align: center; }
div.figure img { max-width: 100%; }
div.equation img { max-width: 100%; }
";
