// SPDX-License-Identifier: GPL-3.0-or-later
//! The KF8-only AZW3 made from Kindle Previewer's joint MOBI of a fixed-layout comic
//! (`fixtures/comic/fixed-layout-rtl.mobi`, fixed-layout spec §4).

use deckle_mobi::azw3::{Stamp, kf8_only};
use deckle_mobi::fixture::{Book, build};
use deckle_mobi::palmdb::{PalmDb, be32};
use deckle_mobi::{MobiError, read_meta};

fn previewer_mobi() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/comic/fixed-layout-rtl.mobi"),
    )
    .unwrap()
}

const PDOC: Stamp<'static> = Stamp {
    cde: "PDOC",
    asin: "B0DECKCOMIC",
};

/// Record 0's EXTH records.
fn exth(rec0: &[u8]) -> Vec<(u32, Vec<u8>)> {
    let at = 16 + be32(rec0, 20).unwrap() as usize;
    assert_eq!(&rec0[at..at + 4], b"EXTH");
    let n = be32(rec0, at + 8).unwrap();
    let mut p = at + 12;
    (0..n)
        .map(|_| {
            let (k, l) = (be32(rec0, p).unwrap(), be32(rec0, p + 4).unwrap() as usize);
            let d = rec0[p + 8..p + l].to_vec();
            p += l;
            (k, d)
        })
        .collect()
}

fn value(rec0: &[u8], kind: u32) -> Option<Vec<u8>> {
    exth(rec0)
        .into_iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, d)| d)
}

#[test]
fn the_joint_mobi_becomes_a_kf8_only_book_with_its_pages() {
    let joint = previewer_mobi();
    let src = PalmDb::parse(&joint).unwrap();
    let out = kf8_only(&joint, &PDOC).unwrap();
    let db = PalmDb::parse(&out).unwrap();
    let rec0 = db.record(0);

    // KF8 header first; records 19..33 of the joint file plus the 5 shared resources.
    assert_eq!(be32(rec0, 0x24), Some(8));
    assert_eq!(db.len(), (src.len() - 18) + 5);
    assert_eq!(db.record(db.len() - 1), [0xE9, 0x8E, 0x0D, 0x0A]);
    assert!(!(0..db.len()).any(|i| db.record(i).starts_with(b"BOUNDARY")));

    // The resources sit where the header's first resource index says: 3 pages, RESC, thumbnail.
    let first = be32(rec0, 0x6C).unwrap() as usize;
    assert_eq!(first, 11);
    for i in 0..3 {
        assert_eq!(db.record(first + i), src.record(6 + i), "page {i}");
    }
    assert!(db.record(first + 3).starts_with(b"RESC"));
    assert!(db.record(first + 4).starts_with(&[0xFF, 0xD8]));

    // Every index at or after the insertion point moved up by 5; the ones before it didn't.
    let at = |field: usize| db.record(be32(rec0, field).unwrap() as usize);
    assert!(at(0xC0).starts_with(b"FDST"));
    assert!(at(0xC8).starts_with(b"FCIS"));
    assert!(at(0xD0).starts_with(b"FLIS"));
    assert!(at(0x100).starts_with(b"DATP"));
    for index in [0xF4, 0xF8, 0xFC] {
        assert!(at(index).starts_with(b"INDX"), "{index:#x}");
    }
    // The KF8 text is untouched.
    assert_eq!(db.record(1), src.record(19));

    // EXTH: stamped, the boundary gone, the layout and direction kept, the name still readable.
    assert_eq!(value(rec0, 501).as_deref(), Some(&b"PDOC"[..]));
    assert_eq!(value(rec0, 113).as_deref(), Some(&b"B0DECKCOMIC"[..]));
    assert_eq!(value(rec0, 504).as_deref(), Some(&b"B0DECKCOMIC"[..]));
    assert_eq!(value(rec0, 121), None);
    assert_eq!(value(rec0, 125), Some(5u32.to_be_bytes().to_vec()));
    assert_eq!(value(rec0, 527).as_deref(), Some(&b"rtl"[..]));
    let meta = read_meta(&out).unwrap();
    assert!(meta.kf8 && meta.fixed_layout && !meta.drm);
    assert_eq!(meta.cde.as_deref(), Some("PDOC"));
    assert_eq!(meta.asin.as_deref(), Some("B0DECKCOMIC"));
    assert_eq!(meta.title.as_deref(), Some("FXL Probe"));
    // EXTH 201 still finds the cover: the first page.
    assert_eq!(meta.cover.as_deref(), Some(src.record(6)));
}

#[test]
fn a_kf8_only_book_is_only_restamped() {
    let once = kf8_only(&previewer_mobi(), &PDOC).unwrap();
    let ebok = Stamp {
        cde: "EBOK",
        asin: "B0DECKOTHER",
    };
    let twice = kf8_only(&once, &ebok).unwrap();
    let (a, b) = (
        PalmDb::parse(&once).unwrap(),
        PalmDb::parse(&twice).unwrap(),
    );
    assert_eq!(a.len(), b.len());
    for i in 1..a.len() {
        assert_eq!(a.record(i), b.record(i), "record {i}");
    }
    let meta = read_meta(&twice).unwrap();
    assert_eq!(meta.cde.as_deref(), Some("EBOK"));
    assert_eq!(meta.asin.as_deref(), Some("B0DECKOTHER"));
    // One of each, not a second copy.
    assert_eq!(
        exth(b.record(0)).iter().filter(|(k, _)| *k == 501).count(),
        1
    );

    // A small KF8 book from the fixture builder (a shorter MOBI header) restamps too.
    let small = build(&Book {
        title: "Picture Book",
        kf8: true,
        fixed_layout: true,
        cover: Some(b"\xFF\xD8cover"),
        ..Default::default()
    });
    let meta = read_meta(&kf8_only(&small, &PDOC).unwrap()).unwrap();
    assert_eq!(meta.title.as_deref(), Some("Picture Book"));
    assert_eq!(meta.cover.as_deref(), Some(&b"\xFF\xD8cover"[..]));
    assert_eq!(meta.cde.as_deref(), Some("PDOC"));
}

#[test]
fn drm_and_books_with_no_kf8_part_are_refused() {
    let drm = build(&Book {
        title: "Locked",
        kf8: true,
        drm: true,
        ..Default::default()
    });
    assert!(matches!(kf8_only(&drm, &PDOC), Err(MobiError::Drm)));
    let old = build(&Book {
        title: "Old",
        ..Default::default()
    });
    assert!(matches!(
        kf8_only(&old, &PDOC),
        Err(MobiError::Unsupported(w)) if w.contains("no KF8 part")
    ));
}
