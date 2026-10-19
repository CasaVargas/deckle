// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::container::read_kfx;
use deckle_kfx::fixup::{
    Book, add_approximate_pages, create_content_fragments, create_position_and_location_maps,
    fix_navigation, fix_structural,
};
use deckle_kfx::ion::Sym;
use deckle_kfx::kpf::read_kpf;

fn prepared(golden: &str) -> (Book, deckle_kfx::container::KfxDoc) {
    let g = read_kfx(&std::fs::read(deckle_kfx::fixtures::path(golden)).unwrap()).unwrap();
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, &g.container_id);
    fix_structural(&mut book).unwrap();
    let has_text = create_content_fragments(&mut book);
    fix_navigation(&mut book);
    create_position_and_location_maps(&mut book, has_text).unwrap();
    (book, g)
}

#[test]
fn navigation_matches_nopages_golden() {
    let (book, g) = prepared("deckle-mini-pdoc-nopages.golden.kfx");
    let nav = Sym::new("$389");
    assert_eq!(
        book.fragments.get_single(&nav).unwrap().value,
        g.fragments.get_single(&nav).unwrap().value
    );
}

#[test]
fn approximate_pages_match_pages_golden() {
    let (mut book, g) = prepared("deckle-mini-pdoc.golden.kfx");
    let pages = add_approximate_pages(&mut book, None).unwrap();
    assert_eq!(pages, 45);
    let nav = Sym::new("$389");
    assert_eq!(
        book.fragments.get_single(&nav).unwrap().value,
        g.fragments.get_single(&nav).unwrap().value
    );
    assert!(
        book.symtab
            .id_of(&Sym::new("APPROXIMATE_PAGE_LIST"))
            .is_some()
    );
    // running it again with auto keeps the existing list
    assert_eq!(add_approximate_pages(&mut book, None).unwrap(), 45);
}

#[test]
fn page_list_entry_landmarks_become_a_page_list() {
    use deckle_kfx::ion::{Struct, Value};
    let (mut book, _) = prepared("deckle-mini-pdoc-nopages.golden.kfx");
    // inject two page_list_entry landmarks into the landmarks container, out of order
    {
        let nav = book.fragments.get_single_mut(&Sym::new("$389")).unwrap();
        let ro = nav.value.as_list_mut().unwrap()[0].as_struct_mut().unwrap();
        let containers = ro.get_mut("$392").unwrap().as_list_mut().unwrap();
        let landmarks = containers
            .iter_mut()
            .find(|c| {
                c.unannotated()
                    .as_struct()
                    .and_then(|s| s.get("$235"))
                    .and_then(Value::as_sym)
                    .map(|s| s.as_str())
                    == Some("$236")
            })
            .unwrap();
        let Value::Annotated { value, .. } = landmarks else {
            panic!()
        };
        let entries = value
            .as_struct_mut()
            .unwrap()
            .get_mut("$247")
            .unwrap()
            .as_list_mut()
            .unwrap();
        for (seq, label) in [(2, "ii"), (1, "i")] {
            let mut pos = Struct::new();
            pos.insert("$155", Value::Int(887));
            pos.insert("$143", Value::Int(0));
            let mut e = Struct::new();
            e.insert("$241", {
                let mut l = Struct::new();
                l.insert(
                    "$244",
                    Value::String(format!("page_list_entry:{seq}:{label}")),
                );
                Value::Struct(l)
            });
            e.insert("$246", Value::Struct(pos));
            entries.push(Value::Annotated {
                annots: vec![Sym::new("$393")],
                value: Box::new(Value::Struct(e)),
            });
        }
    }
    fix_navigation(&mut book);
    let nav = book.fragments.get_single(&Sym::new("$389")).unwrap();
    let containers = nav.value.as_list().unwrap()[0]
        .as_struct()
        .unwrap()
        .get("$392")
        .unwrap()
        .as_list()
        .unwrap();
    let page_list = containers
        .iter()
        .find(|c| {
            c.unannotated()
                .as_struct()
                .and_then(|s| s.get("$235"))
                .and_then(Value::as_sym)
                .map(|s| s.as_str())
                == Some("$237")
        })
        .expect("page list container");
    let entries = page_list
        .unannotated()
        .as_struct()
        .unwrap()
        .get("$247")
        .unwrap()
        .as_list()
        .unwrap();
    let labels: Vec<&str> = entries
        .iter()
        .map(|e| {
            e.unannotated()
                .as_struct()
                .unwrap()
                .get("$241")
                .unwrap()
                .as_struct()
                .unwrap()
                .get("$244")
                .unwrap()
                .as_str()
                .unwrap()
        })
        .collect();
    assert_eq!(labels, ["i", "ii"]);
    // the landmarks container no longer has the page_list_entry rows
    let landmarks = containers
        .iter()
        .find(|c| {
            c.unannotated()
                .as_struct()
                .and_then(|s| s.get("$235"))
                .and_then(Value::as_sym)
                .map(|s| s.as_str())
                == Some("$236")
        })
        .unwrap();
    assert_eq!(
        landmarks
            .unannotated()
            .as_struct()
            .unwrap()
            .get("$247")
            .unwrap()
            .as_list()
            .unwrap()
            .len(),
        1
    );
}
