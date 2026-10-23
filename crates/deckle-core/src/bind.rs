// SPDX-License-Identifier: GPL-3.0-or-later
//! Batch bind: several EPUBs into one book (docs/superpowers/specs/2026-10-23-batch-bind-design.md).
//!
//! Each part keeps its own files, styles and links: everything under its OPF folder is copied
//! into `OEBPS/pNN/` with the same relative paths, so nothing inside a part needs rewriting. The
//! new package has every part's manifest (ids prefixed), their spines one after another, and a
//! contents page with one entry per part and that part's own contents nested under it.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::epub::{EpubReader, join, percent_decode};
use crate::inspect::{NO_TITLE_WARNING, inspect};

/// One part of the bound book.
#[derive(Clone, Debug)]
pub struct Part {
    pub epub: PathBuf,
    /// The part's entry in the contents; its own title when `None`.
    pub title: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct BindOptions {
    pub title: String,
    /// The author; when `None`, the one author every part shares, if they share one.
    pub author: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BindReport {
    pub title: String,
    pub author: Option<String>,
    /// The contents entry of each part, in order.
    pub parts: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("nothing to bind: give at least two files")]
    TooFew,
    #[error("{0}: {1}")]
    Part(String, String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One manifest item of a part.
struct Item {
    id: String,
    href: String,
    media_type: String,
    properties: String,
}

/// A spine entry: the item's id and whether it's in the linear reading order.
type ItemRef = (String, bool);

fn manifest_and_spine(opf: &str) -> Result<(Vec<Item>, Vec<ItemRef>), String> {
    let doc = roxmltree::Document::parse(opf).map_err(|e| e.to_string())?;
    let attr = |n: roxmltree::Node, name: &str| {
        n.attributes()
            .find(|a| a.name() == name)
            .map(|a| a.value().to_owned())
            .unwrap_or_default()
    };
    let items = doc
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .map(|n| Item {
            id: attr(n, "id"),
            href: attr(n, "href"),
            media_type: attr(n, "media-type"),
            properties: attr(n, "properties"),
        })
        .collect();
    let spine = doc
        .descendants()
        .filter(|n| n.has_tag_name("itemref"))
        .map(|n| (attr(n, "idref"), attr(n, "linear") != "no"))
        .collect();
    Ok((items, spine))
}

/// Files a part's `META-INF/encryption.xml` obfuscates (fonts, keyed to that book's identifier):
/// they'd be garbage under the new one, so they're left out.
fn obfuscated(reader: &mut EpubReader) -> HashSet<String> {
    let Ok(xml) = reader.read_string("META-INF/encryption.xml") else {
        return HashSet::new();
    };
    let Ok(doc) = roxmltree::Document::parse(&xml) else {
        return HashSet::new();
    };
    doc.descendants()
        .filter(|n| n.has_tag_name("CipherReference"))
        .filter_map(|n| n.attribute("URI"))
        .map(percent_decode)
        .collect()
}

/// Nested `<ol>` lists from (depth from 0, item html), in order.
fn nest(items: &[(u8, String)]) -> String {
    let mut out = String::from("<ol>");
    let mut depth = 0u8;
    for (i, (d, html)) in items.iter().enumerate() {
        if *d > depth && i > 0 {
            while depth < *d {
                out.push_str("<ol>");
                depth += 1;
            }
        } else {
            if i > 0 {
                out.push_str("</li>");
            }
            while depth > *d {
                out.push_str("</ol></li>");
                depth -= 1;
            }
        }
        out.push_str("<li>");
        out.push_str(html);
    }
    out.push_str("</li>");
    while depth > 0 {
        out.push_str("</ol></li>");
        depth -= 1;
    }
    out.push_str("</ol>");
    out
}

/// Binds `parts`, in order, into one EPUB at `out`.
pub fn bind(parts: &[Part], opts: &BindOptions, out: &Path) -> Result<BindReport, BindError> {
    if parts.len() < 2 {
        return Err(BindError::TooFew);
    }
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut contents: Vec<(u8, String)> = Vec::new();
    let mut report = BindReport::default();
    let mut authors: Vec<Vec<String>> = Vec::new();
    let mut language = String::new();
    for (n, part) in parts.iter().enumerate() {
        let name = part.epub.display().to_string();
        let fail = |e: String| BindError::Part(name.clone(), e);
        let info = inspect(&part.epub).map_err(|e| fail(e.to_string()))?;
        let mut reader = EpubReader::open(&part.epub).map_err(|e| fail(e.to_string()))?;
        let opf = reader
            .read_string(&reader.opf_path.clone())
            .map_err(|e| fail(e.to_string()))?;
        let (items, itemrefs) = manifest_and_spine(&opf).map_err(fail)?;
        let skip = obfuscated(&mut reader);
        let prefix = format!("p{:02}/", n + 1);
        let id = |i: &str| format!("p{:02}-{i}", n + 1);
        if language.is_empty() {
            language = info.language.clone();
        }
        authors.push(info.creators.iter().map(|c| c.name.clone()).collect());

        // The part's own files, its nav and NCX aside (the bound book has one contents).
        let mut kept: HashSet<String> = HashSet::new();
        for item in &items {
            let is_nav = item.properties.split_whitespace().any(|p| p == "nav");
            if is_nav || item.media_type == "application/x-dtbncx+xml" || item.href.is_empty() {
                continue;
            }
            let zip_name = reader.resolve(&item.href);
            if skip.contains(&zip_name) {
                report.warnings.push(format!(
                    "{}: left out an obfuscated font ({}); its text uses the reader's font",
                    info.title, item.href
                ));
                continue;
            }
            let Ok(bytes) = reader.read_bytes(&zip_name) else {
                report
                    .warnings
                    .push(format!("{}: {} is missing", info.title, item.href));
                continue;
            };
            let rel = join(
                "",
                &percent_decode(item.href.split('#').next().unwrap_or("")),
            );
            files.push((format!("OEBPS/{prefix}{rel}"), bytes));
            kept.insert(item.id.clone());
            // Cover-image and nav properties belong to the part, not to the bound book.
            let props: Vec<&str> = item
                .properties
                .split_whitespace()
                .filter(|p| *p != "cover-image" && *p != "nav")
                .collect();
            manifest.push_str(&format!(
                "<item id=\"{}\" href=\"{}{}\" media-type=\"{}\"{}/>",
                escape(&id(&item.id)),
                prefix,
                escape(&item.href),
                escape(&item.media_type),
                if props.is_empty() {
                    String::new()
                } else {
                    format!(" properties=\"{}\"", props.join(" "))
                }
            ));
        }
        let mut first: Option<String> = None;
        for (idref, linear) in &itemrefs {
            if !kept.contains(idref) {
                continue;
            }
            if first.is_none() {
                first = items
                    .iter()
                    .find(|i| &i.id == idref)
                    .map(|i| format!("{prefix}{}", i.href));
            }
            spine.push_str(&format!(
                "<itemref idref=\"{}\"{}/>",
                escape(&id(idref)),
                if *linear { "" } else { " linear=\"no\"" }
            ));
        }
        let Some(first) = first else {
            return Err(fail("it has no pages".into()));
        };
        let title = part.title.clone().unwrap_or_else(|| {
            if info.warnings.iter().any(|w| w == NO_TITLE_WARNING) {
                part.epub
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                info.title.clone()
            }
        });
        contents.push((
            0,
            format!("<a href=\"{}\">{}</a>", escape(&first), escape(&title)),
        ));
        // The part's own contents under it, one level deep below its title. A single entry
        // that just repeats the part (an article's one heading) adds nothing.
        let own: Vec<_> = info.toc.iter().filter(|t| t.depth <= 1).collect();
        if own.len() > 1 {
            for t in own {
                contents.push((
                    1 + t.depth,
                    format!(
                        "<a href=\"{prefix}{}\">{}</a>",
                        escape(&t.href),
                        escape(&t.title)
                    ),
                ));
            }
        }
        report.parts.push(title);
    }

    let author = opts.author.clone().or_else(|| {
        let first = authors.first()?.clone();
        (!first.is_empty() && authors.iter().all(|a| *a == first)).then(|| first.join(", "))
    });
    let language = if language.is_empty() {
        "en".into()
    } else {
        language
    };
    let identifier = {
        let mut h = Sha256::new();
        h.update(opts.title.as_bytes());
        for p in &report.parts {
            h.update(b"|");
            h.update(p.as_bytes());
        }
        let hex: String = h.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("urn:deckle:bind:{hex}")
    };
    let nav = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" \
         xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"{lang}\" lang=\"{lang}\"><head>\
         <title>Contents</title></head><body><nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1>{}\
         </nav></body></html>\n",
        nest(&contents),
        lang = escape(&language)
    );
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
         {manifest}</manifest><spine>{spine}</spine></package>",
        lang = escape(&language),
        title = escape(&opts.title),
        creator = author
            .as_deref()
            .map(|a| format!("<dc:creator>{}</dc:creator>", escape(a)))
            .unwrap_or_default(),
    );

    let out_file = std::fs::File::create(out)?;
    let mut zip = zip::ZipWriter::new(out_file);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| BindError::Io(std::io::Error::other(e));
    zip.start_file("mimetype", stored).map_err(err)?;
    zip.write_all(b"application/epub+zip")?;
    zip.start_file("META-INF/container.xml", deflated)
        .map_err(err)?;
    zip.write_all(
        b"<?xml version=\"1.0\"?>\n<container version=\"1.0\" \
          xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile \
          full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
          </rootfiles></container>\n",
    )?;
    zip.start_file("OEBPS/content.opf", deflated).map_err(err)?;
    zip.write_all(opf.as_bytes())?;
    zip.start_file("OEBPS/nav.xhtml", deflated).map_err(err)?;
    zip.write_all(nav.as_bytes())?;
    let mut written: HashSet<String> = HashSet::new();
    for (name, bytes) in &files {
        if !written.insert(name.clone()) {
            continue;
        }
        zip.start_file(name.as_str(), deflated).map_err(err)?;
        zip.write_all(bytes)?;
    }
    zip.finish().map_err(err)?;

    report.title = opts.title.clone();
    report.author = author;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::nest;

    #[test]
    fn contents_nest_parts_and_their_chapters() {
        let items = [
            (0, "a".to_owned()),
            (1, "a1".to_owned()),
            (1, "a2".to_owned()),
            (0, "b".to_owned()),
        ];
        assert_eq!(
            nest(&items),
            "<ol><li>a<ol><li>a1</li><li>a2</li></ol></li><li>b</li></ol>"
        );
    }
}
