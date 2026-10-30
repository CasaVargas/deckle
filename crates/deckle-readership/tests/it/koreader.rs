// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader's positions as reads (KOReader hub design §C).

use std::io::Write;

use crate::common::*;
use deckle_digest::Config;
use deckle_readership::koreader::{edition_of, spine_index};
use deckle_readership::layout::{Layout, SPINE_SPAN, epub_layout, layout_from_epub};
use deckle_readership::state::State;
use deckle_readership::{record_koreader_position, report};

/// The build's sections, without the front page, as `record_layout` passes them.
fn sections() -> Vec<(String, String)> {
    [
        ("weather", "Weather"),
        ("local", "Local"),
        ("lottery", "Lottery"),
        ("horoscope", "Horoscope"),
    ]
    .iter()
    .map(|(k, n)| ((*k).into(), (*n).into()))
    .collect()
}

/// A paper's EPUB as render.rs writes it: the front page, `s1`…`s4`, then the nav, not linear.
fn paper_epub() -> Vec<u8> {
    let mut items = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
         <item id=\"front\" href=\"front.xhtml\" media-type=\"application/xhtml+xml\"/>",
    );
    let mut spine = String::from("<itemref idref=\"front\"/>");
    for n in 1..=4 {
        items.push_str(&format!(
            "<item id=\"s{n}\" href=\"s{n}.xhtml\" media-type=\"application/xhtml+xml\"/>"
        ));
        spine.push_str(&format!("<itemref idref=\"s{n}\"/>"));
    }
    spine.push_str("<itemref idref=\"nav\" linear=\"no\"/>");
    let opf = format!(
        "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\">\
         <metadata/><manifest>{items}</manifest><spine toc=\"ncx\">{spine}</spine></package>"
    );
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default();
    for (name, body) in [
        ("mimetype", "application/epub+zip".to_owned()),
        (
            "META-INF/container.xml",
            "<?xml version=\"1.0\"?><container version=\"1.0\" \
             xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles>\
             <rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
             </rootfiles></container>"
                .to_owned(),
        ),
        ("OEBPS/content.opf", opf),
    ] {
        z.start_file(name, o).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// The edition of `date` with only an EPUB layout.
fn epub_only(date: &str) -> Layout {
    let epub = epub_layout(&paper_epub(), &sections()).unwrap();
    layout_from_epub(
        epub,
        d(date),
        "The Deckle Daily",
        &format!("deckle-daily-{date}"),
    )
}

/// An xpointer into the `n`th spine file, 1-based.
fn at(n: usize) -> String {
    format!("/body/DocFragment[{n}]/body/div[2]/p[7]/text().12")
}

#[test]
fn the_spine_is_read_from_the_epub_with_each_sections_file() {
    let e = epub_layout(&paper_epub(), &sections()).unwrap();
    assert_eq!(
        e.files,
        [
            "front.xhtml",
            "s1.xhtml",
            "s2.xhtml",
            "s3.xhtml",
            "s4.xhtml",
            "nav.xhtml"
        ]
    );
    let keys: Vec<(&str, usize)> = e
        .sections
        .iter()
        .map(|s| (s.key.as_str(), s.spine))
        .collect();
    assert_eq!(
        keys,
        [
            ("front", 0),
            ("weather", 1),
            ("local", 2),
            ("lottery", 3),
            ("horoscope", 4)
        ]
    );
    // a build that doesn't match the EPUB is refused, not recorded wrong
    assert!(epub_layout(&paper_epub(), &sections()[..3]).is_err());
}

#[test]
fn an_xpointer_reads_its_docfragment_as_a_spine_index() {
    assert_eq!(spine_index(&at(4)), Some(3));
    assert_eq!(spine_index("/body/DocFragment[1]"), Some(0));
    assert_eq!(spine_index("/body/DocFragment[0]/body"), None);
    assert_eq!(spine_index("12"), None);
}

#[test]
fn an_xpointer_in_s3_reaches_the_sections_up_to_the_third() {
    let mut s = State::default();
    s.record_layout(epub_only("2026-10-30"));
    // s3.xhtml is DocFragment[4]: the front page is the first file
    assert!(record_koreader_position(
        &mut s,
        d("2026-10-30"),
        "koreader/KOBOeReader",
        &at(4),
        Some(0.55),
        "2026-10-30T07:40:00Z",
    ));
    let read = &s.reads[&d("2026-10-30")]["koreader/KOBOeReader"];
    assert!(read.opened);
    assert_eq!(read.furthest, Some(3 * SPINE_SPAN + 1));
    assert_eq!(read.last, read.furthest);
    assert_eq!(read.last_read.as_deref(), Some("2026-10-30T07:40:00Z"));
    let layout = &s.editions[&d("2026-10-30")];
    let reached: Vec<&str> = layout
        .sections
        .iter()
        .filter(|x| x.start <= read.furthest.unwrap())
        .map(|x| x.key.as_str())
        .collect();
    assert_eq!(reached, ["front", "weather", "local", "lottery"]);
}

#[test]
fn going_back_keeps_the_furthest_and_an_older_position_changes_nothing() {
    let mut s = State::default();
    s.record_layout(epub_only("2026-10-30"));
    let (date, dev) = (d("2026-10-30"), "koreader/x");
    record_koreader_position(&mut s, date, dev, &at(5), None, "2026-10-30T08:00:00Z");
    record_koreader_position(&mut s, date, dev, &at(2), None, "2026-10-30T09:00:00Z");
    record_koreader_position(&mut s, date, dev, &at(3), None, "2026-10-30T07:00:00Z");
    let read = &s.reads[&date][dev];
    assert_eq!(read.furthest, Some(4 * SPINE_SPAN + 1));
    assert_eq!(read.last, Some(SPINE_SPAN + 1));
    assert_eq!(read.last_read.as_deref(), Some("2026-10-30T09:00:00Z"));
}

#[test]
fn with_a_kfx_layout_the_read_lands_in_kfx_positions() {
    let mut s = State::default();
    let mut kfx = layout(
        "2026-10-30",
        &[0, 1_200, 3_400, 6_100, 8_000], // front, s1…s4 in KFX positions
    );
    for (sec, (key, name)) in kfx.sections[1..].iter_mut().zip(sections()) {
        sec.key = key;
        sec.name = name;
    }
    kfx.epub = Some(epub_layout(&paper_epub(), &sections()).unwrap());
    assert!(kfx.is_kfx());
    s.record_layout(kfx);
    record_koreader_position(
        &mut s,
        d("2026-10-30"),
        "koreader/abc",
        &at(4),
        Some(0.9),
        "2026-10-30T07:40:00Z",
    );
    // lottery (s3.xhtml) starts at 6 100 in the KFX
    assert_eq!(
        s.reads[&d("2026-10-30")]["koreader/abc"].furthest,
        Some(6_101)
    );
}

#[test]
fn without_a_spine_the_percentage_places_the_read() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-30", &[0, 1_000, 5_000])); // recorded before the spine was
    assert!(record_koreader_position(
        &mut s,
        d("2026-10-30"),
        "koreader/abc",
        &at(4),
        Some(0.6),
        "2026-10-30T07:40:00Z",
    ));
    assert_eq!(
        s.reads[&d("2026-10-30")]["koreader/abc"].furthest,
        Some(6_000)
    );
    // no layout for the date, or nothing to place: nothing recorded
    assert!(!record_koreader_position(
        &mut s,
        d("2026-10-29"),
        "koreader/abc",
        &at(4),
        Some(0.6),
        "t",
    ));
    assert!(!record_koreader_position(
        &mut s,
        d("2026-10-30"),
        "koreader/abc",
        "/body/p",
        None,
        "t",
    ));
}

#[test]
fn editions_with_only_an_epub_layout_report() {
    let cfg =
        Config::parse("sections = [\"weather\", \"local\", \"lottery\", \"horoscope\"]\n").unwrap();
    let mut s = State::default();
    let start = d("2026-10-01");
    for i in 0..10u64 {
        let date = start + chrono::Days::new(i);
        s.record_layout(epub_only(&date.to_string()));
        // always to Local (s2.xhtml), never Lottery or Horoscope
        record_koreader_position(&mut s, date, "koreader/k", &at(3), None, NOW);
    }
    let r = report(&s, &cfg);
    assert_eq!(r.opened, 10);
    let shares: Vec<(&str, Option<u8>)> = r
        .sections
        .iter()
        .map(|x| (x.key.as_str(), x.share))
        .collect();
    assert_eq!(
        shares,
        [
            ("weather", Some(100)),
            ("local", Some(100)),
            ("lottery", Some(0)),
            ("horoscope", Some(0)),
        ]
    );
    assert_eq!(r.stop_section.as_deref(), Some("Local"));
    // their lengths are file counts, not text: tomorrow's paper doesn't size from them
    assert!(deckle_readership::sizes::history(&s).whole.is_empty());
}

#[test]
fn an_older_file_without_the_epub_field_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("readership.json");
    std::fs::write(
        &path,
        r#"{"editions":{"2026-10-20":{"date":"2026-10-20","title":"The Deckle Daily",
            "file":"deckle-daily-2026-10-20","end":100,
            "sections":[{"key":"front","name":"Front page","start":0}]}},
            "reads":{"2026-10-20":{"G000":{"opened":true,"furthest":50,"seen":"x"}}}}"#,
    )
    .unwrap();
    let s = State::load(&path);
    let l = &s.editions[&d("2026-10-20")];
    assert_eq!(l.epub, None);
    assert!(l.is_kfx());
    assert_eq!(s.reads[&d("2026-10-20")]["G000"].furthest, Some(50));
    // and a new one round-trips with it
    let mut s = s;
    s.record_layout(epub_only("2026-10-30"));
    s.save(&path).unwrap();
    let back = State::load(&path);
    assert_eq!(back, s);
    assert!(!back.editions[&d("2026-10-30")].is_kfx());
}

#[test]
fn a_build_with_no_kfx_never_replaces_a_kfx_layout() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-30", &[0, 1_000, 5_000]));
    assert!(!s.record_epub_layout(epub_only("2026-10-30")));
    assert!(s.editions[&d("2026-10-30")].is_kfx());
    assert!(s.record_epub_layout(epub_only("2026-10-31")));
    assert!(!s.editions[&d("2026-10-31")].is_kfx());
}

#[test]
fn a_file_on_a_reader_is_the_edition_its_stem_names() {
    let mut s = State::default();
    s.record_layout(epub_only("2026-10-30"));
    assert_eq!(
        edition_of(&s, "Deckle/deckle-daily-2026-10-30.kepub.epub"),
        Some(d("2026-10-30"))
    );
    assert_eq!(
        edition_of(&s, "Deckle/deckle-daily-2026-10-30.epub"),
        Some(d("2026-10-30"))
    );
    assert_eq!(edition_of(&s, "Books/Walden.epub"), None);
    assert_eq!(
        edition_of(&s, "Deckle/deckle-daily-kid-2026-10-30.epub"),
        None
    );
}
