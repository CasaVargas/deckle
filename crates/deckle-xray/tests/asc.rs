// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_xray::asc::{Entity, EntityKind, XRayBook, write_asc};

fn book() -> XRayBook {
    XRayBook {
        asin: "B0TESTDECK".into(),
        lang: "en".into(),
        erl: 78_982,
        entities: vec![
            Entity {
                label: "Elizabeth Bennet".into(),
                kind: EntityKind::Person,
                description: "The second of five Bennet daughters, quick-witted and observant."
                    .into(),
                source: deckle_xray::asc::DescriptionSource::Deckle,
                occurrences: vec![(14, 9), (120, 9), (900, 16)],
            },
            Entity {
                label: "Netherfield".into(),
                kind: EntityKind::Term,
                description: "An estate near Longbourn, let to Mr. Bingley.".into(),
                source: deckle_xray::asc::DescriptionSource::Wikipedia,
                occurrences: vec![(40, 11)],
            },
        ],
    }
}

#[test]
fn writes_the_worddumb_compatible_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("XRAY.entities.B0TESTDECK.asc");
    write_asc(&path, &book()).unwrap();
    let c = rusqlite::Connection::open(&path).unwrap();
    let ver: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver, 1);
    let tables: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        tables,
        [
            "book_metadata",
            "entity",
            "entity_description",
            "entity_excerpt",
            "excerpt",
            "occurrence",
            "source",
            "string",
            "type"
        ]
    );
    // fixed rows
    let e0: (i64, i64) = c
        .query_row(
            "SELECT loc_label, has_info_card FROM entity WHERE id = 0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(e0, (1, 0));
    let n_sources: i64 = c
        .query_row("SELECT count(*) FROM source", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n_sources, 3);
    // our entities: ids from 1, type 1 = person, 2 = term, count = occurrences, has_info_card 1
    let rows: Vec<(i64, String, i64, i64, i64)> = c
        .prepare(
            "SELECT id, label, type, count, has_info_card FROM entity WHERE id > 0 ORDER BY id",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (1, "Elizabeth Bennet".into(), 1, 3, 1),
            (2, "Netherfield".into(), 2, 1, 1)
        ]
    );
    let occ: Vec<(i64, i64, i64)> = c
        .prepare("SELECT entity, start, length FROM occurrence ORDER BY start")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        occ,
        vec![(1, 14, 9), (2, 40, 11), (1, 120, 9), (1, 900, 16)]
    );
    let desc: (String, String, Option<i64>) = c
        .query_row(
            "SELECT text, source_wildcard, source FROM entity_description WHERE entity = 2",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(desc.0, "An estate near Longbourn, let to Mr. Bingley.");
    assert_eq!(desc.1, "Netherfield");
    assert_eq!(desc.2, Some(1), "Wikipedia source id");
    let deckle_src: Option<i64> = c
        .query_row(
            "SELECT source FROM entity_description WHERE entity = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        deckle_src, None,
        "book-derived descriptions carry no source"
    );
    let md: (i64, i64, i64, i64, i64) = c
        .query_row(
            "SELECT srl, erl, num_people, num_terms, num_images FROM book_metadata",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(md, (0, 78_982, 1, 1, 0));
    let types: Vec<(i64, i64, i64, i64, String)> = c
        .prepare(
            "SELECT id, label, singular_label, icon, top_mentioned_entities FROM type ORDER BY id",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        types,
        vec![(1, 14, 15, 1, "1".into()), (2, 16, 17, 2, "2".into())]
    );
    let people_label: String = c
        .query_row(
            "SELECT text FROM string WHERE id = 14 AND language = 'en'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(people_label, "People");
    let wiki_url: String = c
        .query_row("SELECT text FROM string WHERE id = 21", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wiki_url, "https://en.wikipedia.org/wiki/%s");
    let idx: i64 = c
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(idx, 3);
}

#[test]
fn rewriting_replaces_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.asc");
    std::fs::write(&path, b"stale").unwrap();
    write_asc(&path, &book()).unwrap();
    assert!(
        std::fs::read(&path)
            .unwrap()
            .starts_with(b"SQLite format 3")
    );
}
