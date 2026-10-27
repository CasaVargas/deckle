// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::container::{WriteOptions, read_kfx, write_kfx};
use deckle_kfx::ion::Sym;

#[test]
fn golden_reads_and_rewrites_byte_exact() {
    let bytes = std::fs::read(deckle_kfx::fixtures::path(
        "deckle-mini-pdoc-nopages.golden.kfx",
    ))
    .unwrap();
    let doc = read_kfx(&bytes).unwrap();
    assert_eq!(doc.fragments.len(), 40);
    assert_eq!(doc.creator, "kfxlib-20260827");
    assert_eq!(doc.package_version, "");
    assert!(doc.container_id.starts_with("CR!"));
    assert_eq!(doc.container_id.len(), 31);
    assert_eq!(doc.symtab.local_symbols().len(), 30);
    // container-level fragments come first, in kfxlib's order
    let types: Vec<&str> = doc
        .fragments
        .iter()
        .take(4)
        .map(|f| f.ftype.as_str())
        .collect();
    assert_eq!(types, ["$ion_symbol_table", "$270", "$593", "$585"]);
    let c = doc
        .fragments
        .get_single(&Sym::new("$270"))
        .unwrap()
        .value
        .as_struct()
        .unwrap()
        .clone();
    assert_eq!(c.get("$161").unwrap().as_str(), Some("KFX main"));
    assert_eq!(c.get("$181").unwrap().as_list().unwrap().len(), 37);
    let out = write_kfx(
        &doc,
        &WriteOptions {
            container_id: doc.container_id.clone(),
            creator: doc.creator.clone(),
            package_version: doc.package_version.clone(),
        },
    )
    .unwrap();
    assert_eq!(out.len(), bytes.len());
    assert_eq!(out, bytes);
}

#[test]
fn pages_golden_also_roundtrips() {
    let bytes = std::fs::read(deckle_kfx::fixtures::path("deckle-mini-pdoc.golden.kfx")).unwrap();
    let doc = read_kfx(&bytes).unwrap();
    let out = write_kfx(&doc, &WriteOptions::from_doc(&doc)).unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn rejects_non_kfx_input() {
    assert!(read_kfx(b"PK\x03\x04 not a container").is_err());
    assert!(read_kfx(b"CONT").is_err());
}
