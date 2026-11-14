// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::container::read_kfx;
use deckle_kfx::fixup::{Book, create_content_fragments, fix_structural};
use deckle_kfx::ion::Sym;
use deckle_kfx::kpf::read_kpf;

#[test]
fn content_fragments_and_storylines_match_golden() {
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
    assert!(create_content_fragments(&mut book));
    let content = Sym::new("$145");
    let ours: Vec<String> = book
        .fragments
        .all(&content)
        .map(|f| f.fid.clone().unwrap().0)
        .collect();
    assert_eq!(
        ours,
        (1..=9).map(|i| format!("content_{i}")).collect::<Vec<_>>()
    );
    for t in ["$145", "$259"] {
        let ty = Sym::new(t);
        for gf in g.fragments.all(&ty) {
            let o = book
                .fragments
                .get(&gf.ftype, gf.fid.as_ref())
                .unwrap_or_else(|| panic!("missing {t} {:?}", gf.fid));
            assert_eq!(o.value, gf.value, "{t} {:?}", gf.fid);
        }
    }
    assert!(book.symtab.id_of(&Sym::new("content_1")).is_some());
}

/// GHSA-vmh5-733r-v936: each `$146` reference to a `$608` structure copies it, so many
/// references to one large structure counted for nothing. The copies now count against the
/// same limit as the values decoded, and the book is refused past it.
#[test]
fn copies_of_one_structure_are_counted_and_refused_past_the_limit() {
    use deckle_kfx::fragment::Fragment;
    use deckle_kfx::ion::{IonError, Struct, Value};
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, "container");
    // 2 MB of text, referenced 200 times: 400 MB of copies, about 6.25 million units.
    book.fragments.push(Fragment::new(
        "$608",
        Some("big"),
        Value::String("x".repeat(2 << 20)),
    ));
    let mut s = Struct::new();
    s.0.push((
        Sym::new("$146"),
        Value::List(vec![Value::Symbol(Sym::new("big")); 200]),
    ));
    book.fragments
        .push(Fragment::new("$259", Some("many_refs"), Value::Struct(s)));
    let err = fix_structural(&mut book).unwrap_err();
    assert!(
        matches!(
            err,
            deckle_kfx::fixup::FixupError::Ion(IonError::TooManyValues { .. })
        ),
        "{err}"
    );
}

/// A structure referenced once, as Kindle Previewer writes them, is copied as before.
#[test]
fn a_structure_referenced_once_is_copied_in() {
    use deckle_kfx::fragment::Fragment;
    use deckle_kfx::ion::{Struct, Value};
    let kpf = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut book = Book::from_kpf(kpf, "container");
    book.fragments.push(Fragment::new(
        "$608",
        Some("small"),
        Value::String("hello".into()),
    ));
    let mut s = Struct::new();
    s.0.push((
        Sym::new("$146"),
        Value::List(vec![Value::Symbol(Sym::new("small"))]),
    ));
    book.fragments
        .push(Fragment::new("$259", Some("one_ref"), Value::Struct(s)));
    fix_structural(&mut book).unwrap();
    let f = book
        .fragments
        .get(&Sym::new("$259"), Some(&Sym::new("one_ref")))
        .unwrap();
    let Value::Struct(s) = &f.value else { panic!() };
    assert_eq!(
        s.get("$146"),
        Some(&Value::List(vec![Value::String("hello".into())]))
    );
}
