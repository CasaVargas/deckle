// SPDX-License-Identifier: GPL-3.0-or-later
//! A long read as its own book (save-to-paper §5): one article, its photos, and a
//! "Deckle Reads" cover.

use std::path::Path;

use chrono::NaiveDate;

use crate::render::{STYLE, page};
use crate::sources::feeds::photo_token;
use crate::sources::reading::{LongRead, minutes};
use crate::{epub, esc};

fn photo_file(n: usize) -> String {
    format!("images/p{}.jpg", n + 1)
}

pub fn write_long_read(lr: &LongRead, date: NaiveDate, path: &Path) -> Result<(), String> {
    write_article(lr, &format!("urn:deckle:reads:{date}:{}", lr.id), date, path)
}

/// One article as a book with identifier `uid` (an article sent on its own, #21, keeps
/// the same one each time).
pub fn write_article(lr: &LongRead, uid: &str, date: NaiveDate, path: &Path) -> Result<(), String> {
    let cover = crate::cover::render_reads(
        &lr.title,
        lr.byline.as_deref(),
        &lr.outlet,
        minutes(lr.words),
    )?;
    let author = lr.byline.clone().unwrap_or_else(|| lr.outlet.clone());
    let a = &lr.article;
    let mut body = a.body.clone();
    for n in 0..a.photos.len() {
        body = body.replace(&photo_token(n), &photo_file(n));
    }
    let meta = a
        .meta
        .as_deref()
        .map(|m| format!("<p class=\"meta\">{}</p>\n", esc(m)))
        .unwrap_or_default();
    let text = page(
        &lr.title,
        &format!(
            "<h1 class=\"headline\">{}</h1>\n{meta}{body}",
            esc(&lr.title)
        ),
    );
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n\
         <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n\
         <item id=\"cover-image\" href=\"cover.jpg\" media-type=\"image/jpeg\" properties=\"cover-image\"/>\n\
         <item id=\"text\" href=\"text.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
    );
    for n in 0..a.photos.len() {
        manifest.push_str(&format!(
            "<item id=\"p{}\" href=\"{}\" media-type=\"image/jpeg\"/>\n",
            n + 1,
            photo_file(n)
        ));
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"en\">\n\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         <dc:identifier id=\"uid\">{uid}</dc:identifier>\n<dc:title>{title}</dc:title>\n\
         <dc:creator id=\"creator\">{author}</dc:creator>\n<dc:language>en</dc:language>\n\
         <dc:publisher>{outlet}</dc:publisher>\n<dc:date>{date}</dc:date>\n\
         <meta property=\"dcterms:modified\">{date}T00:00:00Z</meta>\n\
         <meta name=\"cover\" content=\"cover-image\"/>\n</metadata>\n\
         <manifest>\n{manifest}</manifest>\n\
         <spine toc=\"ncx\">\n<itemref idref=\"text\"/>\n<itemref idref=\"nav\" linear=\"no\"/>\n</spine>\n</package>\n",
        uid = esc(uid),
        title = esc(&lr.title),
        author = esc(&author),
        outlet = esc(&lr.outlet),
    );
    let nav = page(
        "Contents",
        &format!(
            "<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1><ol><li><a href=\"text.xhtml\">{}</a></li></ol></nav>",
            esc(&lr.title)
        ),
    );
    let ncx = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n\
         <head><meta name=\"dtb:uid\" content=\"{uid}\"/><meta name=\"dtb:depth\" content=\"1\"/></head>\n\
         <docTitle><text>{title}</text></docTitle>\n<navMap>\n\
         <navPoint id=\"text\" playOrder=\"1\"><navLabel><text>{title}</text></navLabel><content src=\"text.xhtml\"/></navPoint>\n\
         </navMap>\n</ncx>\n",
        uid = esc(uid),
        title = esc(&lr.title),
    );
    let mut files: crate::render::Files = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".into(),
            b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
              <container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\
              <rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
              </rootfiles></container>\n"
                .to_vec(),
        ),
        ("OEBPS/content.opf".into(), opf.into_bytes()),
        ("OEBPS/toc.ncx".into(), ncx.into_bytes()),
        ("OEBPS/nav.xhtml".into(), nav),
        ("OEBPS/style.css".into(), STYLE.as_bytes().to_vec()),
        ("OEBPS/cover.jpg".into(), cover),
        ("OEBPS/text.xhtml".into(), text),
    ];
    for (n, jpeg) in a.photos.iter().enumerate() {
        files.push((format!("OEBPS/{}", photo_file(n)), jpeg.clone()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    epub::write(path, &files).map_err(|e| format!("{}: {e}", path.display()))
}
