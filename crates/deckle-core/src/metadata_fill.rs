// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in metadata fill (issue #28): title, author, series, description and subjects from Open
//! Library, by the book's ISBN (the edition record, then its work) or else a title search. Only
//! fields the EPUB leaves empty are filled; the book's own metadata always wins. Off by default,
//! like every Deckle network feature; a lookup that fails is a warning, never a failed send.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::inspect::{BookInfo, Creator, NO_TITLE_WARNING, Series};
use crate::matcher::{Fetcher, urlencode};

const OPEN_LIBRARY: &str = "https://openlibrary.org";
/// Open Library lists dozens of subjects for popular works; a Kindle shows a handful.
const MAX_SUBJECTS: usize = 10;
/// Author records fetched per book (one request each).
const MAX_AUTHORS: usize = 3;

/// What the lookup did, for the run report.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum FillOutcome {
    /// A record was found; `fields` are the ones it filled (possibly none).
    Filled {
        fields: Vec<&'static str>,
        source: &'static str,
    },
    /// The EPUB already has every field: nothing was looked up.
    Complete,
    /// Open Library has no record for this book.
    NotFound,
    /// The lookup failed (network, unreadable JSON); the book converts with its own metadata.
    Error { message: String },
}

/// Metadata from Open Library, or the part of it written into a book.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub series: Option<Series>,
    pub description: Option<String>,
    pub subjects: Vec<String>,
    /// The edition's, found only by ISBN; suggested in the book's editor, never filled at send
    /// time (Books tab redesign §6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// A W3C date (`1994`, `2020-09`, `2020-09-15`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// A language tag (`en`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl Metadata {
    pub fn is_empty(&self) -> bool {
        *self == Metadata::default()
    }
}

/// The fields `info` leaves empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Missing {
    pub title: bool,
    pub authors: bool,
    pub series: bool,
    pub description: bool,
    pub subjects: bool,
}

impl Missing {
    pub fn of(info: &BookInfo) -> Self {
        Missing {
            // `inspect` falls back to the file name when there is no dc:title.
            title: info.title.trim().is_empty()
                || info.warnings.iter().any(|w| w == NO_TITLE_WARNING),
            authors: info.creators.is_empty(),
            series: info.series.is_none(),
            description: info
                .description
                .as_deref()
                .is_none_or(|d| d.trim().is_empty()),
            subjects: info.subjects.is_empty(),
        }
    }

    pub fn any(&self) -> bool {
        self.title || self.authors || self.series || self.description || self.subjects
    }
}

pub fn edition_url(isbn: &str) -> String {
    format!("{OPEN_LIBRARY}/isbn/{isbn}.json")
}

pub fn search_url(title: &str, author: &str) -> String {
    format!(
        "{OPEN_LIBRARY}/search.json?title={}&author={}&fields=key,title,author_name,subject&limit=5",
        urlencode(title),
        urlencode(author)
    )
}

/// `/works/OL…W` or `/authors/OL…A` as a record URL; anything else is refused.
fn record_url(key: &str, kind: &str) -> Option<String> {
    let id = key.strip_prefix(&format!("/{kind}/"))?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| format!("{OPEN_LIBRARY}/{kind}/{id}.json"))
}

fn parse(body: &str) -> Result<Value, String> {
    serde_json::from_str(body).map_err(|e| format!("Open Library returned unreadable JSON: {e}"))
}

fn text(v: Option<&Value>) -> Option<String> {
    // A description is either a string or `{"type": "/type/text", "value": "…"}`.
    let s = match v? {
        Value::String(s) => s.as_str(),
        Value::Object(o) => o.get("value")?.as_str()?,
        _ => return None,
    };
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

fn strings(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in v
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(s)) {
            out.push(s.to_owned());
        }
    }
    out
}

/// `"The Lord of the Rings ; 1"`, `"Discworld, #3"`, `"(Dune -- bk. 2)"` → name and index.
pub fn parse_series(s: &str) -> Option<Series> {
    static INDEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(.*?)(?:\s*[,;:]\s*|\s+-{1,2}\s+|\s+[–—]\s+|\s*#|\s+)(?:(?:vol(?:ume)?|book|bk|no|nr|part|pt)\.?\s*)?#?(\d+(?:\.\d+)?)$",
        )
        .unwrap()
    });
    let s = s
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    if s.is_empty() {
        return None;
    }
    if let Some(c) = INDEX.captures(s) {
        let name = c[1].trim().trim_end_matches([',', ';', ':', '-']).trim();
        if !name.is_empty() {
            return Some(Series {
                name: name.to_owned(),
                index: c[2].parse().ok(),
            });
        }
    }
    Some(Series {
        name: s.to_owned(),
        index: None,
    })
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// Open Library's free-text `publish_date` as a W3C date: `1994`, `September 2020`,
/// `September 15, 2020`, `15 September 2020` or `2020-9-15`. `None` when it can't be read.
pub fn w3c_date(s: &str) -> Option<String> {
    static ISO: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{4})(?:-(\d{1,2})(?:-(\d{1,2}))?)?$").unwrap());
    static WORDS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:(\d{1,2})\s+)?([a-z]+)\.?\s+(?:(\d{1,2}),?\s+)?(\d{4})$").unwrap()
    });
    static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[c©]?\s*(\d{4})\.?$").unwrap());
    let s = s.trim();
    let (y, m, d): (u32, Option<u32>, Option<u32>) = if let Some(c) = ISO.captures(s) {
        (
            c[1].parse().ok()?,
            c.get(2).and_then(|m| m.as_str().parse().ok()),
            c.get(3).and_then(|d| d.as_str().parse().ok()),
        )
    } else if let Some(c) = WORDS.captures(s) {
        let word = c[2].to_lowercase();
        let month = MONTHS.iter().position(|m| word.starts_with(m))? as u32 + 1;
        let day = c.get(1).or(c.get(3)).and_then(|d| d.as_str().parse().ok());
        (c[4].parse().ok()?, Some(month), day)
    } else {
        (YEAR.captures(s)?[1].parse().ok()?, None, None)
    };
    match (m, d) {
        (None, _) => Some(format!("{y:04}")),
        (Some(m), _) if !(1..=12).contains(&m) => None,
        (Some(m), None) => Some(format!("{y:04}-{m:02}")),
        (Some(m), Some(d)) => {
            let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
            let days = [
                31,
                if leap { 29 } else { 28 },
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ];
            (1..=days[m as usize - 1])
                .contains(&d)
                .then(|| format!("{y:04}-{m:02}-{d:02}"))
        }
    }
}

/// Open Library's `/languages/eng` (MARC) as a language tag. `None` for a code not listed.
pub fn marc_language(key: &str) -> Option<String> {
    const TAGS: &[(&str, &str)] = &[
        ("eng", "en"),
        ("fre", "fr"),
        ("fra", "fr"),
        ("ger", "de"),
        ("deu", "de"),
        ("spa", "es"),
        ("ita", "it"),
        ("por", "pt"),
        ("dut", "nl"),
        ("nld", "nl"),
        ("jpn", "ja"),
        ("chi", "zh"),
        ("zho", "zh"),
        ("rus", "ru"),
        ("swe", "sv"),
        ("nor", "no"),
        ("dan", "da"),
        ("fin", "fi"),
        ("pol", "pl"),
        ("kor", "ko"),
        ("ara", "ar"),
        ("heb", "he"),
        ("gre", "el"),
        ("ell", "el"),
        ("tur", "tr"),
        ("cze", "cs"),
        ("ces", "cs"),
        ("hun", "hu"),
        ("lat", "la"),
        ("ukr", "uk"),
    ];
    let code = key.trim().trim_start_matches("/languages/");
    TAGS.iter()
        .find(|(m, _)| *m == code)
        .map(|(_, t)| (*t).to_owned())
}

/// An Open Library record: its metadata and the keys of the records it points to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub metadata: Metadata,
    /// `/works/OL…W` (an edition's work).
    pub work: Option<String>,
    /// `/authors/OL…A`, in order.
    pub authors: Vec<String>,
}

/// An edition record (`/isbn/{isbn}.json`): its metadata, work key and author keys.
pub fn parse_edition(body: &str) -> Result<Record, String> {
    let v = parse(body)?;
    if !v.is_object() {
        return Err("Open Library returned unreadable JSON: not an edition record".into());
    }
    let md = Metadata {
        title: text(v.get("title")),
        authors: Vec::new(),
        series: strings(v.get("series"))
            .first()
            .and_then(|s| parse_series(s)),
        description: text(v.get("description")),
        subjects: strings(v.get("subjects")),
        publisher: strings(v.get("publishers")).into_iter().next(),
        published: text(v.get("publish_date")).and_then(|d| w3c_date(&d)),
        language: v
            .get("languages")
            .and_then(Value::as_array)
            .and_then(|l| l.first())
            .and_then(|l| l.get("key"))
            .and_then(Value::as_str)
            .and_then(marc_language),
    };
    let work = v
        .get("works")
        .and_then(Value::as_array)
        .and_then(|w| w.first())
        .and_then(|w| w.get("key"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let authors = v
        .get("authors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| a.get("key").and_then(Value::as_str).map(str::to_owned))
        .collect();
    Ok(Record {
        metadata: md,
        work,
        authors,
    })
}

/// A work record (`/works/…json`): description, subjects and author keys.
pub fn parse_work(body: &str) -> Result<Record, String> {
    let v = parse(body)?;
    if !v.is_object() {
        return Err("Open Library returned unreadable JSON: not a work record".into());
    }
    let authors = v
        .get("authors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| {
            a.get("author")
                .and_then(|x| x.get("key"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    Ok(Record {
        metadata: Metadata {
            description: text(v.get("description")),
            subjects: strings(v.get("subjects")),
            ..Default::default()
        },
        work: None,
        authors,
    })
}

/// An author record (`/authors/…json`): the display name.
pub fn parse_author(body: &str) -> Result<Option<String>, String> {
    Ok(text(parse(body)?.get("name")))
}

fn same_title(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let (a, b) = (norm(a), norm(b));
    !a.is_empty() && !b.is_empty() && (a == b || a.starts_with(&b) || b.starts_with(&a))
}

/// A search result: the first whose title matches the book's. `None` when none does.
fn parse_search(body: &str, wanted: &str) -> Result<Option<(Metadata, Option<String>)>, String> {
    let v = parse(body)?;
    let Some(docs) = v.get("docs").and_then(Value::as_array) else {
        return Err("Open Library returned unreadable JSON: no search results".into());
    };
    Ok(docs.iter().find_map(|doc| {
        let title = text(doc.get("title"))?;
        same_title(&title, wanted).then(|| {
            (
                Metadata {
                    title: Some(title),
                    authors: strings(doc.get("author_name")),
                    subjects: strings(doc.get("subject")),
                    ..Default::default()
                },
                doc.get("key").and_then(Value::as_str).map(str::to_owned),
            )
        })
    }))
}

/// Looks the book up on Open Library, fetching only what the `missing` fields need.
/// `Ok(None)`: no record. `source` names the route taken.
pub fn lookup(
    info: &BookInfo,
    missing: Missing,
    fetch: &mut Fetcher,
) -> Result<Option<(Metadata, &'static str)>, String> {
    let isbn: Option<String> = info.isbn.as_deref().map(|i| {
        i.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_uppercase()
    });
    let edition = match isbn.filter(|i| !i.is_empty()) {
        Some(isbn) => match fetch(&edition_url(&isbn)) {
            Ok(body) => Some(parse_edition(&body)?),
            // An ISBN Open Library doesn't know: try the title instead.
            Err(e) if e.contains("404") => None,
            Err(e) => return Err(e),
        },
        None => None,
    };
    let (mut md, work, mut author_keys, source) = match edition {
        Some(r) => (r.metadata, r.work, r.authors, "isbn"),
        None => {
            let author = info.creators.first().map(|c| c.name.as_str()).unwrap_or("");
            match parse_search(&fetch(&search_url(&info.title, author))?, &info.title)? {
                Some((md, work)) => (md, work, Vec::new(), "search"),
                None => return Ok(None),
            }
        }
    };
    let need_work = (missing.description && md.description.is_none())
        || (missing.subjects && md.subjects.is_empty())
        || (missing.authors && md.authors.is_empty() && author_keys.is_empty());
    if need_work && let Some(url) = work.as_deref().and_then(|k| record_url(k, "works")) {
        let Record {
            metadata:
                Metadata {
                    description,
                    subjects,
                    ..
                },
            authors,
            ..
        } = parse_work(&fetch(&url)?)?;
        if md.description.is_none() {
            md.description = description;
        }
        // The work's subjects are the curated set; an edition's are often empty or catalog codes.
        if !subjects.is_empty() {
            md.subjects = subjects;
        }
        if author_keys.is_empty() {
            author_keys = authors;
        }
    }
    if missing.authors && md.authors.is_empty() {
        for url in author_keys
            .iter()
            .filter_map(|k| record_url(k, "authors"))
            .take(MAX_AUTHORS)
        {
            if let Some(name) = parse_author(&fetch(&url)?)? {
                md.authors.push(name);
            }
        }
    }
    md.subjects.truncate(MAX_SUBJECTS);
    Ok(Some((md, source)))
}

/// Writes `found` into the fields `info` leaves empty; returns what was written.
pub fn apply(info: &mut BookInfo, found: &Metadata) -> Metadata {
    let missing = Missing::of(info);
    let mut wrote = Metadata::default();
    if missing.title
        && let Some(t) = &found.title
    {
        info.title = t.clone();
        info.warnings.retain(|w| w != NO_TITLE_WARNING);
        wrote.title = Some(t.clone());
    }
    if missing.authors && !found.authors.is_empty() {
        info.creators = found
            .authors
            .iter()
            .map(|name| Creator {
                name: name.clone(),
                file_as: None,
            })
            .collect();
        wrote.authors = found.authors.clone();
    }
    if missing.series && found.series.is_some() {
        info.series = found.series.clone();
        wrote.series = found.series.clone();
    }
    if missing.description && found.description.is_some() {
        info.description = found.description.clone();
        wrote.description = found.description.clone();
    }
    if missing.subjects && !found.subjects.is_empty() {
        info.subjects = found.subjects.clone();
        wrote.subjects = found.subjects.clone();
    }
    wrote
}

fn field_names(m: &Metadata) -> Vec<&'static str> {
    let mut out = Vec::new();
    if m.title.is_some() {
        out.push("title");
    }
    if !m.authors.is_empty() {
        out.push("author");
    }
    if m.series.is_some() {
        out.push("series");
    }
    if m.description.is_some() {
        out.push("description");
    }
    if !m.subjects.is_empty() {
        out.push("subjects");
    }
    out
}

/// The whole fill: no request when nothing is missing, else look up and fill the empty fields.
/// Returns the outcome and what was written (to put into the book's OPF).
pub fn fill(info: &mut BookInfo, fetch: &mut Fetcher) -> (FillOutcome, Metadata) {
    let missing = Missing::of(info);
    if !missing.any() {
        return (FillOutcome::Complete, Metadata::default());
    }
    match lookup(info, missing, fetch) {
        Ok(Some((found, source))) => {
            let wrote = apply(info, &found);
            (
                FillOutcome::Filled {
                    fields: field_names(&wrote),
                    source,
                },
                wrote,
            )
        }
        Ok(None) => (FillOutcome::NotFound, Metadata::default()),
        Err(message) => (FillOutcome::Error { message }, Metadata::default()),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// OPF `<metadata>` children for `m`. Each element declares its own namespace, so it is valid
/// whatever prefixes the book's OPF uses.
pub fn opf_elements(m: &Metadata) -> String {
    const DC: &str = r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#;
    const OPF: &str = r#"xmlns="http://www.idpf.org/2007/opf""#;
    let mut out = String::new();
    if let Some(t) = &m.title {
        out.push_str(&format!("<dc:title {DC}>{}</dc:title>", xml_escape(t)));
    }
    for a in &m.authors {
        out.push_str(&format!("<dc:creator {DC}>{}</dc:creator>", xml_escape(a)));
    }
    if let Some(d) = &m.description {
        out.push_str(&format!(
            "<dc:description {DC}>{}</dc:description>",
            xml_escape(d)
        ));
    }
    for s in &m.subjects {
        out.push_str(&format!("<dc:subject {DC}>{}</dc:subject>", xml_escape(s)));
    }
    if let Some(s) = &m.series {
        out.push_str(&format!(
            r#"<meta {OPF} name="calibre:series" content="{}"/>"#,
            xml_escape(&s.name)
        ));
        if let Some(i) = s.index {
            out.push_str(&format!(
                r#"<meta {OPF} name="calibre:series_index" content="{i}"/>"#
            ));
        }
    }
    out
}

/// `opf` with `m`'s elements added at the end of its `<metadata>`; `None` when it has none.
pub fn add_to_opf(opf: &str, m: &Metadata) -> Option<String> {
    static CLOSE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"</(?:[A-Za-z_][\w.-]*:)?metadata\s*>").unwrap());
    let at = CLOSE.find(opf)?.start();
    let mut out = String::with_capacity(opf.len() + 512);
    out.push_str(&opf[..at]);
    out.push_str(&opf_elements(m));
    out.push_str(&opf[at..]);
    Some(out)
}
