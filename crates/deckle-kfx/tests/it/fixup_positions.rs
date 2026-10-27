// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::container::read_kfx;
use deckle_kfx::fixup::{
    Book, ContentChunk, cpython_int_set_order, create_content_fragments,
    create_position_and_location_maps, fix_structural, generate_approximate_locations,
};
use deckle_kfx::ion::Sym;
use deckle_kfx::kpf::read_kpf;

#[test]
fn position_location_and_capability_fragments_match_golden() {
    let g = read_kfx(
        &std::fs::read(deckle_kfx::fixtures::path(
            "deckle-mini-pdoc-nopages.golden.kfx",
        ))
        .unwrap(),
    )
    .unwrap();
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, &g.container_id);
    fix_structural(&mut book).unwrap();
    let has_text = create_content_fragments(&mut book);
    let report = create_position_and_location_maps(&mut book, has_text).unwrap();
    assert!(!report.has_spim);
    assert!(!report.has_pid_offset);
    assert!(
        !report.generated_location_map,
        "the KPF already carries a $550"
    );
    for t in ["$264", "$265", "$550", "$395", "$593"] {
        let ty = Sym::new(t);
        let gf = g
            .fragments
            .get_single(&ty)
            .unwrap_or_else(|| panic!("golden lacks {t}"));
        let o = book
            .fragments
            .get_single(&ty)
            .unwrap_or_else(|| panic!("missing {t}"));
        assert_eq!(o.value, gf.value, "{t}");
    }
    // the KPF-only mapping fragments are gone
    for t in ["$609", "$610", "$611"] {
        assert_eq!(
            book.fragments.all(&Sym::new(t)).count(),
            0,
            "{t} should be removed"
        );
    }
}

#[test]
fn approximate_locations_every_110_positions() {
    let a = Sym::new("s");
    let chunks = vec![
        ContentChunk {
            pid: 0,
            eid: 1,
            eid_offset: 0,
            length: 50,
            section: a.clone(),
        },
        ContentChunk {
            pid: 50,
            eid: 2,
            eid_offset: 0,
            length: 200,
            section: a.clone(),
        },
        ContentChunk {
            pid: 250,
            eid: 3,
            eid_offset: 0,
            length: 10,
            section: a.clone(),
        },
    ];
    let locs = generate_approximate_locations(&chunks);
    let got: Vec<(i64, i64, i64)> = locs.iter().map(|l| (l.pid, l.eid, l.eid_offset)).collect();
    assert_eq!(got, vec![(0, 1, 0), (110, 2, 60), (220, 2, 170)]);
}

#[test]
fn cpython_set_order_is_reproduced_for_small_int_sets() {
    assert_eq!(
        cpython_int_set_order(&[904, 887, 892, 888, 885, 893, 894, 895]),
        vec![904, 885, 887, 888, 892, 893, 894, 895]
    );
    assert_eq!(cpython_int_set_order(&[911, 901]), vec![901, 911]);
    assert_eq!(
        cpython_int_set_order(&[896, 897, 898, 899, 900, 869, 873, 875]),
        vec![896, 897, 898, 899, 900, 869, 873, 875]
    );
    // duplicates collapse, order of first insertion irrelevant to the table layout
    // 5 and 13 collide in an 8-slot table; CPython probes to slot 2 for 13 (verified with python3)
    assert_eq!(cpython_int_set_order(&[5, 13, 5]), vec![13, 5]);
    // 20 elements force two resizes (8 -> 32 -> 128); 860..879 land in ascending slots
    let twenty: Vec<i64> = (860..880).collect();
    assert_eq!(cpython_int_set_order(&twenty), twenty);
}
