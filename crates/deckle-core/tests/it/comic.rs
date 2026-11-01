// SPDX-License-Identifier: GPL-3.0-or-later
//! A comic (CBZ or folder of page images) through import, inspect and normalize (fixed-layout
//! spec §11).

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use deckle_core::import::{import, needs_import};
use deckle_core::inspect::inspect;

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([90, 90, 90]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A 3-page manga CBZ (pages named so that only natural order gets them right).
fn manga(dir: &Path) -> PathBuf {
    let path = dir.join("manga.cbz");
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let files: [(&str, Vec<u8>); 4] = [
        (
            "ComicInfo.xml",
            b"<ComicInfo><Title>Night Train</Title><Manga>YesAndRightToLeft</Manga></ComicInfo>"
                .to_vec(),
        ),
        ("page10.png", png(30, 40)),
        ("page2.png", png(20, 30)),
        ("page1.png", png(10, 20)),
    ];
    for (n, bytes) in files {
        zw.start_file(n, zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(&bytes).unwrap();
    }
    zw.finish().unwrap();
    path
}

fn entries(epub: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(epub)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().unwrap().into_owned(), b)
        })
        .collect()
}

#[test]
fn a_cbz_imports_as_a_fixed_layout_right_to_left_book() {
    let dir = tempfile::tempdir().unwrap();
    let src = manga(dir.path());
    assert!(needs_import(&src));
    let imported = import(&src, dir.path(), "imported.epub")
        .expect("a comic is importable")
        .unwrap();
    let info = inspect(&imported.epub).unwrap();
    assert_eq!(info.title, "Night Train");
    assert!(info.fixed_layout && info.right_to_left);
    assert_eq!(
        info.spine,
        [
            "pages/p0001.xhtml",
            "pages/p0002.xhtml",
            "pages/p0003.xhtml"
        ]
    );
    assert_eq!(info.cover_href.as_deref(), Some("images/p0001.png"));
}

#[test]
fn a_reflowable_book_is_neither_fixed_layout_nor_right_to_left() {
    let info = inspect(&deckle_kfx::fixtures::path("deckle-mini.epub")).unwrap();
    assert!(!info.fixed_layout && !info.right_to_left);
}

#[test]
fn every_spine_item_pre_paginated_counts_as_fixed_layout() {
    let dir = tempfile::tempdir().unwrap();
    let imported = import(&manga(dir.path()), dir.path(), "imported.epub")
        .unwrap()
        .unwrap()
        .epub;
    // The same book with the layout on each itemref instead of in the metadata.
    let mut out =
        zip::ZipWriter::new(std::fs::File::create(dir.path().join("items.epub")).unwrap());
    for (name, bytes) in entries(&std::fs::read(&imported).unwrap()) {
        let bytes = if name == "OEBPS/content.opf" {
            String::from_utf8(bytes)
                .unwrap()
                .replace(
                    "<meta property=\"rendition:layout\">pre-paginated</meta>",
                    "",
                )
                .replace(
                    "<itemref ",
                    "<itemref properties=\"rendition:layout-pre-paginated\" ",
                )
                .into_bytes()
        } else {
            bytes
        };
        out.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(&bytes).unwrap();
    }
    out.finish().unwrap();
    assert!(
        inspect(&dir.path().join("items.epub"))
            .unwrap()
            .fixed_layout
    );
}

#[test]
fn a_comic_keeps_its_layout_and_every_page_through_the_reader_path() {
    let dir = tempfile::tempdir().unwrap();
    // `book_epub` is what a PocketBook or an EPUB folder gets: import, then normalize.
    let epub = deckle_core::reader::book_epub(&manga(dir.path()), &Default::default()).unwrap();
    let files = entries(&epub);
    let get = |name: &str| {
        files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
    };
    let opf = get("OEBPS/content.opf").unwrap();
    assert!(opf.contains("<meta property=\"rendition:layout\">pre-paginated</meta>"));
    assert!(opf.contains("page-progression-direction=\"rtl\""));
    for (i, size) in [
        (1, "width=10, height=20"),
        (2, "width=20, height=30"),
        (3, "width=30, height=40"),
    ] {
        let page = get(&format!("OEBPS/pages/p{i:04}.xhtml")).unwrap();
        assert!(page.contains(size), "page {i}: {page}");
        // Whatever normalize did to the cover, each page's image is still in the book.
        let src = page.split("src=\"../").nth(1).unwrap();
        let src = &src[..src.find('"').unwrap()];
        assert!(
            files.iter().any(|(n, _)| *n == format!("OEBPS/{src}")),
            "page {i} shows {src}, which is missing"
        );
    }
}

#[test]
fn a_folder_of_page_images_imports_but_a_notes_folder_stays_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let pages = dir.path().join("Picture Book");
    std::fs::create_dir(&pages).unwrap();
    std::fs::write(pages.join("01.png"), png(8, 8)).unwrap();
    std::fs::write(pages.join("02.png"), png(8, 8)).unwrap();
    let book = import(&pages, dir.path(), "pages.epub").unwrap().unwrap();
    let info = inspect(&book.epub).unwrap();
    assert!(info.fixed_layout && !info.right_to_left);
    assert_eq!(info.spine.len(), 2);

    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    std::fs::write(notes.join("a.md"), "# A note\n\nHello.\n").unwrap();
    std::fs::write(notes.join("photo.png"), png(8, 8)).unwrap();
    let book = import(&notes, dir.path(), "notes.epub").unwrap().unwrap();
    assert!(!inspect(&book.epub).unwrap().fixed_layout);
}

#[cfg(unix)] // the fake Previewer is a shell script
mod kindle {
    use std::path::{Path, PathBuf};

    use deckle_core::deliver::Target;
    use deckle_core::pipeline::{RunOptions, XRayMode, run};
    use deckle_kfx::convert::CdeType;

    pub(super) fn opts(target: Target) -> RunOptions {
        RunOptions {
            recaps: None,
            preset: None,
            cover: Default::default(),
            cde_type: CdeType::Pdoc,
            asin: None,
            pages: Some(None),
            target,
            // As the real Previewer 4.0.3 does with a fixed-layout book: a MOBI, no KPF.
            previewer: Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fake_previewer/fixed_layout.sh"),
            ),
            keep_work_dir: false,
            cde_override: None,
            amazon_match: false,
            generated_cover: Some(None),
            metadata_fill: false,
            typography: true,
            popup_notes: false,
            verified_real_asin: false,
            xray: XRayMode::Auto,
            llm: None,
            wikipedia: false,
            word_wise: None,
            replaces: Vec::new(),
            originals: None,
        }
    }

    fn fixed_layout_epub() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/comic/fixed-layout-rtl.epub")
    }

    #[test]
    fn a_fixed_layout_epub_goes_to_a_folder_as_a_stamped_azw3() {
        let out = tempfile::tempdir().unwrap();
        let report = run(
            &fixed_layout_epub(),
            &opts(Target::Folder(out.path().into())),
        );
        assert_eq!(report.error, None, "{report:?}");
        assert!(report.info.fixed_layout && report.info.right_to_left);
        let delivered = report.delivered.clone().unwrap();
        assert_eq!(delivered, out.path().join("Deckle - FXL Probe.azw3"));
        let bytes = std::fs::read(&delivered).unwrap();
        let meta = deckle_mobi::read_meta(&bytes).unwrap();
        assert!(meta.kf8 && meta.fixed_layout);
        // A folder gets PDOC with a generated ASIN, as a KFX would.
        assert_eq!(meta.cde.as_deref(), Some("PDOC"));
        assert_eq!(meta.asin, report.asin);
        assert!(report.asin.as_deref().is_some_and(|a| a.starts_with("B0")));
        assert_eq!(report.kfx, None);
        assert_eq!(report.pages, None);
        // (The fixture's DOCTYPE adds audit warnings of its own; Previewer adds none.)
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w == "fixed-layout book: typography, X-Ray, page numbers skipped"),
            "{:?}",
            report.warnings
        );
        assert!(!report.warnings.iter().any(|w| w.starts_with("previewer:")));
        for stage in ["normalize", "convert", "repackage", "deliver"] {
            assert!(report.stage_ms.contains_key(stage), "{stage}");
        }
    }

    #[test]
    fn a_fixed_layout_book_skips_chapter_recaps() {
        let out = tempfile::tempdir().unwrap();
        let mut o = opts(Target::Folder(out.path().into()));
        o.recaps = Some(out.path().join("cache"));
        o.llm = Some(deckle_core::xray::LlmConfig {
            provider: deckle_xray::llm::Provider::OpenAi,
            model: "test-model".into(),
            endpoint: "http://127.0.0.1:1/v1".into(),
            api_key: String::new(),
        });
        let report = run(&fixed_layout_epub(), &o);
        assert_eq!(report.error, None, "{report:?}");
        assert!(
            report.warnings.iter().any(|w| w
                == "fixed-layout book: typography, X-Ray, page numbers, chapter recaps skipped"),
            "{:?}",
            report.warnings
        );
        assert!(!report.stage_ms.contains_key("recaps"));
        assert_eq!(report.recaps, None);
    }

    #[test]
    fn a_cbz_reaches_a_mass_storage_kindle_as_ebok_with_a_thumbnail() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("documents")).unwrap();
        std::fs::create_dir_all(root.path().join("system/thumbnails")).unwrap();
        std::fs::write(
            root.path().join("system/version.txt"),
            "Kindle 5.16.2.1.1 (4184940002)\n",
        )
        .unwrap();
        let device = deckle_core::deliver::detect_in(&[root.path().to_path_buf()]).remove(0);
        let src = super::manga(root.path());
        let mut o = opts(Target::MassStorage(device));
        o.typography = false;
        o.xray = XRayMode::Off;
        o.pages = None;
        let report = run(&src, &o);
        assert_eq!(report.error, None, "{report:?}");
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        let delivered = report.delivered.clone().unwrap();
        assert_eq!(delivered, root.path().join("documents/Night Train.azw3"));
        let meta = deckle_mobi::read_meta(&std::fs::read(&delivered).unwrap()).unwrap();
        assert_eq!(meta.cde.as_deref(), Some("EBOK"));
        let asin = report.asin.clone().unwrap();
        let thumb: &Path = &root.path().join(format!(
            "system/thumbnails/thumbnail_{asin}_EBOK_portrait.jpg"
        ));
        assert!(thumb.exists(), "{thumb:?}");
        let img = image::load_from_memory(&std::fs::read(thumb).unwrap()).unwrap();
        assert_eq!(img.height(), 500);
    }

    /// A mass-storage Kindle in a temp folder.
    fn kindle_root(root: &Path) -> Target {
        std::fs::create_dir_all(root.join("documents")).unwrap();
        std::fs::create_dir_all(root.join("system/thumbnails")).unwrap();
        std::fs::write(
            root.join("system/version.txt"),
            "Kindle 5.16.2.1.1 (4184940002)\n",
        )
        .unwrap();
        Target::MassStorage(deckle_core::deliver::detect_in(&[root.to_path_buf()]).remove(0))
    }

    /// The page records (11–13) of a delivered AZW3.
    fn pages(azw3: &Path) -> Vec<Vec<u8>> {
        let bytes = std::fs::read(azw3).unwrap();
        let db = deckle_mobi::palmdb::PalmDb::parse(&bytes).unwrap();
        (11..14).map(|i| db.record(i).to_vec()).collect()
    }

    #[test]
    fn only_deckles_own_comics_are_fitted_to_the_kindle() {
        let mut o = opts(Target::Folder("/unused".into()));
        o.typography = false;
        o.xray = XRayMode::Off;
        o.pages = None;
        let previewers = {
            let joint = std::fs::read(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../fixtures/comic/fixed-layout-rtl.mobi"),
            )
            .unwrap();
            let stamp = deckle_mobi::azw3::Stamp {
                cde: "EBOK",
                asin: "B0X",
            };
            let azw3 = deckle_mobi::azw3::kf8_only(&joint, &stamp).unwrap();
            let db = deckle_mobi::palmdb::PalmDb::parse(&azw3).unwrap();
            (11..14).map(|i| db.record(i).to_vec()).collect::<Vec<_>>()
        };

        // A CBZ is Deckle's: its pages are re-encoded for this (grey) Kindle.
        let a = tempfile::tempdir().unwrap();
        o.target = kindle_root(a.path());
        let report = run(&super::manga(a.path()), &o);
        assert_eq!(report.error, None, "{report:?}");
        assert!(report.stage_ms.contains_key("fit"));
        let fitted = pages(&report.delivered.unwrap());
        for (page, original) in fitted.iter().zip(&previewers) {
            assert_ne!(page, original);
            let img = image::load_from_memory(page).unwrap().to_rgb8();
            assert!(img.width() <= 1264 && img.height() <= 1680);
            assert!(img.pixels().all(|p| p[0] == p[1] && p[1] == p[2]), "grey");
        }

        // A publisher's fixed-layout EPUB goes as Previewer made it.
        let b = tempfile::tempdir().unwrap();
        o.target = kindle_root(b.path());
        let report = run(&fixed_layout_epub(), &o);
        assert_eq!(report.error, None, "{report:?}");
        assert!(!report.stage_ms.contains_key("fit"));
        assert_eq!(pages(&report.delivered.unwrap()), previewers);
    }

    #[test]
    fn a_fixed_layout_kindle_book_goes_as_it_is_without_previewer() {
        let dir = tempfile::tempdir().unwrap();
        let joint = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/comic/fixed-layout-rtl.mobi"),
        )
        .unwrap();
        let stamp = deckle_mobi::azw3::Stamp {
            cde: "EBOK",
            asin: "B0STORE001",
        };
        let azw3 = dir.path().join("comic.azw3");
        std::fs::write(&azw3, deckle_mobi::azw3::kf8_only(&joint, &stamp).unwrap()).unwrap();
        let joint_path = dir.path().join("comic.mobi");
        std::fs::write(&joint_path, &joint).unwrap();

        for input in [&azw3, &joint_path] {
            let out = tempfile::tempdir().unwrap();
            let mut o = opts(Target::Folder(out.path().into()));
            // No Previewer anywhere: it isn't needed.
            o.previewer = Some(PathBuf::from("/no/such/previewer"));
            o.typography = false;
            o.xray = XRayMode::Off;
            o.pages = None;
            let report = run(input, &o);
            assert_eq!(report.error, None, "{report:?}");
            assert!(!report.stage_ms.contains_key("convert"));
            assert!(
                report.warnings[0].contains("sent as it is"),
                "{:?}",
                report.warnings
            );
            let delivered = report.delivered.unwrap();
            assert_eq!(delivered, out.path().join("Deckle - FXL Probe.azw3"));
            let meta = deckle_mobi::read_meta(&std::fs::read(&delivered).unwrap()).unwrap();
            assert!(meta.fixed_layout && meta.kf8);
            // Stamped by the cover rule, like any book sent to a folder.
            assert_eq!(meta.cde.as_deref(), Some("PDOC"));
        }

        // DRM is refused before anything else.
        let locked = dir.path().join("locked.azw3");
        std::fs::write(
            &locked,
            deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
                title: "Locked",
                kf8: true,
                fixed_layout: true,
                drm: true,
                ..Default::default()
            }),
        )
        .unwrap();
        let report = run(&locked, &opts(Target::Folder(dir.path().into())));
        assert!(report.error.unwrap().contains("DRM"));

        // Other readers can't take it: there's no EPUB to make.
        let err = deckle_core::reader::book_epub(&azw3, &Default::default()).unwrap_err();
        assert!(err.contains("can't make an EPUB"), "{err}");
    }

    #[test]
    fn a_reflowable_book_refused_enhanced_typesetting_still_fails() {
        // The fixed-layout fake hands back a MOBI; a reflowable book mustn't ship it.
        let out = tempfile::tempdir().unwrap();
        let report = run(
            &deckle_kfx::fixtures::path("deckle-mini.epub"),
            &opts(Target::Folder(out.path().into())),
        );
        let err = report.error.expect("error");
        assert!(err.starts_with("convert: "), "{err}");
        assert!(report.delivered.is_none());
    }
}

#[test]
#[cfg(unix)]
fn a_queued_fixed_layout_book_still_finishes_as_azw3() {
    use deckle_core::deliver::{Conn, Target};
    use deckle_core::pipeline::{Prepared, Wishes, finish};
    // A queue entry keeps Previewer's output as `book.kpf`, whatever it is.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("book.kpf");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/comic/fixed-layout-rtl.mobi"),
        &entry,
    )
    .unwrap();
    let info = deckle_core::inspect::BookInfo {
        title: "Night Train".into(),
        fixed_layout: true,
        ..Default::default()
    };
    let prepared = Prepared::from_parts(
        entry.clone(),
        info,
        entry,
        Wishes::default(),
        Some("Night Train.kfx".into()),
    );
    let out = dir.path().join("out");
    let target = Target::Folder(out.clone());
    let mut opts = kindle::opts(target.clone());
    opts.xray = deckle_core::pipeline::XRayMode::Off;
    let report = finish(&prepared, &mut Conn::new(&target), &opts, &mut |_| {});
    assert_eq!(report.error, None, "{report:?}");
    assert_eq!(report.delivered, Some(out.join("Night Train.azw3")));
}

#[test]
#[cfg(unix)]
fn a_queued_comic_flushes_as_azw3_and_so_does_its_fallback() {
    use deckle_core::deliver::{Conn, Target};
    use deckle_core::pipeline::{Prepared, Wishes};
    use deckle_core::queue::{Finishing, Queue};
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("previewer.mobi");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/comic/fixed-layout-rtl.mobi"),
        &mobi,
    )
    .unwrap();
    let prepared = Prepared::from_parts(
        mobi.clone(),
        deckle_core::inspect::BookInfo {
            title: "Night Train".into(),
            fixed_layout: true,
            ..Default::default()
        },
        mobi,
        Wishes::default(),
        None,
    );
    let finishing = Finishing {
        profile: None,
        word_wise: deckle_core::wordwise::WordWiseOptions {
            level: 5,
            database: None,
            cache_dir: dir.path().join("cache"),
        },
    };
    let q = Queue::new(dir.path().join("outbox"));
    let flush = |kindle: &Path| {
        q.flush_with(
            &mut Conn::new(&Target::Folder(kindle.to_path_buf())),
            None,
            &finishing,
            &mut |_, _, _| {},
        )
        .unwrap()
    };

    // Finished for the Kindle that takes it.
    q.add_prepared(&prepared, "Night Train.kfx", None).unwrap();
    let kindle = dir.path().join("kindle");
    let r = flush(&kindle);
    assert_eq!(r.delivered, [kindle.join("Night Train.azw3")]);
    assert!(deckle_mobi::is_mobi(
        &std::fs::read(&r.delivered[0]).unwrap()
    ));

    // Finishing fails: the fallback goes, still as an AZW3, never under a .kfx name.
    let entry = q.add_prepared(&prepared, "Night Train.kfx", None).unwrap();
    std::fs::write(entry.join("book.kpf"), b"not a KPF").unwrap();
    let other = dir.path().join("other");
    let r = flush(&other);
    assert_eq!(r.delivered, [other.join("Night Train.azw3")]);
    assert!(!other.join("Night Train.kfx").exists());
    assert!(
        r.warnings.iter().any(|w| w.contains("not finished")),
        "{:?}",
        r.warnings
    );
}
