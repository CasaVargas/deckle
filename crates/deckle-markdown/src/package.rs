// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    Error, Note,
    render::{Asset, chapter},
};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xhtml(title: &str, language: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"{}\" lang=\"{}\"><head><title>{}</title><link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/></head><body>{body}</body></html>",
        escape(language),
        escape(language),
        escape(title)
    )
}

pub fn write(
    output: &Path,
    title: &str,
    authors: &[String],
    language: &str,
    notes: &[Note],
    bodies: Vec<String>,
    assets: Vec<Asset>,
) -> Result<usize, Error> {
    let mut files = vec![
        ("mimetype".to_string(), b"application/epub+zip".to_vec()),
        ("META-INF/container.xml".into(), br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_vec()),
    ];
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/><item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>",
    );
    let mut spine = String::new();
    let mut nav = String::from("<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1><ol>");
    let mut contents = 0;
    for (i, (note, body)) in notes.iter().zip(bodies).enumerate() {
        let file = chapter(i);
        files.push((
            format!("OEBPS/{file}"),
            xhtml(&note.title, language, &body).into_bytes(),
        ));
        manifest.push_str(&format!(
            "<item id=\"c{i}\" href=\"{file}\" media-type=\"application/xhtml+xml\"/>"
        ));
        spine.push_str(&format!("<itemref idref=\"c{i}\"/>"));
        nav.push_str(&format!(
            "<li><a href=\"{file}\">{}</a>",
            escape(&note.title)
        ));
        contents += 1;
        let headings: Vec<_> = note.headings.iter().filter(|h| h.level <= 2).collect();
        if !headings.is_empty() {
            nav.push_str("<ol>");
            for h in headings {
                nav.push_str(&format!(
                    "<li><a href=\"{file}#{}\">{}</a></li>",
                    escape(&h.id),
                    escape(&h.text)
                ));
                contents += 1;
            }
            nav.push_str("</ol>");
        }
        nav.push_str("</li>");
    }
    nav.push_str("</ol></nav>");
    files.push((
        "OEBPS/nav.xhtml".into(),
        xhtml("Contents", language, &nav).into_bytes(),
    ));
    files.push(("OEBPS/style.css".into(), b"body { font-family: serif; } img { max-width: 100%; height: auto; } pre { white-space: pre-wrap; } table { border-collapse: collapse; width: 100%; } th, td { border: 1px solid; padding: 0.3em; } blockquote { margin-left: 1em; } h1, h2 { margin-top: 1.5em; }".to_vec()));
    for (i, asset) in assets.into_iter().enumerate() {
        manifest.push_str(&format!(
            "<item id=\"img{i}\" href=\"{}\" media-type=\"{}\"/>",
            asset.name, asset.mime
        ));
        files.push((format!("OEBPS/{}", asset.name), asset.bytes));
    }
    let mut hash = Sha256::new();
    hash.update(title.as_bytes());
    hash.update(b"|");
    hash.update(authors.join("|").as_bytes());
    let id: String = hash.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let creators: String = authors
        .iter()
        .map(|a| format!("<dc:creator>{}</dc:creator>", escape(a)))
        .collect();
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"uid\">urn:deckle:markdown:{id}</dc:identifier><dc:title>{}</dc:title>{creators}<dc:language>{}</dc:language><meta property=\"dcterms:modified\">2000-01-01T00:00:00Z</meta></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>",
        escape(title),
        escape(language)
    );
    files.push(("OEBPS/content.opf".into(), opf.into_bytes()));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(output)?);
    for (name, bytes) in files {
        let method = if name == "mimetype" {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default().compression_method(method),
        )
        .map_err(|e| Error::Invalid(e.to_string()))?;
        zip.write_all(&bytes)?;
    }
    zip.finish().map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(contents)
}
