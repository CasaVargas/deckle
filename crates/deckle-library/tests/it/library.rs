// SPDX-License-Identifier: GPL-3.0-or-later
//! The library: folders and sidecars (library design §6).

use std::io::Write;
use std::path::{Path, PathBuf};

use deckle_core::deliver::{Device, Target};
use deckle_core::pipeline::RunReport;
use deckle_library::{FILE_NAME, Library, LibraryConfig, SCHEMA, auto_add};

/// A minimal valid EPUB 3.
fn epub(path: &Path, id: &str, title: &str, author: &str) -> PathBuf {
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:identifier id="uid">{id}</dc:identifier><dc:title>{title}</dc:title>
<dc:creator>{author}</dc:creator><dc:language>en</dc:language>
<meta property="dcterms:modified">2026-10-21T00:00:00Z</meta>
</metadata>
<manifest>
<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
<item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
</manifest>
<spine><itemref idref="c1"/></spine>
</package>"#
    );
    let page = |body: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\" \
             xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>{title}</title></head><body>{body}</body></html>"
        )
    };
    let f = std::fs::File::create(path).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("mimetype", stored).unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    for (name, body) in [
        (
            "META-INF/container.xml",
            r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_owned(),
        ),
        ("content.opf", opf),
        (
            "nav.xhtml",
            page("<nav epub:type=\"toc\"><ol><li><a href=\"c1.xhtml\">One</a></li></ol></nav>"),
        ),
        ("c1.xhtml", page("<p>Text.</p>")),
    ] {
        z.start_file(name, stored).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    }
    z.finish().unwrap();
    path.to_path_buf()
}

/// `path` under `root`, `/`-separated on every OS.
fn rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap()
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn files_under(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                out.push(rel(&p, root));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn add_files_a_book_under_author_and_title_with_the_pipelines_asin() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let src = deckle_kfx::fixtures::path("deckle-mini.epub");
    let added = lib.add(&src).unwrap();
    assert!(added.new && added.copied);
    let book = &added.book;
    assert_eq!(
        book.dir,
        lib.root
            .join("Testwright, Jane")
            .join("Deckle Mini Fixture")
    );
    assert_eq!(
        files_under(&lib.root),
        [
            "Testwright, Jane/Deckle Mini Fixture/deckle-mini.epub",
            "Testwright, Jane/Deckle Mini Fixture/deckle.json"
        ]
    );
    let info = deckle_core::inspect::inspect(&src).unwrap();
    let asin = deckle_kfx::metadata::generate_asin(&deckle_core::pipeline::asin_seed(&info));
    assert_eq!(book.sidecar.asin, asin);
    assert!(asin.ends_with("DECK"));
    let m = &book.sidecar.metadata;
    assert_eq!(m.title, "Deckle Mini Fixture");
    assert_eq!(m.series.as_ref().unwrap().name, "Fixture Series");
    assert_eq!(book.sidecar.schema, SCHEMA);
    assert_eq!(lib.list(), vec![book.clone()]);
    assert_eq!(lib.find_by_asin(&asin).unwrap().dir, book.dir);
}

#[test]
fn re_adding_updates_in_place_and_keeps_sends() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let v1 = epub(
        &tmp.path().join("draft.epub"),
        "urn:x:1",
        "The Roof",
        "Ada Writer",
    );
    let mut book = lib.add(&v1).unwrap().book;
    lib.record_send(
        &mut book,
        deckle_library::SendRecord {
            device: "ms:Kindle".into(),
            file: "Writer, Ada - The Roof.kfx".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let added_at = book.sidecar.added.clone();
    // Same identifier, new title and file name: the same book, edited.
    let v2 = epub(
        &tmp.path().join("final.epub"),
        "urn:x:1",
        "The Roof, Revised",
        "Ada Writer",
    );
    let again = lib.add(&v2).unwrap();
    assert!(!again.new && again.copied);
    assert_eq!(again.book.dir, book.dir, "stays in its folder");
    assert_eq!(again.book.sidecar.source, "final.epub");
    assert!(!book.dir.join("draft.epub").exists(), "old source replaced");
    assert_eq!(again.book.sidecar.metadata.title, "The Roof, Revised");
    assert_eq!(again.book.sidecar.added, added_at);
    assert_eq!(again.book.sidecar.sends.len(), 1);
    // Unchanged bytes: nothing copied.
    assert!(!lib.add(&v2).unwrap().copied);
    assert_eq!(lib.list().len(), 1);
}

#[test]
fn a_different_book_with_the_same_title_gets_its_own_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let a = epub(&tmp.path().join("a.epub"), "urn:x:a", "Poems", "Ada Writer");
    let b = epub(&tmp.path().join("b.epub"), "urn:x:b", "Poems", "Ada Writer");
    let a = lib.add(&a).unwrap().book;
    let b = lib.add(&b).unwrap().book;
    assert_eq!(a.dir, lib.root.join("Writer, Ada/Poems"));
    assert_eq!(b.dir, lib.root.join("Writer, Ada/Poems (2)"));
    assert_eq!(lib.list().len(), 2);
}

#[test]
fn no_author_and_unsafe_titles_still_make_good_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let src = epub(
        &tmp.path().join("x.epub"),
        "urn:x:q",
        "..What? A/B: Test.",
        "",
    );
    let book = lib.add(&src).unwrap().book;
    assert_eq!(rel(&book.dir, &lib.root), "Unknown Author/What_ A_B_ Test");
}

#[test]
fn a_word_document_is_kept_as_it_is() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let docx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/deckle-mini.docx");
    let book = lib.add(&docx).unwrap().book;
    assert_eq!(book.sidecar.source, "deckle-mini.docx");
    assert_eq!(book.sidecar.metadata.title, "Deckle Mini Document");
    assert!(lib.add(&tmp.path().join("x.kfx")).is_err(), "only sources");
}

#[test]
fn a_text_file_is_kept_as_it_is() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let txt =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/txt/gutenberg-mini.txt");
    assert!(deckle_library::is_source(&txt));
    let book = lib.add(&txt).unwrap().book;
    assert_eq!(book.sidecar.source, "gutenberg-mini.txt");
    assert_eq!(book.sidecar.metadata.title, "The Deckle Mini Novel");
}

#[test]
fn a_pdf_is_kept_as_it_is() {
    // Needs PDFium: tools/fetch-pdfium.sh, or DECKLE_PDFIUM.
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf/book.pdf");
    assert!(deckle_library::is_source(&pdf));
    let book = lib.add(&pdf).unwrap().book;
    assert_eq!(book.sidecar.source, "book.pdf");
    assert_eq!(book.sidecar.metadata.title, "A Small Book");
}

#[test]
fn scan_adopts_bare_and_loose_books_and_reports_problems() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Library");
    let lib = Library::new(&root);
    // A tracked book whose source then goes missing.
    let gone = lib
        .add(&epub(
            &tmp.path().join("g.epub"),
            "urn:x:g",
            "Gone",
            "Ada Writer",
        ))
        .unwrap()
        .book;
    std::fs::remove_file(gone.source_path()).unwrap();
    // A folder with one book and no sidecar: adopted where it is.
    std::fs::create_dir_all(root.join("Mine/Notes")).unwrap();
    epub(
        &root.join("Mine/Notes/notes.epub"),
        "urn:x:n",
        "Notes",
        "Bo Reader",
    );
    // A book dropped at the root: filed into its own folder.
    epub(&root.join("loose.epub"), "urn:x:l", "Loose", "Cy Author");
    // Junk.
    std::fs::create_dir_all(root.join("Bad")).unwrap();
    std::fs::write(root.join("Bad").join(FILE_NAME), "{not json").unwrap();
    std::fs::create_dir_all(root.join(".hidden")).unwrap();
    epub(
        &root.join(".hidden/h.epub"),
        "urn:x:h",
        "Hidden",
        "Ada Writer",
    );

    let report = lib.scan();
    assert_eq!(report.missing_source, vec![gone.dir.clone()]);
    assert_eq!(report.adopted.len(), 2, "{:?}", report.adopted);
    assert!(root.join("Mine/Notes").join(FILE_NAME).is_file());
    assert!(!root.join("loose.epub").exists(), "filed");
    assert!(root.join("Author, Cy/Loose/loose.epub").is_file());
    assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
    assert!(report.problems[0].0.ends_with(format!("Bad/{FILE_NAME}")));
    assert!(
        !root.join(".hidden").join(FILE_NAME).exists(),
        "hidden folders skipped"
    );
    // A second scan has nothing to do.
    let again = lib.scan();
    assert!(again.adopted.is_empty() && again.refreshed.is_empty());
    assert_eq!(again.books, 3);
}

#[test]
fn unknown_fields_survive_and_a_newer_schema_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let src = epub(&tmp.path().join("a.epub"), "urn:x:a", "A", "Ada Writer");
    let book = lib.add(&src).unwrap().book;
    let path = book.sidecar_path();
    let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    v["future_field"] = serde_json::json!({"keep": true});
    v["metadata"]["subjects"] = serde_json::json!(["roofs"]);
    std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
    lib.forget_send("none", "none").unwrap();
    let mut b = lib.list().remove(0);
    lib.record_send(&mut b, Default::default()).unwrap();
    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(after["future_field"]["keep"], true);
    assert_eq!(after["metadata"]["subjects"][0], "roofs");

    v["schema"] = serde_json::json!(SCHEMA + 1);
    std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(lib.add(&src).is_err());
    let report = lib.scan();
    assert_eq!(report.problems.len(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), before, "untouched");
}

#[test]
fn auto_add_records_a_send_only_with_a_library_and_a_delivery() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("deckle/library.json");
    let src = deckle_kfx::fixtures::path("deckle-mini.epub");
    let device = Target::MassStorage(Device {
        root: tmp.path().join("Kindle"),
        model_hint: "Kindle Paperwhite".into(),
        thumbnails_writable: true,
        firmware: None,
        serial: None,
    });
    let report = RunReport {
        delivered: Some(
            tmp.path()
                .join("Kindle/documents/Testwright, Jane - Deckle Mini Fixture.kfx"),
        ),
        cde_type: Some("EBOK".into()),
        asin: Some("B0ABCDDECK".into()),
        end_position: Some(1234),
        ..Default::default()
    };
    // No library: nothing happens.
    assert!(auto_add(&config, &src, &report, &device).unwrap().is_none());

    let root = tmp.path().join("Library");
    LibraryConfig {
        root: Some(root.clone()),
        ..Default::default()
    }
    .save(&config)
    .unwrap();
    let book = auto_add(&config, &src, &report, &device).unwrap().unwrap();
    let send = &book.sidecar.sends[0];
    assert_eq!(send.device, "ms:Kindle Paperwhite");
    assert_eq!(send.device_name, "Kindle Paperwhite");
    assert_eq!(send.file, "Testwright, Jane - Deckle Mini Fixture.kfx");
    assert_eq!(send.cde.as_deref(), Some("EBOK"));
    assert_eq!(send.end, Some(1234), "send_record_carries_end");
    let lib = Library::new(&root);
    assert_eq!(
        lib.find_send("ms:Kindle Paperwhite", &send.file)
            .unwrap()
            .dir,
        book.dir
    );
    // A failed run, a KFX, or auto-add off: nothing.
    let failed = RunReport {
        error: Some("convert: no".into()),
        ..report.clone()
    };
    assert!(auto_add(&config, &src, &failed, &device).unwrap().is_none());
    LibraryConfig {
        root: Some(root),
        auto_add: false,
        ..Default::default()
    }
    .save(&config)
    .unwrap();
    assert!(auto_add(&config, &src, &report, &device).unwrap().is_none());
    // Forgetting the send (removed from the Kindle).
    assert!(lib.forget_send("ms:Kindle Paperwhite", &send.file).unwrap());
    assert!(lib.list()[0].sidecar.sends.is_empty());
}

#[test]
fn a_kindles_books_match_the_library_by_asin_or_send_record() {
    use deckle_core::inventory::DeviceBook;
    use deckle_library::shelf::{shelve, source_cover};
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let mut book = lib
        .add(&deckle_kfx::fixtures::path("deckle-mini.epub"))
        .unwrap()
        .book;
    let file = "Testwright, Jane - Deckle Mini Fixture.kfx";
    let asin = book.sidecar.asin.clone();
    lib.record_send(
        &mut book,
        deckle_library::SendRecord {
            device: "mtp:G0X1".into(),
            file: file.into(),
            cde: Some("PDOC".into()),
            asin: Some(asin.clone()),
            xray: false,
            word_wise: false,
            ..Default::default()
        },
    )
    .unwrap();
    let on_mtp = DeviceBook {
        file: file.into(),
        format: "kfx".into(),
        ..Default::default()
    };
    let by_asin = DeviceBook {
        file: "renamed.kfx".into(),
        asin: Some(book.sidecar.asin.clone()),
        cde: Some("EBOK".into()),
        ..Default::default()
    };
    let other = DeviceBook {
        file: "Store Book.kfx".into(),
        asin: Some("B00STORE00".into()),
        ..Default::default()
    };
    let got = shelve(vec![on_mtp, by_asin, other], Some(&lib), "mtp:G0X1", &now());
    assert_eq!(got[0].library.as_ref().unwrap().dir, book.dir);
    assert_eq!(got[0].book.title.as_deref(), Some("Deckle Mini Fixture"));
    assert_eq!(got[0].book.cde.as_deref(), Some("PDOC"));
    assert!(got[0].book.from_deckle);
    assert!(got[1].library.is_some());
    assert_eq!(
        got[1].book.authors,
        ["Jane Testwright"],
        "display names, not sort form"
    );
    assert_eq!(
        got[1].book.cde.as_deref(),
        Some("EBOK"),
        "the device's own wins"
    );
    assert!(got[2].library.is_none() && !got[2].book.from_deckle);
    // Another Kindle's record doesn't match by file name.
    let elsewhere = DeviceBook {
        file: file.into(),
        ..Default::default()
    };
    assert!(
        shelve(vec![elsewhere], Some(&lib), "ms:Kindle", &now())[0]
            .library
            .is_none()
    );
    assert!(source_cover(&book).is_some());
}

fn now() -> deckle_library::freshness::Current {
    deckle_library::freshness::Current {
        xray_engine: 2,
        wordwise_db: Some("2026.10+3".into()),
    }
}

#[test]
fn the_shelf_says_what_can_be_updated_and_what_is_duplicated() {
    use deckle_core::inventory::DeviceBook;
    use deckle_library::freshness::Reason;
    use deckle_library::shelf::shelve;
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::new(tmp.path().join("Library"));
    let mut book = lib
        .add(&deckle_kfx::fixtures::path("deckle-mini.epub"))
        .unwrap()
        .book;
    let asin = book.sidecar.asin.clone();
    let file = "Testwright, Jane - Deckle Mini Fixture.kfx";
    lib.record_send(
        &mut book,
        deckle_library::SendRecord {
            device: "ms:Kindle".into(),
            file: file.into(),
            cde: Some("EBOK".into()),
            asin: Some(asin.clone()),
            xray: true,
            word_wise: true,
            stamped: true,
            xray_engine: Some(1),
            wordwise_db: Some("2026.10+3".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let on_kindle = DeviceBook {
        file: file.into(),
        asin: Some(asin.clone()),
        xray: true,
        word_wise: false, // the Kindle lost it
        ..Default::default()
    };
    let store_a = DeviceBook {
        file: "Some Store Book.azw3".into(),
        title: Some("The Same Book!".into()),
        authors: vec!["Ada Writer".into()],
        ..Default::default()
    };
    let store_b = DeviceBook {
        file: "the same book.pdf".into(),
        title: Some("the same book".into()),
        authors: vec!["ada writer".into()],
        ..Default::default()
    };
    let got = shelve(
        vec![on_kindle, store_a, store_b],
        Some(&lib),
        "ms:Kindle",
        &now(),
    );
    assert_eq!(
        got[0].updates,
        [
            Reason::XRayEngine { sent: 1, now: 2 },
            Reason::MissingWordWise
        ]
    );
    assert!(!got[0].duplicate);
    assert!(got[1].updates.is_empty(), "not in the library");
    assert!(got[1].duplicate && got[2].duplicate);
    // Sent to another Kindle: nothing to judge here.
    let elsewhere = DeviceBook {
        file: file.into(),
        asin: Some(asin),
        ..Default::default()
    };
    assert!(
        shelve(vec![elsewhere], Some(&lib), "ms:Other", &now())[0]
            .updates
            .is_empty()
    );
}

#[test]
fn a_send_without_end_serialises_without_the_key() {
    let old: deckle_library::sidecar::SendRecord =
        serde_json::from_str(r#"{"device":"ms:K","file":"a.kfx"}"#).unwrap();
    assert_eq!(old.end, None);
    assert!(!serde_json::to_string(&old).unwrap().contains("\"end\""));
}

#[test]
fn sends_to_two_kindles_of_one_model_stay_apart() {
    use deckle_library::{SendRecord, Sidecar};
    let send = |device: &str| SendRecord {
        device: device.into(),
        file: "a.kfx".into(),
        ..Default::default()
    };
    let mut sc = Sidecar::default();
    sc.record(send("ms:Kindle#A"));
    sc.record(send("ms:Kindle#B"));
    assert_eq!(sc.sends.len(), 2);
    assert_eq!(sc.send_to("ms:Kindle#A").unwrap().device, "ms:Kindle#A");
    // A record from before serials still answers for either Kindle until it is re-sent.
    let mut old = Sidecar::default();
    old.record(send("ms:Kindle"));
    assert!(old.send_to("ms:Kindle#A").is_some());
    assert!(old.forget("ms:Kindle#B", "a.kfx"));
}

#[test]
fn markdown_library_snapshot_retains_assets_and_updates_identity() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("notes");
    std::fs::create_dir(&source).unwrap();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/markdown/images/square.png");
    std::fs::copy(fixture, source.join("square.png")).unwrap();
    let note = "# Notes\n\n![A square](square.png)\n";
    std::fs::write(source.join("note.md"), note).unwrap();
    let library = Library::new(temp.path().join("library"));
    let first = library.add(&source).unwrap();
    assert_eq!(first.book.source_path().extension().unwrap(), "epub");
    assert_eq!(
        std::fs::read_to_string(source.join("note.md")).unwrap(),
        note
    );
    std::fs::write(source.join("note.md"), format!("{note}\nMore notes.\n")).unwrap();
    let second = library.add(&source).unwrap();
    assert!(!second.new);
    assert!(second.copied);
    assert_eq!(first.book.sidecar.id, second.book.sidecar.id);
    std::fs::remove_dir_all(&source).unwrap();
    let info = deckle_library::book_info(&second.book.source_path()).unwrap();
    assert_eq!(info.title, "notes");
    let mut epub = deckle_core::epub::EpubReader::open(&second.book.source_path()).unwrap();
    assert!(epub.read_bytes("OEBPS/images/image-001.png").is_ok());
}

#[test]
fn fb2_is_a_source_and_reads_its_metadata() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fb2/mini.fb2.zip");
    assert!(deckle_library::is_source(&fixture));
    let temp = tempfile::tempdir().unwrap();
    let library = Library::new(temp.path().join("library"));
    let added = library.add(&fixture).unwrap();
    assert!(added.new);
    assert!(
        added
            .book
            .source_path()
            .to_string_lossy()
            .ends_with(".fb2.zip")
    );
    let info = deckle_library::book_info(&added.book.source_path()).unwrap();
    assert_eq!(info.title, "The Deckle FB2 Mini");
    assert_eq!(info.series.unwrap().name, "Deckle Fixtures");
}

#[test]
fn odt_is_a_source_and_reads_its_metadata() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/odt/mini.odt");
    assert!(deckle_library::is_source(&fixture));
    let temp = tempfile::tempdir().unwrap();
    let library = Library::new(temp.path().join("library"));
    let added = library.add(&fixture).unwrap();
    assert!(added.new);
    assert_eq!(added.book.source_path().extension().unwrap(), "odt");
    let info = deckle_library::book_info(&added.book.source_path()).unwrap();
    assert_eq!(info.title, "The Deckle ODT Mini");
    assert_eq!(info.creators[0].name, "Ada Testwright");
}

#[test]
fn rtf_is_a_source_and_reads_its_metadata() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rtf/mini.rtf");
    assert!(deckle_library::is_source(&fixture));
    let temp = tempfile::tempdir().unwrap();
    let library = Library::new(temp.path().join("library"));
    let added = library.add(&fixture).unwrap();
    assert!(added.new);
    assert_eq!(added.book.source_path().extension().unwrap(), "rtf");
    let info = deckle_library::book_info(&added.book.source_path()).unwrap();
    assert_eq!(info.title, "The Deckle RTF Mini");
    assert_eq!(info.creators[0].name, "Ada Testwright");
}

#[test]
fn kindle_books_are_sources_and_read_their_metadata() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mobi");
    let temp = tempfile::tempdir().unwrap();
    let library = Library::new(temp.path().join("library"));
    for (name, title) in [
        ("mini6.mobi", "The Deckle MOBI Mini"),
        ("mini.azw3", "The Deckle AZW3 Mini"),
    ] {
        let fixture = fixtures.join(name);
        assert!(deckle_library::is_source(&fixture));
        let added = library.add(&fixture).unwrap();
        assert!(added.new);
        let info = deckle_library::book_info(&added.book.source_path()).unwrap();
        assert_eq!(info.title, title);
    }
}

/// A page image the library tests can use for comics.
fn page_png() -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/markdown/images/square.png"),
    )
    .unwrap()
}

#[test]
fn a_cbz_is_kept_as_it_is_and_a_page_folder_as_an_epub_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let cbz = temp.path().join("Night Train.cbz");
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&cbz).unwrap());
    for name in ["1.png", "2.png"] {
        zw.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(&page_png()).unwrap();
    }
    zw.finish().unwrap();
    assert!(deckle_library::is_source(&cbz));
    let library = Library::new(temp.path().join("library"));
    let added = library.add(&cbz).unwrap();
    assert_eq!(added.book.source_path().extension().unwrap(), "cbz");
    let info = deckle_library::book_info(&added.book.source_path()).unwrap();
    assert_eq!(info.title, "Night Train");
    assert!(info.fixed_layout);

    let pages = temp.path().join("Picture Book");
    std::fs::create_dir(&pages).unwrap();
    std::fs::write(pages.join("01.png"), page_png()).unwrap();
    assert!(deckle_library::is_source(&pages));
    let added = library.add(&pages).unwrap();
    assert_eq!(added.book.source_path().extension().unwrap(), "epub");
    let info = deckle_library::book_info(&added.book.source_path()).unwrap();
    assert_eq!(info.title, "Picture Book");
    assert!(info.fixed_layout);
}
