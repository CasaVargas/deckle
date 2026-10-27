// SPDX-License-Identifier: GPL-3.0-or-later
//! Structural fix-ups must reproduce kfxlib's fragments exactly (compared as Ion values).
use std::collections::BTreeSet;

use deckle_kfx::container::read_kfx;
use deckle_kfx::fixup::{Book, fix_structural};
use deckle_kfx::ion::{Sym, Value};
use deckle_kfx::kpf::read_kpf;

fn golden() -> deckle_kfx::container::KfxDoc {
    read_kfx(
        &std::fs::read(deckle_kfx::fixtures::path(
            "deckle-mini-pdoc-nopages.golden.kfx",
        ))
        .unwrap(),
    )
    .unwrap()
}

fn fixed_book() -> (Book, deckle_kfx::container::KfxDoc) {
    let g = golden();
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, &g.container_id);
    fix_structural(&mut book).unwrap();
    (book, g)
}

#[test]
fn structural_fixups_match_golden_for_585_164_157_538() {
    let (book, g) = fixed_book();
    for t in ["$585", "$164", "$157", "$538"] {
        let ty = Sym::new(t);
        for gf in g.fragments.all(&ty) {
            let ours = book
                .fragments
                .get(&gf.ftype, gf.fid.as_ref())
                .unwrap_or_else(|| panic!("missing {t} {:?}", gf.fid));
            assert_eq!(ours.value, gf.value, "{t} {:?}", gf.fid);
        }
    }
}

/// `$490` must match except the three keys Task 13 stamps, and key order (Task 13 sorts).
#[test]
fn structural_fixups_fill_490_metadata_defaults() {
    let (book, g) = fixed_book();
    let pairs = |v: &Value| -> Vec<(String, BTreeSet<String>)> {
        v.as_struct()
            .unwrap()
            .get("$491")
            .unwrap()
            .as_list()
            .unwrap()
            .iter()
            .map(|cat| {
                let cat = cat.as_struct().unwrap();
                let name = cat.get("$495").unwrap().as_str().unwrap().to_owned();
                let kvs = cat
                    .get("$258")
                    .unwrap()
                    .as_list()
                    .unwrap()
                    .iter()
                    .filter_map(|kv| {
                        let kv = kv.as_struct().unwrap();
                        let k = kv.get("$492").unwrap().as_str().unwrap();
                        if ["ASIN", "content_id", "cde_content_type"].contains(&k) {
                            return None;
                        }
                        Some(format!("{k}={:?}", kv.get("$307").unwrap()))
                    })
                    .collect();
                (name, kvs)
            })
            .collect()
    };
    let ours = pairs(&book.fragments.get_single(&Sym::new("$490")).unwrap().value);
    let theirs = pairs(&g.fragments.get_single(&Sym::new("$490")).unwrap().value);
    assert_eq!(ours.len(), theirs.len(), "category count");
    for (o, t) in ours.iter().zip(theirs.iter()) {
        assert_eq!(o.0, t.0);
        // the golden was built with authors=[] (kfxlib drops existing authors); we keep them
        let o_no_author: BTreeSet<_> =
            o.1.iter()
                .filter(|s| !s.starts_with("author="))
                .cloned()
                .collect();
        assert_eq!(o_no_author, t.1, "category {}", o.0);
    }
    // our fix-up keeps the KPF's author since no metadata was applied yet
    assert!(
        ours[0]
            .1
            .iter()
            .any(|s| s == "author=String(\"Jane Testwright\")"),
        "{:?}",
        ours[0].1
    );
}

#[test]
fn floats_become_decimals_with_six_significant_digits() {
    use deckle_kfx::fixup::float_to_decimal;
    use deckle_kfx::ion::Decimal;
    assert_eq!(
        float_to_decimal(1.2000000476837158),
        Decimal {
            coefficient: 12,
            exponent: -1
        }
    );
    assert_eq!(
        float_to_decimal(0.5583333373069763),
        Decimal {
            coefficient: 558333,
            exponent: -6
        }
    );
    assert_eq!(
        float_to_decimal(1.0),
        Decimal {
            coefficient: 1,
            exponent: 0
        }
    );
    assert_eq!(
        float_to_decimal(0.0),
        Decimal {
            coefficient: 0,
            exponent: 0
        }
    );
    assert_eq!(
        float_to_decimal(0.0004),
        Decimal {
            coefficient: 0,
            exponent: 0
        }
    ); // |x| < 0.001 -> 0
    assert_eq!(
        float_to_decimal(100.0),
        Decimal {
            coefficient: 100,
            exponent: 0
        }
    );
    assert_eq!(
        float_to_decimal(1234567.0),
        Decimal {
            coefficient: 123457,
            exponent: 1
        }
    );
    assert_eq!(
        float_to_decimal(-1.5),
        Decimal {
            coefficient: -15,
            exponent: -1
        }
    );
}
