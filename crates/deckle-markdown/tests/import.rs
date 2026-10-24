// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_markdown::{convert, is_markdown};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markdown")
        .join(name)
}
fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    assert_eq!(zip.by_index(0).unwrap().name().unwrap(), "mimetype");
    assert_eq!(
        zip.by_index(0).unwrap().compression(),
        zip::CompressionMethod::Stored
    );
    (0..zip.len())
        .map(|i| {
            let mut entry = zip.by_index(i).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (entry.name().unwrap().into_owned(), bytes)
        })
        .collect()
}
fn text<'a>(files: &'a BTreeMap<String, Vec<u8>>, path: &str) -> &'a str {
    std::str::from_utf8(&files[path]).unwrap()
}
fn valid(files: &BTreeMap<String, Vec<u8>>) {
    for (name, bytes) in files {
        if name.ends_with(".xhtml") || name.ends_with(".opf") || name.ends_with(".xml") {
            let xml = std::str::from_utf8(bytes).unwrap();
            let doc =
                roxmltree::Document::parse(xml).unwrap_or_else(|e| panic!("{name}: {e}\n{xml}"));
            for node in doc.descendants().filter(|n| n.is_element()) {
                let href = node.attribute("href").or_else(|| node.attribute("src"));
                let Some(href) = href.filter(|s| !s.contains(':')) else {
                    continue;
                };
                let (file, anchor) = href
                    .split_once('#')
                    .map_or((href, None), |(f, a)| (f, Some(a)));
                let target = format!("OEBPS/{file}");
                assert!(files.contains_key(&target), "{name}: missing {target}");
                if let Some(anchor) = anchor {
                    let target_doc = roxmltree::Document::parse(text(files, &target)).unwrap();
                    assert!(
                        target_doc
                            .descendants()
                            .any(|n| n.attribute("id") == Some(anchor)),
                        "missing anchor {href}"
                    );
                }
            }
        }
    }
}

#[test]
fn single_file_preserves_structure_metadata_images_and_navigation() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("book.epub");
    let report = convert(&fixture("mini.md"), &out).unwrap();
    assert_eq!(report.title, "Deckle Markdown Mini");
    assert_eq!(report.chapters, 1);
    assert_eq!(report.contents, 4);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let f = files(&out);
    valid(&f);
    let html = text(&f, "OEBPS/chapter-001.xhtml");
    for tag in [
        "<strong>",
        "<em>",
        "<del>",
        "<ul>",
        "<ol>",
        "<blockquote>",
        "<table>",
        "<img ",
        "<pre>",
    ] {
        assert!(html.contains(tag), "{tag}: {html}");
    }
    assert!(html.contains("[[Not a link]] &lt;literal code&gt;"));
    assert!(html.contains("alt=\"A tiny blue square\""));
    assert!(text(&f, "OEBPS/content.opf").contains("<dc:creator>Ada Testwright</dc:creator>"));
    assert_eq!(
        f["OEBPS/images/image-001.png"],
        std::fs::read(fixture("images/square.png")).unwrap()
    );
    let again = tmp.path().join("again.epub");
    convert(&fixture("mini.md"), &again).unwrap();
    assert_eq!(std::fs::read(out).unwrap(), std::fs::read(again).unwrap());
}

#[test]
fn folder_links_resolve_across_three_chapters_and_duplicate_headings() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("book.epub");
    let report = convert(&fixture("notes"), &out).unwrap();
    assert_eq!(report.chapters, 3);
    assert_eq!(report.title, "notes");
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("Missing"));
    let f = files(&out);
    valid(&f);
    assert!(
        text(&f, "OEBPS/chapter-001.xhtml")
            .contains("href=\"chapter-002.xhtml#observations\">the observations")
    );
    assert!(text(&f, "OEBPS/chapter-002.xhtml").contains("href=\"chapter-001.xhtml#welcome\""));
    let third = text(&f, "OEBPS/chapter-003.xhtml");
    assert!(third.contains("chapter-002.xhtml#observations-2"));
    assert!(third.contains("An unwritten note"));
    assert!(!third.contains("href=\"Missing"));
}

#[test]
fn unsafe_links_html_remote_images_and_missing_anchors_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("note.md");
    std::fs::write(&source, "# Note\n\n<script>alert('x')</script>\n\n[bad](javascript:alert) [missing](#Absent) ![Remote](https://example.com/img.png) ![[Other.md]]\n").unwrap();
    let out = tmp.path().join("out.epub");
    let r = convert(&source, &out).unwrap();
    let f = files(&out);
    valid(&f);
    let html = text(&f, "OEBPS/chapter-001.xhtml");
    assert!(!html.contains("<script>"));
    assert!(!html.contains("href=\"javascript:"));
    assert!(!html.contains("<img"));
    assert!(html.contains("Remote"));
    assert_eq!(r.warnings.len(), 5);
}

#[test]
fn rejects_empty_binary_invalid_frontmatter_and_overlarge_inputs() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("note.md");
    let out = tmp.path().join("out.epub");
    for input in [
        "",
        "\0binary",
        "---\ntitle: [bad\n---\ntext",
        "---\ntitle: No end",
        "---\nlanguage: invalid tag!\n---\ntext",
    ] {
        std::fs::write(&source, input).unwrap();
        assert!(convert(&source, &out).is_err(), "{input:?}");
    }
    let file = std::fs::File::create(&source).unwrap();
    file.set_len(51 * 1024 * 1024).unwrap();
    assert!(
        convert(&source, &out)
            .unwrap_err()
            .to_string()
            .contains("50 MB")
    );
    std::fs::remove_file(source).unwrap();
    assert!(convert(tmp.path(), &out).is_err());
}

#[test]
fn ambiguous_names_are_unlinked_and_nested_hidden_notes_are_not_chapters() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("A.md"), "# A\n\n[[Note]] [[Note.md]]").unwrap();
    std::fs::write(tmp.path().join("Note.md"), "# One").unwrap();
    std::fs::write(tmp.path().join("Note.markdown"), "# Two").unwrap();
    std::fs::write(tmp.path().join(".hidden.md"), "# Hidden").unwrap();
    std::fs::create_dir(tmp.path().join("nested")).unwrap();
    std::fs::write(tmp.path().join("nested/inside.md"), "# Inside").unwrap();
    let out = tmp.path().join("out.epub");
    let r = convert(tmp.path(), &out).unwrap();
    assert_eq!(r.chapters, 3);
    assert!(r.warnings.iter().any(|w| w.contains("link Note;")));
    let f = files(&out);
    valid(&f);
    assert!(text(&f, "OEBPS/chapter-001.xhtml").contains("href=\"chapter-003.xhtml\">Note.md"));
    assert!(is_markdown(tmp.path()));
}

#[test]
fn images_are_deduplicated_and_cannot_escape_the_source_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("notes");
    std::fs::create_dir(&root).unwrap();
    let png = std::fs::read(fixture("images/square.png")).unwrap();
    std::fs::write(root.join("local.png"), &png).unwrap();
    std::fs::write(tmp.path().join("outside.png"), &png).unwrap();
    std::fs::write(
        root.join("note.md"),
        "# Images\n\n![one](local.png) ![two](./local.png) ![outside](../outside.png)\n",
    )
    .unwrap();
    let out = tmp.path().join("out.epub");
    let report = convert(&root, &out).unwrap();
    assert_eq!(report.warnings.len(), 1);
    let f = files(&out);
    valid(&f);
    assert_eq!(
        f.keys().filter(|k| k.starts_with("OEBPS/images/")).count(),
        1
    );
    assert!(text(&f, "OEBPS/chapter-001.xhtml").contains("outside"));
}

#[test]
#[cfg(unix)]
fn symlinked_images_cannot_escape_and_linked_notes_are_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("notes");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(tmp.path().join("secret.md"), "# Secret").unwrap();
    std::fs::copy(fixture("images/square.png"), tmp.path().join("outside.png")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("outside.png"), root.join("image.png")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("secret.md"), root.join("linked.md")).unwrap();
    std::fs::write(root.join("note.md"), "# Notes\n\n![outside](image.png)").unwrap();
    let out = tmp.path().join("out.epub");
    let r = convert(&root, &out).unwrap();
    assert_eq!(r.chapters, 1);
    assert_eq!(r.warnings.len(), 2);
    assert!(!files(&out).keys().any(|k| k.starts_with("OEBPS/images/")));
}
