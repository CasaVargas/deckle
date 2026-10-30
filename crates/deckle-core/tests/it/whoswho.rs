// SPDX-License-Identifier: GPL-3.0-or-later
//! Who's who (docs/superpowers/specs/2026-10-30-whos-who-design.md).
use std::io::{Read, Write};

use deckle_core::whoswho::{PAGE, add_with};
use deckle_core::xray::XRayOptions;

/// An EPUB 3 with a nav and an NCX; chapters live in `OEBPS/text/`.
fn epub(chapters: &[(&str, &str)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut put = |name: &str, body: &str| {
        z.start_file(name, stored).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    };
    put("mimetype", "application/epub+zip");
    put(
        "META-INF/container.xml",
        "<?xml version=\"1.0\"?><container version=\"1.0\" \
         xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile \
         full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
         </rootfiles></container>",
    );
    let items: String = (0..chapters.len())
        .map(|i| {
            format!(
                "<item id=\"c{i}\" href=\"text/c{i}.xhtml\" media-type=\"application/xhtml+xml\"/>"
            )
        })
        .collect();
    let refs: String = (0..chapters.len())
        .map(|i| format!("<itemref idref=\"c{i}\"/>"))
        .collect();
    put(
        "OEBPS/content.opf",
        &format!(
            "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\">\
             <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Pride</dc:title>\
             <dc:creator>Jane Austen</dc:creator><dc:language>en</dc:language></metadata>\
             <manifest><item id=\"nav\" href=\"nav.xhtml\" properties=\"nav\" \
             media-type=\"application/xhtml+xml\"/><item id=\"ncx\" href=\"toc.ncx\" \
             media-type=\"application/x-dtbncx+xml\"/>{items}</manifest>\
             <spine toc=\"ncx\">{refs}</spine></package>"
        ),
    );
    let lis: String = chapters
        .iter()
        .enumerate()
        .map(|(i, (title, _))| format!("<li><a href=\"text/c{i}.xhtml\">{title}</a></li>"))
        .collect();
    put(
        "OEBPS/nav.xhtml",
        &format!(
            "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\" \
             xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>Contents</title></head>\
             <body><nav epub:type=\"toc\"><ol>{lis}</ol></nav></body></html>"
        ),
    );
    put(
        "OEBPS/toc.ncx",
        "<?xml version=\"1.0\"?><ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" \
         version=\"2005-1\"><navMap><navPoint id=\"n0\" playOrder=\"1\"><navLabel><text>One\
         </text></navLabel><content src=\"text/c0.xhtml\"/></navPoint></navMap></ncx>",
    );
    for (i, (title, body)) in chapters.iter().enumerate() {
        put(
            &format!("OEBPS/text/c{i}.xhtml"),
            &format!(
                "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head>\
                 <title>{title}</title></head><body><h1>{title}</h1>{body}</body></html>"
            ),
        );
    }
    z.finish().unwrap().into_inner()
}

/// Parses XHTML that may carry `<!DOCTYPE html>`.
fn parse(s: &str) -> Result<roxmltree::Document<'_>, roxmltree::Error> {
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(s, opts)
}

fn read(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn austen() -> Vec<u8> {
    epub(&[
        (
            "Chapter 1",
            "<p>Elizabeth Bennet looked at Jane. Jane smiled at Elizabeth, and Mr. Darcy \
             watched them both from Netherfield.</p><p>Darcy said nothing. Elizabeth laughed \
             at Netherfield.</p>",
        ),
        (
            "Chapter 2",
            "<p>Netherfield was quiet that evening, and Longbourn quieter still. \
             <a href=\"https://example.com\">Jane</a> wrote to Elizabeth.</p>\
             <p>Mr. Darcy rode to Longbourn &amp; back.</p>",
        ),
        (
            "Mr. Darcy",
            "<p>The next day Elizabeth walked to Longbourn with Mr. Darcy and Jane.</p>",
        ),
    ])
}

/// The LLM, unreachable.
fn offline_post(_: &str, _: &[(String, String)], _: &str) -> Result<String, String> {
    Err("offline".to_owned())
}

/// Wikipedia, unreachable.
fn offline_fetch(_: &str) -> Result<String, String> {
    Err("offline".to_owned())
}

#[test]
fn the_page_lists_people_then_terms_and_is_in_manifest_spine_nav_and_ncx() {
    let (mut post, mut fetch) = (offline_post, offline_fetch);
    let (out, summary) =
        add_with(&austen(), &XRayOptions::default(), &mut post, &mut fetch).unwrap();
    let summary = summary.expect("a page");
    assert!(summary.people >= 3 && summary.terms >= 2, "{summary:?}");
    assert_eq!(summary.source, "basic");

    let opf = read(&out, "OEBPS/content.opf");
    assert!(opf.contains(&format!("href=\"{PAGE}\"")), "{opf}");
    assert!(
        opf.contains("<itemref idref=\"deckle-whoswho\"/></spine>"),
        "{opf}"
    );
    let nav = read(&out, "OEBPS/nav.xhtml");
    assert!(
        nav.contains(&format!("<li><a href=\"{PAGE}\">Who's who</a></li></ol>")),
        "{nav}"
    );
    let ncx = read(&out, "OEBPS/toc.ncx");
    assert!(
        ncx.contains("playOrder=\"2\"") && ncx.contains(&format!("src=\"{PAGE}\"")),
        "{ncx}"
    );

    let page = read(&out, &format!("OEBPS/{PAGE}"));
    parse(&page).expect("well-formed page");
    let people = page.find("<h2>People</h2>").unwrap();
    let terms = page.find("<h2>Places and terms</h2>").unwrap();
    assert!(people < terms);
    let eliz = page.find("<b>Elizabeth</b>").expect("Elizabeth listed");
    let nether = page.find("<b>Netherfield</b>").expect("Netherfield listed");
    assert!(eliz < terms && nether > terms, "{page}");
    assert!(page.contains("First mentioned in Chapter 1"), "{page}");
    assert!(page.contains("href=\"text/c0.xhtml#deckle-ww-"), "{page}");
}

#[test]
fn each_chapter_links_a_names_first_mention_once_and_skips_headings_and_links() {
    let (mut post, mut fetch) = (offline_post, offline_fetch);
    let (out, _) = add_with(&austen(), &XRayOptions::default(), &mut post, &mut fetch).unwrap();
    for i in 0..3 {
        let c = read(&out, &format!("OEBPS/text/c{i}.xhtml"));
        parse(&c).unwrap_or_else(|e| panic!("c{i}: {e}\n{c}"));
        assert!(
            c.contains("xmlns:epub=\"http://www.idpf.org/2007/ops\""),
            "{c}"
        );
        let doc = parse(&c).unwrap();
        let mut linked: Vec<String> = doc
            .descendants()
            .filter(|n| n.attribute("class") == Some("deckle-ww"))
            .map(|n| n.text().unwrap_or("").to_owned())
            .collect();
        // never inside a heading or another link
        for n in doc
            .descendants()
            .filter(|n| n.attribute("class") == Some("deckle-ww"))
        {
            assert!(
                !n.ancestors().skip(1).any(|a| a.tag_name().name() == "a"
                    || a.tag_name().name().starts_with('h') && a.tag_name().name().len() == 2),
                "c{i}: linked inside a heading or link"
            );
            assert!(
                n.attribute("href")
                    .unwrap()
                    .starts_with(&format!("../{PAGE}#deckle-ww-"))
            );
        }
        let total = linked.len();
        linked.sort();
        linked.dedup();
        assert_eq!(linked.len(), total, "c{i}: a name linked twice: {linked:?}");
    }
    let c0 = read(&out, "OEBPS/text/c0.xhtml");
    assert!(
        c0.contains(">Elizabeth</a> Bennet") || c0.contains(">Elizabeth Bennet</a>"),
        "{c0}"
    );
    // `&amp;` before a name doesn't shift the link off it
    let c1 = read(&out, "OEBPS/text/c1.xhtml");
    assert!(
        c1.contains("Longbourn</a> quieter") || c1.contains(">Longbourn</a>"),
        "{c1}"
    );
    assert!(c1.contains("&amp; back."), "{c1}");
    // The chapter titled "Mr. Darcy" keeps its heading plain.
    let c2 = read(&out, "OEBPS/text/c2.xhtml");
    assert!(c2.contains("<h1>Mr. Darcy</h1>"), "{c2}");
}

#[test]
fn a_book_with_too_few_names_comes_back_unchanged() {
    let (mut post, mut fetch) = (offline_post, offline_fetch);
    let thin = epub(&[("One", "<p>It was a dark night. It rained.</p>")]);
    let (out, summary) = add_with(&thin, &XRayOptions::default(), &mut post, &mut fetch).unwrap();
    assert!(summary.is_none());
    assert_eq!(out, thin);
}

#[test]
fn a_second_pass_leaves_a_book_that_has_the_page() {
    let (mut post, mut fetch) = (offline_post, offline_fetch);
    let (once, _) = add_with(&austen(), &XRayOptions::default(), &mut post, &mut fetch).unwrap();
    let (twice, summary) = add_with(&once, &XRayOptions::default(), &mut post, &mut fetch).unwrap();
    assert!(summary.is_none());
    assert_eq!(twice, once);
}

#[test]
fn a_kobo_copy_keeps_the_links() {
    let (mut post, mut fetch) = (offline_post, offline_fetch);
    let (out, _) = add_with(&austen(), &XRayOptions::default(), &mut post, &mut fetch).unwrap();
    let kepub = deckle_core::kepub::convert(&out).unwrap();
    let page = read(&kepub, &format!("OEBPS/{PAGE}"));
    parse(&page).expect("well-formed KEPUB page");
    assert!(page.contains("koboSpan"));
    let c0 = read(&kepub, "OEBPS/text/c0.xhtml");
    parse(&c0).expect("well-formed KEPUB chapter");
    assert!(c0.contains("class=\"deckle-ww\""));
}

#[test]
fn two_chapters_in_one_file_each_link_a_name() {
    let (mut post, mut fetch) = (offline_post, offline_fetch);
    let one_file = epub(&[(
        "Volume I",
        "<h2>Chapter 1</h2><p>Elizabeth Bennet met Jane at Netherfield. Elizabeth smiled at \
         Mr. Darcy.</p><h2>Chapter 2</h2><p>Jane wrote to Elizabeth from Longbourn. Mr. Darcy \
         rode to Netherfield and Longbourn.</p><p>Elizabeth and Jane walked.</p>",
    )]);
    let (out, _) = add_with(&one_file, &XRayOptions::default(), &mut post, &mut fetch).unwrap();
    let c0 = read(&out, "OEBPS/text/c0.xhtml");
    let doc = parse(&c0).unwrap();
    let ids: Vec<&str> = doc
        .descendants()
        .filter(|n| n.attribute("class") == Some("deckle-ww") && n.text() == Some("Elizabeth"))
        .filter_map(|n| n.attribute("id"))
        .collect();
    assert_eq!(ids.len(), 2, "{c0}");
    assert!(
        // the chapter's own <h1>, then the two <h2>s
        ids[0].ends_with("-s2") && ids[1].ends_with("-s3"),
        "{ids:?}"
    );
}
