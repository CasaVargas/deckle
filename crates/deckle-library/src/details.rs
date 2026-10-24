// SPDX-License-Identifier: GPL-3.0-or-later
//! Fix details (Books and Kindle design §8): the title, authors and series of a book in Deckle's
//! library, written into the library copy's OPF and its sidecar. Never the original file, and
//! never calibre. The book keeps its identity and ASIN, so its sends still match it.

use std::io::{Read, Write};
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{Book, Library, SeriesRef};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Details {
    pub title: String,
    pub authors: Vec<String>,
    pub series: Option<SeriesRef>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn index_text(i: f32) -> String {
    if i.fract() == 0.0 {
        format!("{}", i as i64)
    } else {
        format!("{i}")
    }
}

/// `opf` with its title, creators and series replaced by `d`'s. `None` when it has no
/// `<metadata>`.
pub fn rewrite_opf(opf: &str, d: &Details) -> Option<String> {
    let open = Regex::new(r"(?is)<(?:[\w.-]+:)?metadata\b[^>]*>")
        .unwrap()
        .find(opf)?;
    let close = Regex::new(r"(?is)</(?:[\w.-]+:)?metadata\s*>")
        .unwrap()
        .find_at(opf, open.end())?;
    let mut block = opf[open.end()..close.start()].to_owned();
    let id_of = |tag: &str| {
        Regex::new(r#"\bid\s*=\s*["']([^"']+)["']"#)
            .unwrap()
            .captures(tag)
            .map(|c| c[1].to_owned())
    };
    let drop_refines = |block: &mut String, id: &str| {
        let re = Regex::new(&format!(
            r#"(?is)\s*<meta\b[^>]*\brefines\s*=\s*["']#{}["'][^>]*?(?:/>|>.*?</meta>)"#,
            regex::escape(id)
        ))
        .unwrap();
        *block = re.replace_all(block, "").into_owned();
    };

    // Creators, and the EPUB 3 metas that refine them.
    let creator = Regex::new(r"(?is)\s*<dc:creator\b[^>]*?(?:/>|>.*?</dc:creator>)").unwrap();
    let ids: Vec<String> = creator
        .find_iter(&block)
        .filter_map(|m| id_of(m.as_str()))
        .collect();
    block = creator.replace_all(&block, "").into_owned();
    for id in &ids {
        drop_refines(&mut block, id);
    }
    let creators: String = d
        .authors
        .iter()
        .filter(|a| !a.trim().is_empty())
        .map(|a| format!("\n    <dc:creator>{}</dc:creator>", esc(a.trim())))
        .collect();

    // The title: the first keeps its attributes, the text changes.
    let title = Regex::new(r"(?is)<dc:title\b([^>]*)>.*?</dc:title>").unwrap();
    let new_title = |attrs: &str| {
        format!(
            "<dc:title{attrs}>{}</dc:title>{creators}",
            esc(d.title.trim())
        )
    };
    block = match title.captures(&block) {
        Some(c) => {
            let m = c.get(0).unwrap();
            format!(
                "{}{}{}",
                &block[..m.start()],
                new_title(&c[1]),
                &block[m.end()..]
            )
        }
        None => format!("\n    {}{block}", new_title("")),
    };

    // Series: calibre's metas and EPUB 3's collection, with its refines.
    let calibre = Regex::new(
        r#"(?is)\s*<meta\b[^>]*\bname\s*=\s*["']calibre:series(?:_index)?["'][^>]*?(?:/>|>\s*</meta>)"#,
    )
    .unwrap();
    block = calibre.replace_all(&block, "").into_owned();
    let collection = Regex::new(
        r#"(?is)\s*<meta\b[^>]*\bproperty\s*=\s*["']belongs-to-collection["'][^>]*>.*?</meta>"#,
    )
    .unwrap();
    let ids: Vec<String> = collection
        .find_iter(&block)
        .filter_map(|m| id_of(m.as_str()))
        .collect();
    block = collection.replace_all(&block, "").into_owned();
    for id in &ids {
        drop_refines(&mut block, id);
    }
    if let Some(s) = d.series.as_ref().filter(|s| !s.name.trim().is_empty()) {
        let name = esc(s.name.trim());
        block.push_str(&format!(
            "\n    <meta name=\"calibre:series\" content=\"{name}\"/>"
        ));
        if let Some(i) = s.index {
            block.push_str(&format!(
                "\n    <meta name=\"calibre:series_index\" content=\"{}\"/>",
                index_text(i)
            ));
        }
        let epub3 = Regex::new(r#"(?is)<package\b[^>]*\bversion\s*=\s*["']3"#)
            .unwrap()
            .is_match(opf);
        if epub3 {
            block.push_str(&format!(
                "\n    <meta property=\"belongs-to-collection\" id=\"deckle-series\">{name}</meta>\n    <meta refines=\"#deckle-series\" property=\"collection-type\">series</meta>"
            ));
            if let Some(i) = s.index {
                block.push_str(&format!(
                    "\n    <meta refines=\"#deckle-series\" property=\"group-position\">{}</meta>",
                    index_text(i)
                ));
            }
        }
        block.push('\n');
    }
    Some(format!(
        "{}{block}{}",
        &opf[..open.end()],
        &opf[close.start()..]
    ))
}

/// The EPUB at `path` with its OPF rewritten: every other entry is copied as it is, the
/// `mimetype` first and stored.
fn rewrite_epub(path: &Path, d: &Details) -> Result<Vec<u8>, String> {
    let opf_path = deckle_core::epub::EpubReader::open(path)
        .map_err(|e| e.to_string())?
        .opf_path;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zin = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zout = zip::ZipWriter::new(&mut out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let zerr = |e: zip::result::ZipError| e.to_string();
    zout.start_file("mimetype", stored).map_err(zerr)?;
    zout.write_all(b"application/epub+zip")
        .map_err(|e| e.to_string())?;
    for i in 0..zin.len() {
        let mut f = zin.by_index(i).map_err(zerr)?;
        let name = f.name().map_err(zerr)?.into_owned();
        if name == "mimetype" || f.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if name == opf_path {
            let opf = String::from_utf8(bytes).map_err(|_| "the OPF isn't UTF-8")?;
            bytes = rewrite_opf(&opf, d)
                .ok_or("the OPF has no <metadata>")?
                .into_bytes();
        }
        zout.start_file(name, deflated).map_err(zerr)?;
        zout.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    zout.finish().map_err(zerr)?;
    Ok(out.into_inner())
}

/// Writes `d` into the library book in `dir`: its EPUB's OPF, then its sidecar.
pub fn fix(lib: &Library, dir: &Path, d: &Details) -> Result<Book, String> {
    if d.title.trim().is_empty() {
        return Err("a book needs a title".into());
    }
    let mut book = lib
        .list()
        .into_iter()
        .find(|b| b.dir == dir)
        .ok_or_else(|| format!("{}: not a book in the library", dir.display()))?;
    let src = book.source_path();
    if !src
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("epub"))
    {
        return Err(
            "only an EPUB's details can be fixed; this book's copy is another kind of file".into(),
        );
    }
    let bytes = rewrite_epub(&src, d)?;
    crate::write_atomic(&src, &bytes)?;
    let m = &mut book.sidecar.metadata;
    if m.authors != d.authors {
        m.author_sort = None;
    }
    m.title = d.title.trim().to_owned();
    m.authors = d
        .authors
        .iter()
        .map(|a| a.trim().to_owned())
        .filter(|a| !a.is_empty())
        .collect();
    m.series = d.series.clone().filter(|s| !s.name.trim().is_empty());
    book.sidecar.sha256 = crate::sha256_hex(&bytes);
    lib.save(&book)?;
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPF: &str = r##"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">x</dc:identifier><dc:title id="t">Wrong</dc:title><dc:creator id="c1">Someone</dc:creator><meta refines="#c1" property="role" scheme="marc:relators">aut</meta><meta refines="#t" property="title-type">main</meta><meta name="calibre:series" content="Old"/><meta property="belongs-to-collection" id="s">Old</meta><meta refines="#s" property="group-position">9</meta></metadata><manifest/></package>"##;

    #[test]
    fn the_title_authors_and_series_are_replaced_and_their_refines_go() {
        let out = rewrite_opf(
            OPF,
            &Details {
                title: "The Lantern & the Wick".into(),
                authors: vec!["Ada Field".into(), "Bo Reed".into()],
                series: Some(SeriesRef {
                    name: "Lamps".into(),
                    index: Some(2.0),
                }),
            },
        )
        .unwrap();
        roxmltree::Document::parse(&out).unwrap();
        assert!(out.contains("<dc:title id=\"t\">The Lantern &amp; the Wick</dc:title>"));
        assert!(
            out.contains("refines=\"#t\""),
            "the title's own refines stay"
        );
        assert!(!out.contains("Someone") && !out.contains("#c1") && !out.contains("Old"));
        assert!(out.contains("<dc:creator>Ada Field</dc:creator>"));
        assert!(out.contains("<meta name=\"calibre:series_index\" content=\"2\"/>"));
        assert!(out.contains("property=\"group-position\">2</meta>"));
    }

    #[test]
    fn a_fixed_book_reads_back_with_its_new_details_and_keeps_its_asin() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::new(tmp.path().join("Library"));
        let original = deckle_kfx::fixtures::path("deckle-mini.epub");
        let before = std::fs::read(&original).unwrap();
        let book = lib.add(&original).unwrap().book;
        let d = Details {
            title: "Fixed Title".into(),
            authors: vec!["New Author".into()],
            series: Some(SeriesRef {
                name: "Fixes".into(),
                index: Some(1.5),
            }),
        };
        let fixed = fix(&lib, &book.dir, &d).unwrap();
        let info = deckle_core::inspect::inspect(&fixed.source_path()).unwrap();
        assert_eq!(info.title, "Fixed Title");
        assert_eq!(info.creators[0].name, "New Author");
        let s = info.series.unwrap();
        assert_eq!((s.name.as_str(), s.index), ("Fixes", Some(1.5)));
        assert_eq!(fixed.sidecar.asin, book.sidecar.asin);
        assert_eq!(lib.list()[0].sidecar.metadata.title, "Fixed Title");
        assert_eq!(
            std::fs::read(&original).unwrap(),
            before,
            "the original is untouched"
        );
        assert!(fix(&lib, &book.dir, &Details::default()).is_err());
    }
}
