// SPDX-License-Identifier: GPL-3.0-or-later
//! Chapter recaps, the book summary and the series recap
//! (docs/superpowers/specs/2026-11-01-ai-extras-design.md): a short recap the owner's LLM writes
//! at the end of each chapter, a Summary page made from those recaps, and a Previously page made
//! from the earlier books of a series, all labelled as AI-generated.
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
/// Recapped chapters a book needs for a summary: one chapter's recap already is one.
const MIN_SUMMARY_CHAPTERS: usize = 2;

/// Why a recap failed.
#[derive(Debug)]
struct Failure {
    message: String,
    /// The request didn't get an answer (connection, timeout or HTTP status).
    unreachable: bool,
}

const SYSTEM: &str =
    "You write short chapter recaps for an e-reader. Answer with the recap text only.";
const SUMMARY_SYSTEM: &str =
    "You write book summaries for an e-reader. Answer with the summary text only.";
/// The Summary page's file, next to the OPF (design §13.4).
pub const SUMMARY_PAGE: &str = "deckle-summary.xhtml";
/// The Previously page's file, next to the OPF (design §14.4).
pub const PREVIOUSLY_PAGE: &str = "deckle-previously.xhtml";
const RULES: &str = "Say only what happens in this chapter. Do not guess at or hint about \
anything that happens later. Do not judge or praise the writing.";
const SUMMARY_RULES: &str = "Do not judge or praise the writing.";

#[derive(Clone, Debug)]
pub struct RecapOptions {
    pub llm: LlmConfig,
    /// Recaps are kept under `{cache_dir}/recaps/`; `None`: no cache.
    pub cache_dir: Option<PathBuf>,
    /// The recaps go at the end of their chapters (design §7).
    pub chapters: bool,
    /// A Summary page made from the recaps goes after the last chapter (design §13).
    pub summary: bool,
    /// A Previously page made from the earlier books of the series goes before the first chapter
    /// (design §14). Needs `cache_dir`, where the earlier books' records are.
    pub series: bool,
}

/// What a run asks for, without the LLM, which comes from the run's own setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extras {
    /// Recaps are cached under `{cache_dir}/recaps/`.
    pub cache_dir: PathBuf,
    pub chapters: bool,
    pub summary: bool,
    pub series: bool,
}

impl Extras {
    /// Chapter recaps only, cached in `cache_dir`.
    pub fn chapters(cache_dir: PathBuf) -> Extras {
        Extras {
            cache_dir,
            chapters: true,
            summary: false,
            series: false,
        }
    }

    pub fn options(&self, llm: LlmConfig) -> RecapOptions {
        RecapOptions {
            llm,
            cache_dir: Some(self.cache_dir.clone()),
            chapters: self.chapters,
            summary: self.summary,
            series: self.series,
        }
    }

    /// The extras asked for, by name, for warnings: "chapter recaps and the book summary".
    pub fn names(&self) -> String {
        let named: Vec<&str> = [
            (self.chapters, "chapter recaps"),
            (self.summary, "the book summary"),
            (self.series, "the series recap"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        match named.as_slice() {
            [] => String::new(),
            [one] => (*one).to_owned(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }

    /// The warning for a run without an LLM.
    pub fn needs_llm(&self) -> String {
        let names = self.names();
        let verb = if self.chapters || names.contains(" and ") {
            "need"
        } else {
            "needs"
        };
        let mut c = names.chars();
        let first: String = c
            .next()
            .map(|f| f.to_uppercase().chain(c).collect())
            .unwrap_or_default();
        format!("{first} {verb} an LLM; set one in Settings")
    }
}

/// What a book got.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecapSummary {
    /// Chapters long enough for a recap.
    pub chapters: usize,
    /// Recaps made (written into the chapters when [`RecapOptions::chapters`] is on).
    pub recapped: usize,
    /// Of those, how many came from the cache.
    pub cached: usize,
    pub model: String,
    /// A Summary page went in.
    #[serde(default)]
    pub summary: bool,
    /// A Previously page went in.
    #[serde(default)]
    pub series: bool,
    /// Chapters whose recap failed, and a summary that couldn't be made.
    pub warnings: Vec<String>,
}

/// Writes a recap at the end of each chapter of the EPUB `bytes` and/or a Summary page after the
/// last one, as `opts` asks. The bytes come back unchanged with `None` for a fixed-layout book, a
/// book that already has recaps, or one with no chapter of [`MIN_WORDS`]; unchanged with a
/// summary when every recap failed. `Err` only for an EPUB that can't be read. Real HTTP.
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
    let has = |path: &str| files.iter().any(|(n, _)| n == path);
    let summary_path = book.page_at(SUMMARY_PAGE);
    let want_summary = opts.summary && !has(&summary_path);
    // The book's place in its series, for its record and its Previously page (design §14).
    let me = opts
        .cache_dir
        .as_ref()
        .filter(|_| !book.fixed_layout)
        .and_then(|_| series_book(bytes));
    let previously_path = book.page_at(PREVIOUSLY_PAGE);
    let want_series = opts.series && me.is_some() && !has(&previously_path);
    if book.fixed_layout || !(opts.chapters || want_summary || want_series) {
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
            if c.matter || word_count(&c.text) < MIN_WORDS {
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
    // `{label}: {recap}` in reading order, for the summary.
    let mut made: Vec<String> = Vec::new();
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
                made.push(format!("{label}: {text}"));
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
    let missing = todo.len() - summary.recapped;
    summary.warnings = failed;
    if summary.recapped == 0 {
        return Ok((bytes.to_vec(), Some(summary)));
    }

    // The book summary (design §13): from every chapter's recap, or not at all.
    let mut summary_text = None;
    if want_summary && missing > 0 {
        summary.warnings.push(format!(
            "Book summary skipped: {missing} {} no recap",
            if missing == 1 {
                "chapter has"
            } else {
                "chapters have"
            }
        ));
    } else if want_summary && made.len() >= MIN_SUMMARY_CHAPTERS {
        match writer.summary(&made) {
            Ok((text, _)) => summary_text = Some(text),
            Err(e) => summary
                .warnings
                .push(format!("Book summary skipped: {}", e.message)),
        }
    }

    // This book's record for the later books of its series (design §14.2), and its Previously
    // page from the earlier ones.
    let mut previously = None;
    if let (Some(dir), Some(me)) = (&opts.cache_dir, &me) {
        if missing == 0 {
            write_record(dir, me, &writer.model, &made, summary_text.as_deref());
        }
        if want_series {
            let earlier = earlier_records(dir, me);
            // Whole-numbered books expect one record per earlier number.
            let expected = if me.index.fract() == 0.0 {
                me.index as usize - 1
            } else {
                0
            };
            if earlier.is_empty() {
                if me.index > 1.0 {
                    summary.warnings.push(format!(
                        "Series recap skipped: no earlier book of {} was sent with recaps",
                        me.series
                    ));
                }
            } else {
                if earlier.len() < expected {
                    summary.warnings.push(format!(
                        "Series recap covers {} of {expected} earlier books",
                        earlier.len()
                    ));
                }
                match writer.previously(me, &earlier) {
                    Ok((text, _)) => {
                        previously =
                            Some(previously_page(&book, me, &earlier, &text, &writer.model))
                    }
                    Err(e) => summary
                        .warnings
                        .push(format!("Series recap skipped: {}", e.message)),
                }
            }
        }
    }
    if !opts.chapters && summary_text.is_none() && previously.is_none() {
        return Ok((bytes.to_vec(), Some(summary)));
    }

    if opts.chapters {
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
    }
    if let Some(text) = summary_text {
        files.push((
            summary_path,
            summary_page(&book, &text, &writer.model).into_bytes(),
        ));
        book.register_page(&mut files, "deckle-summary", SUMMARY_PAGE, "Summary")?;
        summary.summary = true;
    }
    if let Some(page) = previously {
        files.push((previously_path, page.into_bytes()));
        let first = &book.spine[todo[0].0.doc];
        book.register_page_before(
            &mut files,
            ("deckle-previously", PREVIOUSLY_PAGE, "Previously"),
            first,
        )?;
        summary.series = true;
    }
    Ok((crate::kepub::write(&files)?, Some(summary)))
}

/// A book's place in its series (design §14.2), from its OPF as `inspect` reads it.
#[derive(Clone, Debug)]
struct SeriesBook {
    title: String,
    author: String,
    series: String,
    index: f32,
}

fn series_book(bytes: &[u8]) -> Option<SeriesBook> {
    let dir = tempfile::tempdir().ok()?;
    let path = dir.path().join("book.epub");
    std::fs::write(&path, bytes).ok()?;
    let info = crate::inspect::inspect(&path).ok()?;
    let series = info.series?;
    Some(SeriesBook {
        author: info
            .creators
            .first()
            .map(|c| c.name.clone())
            .unwrap_or_default(),
        title: info.title,
        series: series.name,
        index: series.index.filter(|i| i.is_finite() && *i > 0.0)?,
    })
}

/// What a recapped book of a series leaves for the later books (design §14.2).
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Record {
    title: String,
    author: String,
    series: String,
    index: f32,
    model: String,
    recaps: Vec<String>,
    summary: Option<String>,
}

/// Lowercased, whitespace collapsed: how authors and series names are matched.
fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn records_dir(cache_dir: &std::path::Path) -> PathBuf {
    cache_dir.join("recaps").join("books")
}

fn write_record(
    cache_dir: &std::path::Path,
    me: &SeriesBook,
    model: &str,
    recaps: &[String],
    summary: Option<&str>,
) {
    let mut h = Sha256::new();
    h.update(format!("{}\0{}\0{}", norm(&me.author), norm(&me.series), me.index).as_bytes());
    let key: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let record = Record {
        title: me.title.clone(),
        author: me.author.clone(),
        series: me.series.clone(),
        index: me.index,
        model: model.to_owned(),
        recaps: recaps.to_vec(),
        summary: summary.map(str::to_owned),
    };
    let dir = records_dir(cache_dir);
    // A record that can't be written only means a later book's Previously page lacks this one.
    if let Ok(json) = serde_json::to_vec_pretty(&record) {
        let _ = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(dir.join(format!("{key}.json")), json));
    }
}

/// The records of `me`'s series and author with a lower number, in series order.
fn earlier_records(cache_dir: &std::path::Path, me: &SeriesBook) -> Vec<Record> {
    let (author, series) = (norm(&me.author), norm(&me.series));
    let mut out: Vec<Record> = std::fs::read_dir(records_dir(cache_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<Record>(&b).ok())
        .filter(|r| norm(&r.author) == author && norm(&r.series) == series && r.index < me.index)
        .collect();
    out.sort_by(|a, b| a.index.total_cmp(&b.index));
    out.dedup_by(|a, b| a.index == b.index);
    out
}

/// A series number as people write it: `2`, `2.5`.
fn number(index: f32) -> String {
    if index.fract() == 0.0 {
        format!("{}", index as i64)
    } else {
        format!("{index}")
    }
}

/// The Previously page (design §14.4).
fn previously_page(
    book: &Book,
    me: &SeriesBook,
    earlier: &[Record],
    text: &str,
    model: &str,
) -> String {
    let covered = earlier
        .iter()
        .map(|r| format!("Book {}: {}", number(r.index), r.title))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html \
         xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{l}\" lang=\"{l}\"><head><title>Previously</title></head><body>\
         <section epub:type=\"prologue\" id=\"deckle-previously\"><h1>Previously</h1>\
         <p><i>What happened in the earlier books of {s}: {c}.</i></p>{p}\
         <p style=\"font-size: 0.8em; font-style: italic;\">AI-generated summary ({m}), not part \
         of the original book.</p></section></body></html>\n",
        l = esc(&book.lang),
        s = esc(&me.series),
        c = esc(&covered),
        p = paragraphs(text),
        m = esc(model)
    )
}

/// `text`'s paragraphs (split on blank lines) as `<p>`s.
fn paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            format!(
                "<p>{}</p>",
                esc(&p.split_whitespace().collect::<Vec<_>>().join(" "))
            )
        })
        .collect()
}

/// The Summary page (design §13.4): `text`'s paragraphs, split on blank lines.
fn summary_page(book: &Book, text: &str, model: &str) -> String {
    let paras = paragraphs(text);
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html \
         xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{l}\" lang=\"{l}\"><head><title>Summary</title></head><body>\
         <section epub:type=\"appendix\" id=\"deckle-summary\"><h1>Summary</h1>\
         <p><i>This summary covers the whole book, including the ending.</i></p>{paras}\
         <p style=\"font-size: 0.8em; font-style: italic;\">AI-generated summary ({m}), not part \
         of the original book.</p></section></body></html>\n",
        l = esc(&book.lang),
        m = esc(model)
    )
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
        store(file, &recap);
        Ok((recap, false))
    }

    /// The book summary from `recaps` (`{label}: {recap}`, in order), and whether it came from
    /// the cache (design §13.3).
    fn summary(&mut self, recaps: &[String]) -> Result<(String, bool), Failure> {
        let head = self.header.clone();
        self.condense(
            "summary",
            &recaps.join("\n\n"),
            |all| {
                format!(
                    "{head}\n\nThese are recaps of each chapter of the book, in order. Write a \
                     summary of the whole book in 2–3 short paragraphs, in the book's language, \
                     past tense, ending included. {SUMMARY_RULES}\n\n{all}"
                )
            },
            |i, n, part| {
                format!(
                    "{head}\n\nThese are recaps of the chapters in part {i} of {n} of the book, \
                     in order. Write a one-paragraph summary of this part, in the book's \
                     language, past tense. {SUMMARY_RULES}\n\n{part}"
                )
            },
            |got| {
                format!(
                    "{head}\n\nThese are summaries of the parts of the book, in order. Combine \
                     them into one summary of the whole book in 2–3 short paragraphs, in the \
                     book's language, past tense, ending included. {SUMMARY_RULES}\n\n{got}"
                )
            },
        )
    }

    /// The Previously page's text from the earlier books' records, in series order, and whether
    /// it came from the cache (design §14.3). Nothing of this book's text is sent.
    fn previously(
        &mut self,
        me: &SeriesBook,
        earlier: &[Record],
    ) -> Result<(String, bool), Failure> {
        let head = format!(
            "{} · Series: {}, book {}",
            self.header,
            me.series,
            number(me.index)
        );
        let input = earlier
            .iter()
            .map(|r| {
                let body = r.summary.clone().unwrap_or_else(|| r.recaps.join("\n"));
                format!("Book {}: {}\n{body}", number(r.index), r.title)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let ask = "Write a recap of what happened in them for a reader about to start this \
                   book, in 2–4 short paragraphs, in this book's language, past tense, including \
                   how each book ended.";
        self.condense(
            "previously",
            &input,
            |all| {
                format!(
                    "{head}\n\nThese are summaries of the earlier books in this series, in \
                     order. {ask} {SUMMARY_RULES}\n\n{all}"
                )
            },
            |i, n, part| {
                format!(
                    "{head}\n\nThis is part {i} of {n} of the summaries of the earlier books in \
                     this series, in order. Write a one-paragraph recap of this part, in this \
                     book's language, past tense. {SUMMARY_RULES}\n\n{part}"
                )
            },
            |got| {
                format!(
                    "{head}\n\nThese are recaps of the parts of the earlier books in this \
                     series, in order. Combine them. {ask} {SUMMARY_RULES}\n\n{got}"
                )
            },
        )
    }

    /// One text from `input`, through the cache (`{prefix}-{key}.txt`): one request when it fits
    /// a request, else a request per part (`part(i, n, part)`, from 1) and one to `merge` them.
    fn condense(
        &mut self,
        prefix: &str,
        input: &str,
        whole: impl Fn(&str) -> String,
        part: impl Fn(usize, usize, &str) -> String,
        merge: impl Fn(&str) -> String,
    ) -> Result<(String, bool), Failure> {
        let file = self
            .cache
            .as_ref()
            .map(|d| d.join(format!("{prefix}-{}.txt", self.key_of(input))));
        if let Some(hit) = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok())
            && !hit.trim().is_empty()
        {
            return Ok((hit.trim().to_owned(), true));
        }
        let parts = parts(input, self.part_words);
        let text = if parts.len() <= 1 {
            self.ask_as(SUMMARY_SYSTEM, &whole(input))?
        } else {
            let n = parts.len();
            let mut got = Vec::with_capacity(n);
            for (i, p) in parts.iter().enumerate() {
                got.push(self.ask_as(SUMMARY_SYSTEM, &part(i + 1, n, p))?);
            }
            self.ask_as(SUMMARY_SYSTEM, &merge(&got.join("\n\n")))?
        };
        store(file, &text);
        Ok((text, false))
    }

    fn ask(&mut self, user: &str) -> Result<String, Failure> {
        self.ask_as(SYSTEM, user)
    }

    fn ask_as(&mut self, system: &str, user: &str) -> Result<String, Failure> {
        let raw = llm::complete(
            self.provider,
            &self.model,
            &self.key,
            &self.endpoint,
            system,
            user,
            MAX_TOKENS,
            self.post,
        )
        .map_err(|e| Failure {
            unreachable: matches!(e, llm::LlmError::Http(_)),
            message: e.to_string(),
        })?;
        clean(&raw).ok_or_else(|| Failure {
            message: "the LLM answered with nothing".to_owned(),
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

/// Keeps `text` in the cache `file`; one that can't be written only costs a request next time.
fn store(file: Option<PathBuf>, text: &str) {
    if let Some(f) = file {
        let _ = f
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::write(&f, text));
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
    /// Front or back matter (a licence, acknowledgments, an index): no recap.
    pub(crate) matter: bool,
}

/// Elements a recap is never put in front of when backing up from a heading.
const NOT_BEFORE: &[&str] = &["html", "head", "body"];
/// Inline elements that, empty, are link targets for the heading after them.
const ANCHORS: &[&str] = &["a", "span"];
/// `epub:type` (and `role="doc-…"`) values that mark front and back matter.
const MATTER_TYPES: &[&str] = &[
    "frontmatter",
    "backmatter",
    "titlepage",
    "halftitlepage",
    "copyright-page",
    "colophon",
    "imprint",
    "imprimatur",
    "acknowledgments",
    "credits",
    "contributors",
    "dedication",
    "toc",
    "loi",
    "lot",
    "index",
    "bibliography",
    "glossary",
    "endnotes",
    "rearnotes",
    "errata",
];
/// Heading words that name front and back matter.
const MATTER_NAMES: &[&str] = &[
    "project gutenberg",
    "copyright",
    "acknowledg",
    "about the author",
    "bibliography",
    "colophon",
];
/// Elements with no closing tag in HTML (XHTML closes them itself).
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "wbr",
];

/// Whether an opening tag marks its element as front or back matter: by `epub:type`, by
/// `role="doc-…"`, or Project Gutenberg's `pg-boilerplate` class.
fn marks_matter(tag: &str) -> bool {
    let tokens = |name: &str| {
        attr(tag, name)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    tokens("epub:type")
        .iter()
        .any(|t| MATTER_TYPES.contains(&t.as_str()))
        || tokens("role").iter().any(|t| {
            t.strip_prefix("doc-")
                .is_some_and(|t| MATTER_TYPES.contains(&t))
        })
        || tokens("class").iter().any(|t| t == "pg-boilerplate")
}

/// The value of attribute `name` in an opening tag.
fn attr<'t>(tag: &'t str, name: &str) -> Option<&'t str> {
    let mut from = 0;
    while let Some(at) = tag[from..].find(name).map(|a| from + a) {
        from = at + name.len();
        let before = tag[..at].chars().next_back();
        let rest = tag[from..].trim_start();
        if !before.is_some_and(char::is_whitespace) || !rest.starts_with('=') {
            continue;
        }
        let rest = rest[1..].trim_start();
        let quote = rest.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        return rest[1..].find(quote).map(|e| &rest[1..1 + e]);
    }
    None
}

/// The chapters of document `doc_index`, in order (the first may be empty: text before the
/// first heading).
pub(crate) fn chapters(doc_index: usize, xhtml: &str) -> Vec<Chapter> {
    // Each opening h1/h2: (byte offset of its tag, its text, inside front or back matter).
    // Counted as `scan` counts them.
    let mut headings: Vec<(usize, String, bool)> = Vec::new();
    // (tag start, content start, name, inside matter)
    let mut open: Option<(usize, usize, &str, bool)> = None;
    // The open elements, and whether each marks matter.
    let mut stack: Vec<(&str, bool)> = Vec::new();
    let mut body_matter = false;
    let b = xhtml.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let end = tag_end(xhtml, i);
        let tag = &xhtml[i..end];
        let Some((name, closing, selfclosing)) = tag_name(tag) else {
            i = end;
            continue;
        };
        if closing {
            if let Some(k) = stack.iter().rposition(|(n, _)| *n == name) {
                stack.truncate(k);
            }
        } else if !selfclosing && !VOID.contains(&name) {
            stack.push((name, marks_matter(tag)));
            if name == "body" {
                body_matter = stack.iter().any(|(_, m)| *m);
            }
        }
        if name == "h1" || name == "h2" {
            let matter = stack.iter().any(|(_, m)| *m) || (selfclosing && marks_matter(tag));
            if !closing {
                if let Some((start, from, _, m)) = open.take() {
                    headings.push((start, heading_text(&xhtml[from..i]), m));
                }
                if selfclosing {
                    headings.push((i, String::new(), matter));
                } else {
                    open = Some((i, end, name, matter));
                }
            } else if let Some((start, from, n, m)) = open
                && n == name
            {
                headings.push((start, heading_text(&xhtml[from..i]), m));
                open = None;
            }
        }
        i = end;
    }
    if let Some((start, from, _, m)) = open {
        headings.push((start, heading_text(&xhtml[from..]), m));
    }

    let body_end = xhtml.rfind("</body").unwrap_or(xhtml.len());
    let doc = scan(xhtml);
    let mut out: Vec<Chapter> = (0..=headings.len())
        .map(|section| Chapter {
            doc: doc_index,
            section,
            insert_at: headings
                .get(section)
                .map_or(body_end, |&(at, _, _)| back_up(xhtml, at)),
            heading: section
                .checked_sub(1)
                .map(|h| headings[h].1.clone())
                .filter(|h| !h.is_empty()),
            text: String::new(),
            matter: match section.checked_sub(1).map(|h| &headings[h]) {
                None => body_matter,
                Some((_, text, marked)) => {
                    let text = text.to_lowercase();
                    *marked || MATTER_NAMES.iter().any(|n| text.contains(n))
                }
            },
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
