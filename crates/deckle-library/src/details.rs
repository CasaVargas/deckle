// SPDX-License-Identifier: GPL-3.0-or-later
//! Fix details (Books and Kindle design §8, Books tab redesign §5): the title, authors, series
//! and the rest of a book in Deckle's library, written into the library copy's OPF and its
//! sidecar. Never the original file, and never calibre. The book keeps its identity and ASIN, so
//! its sends still match it.

use std::io::{Read, Write};
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{Book, Library, SeriesRef};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Details {
    pub title: String,
    pub authors: Vec<String>,
    pub series: Option<SeriesRef>,
    // The rest: `None` leaves the book's own as it is; `Some` empty removes it.
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub publisher: Option<String>,
    /// A year, or a date like 2020-09-15 (W3CDTF, as the OPF wants).
    pub published: Option<String>,
    /// A language tag (en, en-GB). Empty keeps the book's own: every EPUB needs one.
    pub language: Option<String>,
    pub isbn: Option<String>,
    pub title_sort: Option<String>,
    /// The first author's sorting name ("Corey, James S. A.").
    pub author_sort: Option<String>,
}

const DC: &str = "http://purl.org/dc/elements/1.1/";

/// `s` trimmed, `None` when that leaves nothing.
fn filled(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// An ISBN-10 or -13 without its hyphens and spaces, the check letter upper-cased.
pub fn normal_isbn(s: &str) -> Option<String> {
    let n: String = s
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .collect::<String>()
        .to_uppercase();
    let ok = (n.len() == 13 && n.chars().all(|c| c.is_ascii_digit()))
        || (n.len() == 10
            && n[..9].chars().all(|c| c.is_ascii_digit())
            && n[9..].chars().all(|c| c.is_ascii_digit() || c == 'X'));
    ok.then_some(n)
}

/// What stops `d` being written, in the words the editor shows.
pub fn check(d: &Details) -> Result<(), String> {
    if d.title.trim().is_empty() {
        return Err("a book needs a title".into());
    }
    if let Some(p) = filled(&d.published)
        && !Regex::new(r"^\d{4}(-\d{2}(-\d{2})?)?$")
            .unwrap()
            .is_match(p)
    {
        return Err("Published: a year, or a date like 2020-09-15.".into());
    }
    if let Some(l) = filled(&d.language)
        && !Regex::new(r"^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$")
            .unwrap()
            .is_match(l)
    {
        return Err("Language: a code like en or en-GB.".into());
    }
    if let Some(i) = filled(&d.isbn)
        && normal_isbn(i).is_none()
    {
        return Err("ISBN: 10 or 13 digits.".into());
    }
    Ok(())
}

/// Whether an identifier is an ISBN, and which.
fn isbn_of(text: &str, scheme: Option<&str>) -> Option<String> {
    let t = text.trim();
    let t = t
        .strip_prefix("urn:isbn:")
        .or_else(|| t.strip_prefix("URN:ISBN:"))
        .unwrap_or(t);
    if scheme.is_some_and(|s| s.eq_ignore_ascii_case("isbn")) || t.len() != text.trim().len() {
        return normal_isbn(t).or_else(|| Some(t.to_owned()));
    }
    normal_isbn(t)
}

/// A book's details as its OPF has them; every optional field is `Some`, empty when absent.
pub fn read(opf: &str) -> Result<Details, String> {
    let doc = roxmltree::Document::parse(opf).map_err(|e| e.to_string())?;
    let meta = doc
        .descendants()
        .find(|n| n.tag_name().name() == "metadata")
        .ok_or("the OPF has no <metadata>")?;
    let dc = |name: &'static str| {
        meta.children()
            .filter(move |n| n.tag_name().namespace() == Some(DC) && n.tag_name().name() == name)
    };
    let text = |n: roxmltree::Node| n.text().map(str::trim).unwrap_or("").to_owned();
    let attr = |n: roxmltree::Node, name: &str| {
        n.attributes()
            .find(|a| a.name() == name)
            .map(|a| a.value().to_owned())
    };
    let metas: Vec<roxmltree::Node> = meta
        .children()
        .filter(|n| n.tag_name().name() == "meta")
        .collect();
    let refine = |id: Option<String>, property: &str| {
        let id = id?;
        metas
            .iter()
            .find(|m| {
                attr(**m, "refines").as_deref() == Some(&format!("#{id}"))
                    && attr(**m, "property").as_deref() == Some(property)
            })
            .map(|m| text(*m))
    };
    let named = |name: &str| {
        metas
            .iter()
            .find(|m| attr(**m, "name").as_deref() == Some(name))
            .and_then(|m| attr(*m, "content"))
    };
    let authors: Vec<roxmltree::Node> = dc("creator")
        .filter(|c| {
            let role = attr(*c, "role").or_else(|| refine(attr(*c, "id"), "role"));
            role.is_none_or(|r| r == "aut")
        })
        .collect();
    let series = match named("calibre:series") {
        Some(name) => Some(SeriesRef {
            name,
            index: named("calibre:series_index").and_then(|i| i.parse().ok()),
        }),
        None => metas
            .iter()
            .find(|m| attr(**m, "property").as_deref() == Some("belongs-to-collection"))
            .map(|m| SeriesRef {
                name: text(*m),
                index: refine(attr(*m, "id"), "group-position").and_then(|i| i.parse().ok()),
            }),
    };
    let unique = doc.root_element().attribute("unique-identifier");
    // Another ISBN wins over the unique identifier's: that one names the file, not the edition.
    let isbns: Vec<(Option<String>, String)> = dc("identifier")
        .filter_map(|i| isbn_of(&text(i), attr(i, "scheme").as_deref()).map(|n| (attr(i, "id"), n)))
        .collect();
    let isbn = isbns
        .iter()
        .find(|(id, _)| id.as_deref() != unique)
        .or(isbns.first())
        .map(|(_, n)| n.clone());
    let first_title = dc("title").next();
    Ok(Details {
        title: first_title.map(text).unwrap_or_default(),
        authors: authors
            .iter()
            .map(|a| text(*a))
            .filter(|a| !a.is_empty())
            .collect(),
        series,
        description: Some(dc("description").next().map(text).unwrap_or_default()),
        tags: Some(dc("subject").map(text).filter(|t| !t.is_empty()).collect()),
        publisher: Some(dc("publisher").next().map(text).unwrap_or_default()),
        published: Some(
            dc("date")
                .find(|d| attr(*d, "event").is_none_or(|e| e == "publication"))
                .map(text)
                .unwrap_or_default(),
        ),
        language: Some(dc("language").next().map(text).unwrap_or_default()),
        isbn: Some(isbn.unwrap_or_default()),
        title_sort: Some(
            named("calibre:title_sort")
                .or_else(|| first_title.and_then(|t| refine(attr(t, "id"), "file-as")))
                .unwrap_or_default(),
        ),
        author_sort: Some(
            authors
                .first()
                .and_then(|a| attr(*a, "file-as").or_else(|| refine(attr(*a, "id"), "file-as")))
                .unwrap_or_default(),
        ),
    })
}

/// The details of the library book whose copy is at `path`.
pub fn read_book(path: &Path) -> Result<Details, String> {
    let mut epub = deckle_core::epub::EpubReader::open(path).map_err(|e| e.to_string())?;
    let opf = epub
        .read_string(&epub.opf_path.clone())
        .map_err(|e| e.to_string())?;
    read(&opf)
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
    let epub3 = Regex::new(r#"(?is)<package\b[^>]*\bversion\s*=\s*["']3"#)
        .unwrap()
        .is_match(opf);
    let opf_ns = opf.contains("xmlns:opf=");
    let sort = filled(&d.author_sort);
    let creators: String = d
        .authors
        .iter()
        .filter(|a| !a.trim().is_empty())
        .enumerate()
        .map(|(i, a)| match sort.filter(|_| i == 0) {
            Some(s) if epub3 => format!(
                "\n    <dc:creator id=\"deckle-creator\">{}</dc:creator>\n    <meta refines=\"#deckle-creator\" property=\"file-as\">{}</meta>",
                esc(a.trim()),
                esc(s)
            ),
            Some(s) if opf_ns => format!(
                "\n    <dc:creator opf:file-as=\"{}\">{}</dc:creator>",
                esc(s),
                esc(a.trim())
            ),
            _ => format!("\n    <dc:creator>{}</dc:creator>", esc(a.trim())),
        })
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
    }

    // The rest, each only when it's asked for.
    let dc_re = |tag: &str| {
        Regex::new(&format!(r"(?is)\s*<dc:{tag}\b[^>]*?(?:/>|>.*?</dc:{tag}>)")).unwrap()
    };
    let mut extra = String::new();
    let mut put = |tag: &str, v: &str| {
        if !v.trim().is_empty() {
            extra.push_str(&format!("\n    <dc:{tag}>{}</dc:{tag}>", esc(v.trim())));
        }
    };
    for (tag, value) in [("description", &d.description), ("publisher", &d.publisher)] {
        if let Some(v) = value {
            block = dc_re(tag).replace_all(&block, "").into_owned();
            put(tag, v);
        }
    }
    if let Some(tags) = &d.tags {
        block = dc_re("subject").replace_all(&block, "").into_owned();
        tags.iter().for_each(|t| put("subject", t));
    }
    if let Some(p) = &d.published {
        // Only the publication date: EPUB 2's modification and other dates stay.
        let event = Regex::new(r#"\bevent\s*=\s*["']([^"']+)["']"#).unwrap();
        block = dc_re("date")
            .replace_all(&block, |c: &regex::Captures| match event.captures(&c[0]) {
                Some(e) if &e[1] != "publication" => c[0].to_owned(),
                _ => String::new(),
            })
            .into_owned();
        put("date", p);
    }
    if let Some(l) = filled(&d.language) {
        block = dc_re("language").replace_all(&block, "").into_owned();
        put("language", l);
    }
    if let Some(i) = &d.isbn {
        // ISBNs go, except the package's unique identifier, which names the book.
        let unique =
            Regex::new(r#"(?is)<package\b[^>]*\bunique-identifier\s*=\s*["']([^"']+)["']"#)
                .unwrap()
                .captures(opf)
                .map(|c| c[1].to_owned());
        let inner = Regex::new(r"(?is)>(.*?)</").unwrap();
        let scheme = Regex::new(r#"\bscheme\s*=\s*["']([^"']+)["']"#).unwrap();
        let mut kept = None;
        block = dc_re("identifier")
            .replace_all(&block, |c: &regex::Captures| {
                let m = &c[0];
                let text = inner
                    .captures(m)
                    .map(|t| t[1].to_owned())
                    .unwrap_or_default();
                let found = isbn_of(&text, scheme.captures(m).as_ref().map(|s| &s[1]));
                if id_of(m).is_some() && id_of(m) == unique {
                    kept = found;
                    m.to_owned()
                } else if found.is_some() {
                    String::new()
                } else {
                    m.to_owned()
                }
            })
            .into_owned();
        if let Some(n) = normal_isbn(i.trim()).filter(|n| kept.as_ref() != Some(n)) {
            put("identifier", &format!("urn:isbn:{n}"));
        }
    }
    if let Some(t) = &d.title_sort {
        let re = Regex::new(
            r#"(?is)\s*<meta\b[^>]*\bname\s*=\s*["']calibre:title_sort["'][^>]*?(?:/>|>\s*</meta>)"#,
        )
        .unwrap();
        block = re.replace_all(&block, "").into_owned();
        if !t.trim().is_empty() {
            extra.push_str(&format!(
                "\n    <meta name=\"calibre:title_sort\" content=\"{}\"/>",
                esc(t.trim())
            ));
        }
    }
    let block = format!("{}{extra}\n", block.trim_end());
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
    check(d)?;
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
    if d.author_sort.is_some() {
        m.author_sort = filled(&d.author_sort).map(str::to_owned);
    }
    if let Some(l) = filled(&d.language) {
        m.language = l.to_owned();
    }
    if let Some(i) = &d.isbn {
        m.isbn = normal_isbn(i);
    }
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
                ..Details::default()
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

    const OPF2: &str = r##"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf"><dc:identifier id="uid" opf:scheme="ISBN">9780000000002</dc:identifier><dc:identifier opf:scheme="ISBN">0-306-40615-2</dc:identifier><dc:title>Old</dc:title><dc:creator opf:role="aut" opf:file-as="Field, Ada">Ada Field</dc:creator><dc:creator opf:role="edt">An Editor</dc:creator><dc:language>en</dc:language><dc:description>&lt;p&gt;Old blurb&lt;/p&gt;</dc:description><dc:subject>Old tag</dc:subject><dc:publisher>Old House</dc:publisher><dc:date opf:event="publication">1999</dc:date><dc:date opf:event="modification">2024-01-01</dc:date><meta name="calibre:title_sort" content="Old"/></metadata><manifest/></package>"##;

    fn full() -> Details {
        Details {
            title: "The New One".into(),
            authors: vec!["Ada Field".into()],
            series: None,
            description: Some("<p>A blurb & more</p>".into()),
            tags: Some(vec!["Fantasy".into(), "Book club".into()]),
            publisher: Some("Bloomsbury".into()),
            published: Some("2020-09-15".into()),
            language: Some("en-GB".into()),
            isbn: Some("9781635575637".into()),
            title_sort: Some("New One, The".into()),
            author_sort: Some("Field, Ada".into()),
        }
    }

    #[test]
    fn every_field_is_written_and_reads_back() {
        for opf in [OPF, OPF2] {
            let out = rewrite_opf(opf, &full()).unwrap();
            roxmltree::Document::parse(&out).unwrap();
            let back = read(&out).unwrap();
            assert_eq!(back, full(), "{out}");
        }
    }

    #[test]
    fn rewriting_twice_changes_nothing_more() {
        for opf in [OPF, OPF2] {
            let once = rewrite_opf(opf, &full()).unwrap();
            assert_eq!(rewrite_opf(&once, &full()).unwrap(), once);
        }
    }

    #[test]
    fn none_leaves_the_books_own_and_empty_removes_it() {
        let keep = Details {
            title: "T".into(),
            authors: vec!["Ada Field".into()],
            ..Details::default()
        };
        let out = rewrite_opf(OPF2, &keep).unwrap();
        for own in [
            "Old blurb",
            "Old tag",
            "Old House",
            ">1999<",
            "calibre:title_sort",
            ">en<",
        ] {
            assert!(out.contains(own), "{own} kept: {out}");
        }
        let empty = Details {
            description: Some(String::new()),
            tags: Some(vec![]),
            publisher: Some(" ".into()),
            published: Some(String::new()),
            language: Some(String::new()),
            isbn: Some(String::new()),
            title_sort: Some(String::new()),
            ..keep
        };
        let out = rewrite_opf(OPF2, &empty).unwrap();
        for gone in [
            "Old blurb",
            "Old tag",
            "Old House",
            ">1999<",
            "calibre:title_sort",
            "0-306-40615-2",
        ] {
            assert!(!out.contains(gone), "{gone} removed: {out}");
        }
        assert!(out.contains(">en<"), "a book keeps a language");
        assert!(
            out.contains("opf:event=\"modification\""),
            "other dates stay"
        );
        assert!(out.contains("id=\"uid\""), "the unique identifier stays");
    }

    #[test]
    fn reading_skips_non_authors_and_finds_the_isbn() {
        let d = read(OPF2).unwrap();
        assert_eq!(d.authors, vec!["Ada Field".to_owned()]);
        assert_eq!(
            d.isbn.as_deref(),
            Some("0306406152"),
            "the edition's ISBN, not the unique id's"
        );
        assert_eq!(d.published.as_deref(), Some("1999"));
        assert_eq!(d.description.as_deref(), Some("<p>Old blurb</p>"));
        assert_eq!(d.author_sort.as_deref(), Some("Field, Ada"));
    }

    #[test]
    fn bad_values_are_refused_in_plain_words() {
        for (d, word) in [
            (
                Details {
                    published: Some("Sept 2020".into()),
                    ..full()
                },
                "Published",
            ),
            (
                Details {
                    isbn: Some("12345".into()),
                    ..full()
                },
                "ISBN",
            ),
            (
                Details {
                    language: Some("English".into()),
                    ..full()
                },
                "Language",
            ),
        ] {
            assert!(check(&d).unwrap_err().contains(word));
        }
        assert!(check(&full()).is_ok());
        assert_eq!(
            normal_isbn("978-1-63557-563-7").as_deref(),
            Some("9781635575637")
        );
        assert_eq!(normal_isbn("0-306-40615-x").as_deref(), Some("030640615X"));
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
            ..Details::default()
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
