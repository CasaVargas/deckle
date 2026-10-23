// SPDX-License-Identifier: GPL-3.0-or-later
//! A plain EPUB 3 (with an NCX for older readers) from a rebuilt book.

use std::io::Write;
use std::path::Path;

use crate::MobiError;

/// A file the book carries besides its parts, at `path` under `OEBPS/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub path: String,
    pub media_type: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocEntry {
    /// 0 for a top-level entry.
    pub level: usize,
    pub title: String,
    /// Under `OEBPS/`, with its fragment.
    pub href: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Book {
    pub title: String,
    pub authors: Vec<String>,
    pub language: String,
    pub identifier: String,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub isbn: Option<String>,
    pub subjects: Vec<String>,
    pub date: Option<String>,
    /// The cover image's path under `OEBPS/`.
    pub cover: Option<String>,
    /// `(path under OEBPS/, XHTML)`, in reading order.
    pub parts: Vec<(String, String)>,
    pub resources: Vec<Resource>,
    pub toc: Vec<TocEntry>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn id_for(path: &str) -> String {
    let mut id: String = path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    id.insert(0, 'f');
    id
}

fn opf(b: &Book) -> String {
    let mut meta = format!(
        "<dc:identifier id=\"bookid\">{}</dc:identifier>\n<dc:title>{}</dc:title>\n<dc:language>{}</dc:language>\n",
        esc(&b.identifier),
        esc(&b.title),
        esc(&b.language)
    );
    for a in &b.authors {
        meta.push_str(&format!("<dc:creator>{}</dc:creator>\n", esc(a)));
    }
    let optional = [
        ("publisher", &b.publisher),
        ("description", &b.description),
        ("date", &b.date),
    ];
    for (tag, v) in optional {
        if let Some(v) = v {
            meta.push_str(&format!("<dc:{tag}>{}</dc:{tag}>\n", esc(v)));
        }
    }
    if let Some(isbn) = &b.isbn {
        meta.push_str(&format!(
            "<dc:identifier>urn:isbn:{}</dc:identifier>\n",
            esc(isbn)
        ));
    }
    for s in &b.subjects {
        meta.push_str(&format!("<dc:subject>{}</dc:subject>\n", esc(s)));
    }
    meta.push_str("<meta property=\"dcterms:modified\">2000-01-01T00:00:00Z</meta>\n");
    if let Some(c) = &b.cover {
        meta.push_str(&format!(
            "<meta name=\"cover\" content=\"{}\"/>\n",
            id_for(c)
        ));
    }
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n<item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n",
    );
    for (p, text) in &b.parts {
        let svg = if text.contains("<svg") {
            " properties=\"svg\""
        } else {
            ""
        };
        manifest.push_str(&format!(
            "<item id=\"{}\" href=\"{}\" media-type=\"application/xhtml+xml\"{svg}/>\n",
            id_for(p),
            esc(p)
        ));
    }
    for r in &b.resources {
        let cover = if b.cover.as_deref() == Some(r.path.as_str()) {
            " properties=\"cover-image\""
        } else {
            ""
        };
        manifest.push_str(&format!(
            "<item id=\"{}\" href=\"{}\" media-type=\"{}\"{cover}/>\n",
            id_for(&r.path),
            esc(&r.path),
            esc(&r.media_type)
        ));
    }
    let spine: String = b
        .parts
        .iter()
        .map(|(p, _)| format!("<itemref idref=\"{}\"/>\n", id_for(p)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\">\n<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n{meta}</metadata>\n<manifest>\n{manifest}</manifest>\n<spine toc=\"ncx\">\n{spine}</spine>\n</package>\n"
    )
}

/// The table of contents, nested by level; the first part when the book has none.
fn toc_or_start(b: &Book) -> Vec<TocEntry> {
    if !b.toc.is_empty() {
        return b.toc.clone();
    }
    b.parts
        .first()
        .map(|(p, _)| {
            vec![TocEntry {
                level: 0,
                title: b.title.clone(),
                href: p.clone(),
            }]
        })
        .unwrap_or_default()
}

fn nav(b: &Book) -> String {
    let mut out = String::new();
    // How many lists are open, and whether the innermost has an item open.
    let mut depth = 0usize;
    let mut item_open = false;
    for e in toc_or_start(b) {
        // Never more than one level deeper than the entry before.
        let want = (e.level + 1).min(depth + 1);
        if want > depth {
            out.push_str("<ol>");
            depth += 1;
        } else {
            while depth > want {
                out.push_str("</li></ol>");
                depth -= 1;
            }
            if item_open {
                out.push_str("</li>");
            }
        }
        out.push_str(&format!(
            "<li><a href=\"{}\">{}</a>",
            esc(&e.href),
            esc(&e.title)
        ));
        item_open = true;
    }
    while depth > 0 {
        out.push_str("</li></ol>");
        depth -= 1;
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>{t}</title></head><body><nav epub:type=\"toc\" id=\"toc\"><h1>{t}</h1>{out}</nav></body></html>\n",
        t = esc(&b.title)
    )
}

fn ncx(b: &Book) -> String {
    // Entries that point at the same place share a play order (EPUBCheck RSC-005).
    let mut order: Vec<&str> = Vec::new();
    let entries = toc_or_start(b);
    let points: String = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let n = match order.iter().position(|h| *h == e.href) {
                Some(p) => p + 1,
                None => {
                    order.push(&e.href);
                    order.len()
                }
            };
            format!(
                "<navPoint id=\"p{id}\" playOrder=\"{n}\"><navLabel><text>{}</text></navLabel><content src=\"{}\"/></navPoint>\n",
                esc(&e.title),
                esc(&e.href),
                id = i + 1
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\"><head><meta name=\"dtb:uid\" content=\"{}\"/></head><docTitle><text>{}</text></docTitle><navMap>\n{points}</navMap></ncx>\n",
        esc(&b.identifier),
        esc(&b.title)
    )
}

/// Writes `b` to `out` (replacing a file there).
pub fn write(out: &Path, b: &Book) -> Result<(), MobiError> {
    use zip::write::SimpleFileOptions;
    let file = std::fs::File::create(out)?;
    let mut z = zip::ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let zerr = |e: zip::result::ZipError| MobiError::Io(std::io::Error::other(e));
    z.start_file("mimetype", stored).map_err(zerr)?;
    z.write_all(b"application/epub+zip")?;
    z.start_file("META-INF/container.xml", deflated)
        .map_err(zerr)?;
    z.write_all(b"<?xml version=\"1.0\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>\n")?;
    z.start_file("OEBPS/content.opf", deflated).map_err(zerr)?;
    z.write_all(opf(b).as_bytes())?;
    z.start_file("OEBPS/nav.xhtml", deflated).map_err(zerr)?;
    z.write_all(nav(b).as_bytes())?;
    z.start_file("OEBPS/toc.ncx", deflated).map_err(zerr)?;
    z.write_all(ncx(b).as_bytes())?;
    for (p, text) in &b.parts {
        z.start_file(format!("OEBPS/{p}"), deflated).map_err(zerr)?;
        z.write_all(text.as_bytes())?;
    }
    for r in &b.resources {
        let opts =
            if r.media_type.starts_with("image/jpeg") || r.media_type.starts_with("image/png") {
                stored
            } else {
                deflated
            };
        z.start_file(format!("OEBPS/{}", r.path), opts)
            .map_err(zerr)?;
        z.write_all(&r.data)?;
    }
    z.finish().map_err(zerr)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nav_nests_by_level() {
        let b = Book {
            title: "T".into(),
            toc: vec![
                TocEntry {
                    level: 0,
                    title: "One".into(),
                    href: "a".into(),
                },
                TocEntry {
                    level: 1,
                    title: "One.1".into(),
                    href: "b".into(),
                },
                TocEntry {
                    level: 0,
                    title: "Two".into(),
                    href: "c".into(),
                },
            ],
            ..Default::default()
        };
        let n = nav(&b);
        assert!(n.contains("<ol><li><a href=\"a\">One</a><ol><li><a href=\"b\">One.1</a></li></ol></li><li><a href=\"c\">Two</a></li></ol>"), "{n}");
        roxmltree::Document::parse(&n).unwrap();
    }
}
