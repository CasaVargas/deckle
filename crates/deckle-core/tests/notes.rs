// SPDX-License-Identifier: GPL-3.0-or-later
//! Pop-up notes acceptance (#31, design §6) against `fixtures/notes/`.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use deckle_core::normalize::{NormalizeOptions, NormalizeReport, normalize};
use deckle_core::notes::NotesReport;

const FIXTURES: &[&str] = &[
    "semantic.epub",
    "calibre.epub",
    "indesign.epub",
    "pandoc.epub",
    "trap.epub",
];
const OPS: &str = "http://www.idpf.org/2007/ops";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/notes")
        .join(name)
}

fn notes_on() -> NormalizeOptions {
    NormalizeOptions {
        popup_notes: true,
        ..Default::default()
    }
}

fn run(src: &Path, opts: &NormalizeOptions) -> (tempfile::NamedTempFile, NormalizeReport) {
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let report = normalize(src, out.path(), opts).unwrap();
    (out, report)
}

fn entries(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut v = Vec::new();
            f.read_to_end(&mut v).unwrap();
            (f.name().unwrap().into_owned(), v)
        })
        .collect()
}

fn xhtml(path: &Path) -> BTreeMap<String, String> {
    entries(path)
        .into_iter()
        .filter(|(n, _)| n.ends_with(".xhtml"))
        .map(|(n, v)| (n, String::from_utf8(v).unwrap()))
        .collect()
}

fn epub_type<'a>(n: roxmltree::Node<'a, '_>) -> Vec<&'a str> {
    n.attribute((OPS, "type"))
        .map(|t| t.split_whitespace().collect())
        .unwrap_or_default()
}

/// Checks the book is still well-formed (the epub namespace is declared wherever it's used),
/// every fragment link resolves, and every noteref points at a pop-up `aside`. Returns the
/// number of noterefs.
fn check_popups(path: &Path) -> usize {
    let docs = xhtml(path);
    let parsed: BTreeMap<&str, roxmltree::Document> = docs
        .iter()
        .map(|(n, t)| {
            let doc = roxmltree::Document::parse(t).unwrap_or_else(|e| panic!("{n}: {e}\n{t}"));
            (n.as_str(), doc)
        })
        .collect();
    let mut ids: HashMap<(String, String), roxmltree::Node> = HashMap::new();
    for (name, doc) in &parsed {
        for n in doc.descendants().filter(|n| n.is_element()) {
            if let Some(id) = n.attribute("id") {
                ids.insert((name.to_string(), id.to_owned()), n);
            }
        }
    }
    let mut noterefs = 0;
    for (name, doc) in &parsed {
        let dir = name.rsplit_once('/').map_or("", |(d, _)| d);
        for a in doc.descendants().filter(|n| n.has_tag_name("a")) {
            let Some((file, frag)) = a.attribute("href").and_then(|h| h.split_once('#')) else {
                continue;
            };
            let target_doc = if file.is_empty() {
                name.to_string()
            } else {
                format!("{dir}/{file}")
            };
            let target = ids
                .get(&(target_doc.clone(), frag.to_owned()))
                .unwrap_or_else(|| panic!("{name}: link to {target_doc}#{frag} doesn't resolve"));
            if epub_type(a).contains(&"noteref") {
                noterefs += 1;
                assert!(
                    target.has_tag_name("aside")
                        && epub_type(*target)
                            .iter()
                            .any(|t| ["footnote", "endnote", "rearnote"].contains(t)),
                    "{name}: noteref to #{frag} isn't a pop-up aside"
                );
            }
        }
    }
    noterefs
}

fn report(semantic: u32, heuristic: u32, skipped_one_way: u32) -> Option<NotesReport> {
    Some(NotesReport {
        semantic,
        heuristic,
        skipped_one_way,
    })
}

#[test]
fn flag_off_is_byte_identical() {
    for name in FIXTURES {
        let (out, r) = run(&fixture(name), &NormalizeOptions::default());
        assert_eq!(r.notes, None, "{name}");
        assert_eq!(entries(out.path()), entries(&fixture(name)), "{name}");
    }
}

#[test]
fn semantic_notes_become_asides() {
    let (out, r) = run(&fixture("semantic.epub"), &notes_on());
    assert_eq!(r.notes, report(3, 0, 0));
    assert_eq!(check_popups(out.path()), 4);
    let docs = xhtml(out.path());
    let ch1 = &docs["OEBPS/ch1.xhtml"];
    assert!(
        ch1.contains(r##"<aside epub:type="footnote" id="n1"><p><a href="#r1">1</a>"##),
        "{ch1}"
    );
    assert!(
        ch1.contains(r##"<aside epub:type="footnote" id="n3">* A starred note"##),
        "{ch1}"
    );
    // The ARIA-only noteref gets its epub:type; the rest of the tag stays.
    assert!(
        ch1.contains(
            r##"<a role="doc-noteref" href="notes.xhtml#e1" id="r2" epub:type="noteref">2</a>"##
        ),
        "{ch1}"
    );
    // The endnote in an `ol start="2"` keeps its number in the text.
    let notes = &docs["OEBPS/notes.xhtml"];
    assert!(
        notes.contains(
            r##"<aside role="doc-endnote" id="e1" epub:type="endnote"><p>2. Endnote two"##
        ),
        "{notes}"
    );
}

#[test]
fn calibre_notes_convert_and_gain_the_epub_namespace() {
    let (out, r) = run(&fixture("calibre.epub"), &notes_on());
    assert_eq!(r.notes, report(0, 3, 0));
    assert_eq!(check_popups(out.path()), 4);
    let ch1 = &xhtml(out.path())["OEBPS/ch1.xhtml"];
    assert!(
        ch1.contains(r##"xmlns:epub="http://www.idpf.org/2007/ops""##),
        "{ch1}"
    );
    assert!(
        ch1.contains(r##"<a class="footnote-link" href="#fn1" id="fnref1b" epub:type="noteref"><sup>1</sup></a>"##),
        "{ch1}"
    );
    assert!(
        ch1.contains(r##"<aside class="calibre4" id="fn2" epub:type="footnote"><a href="#fnref2">↩</a> Second note.</aside>"##),
        "{ch1}"
    );
    assert!(
        ch1.contains(
            r##"<aside class="calibre5" id="fn3" epub:type="footnote"><p class="calibre4">"##
        ),
        "{ch1}"
    );
}

#[test]
fn indesign_endnotes_convert_and_other_links_still_resolve() {
    let (out, r) = run(&fixture("indesign.epub"), &notes_on());
    assert_eq!(r.notes, report(0, 3, 1));
    assert_eq!(check_popups(out.path()), 3);
    let docs = xhtml(out.path());
    let notes = &docs["OEBPS/notes.xhtml"];
    assert_eq!(notes.matches("<aside ").count(), 3, "{notes}");
    // The backlinks stay plain links, and the appendix stays a paragraph.
    assert!(!notes.contains("noteref"), "{notes}");
    assert!(
        notes.contains(r##"<p class="Body" id="p-extra">"##),
        "{notes}"
    );
    assert!(
        notes.contains(r##"<h1 class="Heading" id="notes-top">"##),
        "{notes}"
    );
    assert!(
        docs["OEBPS/ch2.xhtml"].contains(
            r##"<sup><a class="_idFootnoteLink" href="notes.xhtml#p-extra">4</a></sup>"##
        )
    );
}

#[test]
fn pandoc_list_notes_keep_their_numbers() {
    let (out, r) = run(&fixture("pandoc.epub"), &notes_on());
    assert_eq!(r.notes, report(0, 3, 0));
    assert_eq!(check_popups(out.path()), 3);
    let ch1 = &xhtml(out.path())["OEBPS/ch1.xhtml"];
    assert!(
        ch1.contains(r##"<aside id="fn2" epub:type="footnote"><p>2. Pandoc note two.<a href="#fnref2" class="footnote-back">↩︎</a></p></aside>"##),
        "{ch1}"
    );
}

#[test]
fn trap_is_byte_identical() {
    let (out, r) = run(&fixture("trap.epub"), &notes_on());
    assert_eq!(r.notes, report(0, 0, 1));
    assert_eq!(entries(out.path()), entries(&fixture("trap.epub")));
}

#[test]
fn twice_is_once() {
    for name in FIXTURES {
        let (once, _) = run(&fixture(name), &notes_on());
        let (twice, r) = run(once.path(), &notes_on());
        assert_eq!(entries(twice.path()), entries(once.path()), "{name}");
        let n = r.notes.unwrap();
        assert_eq!((n.semantic, n.heuristic), (0, 0), "{name}");
    }
}

#[test]
fn docx_imports_are_already_pop_ups() {
    let docx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/deckle-mini.docx");
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("mini.epub");
    deckle_docx::convert(&docx, &epub).unwrap();
    let (out, r) = run(&epub, &notes_on());
    assert_eq!(r.notes, report(0, 0, 0));
    assert_eq!(xhtml(out.path()), xhtml(&epub));
    assert_eq!(check_popups(out.path()), 3);
}
