// SPDX-License-Identifier: GPL-3.0-or-later
//! Every parse of file content goes through `deckle_limits::parse_xml`, which refuses XML nested
//! too deep before `roxmltree` recurses (nesting-depth design, mechanism 1). This walks the
//! workspace's Rust sources and fails on a direct `Document::parse` outside that wrapper.
//! Test code (`tests/` folders and items marked `#[cfg(test)]`) parses Deckle's own output and
//! may call `roxmltree` directly.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` file under `dir`, skipping build output, `node_modules` and `tests/` folders.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if path.is_dir() {
            if !matches!(name.as_ref(), "target" | "node_modules" | "tests") {
                sources(&path, out);
            }
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// The file without the items marked `#[cfg(test)]`, the code that ships. Only the marked item
/// is left out (a `mod x;` line, a whole `mod tests { … }`, a helper `fn`), not the rest of the
/// file. An item ends at a line ending in `;` or `}` (a one-line item), or, when its line ends
/// in `{`, at the closing brace on the item's own indentation: the workspace is kept formatted
/// by `cargo fmt`, which the pre-push hook checks.
fn shipped(text: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let indent = |l: &str| l.len() - l.trim_start().len();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(rest) = lines[i].trim().strip_prefix("#[cfg(test)]") else {
            out.push_str(lines[i]);
            i += 1;
            continue;
        };
        // The marked item's first line: this one when the item follows on it, else the next
        // line that is not blank or another attribute.
        let mut j = i;
        if rest.trim().is_empty() {
            j += 1;
            while j < lines.len()
                && (lines[j].trim().is_empty() || lines[j].trim_start().starts_with("#["))
            {
                j += 1;
            }
        }
        let Some(first) = lines.get(j) else { break };
        let close = &first[..indent(first)];
        // Through a signature over several lines to the line that ends in `{`, `;` or `}`.
        let mut k = j;
        while k < lines.len() && !lines[k].trim_end().ends_with(['{', ';', '}']) {
            k += 1;
        }
        if k < lines.len() && lines[k].trim_end().ends_with('{') {
            k += 1;
            while k < lines.len() {
                let line = lines[k].trim_end();
                if line.starts_with(close) && matches!(&line[close.len()..], "}" | "};" | "},") {
                    break;
                }
                k += 1;
            }
        }
        i = k + 1;
    }
    out
}

/// The shipped code calls `roxmltree` directly: `Document::parse…`, or `Document` under
/// another name to call it by.
fn offends(text: &str) -> bool {
    let code = shipped(text);
    code.contains("Document::parse") || code.contains("Document as")
}

fn all_sources() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in ["crates", "apps/desktop/src-tauri", "data"] {
        let dir = root().join(dir);
        if dir.is_dir() {
            sources(&dir, &mut files);
        }
    }
    files
}

#[test]
fn the_walk_sees_the_source_tree() {
    let files = all_sources();
    let has = |end: &str| files.iter().any(|f| f.ends_with(end));
    assert!(has("crates/deckle-docx/src/package.rs"));
    assert!(has("crates/deckle-core/src/epub.rs"));
    assert!(!has("crates/deckle-core/tests/it/xml_boundary.rs"));
    let package =
        std::fs::read_to_string(root().join("crates/deckle-docx/src/package.rs")).unwrap();
    assert!(shipped(&package).contains("deckle_limits::parse_xml("));
}

#[test]
fn shipped_leaves_out_only_the_items_marked_for_tests() {
    let text = "fn a() {}\n#[cfg(test)]\nmod fake;\nfn b() {}\n#[cfg(test)]\nfn helper() {}\n\
                fn c() {}\n#[cfg(test)]\n#[allow(dead_code)]\nfn signature(\n    x: u8,\n) -> u8 {\n    \
                x\n}\n#[cfg(test)]\nmod tests {\n    fn d() {\n    }\n}\n#[cfg(test)] use a::b;\n\
                fn e() {}\n";
    assert_eq!(
        shipped(text),
        "fn a() {}\nfn b() {}\nfn c() {}\nfn e() {}\n"
    );
    assert_eq!(shipped("fn a() {}\n"), "fn a() {}\n");
}

#[test]
fn a_direct_parse_after_a_test_module_line_and_an_alias_are_caught() {
    assert!(offends(
        "#[cfg(test)]\nmod fake;\nfn f(x: &str) {\n    roxmltree::Document::parse(x);\n}\n"
    ));
    assert!(offends("use roxmltree::Document as D;\n"));
    assert!(offends("use roxmltree::{Document as D, Node};\n"));
    assert!(!offends(
        "#[cfg(test)]\nmod tests {\n    fn t() {\n        roxmltree::Document::parse(\"<a/>\");\n    }\n}\n"
    ));
}

#[test]
fn only_deckle_limits_calls_document_parse() {
    let offenders: Vec<String> = all_sources()
        .into_iter()
        .filter(|f| !f.components().any(|c| c.as_os_str() == "deckle-limits"))
        .filter(|f| offends(&std::fs::read_to_string(f).unwrap()))
        .map(|f| f.strip_prefix(root()).unwrap_or(&f).display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "parse file content with deckle_limits::parse_xml, not roxmltree::Document::parse: {offenders:?}"
    );
}
