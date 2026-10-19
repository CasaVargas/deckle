// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::container::read_kfx;
use deckle_kfx::fixup::{
    Book, create_content_fragments, create_position_and_location_maps, fix_navigation,
    fix_structural,
};
use deckle_kfx::ion::Sym;
use deckle_kfx::kpf::read_kpf;
use deckle_kfx::metadata::{CdeType, Metadata, MetadataError, apply, author_sort, generate_asin};

#[test]
fn ebok_stamping_matches_ebok_golden_490() {
    let g = read_kfx(
        &std::fs::read(deckle_kfx::fixtures::path("deckle-mini-ebok.golden.kfx")).unwrap(),
    )
    .unwrap();
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, &g.container_id);
    fix_structural(&mut book).unwrap();
    fix_navigation(&mut book);
    let t = create_content_fragments(&mut book);
    create_position_and_location_maps(&mut book, t).unwrap();
    apply(
        &mut book,
        &Metadata {
            cde_type: Some(CdeType::Ebok),
            asin: Some("B0DECK000002".into()),
            authors: Some(vec![]),
            ..Default::default()
        },
    )
    .unwrap();
    let md = Sym::new("$490");
    assert_eq!(
        book.fragments.get_single(&md).unwrap().value,
        g.fragments.get_single(&md).unwrap().value
    );
}

#[test]
fn authors_are_stored_in_sort_form_and_title_replaced() {
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, "CR!TEST");
    fix_structural(&mut book).unwrap();
    apply(
        &mut book,
        &Metadata {
            title: Some("New Title".into()),
            authors: Some(vec!["Jane Testwright".into(), "Smith, John".into()]),
            language: Some("spa".into()),
            ..Default::default()
        },
    )
    .unwrap();
    use deckle_kfx::ion::Value;
    let authors: Vec<String> = book
        .fragments
        .get_single(&Sym::new("$490"))
        .unwrap()
        .value
        .as_struct()
        .unwrap()
        .get("$491")
        .unwrap()
        .as_list()
        .unwrap()[0]
        .as_struct()
        .unwrap()
        .get("$258")
        .unwrap()
        .as_list()
        .unwrap()
        .iter()
        .filter_map(|kv| {
            let s = kv.as_struct()?;
            (s.get("$492")?.as_str()? == "author")
                .then(|| s.get("$307")?.as_str().map(str::to_owned))
                .flatten()
        })
        .collect();
    assert_eq!(authors, ["Testwright, Jane", "Smith, John"]);
    assert_eq!(
        book.title_metadata("title"),
        Some(&Value::string("New Title"))
    );
    assert_eq!(book.title_metadata("language"), Some(&Value::string("es")));
}

#[test]
fn author_sort_cases() {
    assert_eq!(author_sort("Jane Testwright"), "Testwright, Jane");
    assert_eq!(author_sort("Smith, John"), "Smith, John");
    assert_eq!(author_sort("Plato"), "Plato");
    assert_eq!(
        author_sort("Gabriel García Márquez"),
        "Márquez, Gabriel García"
    );
}

#[test]
fn generated_asins_are_recognizable_and_stable() {
    let a = generate_asin(b"x");
    assert_eq!(a.len(), 10);
    assert!(a.starts_with("B0"));
    assert!(a.ends_with("DECK"));
    assert_eq!(a, generate_asin(b"x"));
    assert_ne!(a, generate_asin(b"y"));
    assert!(
        a.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    );
}

#[test]
fn png_cover_is_rejected() {
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, "CR!TEST");
    fix_structural(&mut book).unwrap();
    let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
    match apply(
        &mut book,
        &Metadata {
            cover_jpeg: Some(png),
            ..Default::default()
        },
    ) {
        Err(MetadataError::CoverNotJpeg) => {}
        other => panic!("expected CoverNotJpeg, got {other:?}"),
    }
}

#[test]
fn jpeg_cover_replaces_the_resource_bytes_and_dimensions() {
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, "CR!TEST");
    fix_structural(&mut book).unwrap();
    // minimal JFIF header followed by an SOF0 marker declaring 300x450 (height, width big-endian)
    let mut jpg = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00".to_vec();
    jpg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0xC2, 0x01, 0x2C, 0x03]);
    jpg.extend_from_slice(&[
        0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xD9,
    ]);
    apply(
        &mut book,
        &Metadata {
            cover_jpeg: Some(jpg.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let res = book
        .fragments
        .get(&Sym::new("$164"), Some(&Sym::new("e6")))
        .unwrap()
        .value
        .as_struct()
        .unwrap()
        .clone();
    assert_eq!(res.get("$422").unwrap().as_int(), Some(300));
    assert_eq!(res.get("$423").unwrap().as_int(), Some(450));
    let loc = res.get("$165").unwrap().as_str().unwrap().to_owned();
    let raw = book
        .fragments
        .get(&Sym::new("$417"), Some(&Sym::new(&loc)))
        .unwrap();
    assert_eq!(raw.value.as_blob().unwrap(), jpg.as_slice());
}
