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
