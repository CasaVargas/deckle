// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::thumbnail::{cover_jpeg_from_kfx, install, render, sync, thumbnail_name};
use deckle_kfx::convert::CdeType;

fn mini_cover_png() -> Vec<u8> {
    let mut r =
        deckle_core::epub::EpubReader::open(&deckle_kfx::fixtures::path("deckle-mini.epub"))
            .unwrap();
    r.read_bytes("OEBPS/cover.png").unwrap()
}

#[test]
fn thumbnail_names_follow_calibre() {
    assert_eq!(
        thumbnail_name("B0ABCDDECK", CdeType::Ebok),
        "thumbnail_B0ABCDDECK_EBOK_portrait.jpg"
    );
    assert_eq!(
        thumbnail_name("B0ABCDDECK", CdeType::Pdoc),
        "thumbnail_B0ABCDDECK_PDOC_portrait.jpg"
    );
}

#[test]
fn render_scales_to_the_requested_height_as_jpeg() {
    let jpeg = render(&mini_cover_png(), 500).unwrap();
    assert!(jpeg.starts_with(&[0xFF, 0xD8, 0xFF]));
    let img = image::load_from_memory(&jpeg).unwrap();
    assert_eq!(img.height(), 500);
    assert_eq!(img.width(), 333); // 600x900 cover keeps its ratio
}

#[test]
fn install_writes_the_device_thumbnail_and_a_restore_copy_and_sync_restores_it() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("system/thumbnails")).unwrap();
    let jpeg = render(&mini_cover_png(), 500).unwrap();
    let name = thumbnail_name("B0TESTDECK", CdeType::Ebok);
    let written = install(root.path(), &name, &jpeg).unwrap();
    assert_eq!(written, root.path().join("system/thumbnails").join(&name));
    assert_eq!(std::fs::read(&written).unwrap(), jpeg);
    assert_eq!(
        std::fs::read(root.path().join("deckle-covers").join(&name)).unwrap(),
        jpeg
    );
    // Amazon's "no image" overwrite: a tiny file replaces ours; sync puts ours back.
    std::fs::write(&written, b"\xFF\xD8tiny").unwrap();
    assert_eq!(sync(root.path()).unwrap(), 1);
    assert_eq!(std::fs::read(&written).unwrap(), jpeg);
    // Deleted entirely: restored too. Unchanged: nothing to do.
    std::fs::remove_file(&written).unwrap();
    assert_eq!(sync(root.path()).unwrap(), 1);
    assert_eq!(sync(root.path()).unwrap(), 0);
}

#[test]
fn sync_without_a_cache_is_a_no_op() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(sync(root.path()).unwrap(), 0);
}

#[test]
fn cover_jpeg_is_read_back_from_a_kfx() {
    let kfx = std::fs::read(deckle_kfx::fixtures::path("deckle-mini-pdoc.golden.kfx")).unwrap();
    let cover = cover_jpeg_from_kfx(&kfx).expect("cover");
    assert!(cover.starts_with(&[0xFF, 0xD8, 0xFF, 0xE0]));
    assert_eq!(
        deckle_kfx::metadata::jpeg_dimensions(&cover),
        Some((600, 900))
    );
    assert!(cover_jpeg_from_kfx(b"CONT nope").is_none());
}
