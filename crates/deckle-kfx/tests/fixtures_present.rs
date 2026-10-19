// SPDX-License-Identifier: GPL-3.0-or-later
#[test]
fn mini_fixtures_exist_with_expected_sizes() {
    let p = deckle_kfx::fixtures::path("deckle-mini.kpf");
    assert_eq!(std::fs::metadata(&p).unwrap().len(), 19241);
    let g = deckle_kfx::fixtures::path("deckle-mini-pdoc-nopages.golden.kfx");
    assert_eq!(std::fs::metadata(&g).unwrap().len(), 93845);
}
