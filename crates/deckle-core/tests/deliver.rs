// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::deliver::{Target, deliver, detect_in, device_filename};
use deckle_core::inspect::{BookInfo, Creator};

fn info(title: &str, author: &str) -> BookInfo {
    BookInfo {
        title: title.into(),
        creators: vec![Creator {
            name: author.into(),
            file_as: None,
        }],
        language: "en".into(),
        ..Default::default()
    }
}

#[test]
fn device_filename_is_ascii_safe_and_sort_ordered() {
    let name = device_filename(&info("Ærø: “Hello”/World?", "Jane Testwright"));
    assert_eq!(name, "Testwright, Jane - AEro_ _Hello__World_.kfx");
    let plain = device_filename(&info("Pride and Prejudice", "Jane Austen"));
    assert_eq!(plain, "Austen, Jane - Pride and Prejudice.kfx");
    let no_author = device_filename(&BookInfo {
        title: "Anon".into(),
        ..Default::default()
    });
    assert_eq!(no_author, "Anon.kfx");
    let long = device_filename(&info(&"x".repeat(300), "A B"));
    assert!(long.len() <= 120, "{}", long.len());
    assert!(long.ends_with(".kfx"));
}

#[test]
fn file_as_wins_over_derived_sort_name() {
    let mut i = info("T", "Jane Testwright");
    i.creators[0].file_as = Some("Testwright, J.".into());
    assert_eq!(device_filename(&i), "Testwright, J. - T.kfx");
}

#[test]
fn deliver_to_folder_writes_the_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = deliver(
        b"KFXDATA",
        &info("A Book", "Some Author"),
        &Target::Folder(dir.path().to_path_buf()),
    )
    .unwrap();
    assert_eq!(path, dir.path().join("Author, Some - A Book.kfx"));
    assert_eq!(std::fs::read(&path).unwrap(), b"KFXDATA");
}

#[test]
fn mass_storage_kindle_is_detected_and_written_under_documents() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("documents")).unwrap();
    std::fs::create_dir_all(root.path().join("system/thumbnails")).unwrap();
    std::fs::write(
        root.path().join("system/version.txt"),
        "Kindle 5.16.2.1.1 (4184940002)\n",
    )
    .unwrap();
    let not_kindle = tempfile::tempdir().unwrap();
    let devices = detect_in(&[root.path().to_path_buf(), not_kindle.path().to_path_buf()]);
    assert_eq!(devices.len(), 1);
    let d = &devices[0];
    assert_eq!(d.root, root.path());
    assert!(d.thumbnails_writable);
    assert!(d.model_hint.starts_with("Kindle 5.16.2.1.1"));
    let path = deliver(b"KFX", &info("B", "C D"), &Target::MassStorage(d.clone())).unwrap();
    assert_eq!(path, root.path().join("documents/D, C - B.kfx"));
    assert!(path.exists());
}

#[test]
fn mass_storage_delivery_installs_the_thumbnail_when_asked() {
    use deckle_core::deliver::DeliverOptions;
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("documents")).unwrap();
    std::fs::create_dir_all(root.path().join("system/thumbnails")).unwrap();
    let d = detect_in(&[root.path().to_path_buf()]).remove(0);
    let opts = DeliverOptions {
        thumbnail: Some((
            "thumbnail_B0XXXXDECK_EBOK_portrait.jpg".into(),
            b"\xFF\xD8\xFF\xE0jpeg".to_vec(),
        )),
        sidecars: Vec::new(),
    };
    let path = deckle_core::deliver::deliver_with(
        b"KFX",
        &info("T", "A B"),
        &Target::MassStorage(d),
        &opts,
    )
    .unwrap();
    assert!(path.exists());
    let thumb = root
        .path()
        .join("system/thumbnails/thumbnail_B0XXXXDECK_EBOK_portrait.jpg");
    assert_eq!(std::fs::read(&thumb).unwrap(), b"\xFF\xD8\xFF\xE0jpeg");
    assert!(
        root.path()
            .join("deckle-covers/thumbnail_B0XXXXDECK_EBOK_portrait.jpg")
            .exists()
    );
}

#[test]
fn folder_delivery_ignores_thumbnails() {
    use deckle_core::deliver::DeliverOptions;
    let dir = tempfile::tempdir().unwrap();
    let opts = DeliverOptions {
        thumbnail: Some(("t.jpg".into(), vec![1])),
        sidecars: Vec::new(),
    };
    deckle_core::deliver::deliver_with(
        b"KFX",
        &info("T", "A B"),
        &Target::Folder(dir.path().to_path_buf()),
        &opts,
    )
    .unwrap();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn sidecars_land_in_the_sdr_folder_next_to_the_book() {
    use deckle_core::deliver::DeliverOptions;
    let opts = DeliverOptions {
        thumbnail: None,
        sidecars: vec![("XRAY.entities.B0ABCDDECK.asc".into(), b"sqlite".to_vec())],
    };
    // folder export
    let out = tempfile::tempdir().unwrap();
    let path = deckle_core::deliver::deliver_with(
        b"KFX",
        &info("Title", "A B"),
        &Target::Folder(out.path().to_path_buf()),
        &opts,
    )
    .unwrap();
    let sdr = path.with_extension("sdr");
    assert_eq!(sdr.file_name().unwrap(), "B, A - Title.sdr");
    assert_eq!(
        std::fs::read(sdr.join("XRAY.entities.B0ABCDDECK.asc")).unwrap(),
        b"sqlite"
    );
    // mass storage: under documents/
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("documents")).unwrap();
    std::fs::create_dir_all(root.path().join("system/thumbnails")).unwrap();
    let d = detect_in(&[root.path().to_path_buf()]).remove(0);
    let path = deckle_core::deliver::deliver_with(
        b"KFX",
        &info("Title", "A B"),
        &Target::MassStorage(d),
        &opts,
    )
    .unwrap();
    assert!(
        root.path()
            .join("documents/B, A - Title.sdr/XRAY.entities.B0ABCDDECK.asc")
            .exists(),
        "{path:?}"
    );
}
