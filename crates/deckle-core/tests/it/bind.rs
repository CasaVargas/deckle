// SPDX-License-Identifier: GPL-3.0-or-later
//! Batch bind (batch-bind design §3): parts into one EPUB that inspects cleanly and runs through
//! the pipeline.

use std::io::Read;
use std::path::{Path, PathBuf};

use deckle_core::bind::{BindError, BindOptions, Part, bind};
use deckle_core::import::import;
use deckle_core::inspect::inspect;

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

/// The mini EPUB, plus a Word document and a text file imported to EPUBs.
fn parts(dir: &Path) -> Vec<Part> {
    let docx = import(&fixture("docx/deckle-mini.docx"), dir, "docx.epub")
        .unwrap()
        .unwrap();
    let txt = import(&fixture("txt/gutenberg-mini.txt"), dir, "txt.epub")
        .unwrap()
        .unwrap();
    vec![
        Part {
            epub: deckle_kfx::fixtures::path("deckle-mini.epub"),
            title: None,
        },
        Part {
            epub: docx.epub,
            title: Some("A Word Document".into()),
        },
        Part {
            epub: txt.epub,
            title: None,
        },
    ]
}

fn names(epub: &Path) -> Vec<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(epub).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
        .collect()
}

fn read(epub: &Path, name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(epub).unwrap()).unwrap();
    let mut s = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

#[test]
fn parts_bind_into_one_book_that_inspects_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let parts = parts(tmp.path());
    let spines: usize = parts
        .iter()
        .map(|p| inspect(&p.epub).unwrap().spine.len())
        .sum();
    let out = tmp.path().join("bound.epub");
    let report = bind(
        &parts,
        &BindOptions {
            title: "Course Packet".into(),
            author: Some("A. Teacher".into()),
        },
        &out,
    )
    .unwrap();
    assert_eq!(
        report.parts,
        [
            "Deckle Mini Fixture",
            "A Word Document",
            "The Deckle Mini Novel"
        ]
    );

    let info = inspect(&out).unwrap();
    assert_eq!(info.title, "Course Packet");
    assert_eq!(info.creators[0].name, "A. Teacher");
    assert_eq!(info.spine.len(), spines, "every part's pages, in order");
    assert!(info.spine[0].starts_with("p01/") && info.spine.last().unwrap().starts_with("p03/"));
    assert!(
        info.cover_href.is_none(),
        "no part's cover stands for the book"
    );
    let top: Vec<&str> = info
        .toc
        .iter()
        .filter(|t| t.depth == 0)
        .map(|t| t.title.as_str())
        .collect();
    assert_eq!(
        top,
        report.parts.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert!(
        info.toc
            .iter()
            .any(|t| t.depth == 1 && t.href.starts_with("p03/")),
        "the novel's chapters nest under it: {:?}",
        info.toc
    );
    assert!(info.warnings.is_empty(), "{:?}", info.warnings);

    // Every manifest file is in the zip, under its part's folder; one contents page.
    let files = names(&out);
    let opf = read(&out, "OEBPS/content.opf");
    for href in opf
        .split("href=\"")
        .skip(1)
        .map(|s| s.split('"').next().unwrap())
    {
        let name = format!("OEBPS/{}", deckle_core::epub::percent_decode(href));
        assert!(files.contains(&name), "{name} missing from the zip");
    }
    assert_eq!(opf.matches("properties=\"nav\"").count(), 1);
    assert_eq!(files.first().map(String::as_str), Some("mimetype"));
}

#[test]
fn one_file_is_not_a_bind() {
    let tmp = tempfile::tempdir().unwrap();
    let one = vec![Part {
        epub: deckle_kfx::fixtures::path("deckle-mini.epub"),
        title: None,
    }];
    assert!(matches!(
        bind(&one, &BindOptions::default(), &tmp.path().join("o.epub")),
        Err(BindError::TooFew)
    ));
}

#[test]
fn the_shared_author_is_kept_when_none_is_given() {
    let tmp = tempfile::tempdir().unwrap();
    let mini = deckle_kfx::fixtures::path("deckle-mini.epub");
    let twice = vec![
        Part {
            epub: mini.clone(),
            title: Some("First".into()),
        },
        Part {
            epub: mini,
            title: Some("Second".into()),
        },
    ];
    let report = bind(
        &twice,
        &BindOptions {
            title: "Twice".into(),
            author: None,
        },
        &tmp.path().join("o.epub"),
    )
    .unwrap();
    assert_eq!(report.author.as_deref(), Some("Jane Testwright"));
}
