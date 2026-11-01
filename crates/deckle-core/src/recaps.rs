// SPDX-License-Identifier: GPL-3.0-or-later
//! Chapter recaps (docs/superpowers/specs/2026-11-01-ai-extras-design.md): a short recap the
//! owner's LLM writes at the end of each chapter, labelled as AI-generated.
//!
//! Like [`crate::whoswho`], the text is read and the recaps are written in one pass over each
//! document's raw XHTML: nothing is parsed into a tree and written back.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use deckle_xray::llm::{self, Provider};

use crate::whoswho::{Book, decode, esc, scan, tag_end, tag_name, unzip};
use crate::xray::{LlmConfig, Post};

/// Chapters shorter than this get no recap (title pages, contents, epigraphs).
pub const MIN_WORDS: usize = 800;
/// Words per request on a self-hosted server: fits a local model's default 4k context.
pub const LOCAL_PART_WORDS: usize = 2_000;
/// Words per request on a hosted provider: in practice one request per chapter.
pub const HOSTED_PART_WORDS: usize = 40_000;

/// Output budget per request: a recap is ~150 tokens, the rest is room for reasoning models,
/// whose thinking counts against it.
pub const MAX_TOKENS: u64 = 4096;
/// Goes into the cache key: bump it when the prompts change.
const PROMPT_VERSION: u32 = 1;
/// Failed chapters named in the warnings; the rest are counted.
const MAX_WARNINGS: usize = 3;
/// Requests in a row that can't reach the LLM before the remaining chapters are skipped.
const MAX_UNREACHABLE: usize = 3;

/// Why a recap failed.
#[derive(Debug)]
struct Failure {
    message: String,
    /// The request didn't get an answer (connection, timeout or HTTP status).
    unreachable: bool,
}

const SYSTEM: &str =
    "You write short chapter recaps for an e-reader. Answer with the recap text only.";
const RULES: &str = "Say only what happens in this chapter. Do not guess at or hint about \
anything that happens later. Do not judge or praise the writing.";

#[derive(Clone, Debug)]
pub struct RecapOptions {
    pub llm: LlmConfig,
    /// Recaps are kept under `{cache_dir}/recaps/`; `None`: no cache.
    pub cache_dir: Option<PathBuf>,
}

/// What a book got.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecapSummary {
    /// Chapters long enough for a recap.
    pub chapters: usize,
    /// Recaps written into the book.
    pub recapped: usize,
    /// Of those, how many came from the cache.
    pub cached: usize,
    pub model: String,
    /// Chapters whose recap failed.
    pub warnings: Vec<String>,
}

/// Writes a recap at the end of each chapter of the EPUB `bytes`. The bytes come back unchanged
/// with `None` for a fixed-layout book, a book that already has recaps, or one with no chapter
/// of [`MIN_WORDS`]; unchanged with a summary when every recap failed. `Err` only for an EPUB
/// that can't be read. Real HTTP.
pub fn add(bytes: &[u8], opts: &RecapOptions) -> Result<(Vec<u8>, Option<RecapSummary>), String> {
    add_with(bytes, opts, &mut llm::http_post)
}

/// [`add`] with the HTTP call injected (tests).
pub fn add_with(
    bytes: &[u8],
    opts: &RecapOptions,
    post: &mut Post<'_>,
) -> Result<(Vec<u8>, Option<RecapSummary>), String> {
    let mut files = unzip(bytes)?;
    let book = Book::read(&files)?;
    if book.fixed_layout {
        return Ok((bytes.to_vec(), None));
    }
    let xhtml_of = |p: &str| {
        files
            .iter()
            .find(|(n, _)| n == p)
            .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
    };
    let docs: Vec<Option<String>> = book.spine.iter().map(|p| xhtml_of(p)).collect();
    if docs
        .iter()
        .flatten()
        .any(|x| x.contains("id=\"deckle-recap-"))
    {
        return Ok((bytes.to_vec(), None));
    }
    let labels = book.nav_labels(&files);

    // The chapters that get a recap, with their labels.
    let mut todo: Vec<(Chapter, String)> = Vec::new();
    for (di, (path, xhtml)) in book.spine.iter().zip(&docs).enumerate() {
        let Some(xhtml) = xhtml else { continue };
        if book.nonlinear.contains(path) {
            continue;
        }
        let chs = chapters(di, xhtml);
        let with_text = chs.iter().filter(|c| !c.text.trim().is_empty()).count();
        for c in chs {
            if word_count(&c.text) < MIN_WORDS {
                continue;
            }
            let label = c
                .heading
                .clone()
                .or_else(|| {
                    (with_text == 1)
                        .then(|| labels.get(path).cloned())
                        .flatten()
                })
                .unwrap_or_else(|| format!("Chapter {}", todo.len() + 1));
            todo.push((c, label));
        }
    }
    if todo.is_empty() {
        return Ok((bytes.to_vec(), None));
    }

    let mut writer = Writer::new(&opts.llm, &book, opts.cache_dir.as_deref(), post);
    let mut summary = RecapSummary {
        chapters: todo.len(),
        model: writer.model.clone(),
        ..Default::default()
    };
    let mut failed = Vec::new();
    let mut inserts: Vec<Vec<(usize, String)>> = vec![Vec::new(); docs.len()];
    // Requests that couldn't reach the LLM, in a row: a server that hangs costs each chapter a
    // full timeout, so after a few the rest are skipped.
    let mut unreachable = 0;
    let mut abandoned = 0;
    for (c, label) in &todo {
        if unreachable >= MAX_UNREACHABLE {
            abandoned += 1;
            continue;
        }
        match writer.recap(label, &c.text) {
            Ok((text, cached)) => {
                unreachable = 0;
                summary.recapped += 1;
                summary.cached += usize::from(cached);
                inserts[c.doc].push((c.insert_at, aside(c, &text, &writer.model)));
            }
            Err(e) => {
                unreachable = if e.unreachable { unreachable + 1 } else { 0 };
                failed.push(format!("Chapter recap skipped for {label}: {}", e.message));
            }
        }
    }
    if failed.len() > MAX_WARNINGS {
        let more = failed.len() - MAX_WARNINGS;
        failed.truncate(MAX_WARNINGS);
        failed.push(format!("…and {more} more chapter recaps skipped"));
    }
    if abandoned > 0 {
        failed.push(format!(
            "The LLM stopped answering: {abandoned} more chapters skipped"
        ));
    }
    summary.warnings = failed;
    if summary.recapped == 0 {
        return Ok((bytes.to_vec(), Some(summary)));
    }

    for (di, mut ins) in inserts.into_iter().enumerate() {
        if ins.is_empty() {
            continue;
        }
        let (Some(xhtml), Some(slot)) = (
            &docs[di],
            files.iter_mut().find(|(n, _)| *n == book.spine[di]),
        ) else {
            continue;
        };
        let mut out = xhtml.clone();
        // Back to front, so earlier offsets stay put.
        ins.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        for (at, html) in ins {
            out.insert_str(at, &html);
        }
        slot.1 = out.into_bytes();
    }
    Ok((crate::kepub::write(&files)?, Some(summary)))
}

/// The recap's markup (design §7).
fn aside(c: &Chapter, text: &str, model: &str) -> String {
    format!(
        "<aside class=\"deckle-recap\" id=\"deckle-recap-d{}-s{}\" role=\"note\" \
         style=\"margin-top: 2em; padding-top: 0.6em; border-top: 1px solid currentColor; \
         font-size: 0.9em;\"><p style=\"font-weight: bold; margin: 0;\">Chapter recap</p>\
         <p style=\"margin: 0.3em 0 0;\">{}</p><p style=\"font-size: 0.8em; font-style: italic; \
         margin: 0.3em 0 0;\">AI-generated summary ({}), not part of the original book.</p></aside>",
        c.doc,
        c.section,
        esc(text),
        esc(model)
    )
}

/// Asks the LLM for recaps, through the cache.
struct Writer<'a, 'p> {
    provider: Provider,
    model: String,
    key: String,
    endpoint: String,
    /// Words per request.
    part_words: usize,
    header: String,
    cache: Option<PathBuf>,
    post: &'a mut Post<'p>,
}

impl<'a, 'p> Writer<'a, 'p> {
    fn new(
        cfg: &LlmConfig,
        book: &Book,
        cache_dir: Option<&std::path::Path>,
        post: &'a mut Post<'p>,
    ) -> Self {
        let model = if cfg.model.is_empty() {
            cfg.provider.default_model().to_owned()
        } else {
            cfg.model.clone()
        };
        let key = if cfg.api_key.is_empty() {
            std::env::var(cfg.provider.env_key()).unwrap_or_default()
        } else {
            cfg.api_key.clone()
        };
        let endpoint = cfg.endpoint.trim_end_matches('/');
        // A self-hosted OpenAI-compatible server, as `llm` tells them apart.
        let local = cfg.provider == Provider::OpenAi
            && !endpoint.is_empty()
            && endpoint != Provider::OpenAi.default_endpoint();
        Writer {
            provider: cfg.provider,
            model,
            key,
            endpoint: cfg.endpoint.clone(),
            part_words: if local {
                LOCAL_PART_WORDS
            } else {
                HOSTED_PART_WORDS
            },
            header: format!(
                "Book: {} · Author: {} · Language: {}",
                book.title, book.author, book.lang
            ),
            cache: cache_dir.map(|d| d.join("recaps")),
            post,
        }
    }

    /// The chapter's recap, and whether it came from the cache.
    fn recap(&mut self, label: &str, text: &str) -> Result<(String, bool), Failure> {
        let file = self
            .cache
            .as_ref()
            .map(|d| d.join(format!("{}.txt", self.key_of(text))));
        if let Some(hit) = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok())
            && !hit.trim().is_empty()
        {
            return Ok((hit.trim().to_owned(), true));
        }
        let head = format!("{} · Chapter: {label}", self.header);
        let parts = parts(text, self.part_words);
        let recap = if parts.len() <= 1 {
            self.ask(&format!(
                "{head}\n\nWrite a recap of this chapter in 2–4 sentences, in the book's \
                 language, present tense. {RULES}\n\n{text}"
            ))?
        } else {
            let n = parts.len();
            let mut got = Vec::with_capacity(n);
            for (i, part) in parts.iter().enumerate() {
                got.push(self.ask(&format!(
                    "{head}\n\nThis is part {} of {n} of the chapter. Write a recap of this part \
                     in 1–2 sentences, in the book's language, present tense. {RULES}\n\n{part}",
                    i + 1
                ))?);
            }
            self.ask(&format!(
                "{head}\n\nThese are recaps of the parts of one chapter, in order. Combine them \
                 into one recap of the chapter in 2–4 sentences, in the book's language, present \
                 tense. {RULES}\n\n{}",
                got.join("\n")
            ))?
        };
        if let Some(f) = file {
            // A cache that can't be written only costs a request next time.
            let _ = f
                .parent()
                .map(std::fs::create_dir_all)
                .transpose()
                .and_then(|_| std::fs::write(&f, &recap));
        }
        Ok((recap, false))
    }

    fn ask(&mut self, user: &str) -> Result<String, Failure> {
        let raw = llm::complete(
            self.provider,
            &self.model,
            &self.key,
            &self.endpoint,
            SYSTEM,
            user,
            MAX_TOKENS,
            self.post,
        )
        .map_err(|e| Failure {
            unreachable: matches!(e, llm::LlmError::Http(_)),
            message: e.to_string(),
        })?;
        clean(&raw).ok_or_else(|| Failure {
            message: "the LLM answered with an empty recap".to_owned(),
            unreachable: false,
        })
    }

    /// Cache file name: the prompt version, the provider, the model and the chapter's text.
    fn key_of(&self, text: &str) -> String {
        let mut h = Sha256::new();
        h.update(format!("{PROMPT_VERSION}\0{:?}\0{}\0", self.provider, self.model).as_bytes());
        h.update(text.as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// The recap without a "Recap:" label, wrapping quotes or emphasis; `None` when nothing's left.
fn clean(raw: &str) -> Option<String> {
    // Characters XML 1.0 forbids would make the chapter unreadable to Previewer.
    let raw: String = raw
        .chars()
        .filter(|&c| !matches!(c, '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}'))
        .collect();
    let mut s = raw.trim();
    for label in ["recap:", "summary:"] {
        if s.get(..label.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(label))
        {
            s = s[label.len()..].trim_start();
        }
    }
    for (open, close) in [("\"", "\""), ("“", "”"), ("**", "**"), ("*", "*")] {
        if s.len() > open.len() + close.len() && s.starts_with(open) && s.ends_with(close) {
            s = s[open.len()..s.len() - close.len()].trim();
        }
    }
    (!s.is_empty()).then(|| s.to_owned())
}

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
/// Inline elements that, empty, are link targets for the heading after them.
const ANCHORS: &[&str] = &["a", "span"];

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
            // A TOC target (`<a id="ch3"/>`) belongs to the chapter after it.
            Some((name, false, true)) if ANCHORS.contains(&name) => at = lt,
            // So does an empty one, `<a id="ch3"></a>`.
            Some((name, true, false)) if ANCHORS.contains(&name) => {
                let inner = before[..lt].trim_end();
                let open = inner.rfind('<').filter(|_| inner.ends_with('>'));
                match open.and_then(|o| tag_name(&inner[o..]).map(|t| (o, t))) {
                    Some((o, (n, false, false))) if n == name => at = o,
                    _ => return at,
                }
            }
            _ => return at,
        }
    }
}
