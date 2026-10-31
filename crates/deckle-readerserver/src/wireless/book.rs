// SPDX-License-Identifier: GPL-3.0-or-later
//! A book made ready to send (calibre bridge design §4.3): where it goes on the reader, and the
//! record calibre would send with it.

use std::path::Path;

use serde_json::{Value, json};
use sha1::{Digest, Sha1};

/// The longest a path component is made.
const MAX_COMPONENT: usize = 80;

/// A book ready to go.
#[derive(Clone, Debug, PartialEq)]
pub struct Outgoing {
    pub lpath: String,
    pub metadata: Value,
    pub bytes: Vec<u8>,
    pub title: String,
}

/// The book at `path`, read for its title, authors, series, tags and calibre id. A file that
/// isn't an EPUB (or won't open as one) goes by its file name. Papers (`deckle-daily-…`) go in
/// `Deckle/`; anything else in `<author>/<title> - <author>.<ext>`.
pub fn from_file(path: &Path, now: &str) -> Result<Outgoing, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "book".into());
    let info = matches!(ext.as_str(), "epub" | "kepub")
        .then(|| deckle_core::inspect::inspect(path).ok())
        .flatten();
    let title = info
        .as_ref()
        .map(|i| i.title.trim().to_owned())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| stem.clone());
    let authors: Vec<String> = info
        .as_ref()
        .map(|i| {
            i.creators
                .iter()
                .map(|c| c.name.trim().to_owned())
                .filter(|n| !n.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let author_sort = info
        .as_ref()
        .and_then(|i| i.creators.first())
        .and_then(|c| c.file_as.clone())
        .unwrap_or_else(|| authors.first().cloned().unwrap_or_default());
    let uuid = info
        .as_ref()
        .and_then(|i| i.calibre_id.clone())
        .unwrap_or_else(|| stable_uuid(&title, &authors));
    let paper = stem.starts_with("deckle-daily-");
    let lpath = if paper {
        format!("Deckle/{}.{ext}", clean(&stem))
    } else {
        lpath(&title, &authors, &ext)
    };
    let (series, series_index) = match info.as_ref().and_then(|i| i.series.clone()) {
        Some(s) if !s.name.is_empty() => (json!(s.name), json!(s.index.unwrap_or(1.0))),
        _ => (Value::Null, json!(1.0)),
    };
    let tags = info
        .as_ref()
        .map(|i| i.subjects.clone())
        .unwrap_or_default();
    let metadata = json!({
        "uuid": uuid,
        "title": title,
        "title_sort": title,
        "authors": if authors.is_empty() { vec!["Unknown".to_owned()] } else { authors.clone() },
        "author_sort": author_sort,
        "tags": tags,
        "series": series,
        "series_index": series_index,
        "_series_sort_": series.as_str().unwrap_or(""),
        "lpath": lpath,
        "size": bytes.len(),
        "last_modified": now,
        "mime": mime(&ext),
    });
    Ok(Outgoing {
        lpath,
        metadata,
        bytes,
        title,
    })
}

/// A book made ready from `source` (imported if need be, then cleaned, as a Kobo gets it:
/// [`deckle_core::reader::book_epub`]), described from what it became. The calibre id the
/// source carries is kept.
pub fn built(source: &Path, now: &str) -> Result<Outgoing, String> {
    let bytes = deckle_core::reader::book_epub(source, None)?;
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "book".into());
    let file = work.path().join(format!("{stem}.epub"));
    std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
    let mut out = from_file(&file, now)?;
    let source_id = source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
        .then(|| deckle_core::inspect::inspect(source).ok())
        .flatten()
        .and_then(|i| i.calibre_id);
    if let Some(id) = source_id {
        out.metadata["uuid"] = json!(id);
    }
    Ok(out)
}

fn mime(ext: &str) -> &'static str {
    match ext {
        "epub" | "kepub" => "application/epub+zip",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// `<author>/<title> - <author>.<ext>`, as calibre's default template makes it, with each
/// part made safe for any reader's file system.
pub fn lpath(title: &str, authors: &[String], ext: &str) -> String {
    let author = authors
        .first()
        .map(String::as_str)
        .filter(|a| !a.trim().is_empty())
        .unwrap_or("Unknown");
    let dir = clean(author);
    let name = clean(&format!("{title} - {author}"));
    format!("{dir}/{name}.{ext}")
}

/// A path component with nothing a file system or KOReader's checks refuse: no separators, no
/// `..`, no control characters, no leading dot or trailing dot or space, at most 80 characters.
pub fn clean(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    out = out.chars().take(MAX_COMPONENT).collect();
    let trimmed = out.trim().trim_end_matches('.').trim();
    let mut t = trimmed.to_owned();
    if t.starts_with('.') || t.starts_with('_') {
        t.replace_range(..1, "x");
    }
    if t.is_empty() { "x".into() } else { t }
}

/// A UUID that stays the same for the same title and authors, for a book calibre never saw
/// (shaped as a version 5 UUID, from SHA-1).
pub fn stable_uuid(title: &str, authors: &[String]) -> String {
    let mut h = Sha1::new();
    h.update(b"deckle-book\0");
    h.update(title.trim().to_lowercase().as_bytes());
    for a in authors {
        h.update(b"\0");
        h.update(a.trim().to_lowercase().as_bytes());
    }
    let d = h.finalize();
    let mut b = [0u8; 16];
    b.copy_from_slice(&d[..16]);
    b[6] = (b[6] & 0x0f) | 0x50;
    b[8] = (b[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(b).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_safe_and_shaped_as_calibre_makes_them() {
        assert_eq!(
            lpath("Dune", &["Frank Herbert".into()], "epub"),
            "Frank Herbert/Dune - Frank Herbert.epub"
        );
        assert_eq!(
            lpath("../a/b: c?", &[], "epub"),
            "Unknown/x._a_b_ c_ - Unknown.epub"
        );
        assert_eq!(clean("..."), "x");
        assert_eq!(clean(".hidden"), "xhidden");
        assert!(clean(&"a".repeat(300)).len() <= MAX_COMPONENT);
    }

    #[test]
    fn a_book_keeps_its_uuid() {
        let a = stable_uuid("Dune", &["Frank Herbert".into()]);
        assert_eq!(a, stable_uuid(" dune ", &["frank herbert".into()]));
        assert_ne!(a, stable_uuid("Dune Messiah", &["Frank Herbert".into()]));
        assert_eq!(&a[14..15], "5");
    }
}
