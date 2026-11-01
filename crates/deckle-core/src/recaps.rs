// SPDX-License-Identifier: GPL-3.0-or-later
//! Chapter recaps (docs/superpowers/specs/2026-11-01-ai-extras-design.md): a short recap the
//! owner's LLM writes at the end of each chapter, labelled as AI-generated.
//!
//! Like [`crate::whoswho`], the text is read and the recaps are written in one pass over each
//! document's raw XHTML: nothing is parsed into a tree and written back.

use crate::whoswho::{decode, scan, tag_end, tag_name};

/// Chapters shorter than this get no recap (title pages, contents, epigraphs).
pub const MIN_WORDS: usize = 800;
/// Words per request on a self-hosted server: fits a local model's default 4k context.
pub const LOCAL_PART_WORDS: usize = 2_000;
/// Words per request on a hosted provider: in practice one request per chapter.
pub const HOSTED_PART_WORDS: usize = 40_000;

/// A character that is a word by itself: Han, kana, Hangul and Thai aren't written with spaces.
fn one_char_word(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{3040}'..='\u{30FF}'
        | '\u{AC00}'..='\u{D7AF}'
        | '\u{0E00}'..='\u{0E7F}')
}

/// Byte ranges of the words of `text` (see [`word_count`]).
fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() || one_char_word(c) {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
            if one_char_word(c) {
                out.push((i, i + c.len_utf8()));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}

/// Words in `text`: whitespace-separated runs, except that each Han, kana, Hangul or Thai
/// character is a word on its own.
pub fn word_count(text: &str) -> usize {
    word_spans(text).len()
}

/// `text` in parts of at most `max_words`: its paragraphs (`\n\n`-separated) packed in order, a
/// paragraph longer than `max_words` cut at word boundaries.
pub fn parts(text: &str, max_words: usize) -> Vec<String> {
    let max_words = max_words.max(1);
    let mut pieces: Vec<(&str, usize)> = Vec::new();
    for para in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        let spans = word_spans(para);
        if spans.len() <= max_words {
            pieces.push((para, spans.len()));
            continue;
        }
        for chunk in spans.chunks(max_words) {
            pieces.push((&para[chunk[0].0..chunk[chunk.len() - 1].1], chunk.len()));
        }
    }
    let mut out = Vec::new();
    let mut part = String::new();
    let mut words = 0;
    for (piece, n) in pieces {
        if words > 0 && words + n > max_words {
            out.push(std::mem::take(&mut part));
            words = 0;
        }
        if !part.is_empty() {
            part.push_str("\n\n");
        }
        part.push_str(piece);
        words += n;
    }
    if !part.is_empty() {
        out.push(part);
    }
    out
}

/// One chapter of a spine document: a document split at each `h1`/`h2`, as in Who's who.
#[derive(Clone, Debug)]
pub(crate) struct Chapter {
    /// Spine index of its document.
    pub(crate) doc: usize,
    /// Count of `h1`/`h2` headings before it in its document (Who's who's `section`).
    pub(crate) section: usize,
    /// Byte offset in the document's XHTML where its recap goes.
    pub(crate) insert_at: usize,
    /// The text of the heading that opens it.
    pub(crate) heading: Option<String>,
    /// Its text, blocks separated by a blank line.
    pub(crate) text: String,
}

/// Elements a recap is never put in front of when backing up from a heading.
const NOT_BEFORE: &[&str] = &["html", "head", "body"];

/// The chapters of document `doc_index`, in order (the first may be empty: text before the
/// first heading).
pub(crate) fn chapters(doc_index: usize, xhtml: &str) -> Vec<Chapter> {
    // Each opening h1/h2: (byte offset of its tag, its text). Counted as `scan` counts them.
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut open: Option<(usize, usize, &str)> = None; // (tag start, content start, name)
    let b = xhtml.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let end = tag_end(xhtml, i);
        if let Some((name, closing, selfclosing)) = tag_name(&xhtml[i..end])
            && (name == "h1" || name == "h2")
        {
            if !closing {
                if let Some((start, from, _)) = open.take() {
                    headings.push((start, heading_text(&xhtml[from..i])));
                }
                if selfclosing {
                    headings.push((i, String::new()));
                } else {
                    open = Some((i, end, name));
                }
            } else if let Some((start, from, n)) = open
                && n == name
            {
                headings.push((start, heading_text(&xhtml[from..i])));
                open = None;
            }
        }
        i = end;
    }
    if let Some((start, from, _)) = open {
        headings.push((start, heading_text(&xhtml[from..])));
    }

    let body_end = xhtml.rfind("</body").unwrap_or(xhtml.len());
    let doc = scan(xhtml);
    let mut out: Vec<Chapter> = (0..=headings.len())
        .map(|section| Chapter {
            doc: doc_index,
            section,
            insert_at: headings
                .get(section)
                .map_or(body_end, |&(at, _)| back_up(xhtml, at)),
            heading: section
                .checked_sub(1)
                .map(|h| headings[h].1.clone())
                .filter(|h| !h.is_empty()),
            text: String::new(),
        })
        .collect();
    let mut last_block = None;
    for seg in &doc.segs {
        let Some(ch) = out.get_mut(seg.section) else {
            continue;
        };
        if !ch.text.is_empty() && last_block != Some((seg.section, seg.block)) {
            ch.text.push_str("\n\n");
        }
        ch.text.push_str(&seg.text);
        last_block = Some((seg.section, seg.block));
    }
    out
}

/// A heading's raw inner XHTML as plain text, whitespace collapsed.
fn heading_text(raw: &str) -> String {
    let mut text = String::new();
    let mut i = 0;
    while i < raw.len() {
        if raw.as_bytes()[i] == b'<' {
            i = tag_end(raw, i);
            text.push(' ');
            continue;
        }
        let end = raw[i..].find('<').map_or(raw.len(), |e| i + e);
        text.push_str(&decode(&raw[i..end], 0).0);
        i = end;
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `at`, moved back over opening tags that come right before it (only whitespace between), so
/// a recap lands before `<div class="chapter"><h2>` rather than inside the next chapter.
fn back_up(xhtml: &str, mut at: usize) -> usize {
    loop {
        let before = xhtml[..at].trim_end();
        if !before.ends_with('>') {
            return at;
        }
        let Some(lt) = before.rfind('<') else {
            return at;
        };
        match tag_name(&before[lt..]) {
            Some((name, false, false)) if !NOT_BEFORE.contains(&name) => at = lt,
            _ => return at,
        }
    }
}
