// SPDX-License-Identifier: GPL-3.0-or-later
//! One web article as its own book, now (web-article design): an address, a `.webloc` or
//! `.url` link file, or a saved `.html` page, extracted whole and written as an EPUB that the
//! caller sends through the usual pipeline.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;

use crate::config::Config;
use crate::fetch::Net;
use crate::inbox::urls_in;
use crate::reading::{fnv, normalize_url};
use crate::sources::feeds::{by_line, host, story_body};
use crate::sources::reading::{LongRead, slug};
use crate::{Article, article, longread};

/// Link files and saved pages `deckle run` builds into an article.
pub const ARTICLE_EXTENSIONS: [&str; 4] = ["webloc", "url", "html", "htm"];

/// What a `deckle run` argument names: a web address, or a file by its extension.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Source {
    Url(String),
    Page(PathBuf),
}

/// The book written for one article.
#[derive(Clone, Debug)]
pub struct WebBook {
    pub path: PathBuf,
    pub title: String,
    pub words: usize,
}

fn is_address(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

/// A web address, or a file with one of [`ARTICLE_EXTENSIONS`] (any case).
pub fn is_article(target: &str) -> bool {
    is_address(target)
        || extension(Path::new(target)).is_some_and(|e| ARTICLE_EXTENSIONS.contains(&e.as_str()))
}

static CANONICAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)<(?:link[^>]+rel=["']canonical["'][^>]+href|meta[^>]+property=["']og:url["'][^>]+content)=["'](https?://[^"']+)"#,
    )
    .unwrap()
});

/// The address a saved page came from, when it says (`rel=canonical`, `og:url`).
fn saved_from(html: &str) -> Option<String> {
    CANONICAL.captures(html).map(|c| c[1].to_owned())
}

fn source(target: &str) -> Result<Source, String> {
    if is_address(target) {
        return Ok(Source::Url(target.to_owned()));
    }
    let path = Path::new(target);
    match extension(path).as_deref() {
        Some(ext @ ("webloc" | "url")) => {
            let url = urls_in(path, ext)?.remove(0);
            if is_address(&url) {
                Ok(Source::Url(url))
            } else {
                Err(format!("{url} is not a web address"))
            }
        }
        Some("html" | "htm") => std::fs::canonicalize(path)
            .map(Source::Page)
            .map_err(|e| e.to_string()),
        _ => Err("not a web address, a link file (.webloc, .url) or a saved page (.html)".into()),
    }
}

/// Builds `target` into `dir/<slug>-<id>.epub`. A saved page is read from disk, never fetched;
/// `cfg` supplies the paper's photo settings.
pub fn build(
    target: &str,
    cfg: &Config,
    net: &dyn Net,
    dir: &Path,
    date: NaiveDate,
) -> Result<WebBook, String> {
    let (html, base, outlet) = match source(target)? {
        Source::Url(url) => {
            let url = if article::is_google_news(&url) {
                article::resolve_google_news(&url, net)
                    .ok_or("couldn't follow the Google News link")?
            } else {
                url
            };
            let bytes = net.get_page(&url)?;
            let outlet = host(&url).to_owned();
            (String::from_utf8_lossy(&bytes).into_owned(), url, outlet)
        }
        Source::Page(file) => {
            let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
            let base = url::Url::from_file_path(&file)
                .map_err(|_| format!("{}: not an absolute path", file.display()))?;
            let html = String::from_utf8_lossy(&bytes).into_owned();
            let outlet = saved_from(&html)
                .map(|u| host(&u).to_owned())
                .unwrap_or_else(|| "a saved page".to_owned());
            (html, base.to_string(), outlet)
        }
    };
    let x = article::extract_with(&html, &base, None).ok_or_else(|| {
        format!(
            "no readable article (under {} words: a paywall, a sign-in page, or a video)",
            article::MIN_WORDS
        )
    })?;
    let title = x.title.clone().unwrap_or_else(|| outlet.clone());
    let (body, photos, extra) = story_body(&x, &outlet, cfg, net);
    let id = format!("{:016x}", fnv(&normalize_url(&base)));
    let stem = format!("{}-{}", slug(&title), &id[..8]);
    let lr = LongRead {
        id: id.clone(),
        title: title.clone(),
        byline: x.byline.clone(),
        outlet: outlet.clone(),
        words: x.words,
        article: Article {
            title: title.clone(),
            meta: x.byline.as_deref().map(by_line),
            body,
            teaser: x.teaser.clone(),
            photos,
            // A saved page's base is a `file:` path, not a link.
            url: base.starts_with("http").then(|| base.clone()),
            outlet: Some(outlet.clone()),
            extra,
        },
        file_stem: stem.clone(),
    };
    let path = dir.join(format!("{stem}.epub"));
    longread::write_article(&lr, &format!("urn:deckle:article:{id}"), None, date, &path)?;
    Ok(WebBook {
        path,
        title,
        words: x.words,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_that_are_articles() {
        assert!(is_article("https://ex.com/a"));
        assert!(is_article("http://ex.com/a"));
        assert!(is_article("Essay.WEBLOC"));
        assert!(is_article("link.url"));
        assert!(is_article("saved.htm"));
        assert!(!is_article("book.epub"));
        assert!(!is_article("notes.docx"));
        assert!(!is_article("ex.com/a"));
    }

    #[test]
    fn a_saved_page_names_where_it_came_from() {
        assert_eq!(
            saved_from(
                r#"<link rel="canonical" href="https://en.wikipedia.org/wiki/Deckle_edge"/>"#
            )
            .as_deref(),
            Some("https://en.wikipedia.org/wiki/Deckle_edge")
        );
        assert_eq!(
            saved_from(r#"<meta property='og:url' content='https://ex.com/a'>"#).as_deref(),
            Some("https://ex.com/a")
        );
        assert_eq!(saved_from("<p>no address</p>"), None);
    }

    #[test]
    fn a_link_file_without_a_web_address_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.url");
        std::fs::write(&p, "[InternetShortcut]\nURL=file:///etc/passwd\n").unwrap();
        let err = source(&p.to_string_lossy()).unwrap_err();
        assert!(err.contains("not a web address"), "{err}");
    }
}
