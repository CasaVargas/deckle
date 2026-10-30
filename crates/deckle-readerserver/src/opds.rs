// SPDX-License-Identifier: GPL-3.0-or-later
//! The OPDS 1.2 catalog (KOReader hub design §A.3): a navigation feed, then acquisition feeds
//! for today's paper, the recent papers and the books waiting for the e-readers. A file is asked
//! for by an opaque id looked up in the catalog of the moment, so no path ever comes from the
//! client.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::{Catalog, Ctx, auth};

/// Recent papers listed.
pub const RECENT: usize = 14;
/// Books listed.
pub const BOOKS: usize = 200;

const NAV: &str = "application/atom+xml;profile=opds-catalog;kind=navigation";
const ACQ: &str = "application/atom+xml;profile=opds-catalog;kind=acquisition";

fn unauthorized() -> Response {
    let mut r = (
        StatusCode::UNAUTHORIZED,
        "sign in with the reader server's account",
    )
        .into_response();
    r.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Basic realm=\"Deckle\", charset=\"UTF-8\""),
    );
    r
}

fn feed(kind: &'static str, body: String) -> Response {
    ([(header::CONTENT_TYPE, kind)], body).into_response()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The id a file is asked for by: a SHA-1 of its path.
pub fn file_id(path: &Path) -> String {
    Sha1::digest(path.to_string_lossy().as_bytes())
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn head(id: &str, title: &str, self_href: &str, kind: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\" \
         xmlns:opds=\"http://opds-spec.org/2010/catalog\" xmlns:dc=\"http://purl.org/dc/terms/\">\
         <id>urn:deckle:{id}</id><title>{}</title><updated>{}</updated>\
         <author><name>Deckle</name></author>\
         <link rel=\"self\" href=\"{self_href}\" type=\"{kind}\"/>\
         <link rel=\"start\" href=\"/opds\" type=\"{NAV}\"/>",
        esc(title),
        now()
    )
}

pub async fn root(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> Response {
    if !auth::basic_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    let mut s = head("root", "Deckle", "/opds", NAV);
    for (id, title, about, href) in [
        (
            "today",
            "Today's paper",
            "The newest edition",
            "/opds/today",
        ),
        (
            "papers",
            "Recent papers",
            "The last two weeks of editions",
            "/opds/papers",
        ),
        (
            "books",
            "Books",
            "Books waiting for your e-readers",
            "/opds/books",
        ),
    ] {
        s.push_str(&format!(
            "<entry><title>{title}</title><id>urn:deckle:{id}</id><updated>{}</updated>\
             <content type=\"text\">{about}</content>\
             <link rel=\"subsection\" href=\"{href}\" type=\"{ACQ}\"/></entry>",
            now()
        ));
    }
    s.push_str("</feed>\n");
    feed(NAV, s)
}

/// One acquisition entry: the EPUB, a KEPUB for a Kobo, and the cover. `name` is the file
/// name it downloads as.
fn entry(path: &Path, name: &str, title: &str, updated: &str) -> String {
    let id = file_id(path);
    format!(
        "<entry><title>{}</title><id>urn:deckle:file:{id}</id><updated>{updated}</updated>\
         <author><name>Deckle</name></author><content type=\"text\">{}</content>\
         <link rel=\"http://opds-spec.org/acquisition\" href=\"/opds/file/{id}\" \
         type=\"application/epub+zip\" title=\"EPUB\"/>\
         <link rel=\"http://opds-spec.org/acquisition\" href=\"/opds/file/{id}?format=kepub\" \
         type=\"application/kepub+zip\" title=\"KEPUB (Kobo)\"/>\
         <link rel=\"http://opds-spec.org/image\" href=\"/opds/cover/{id}\" type=\"image/jpeg\"/>\
         <link rel=\"http://opds-spec.org/image/thumbnail\" href=\"/opds/cover/{id}\" \
         type=\"image/jpeg\"/></entry>",
        esc(title),
        esc(name)
    )
}

fn paper_entries(c: &Catalog, n: usize) -> String {
    c.papers
        .iter()
        .take(n)
        .map(|p| {
            let title = format!("{} — {}", p.title, p.date.format("%A, %B %-d, %Y"));
            let name = format!("{}.epub", p.stem);
            entry(&p.epub, &name, &title, &format!("{}T00:00:00Z", p.date))
        })
        .collect()
}

pub async fn today(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> Response {
    if !auth::basic_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    let c = (ctx.hooks.source)();
    let mut s = head("today", "Today's paper", "/opds/today", ACQ);
    s.push_str(&paper_entries(&c, 1));
    s.push_str("</feed>\n");
    feed(ACQ, s)
}

pub async fn papers(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> Response {
    if !auth::basic_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    let c = (ctx.hooks.source)();
    let mut s = head("papers", "Recent papers", "/opds/papers", ACQ);
    s.push_str(&paper_entries(&c, RECENT));
    s.push_str("</feed>\n");
    feed(ACQ, s)
}

pub async fn books(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> Response {
    if !auth::basic_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    let c = (ctx.hooks.source)();
    let mut s = head("books", "Books", "/opds/books", ACQ);
    for b in c.books.iter().take(BOOKS) {
        let title = b
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let updated = std::fs::metadata(b)
            .and_then(|m| m.modified())
            .map(|t| {
                chrono::DateTime::<chrono::Utc>::from(t)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string()
            })
            .unwrap_or_else(|_| now());
        let name = b.file_name().unwrap_or_default().to_string_lossy();
        s.push_str(&entry(b, &name, &title, &updated));
    }
    s.push_str("</feed>\n");
    feed(ACQ, s)
}

/// The file with id `id` in the catalog of the moment, and the stem it downloads as.
fn lookup(c: &Catalog, id: &str) -> Option<(PathBuf, String)> {
    let stem = |p: &Path| {
        p.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    c.papers
        .iter()
        .take(RECENT)
        .map(|p| (p.epub.clone(), p.stem.clone()))
        .chain(c.books.iter().take(BOOKS).map(|b| (b.clone(), stem(b))))
        .find(|(p, _)| file_id(p) == id)
}

#[derive(Deserialize)]
pub struct FileQuery {
    format: Option<String>,
}

pub async fn file(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<FileQuery>,
) -> Response {
    if !auth::basic_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    let Some((path, stem)) = lookup(&(ctx.hooks.source)(), &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let (bytes, name, mime) = if q.format.as_deref() == Some("kepub") {
        let stem = stem.trim_end_matches(".kepub").to_owned();
        match deckle_core::kepub::convert(&bytes) {
            Ok(k) => (k, format!("{stem}.kepub.epub"), "application/kepub+zip"),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    } else {
        (bytes, format!("{stem}.epub"), "application/epub+zip")
    };
    let served = ctx.hooks.served.clone();
    let (record_name, record_bytes) = (name.clone(), bytes.clone());
    tokio::task::spawn_blocking(move || served(&record_name, &record_bytes));
    // A file name the header can carry: no quotes or control characters.
    let safe: String = name
        .chars()
        .map(|c| if c == '"' || c.is_control() { '_' } else { c })
        .collect();
    let ascii: String = safe
        .chars()
        .map(|c| if c.is_ascii() { c } else { '_' })
        .collect();
    let disposition = format!("attachment; filename=\"{ascii}\"");
    (
        [
            (header::CONTENT_TYPE, mime.to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    )
        .into_response()
}

pub async fn cover(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<String>,
) -> Response {
    if !auth::basic_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    let Some((path, _)) = lookup(&(ctx.hooks.source)(), &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::task::spawn_blocking(move || cover_bytes(&path)).await {
        Ok(Some((bytes, mime))) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The EPUB's cover image and its type.
fn cover_bytes(path: &Path) -> Option<(Vec<u8>, String)> {
    let info = deckle_core::inspect::inspect(path).ok()?;
    let href = info.cover_href?;
    let mut r = deckle_core::epub::EpubReader::open(path).ok()?;
    let name = r.resolve(&href);
    let bytes = r.read_bytes(&name).ok()?;
    let mime = if name.to_ascii_lowercase().ends_with(".png") {
        "image/png"
    } else {
        "image/jpeg"
    };
    Some((bytes, mime.to_owned()))
}
