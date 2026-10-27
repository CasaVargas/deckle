// SPDX-License-Identifier: GPL-3.0-or-later
//! End-to-end oracle: our KPF -> KFX output must equal kfxlib's byte for byte.
use deckle_kfx::container::read_kfx;
use deckle_kfx::convert::{CdeType, ConvertOptions, ConvertOutput, kpf_to_kfx};

fn opts_for(golden: &str) -> (ConvertOptions, Vec<u8>) {
    let gbytes = std::fs::read(deckle_kfx::fixtures::path(golden)).unwrap();
    let g = read_kfx(&gbytes).unwrap();
    let opts = ConvertOptions {
        cde_type: CdeType::Pdoc,
        asin: Some("B0DECK000002".into()),
        // the goldens were made with kfxlib authors=[], which removes the KPF's author
        authors: Some(Vec::new()),
        container_id: Some(g.container_id.clone()),
        creator: Some(g.creator.clone()),
        ..ConvertOptions::default()
    };
    (opts, gbytes)
}

#[test]
fn mini_kpf_to_kfx_matches_nopages_golden_byte_for_byte() {
    let (opts, gbytes) = opts_for("deckle-mini-pdoc-nopages.golden.kfx");
    let out = kpf_to_kfx(&deckle_kfx::fixtures::path("deckle-mini.kpf"), &opts).unwrap();
    assert_eq!(out.pages, None);
    assert_eq!(out.bytes.len(), gbytes.len());
    assert_eq!(out.bytes, gbytes);
}

#[test]
fn mini_kpf_to_kfx_with_pages_matches_pages_golden() {
    let (mut opts, gbytes) = opts_for("deckle-mini-pdoc.golden.kfx");
    opts.approximate_pages = Some(None);
    let out = kpf_to_kfx(&deckle_kfx::fixtures::path("deckle-mini.kpf"), &opts).unwrap();
    assert_eq!(out.pages, Some(45));
    assert_eq!(out.bytes, gbytes);
}

#[test]
fn over_16mib_output_is_a_warning_not_an_error() {
    assert_eq!(
        ConvertOutput::size_warning(25_467_398),
        Some("KFX is 25.5 MB; some devices lock up above 16 MB".to_string())
    );
    assert_eq!(ConvertOutput::size_warning(10 * 1024 * 1024), None);
}

#[test]
fn default_options_generate_a_recognizable_container_id() {
    let out = kpf_to_kfx(
        &deckle_kfx::fixtures::path("deckle-mini.kpf"),
        &ConvertOptions::default(),
    )
    .unwrap();
    let doc = read_kfx(&out.bytes).unwrap();
    assert!(doc.container_id.starts_with("CR!"));
    assert_eq!(doc.container_id.len(), 31);
    assert!(doc.creator.starts_with("deckle-"));
}
