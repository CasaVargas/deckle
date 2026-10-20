// SPDX-License-Identifier: GPL-3.0-or-later
//! RSS / Atom feeds: the Local section (a Google News search plus local papers) and one
//! section per `[[feeds]]` entry. Feed HTML is reduced to text paragraphs (v0).

use std::collections::HashSet;
use std::sync::LazyLock;

use chrono::{DateTime, Local, Utc};
use regex::Regex;

use crate::config::{Config, FeedConfig};
use crate::fetch::{Get, encode};
use crate::{Article, Section, esc};

/// Paragraphs kept per article.
const MAX_PARAGRAPHS: usize = 40;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub title: String,
    pub source: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub paragraphs: Vec<String>,
    pub link: Option<String>,
}

/// A Google News search limited to the last `max_age_hours` (rounded up to days): without
/// `when:` the results are by relevance and mostly months old.
pub fn google_news_url(query: &str, max_age_hours: u32) -> String {
    let query = if query.contains("when:") {
        query.to_owned()
    } else {
        format!("{query} when:{}d", max_age_hours.div_ceil(24).max(1))
    };
    format!(
        "https://news.google.com/rss/search?q={}&hl=en-US&gl=US&ceid=US:en",
        encode(&query)
    )
}

/// Feed title and its items, newest first, without items older than `max_age_hours` (items
/// without a date are kept).
pub fn parse(
    bytes: &[u8],
    now: DateTime<Utc>,
    max_age_hours: u32,
) -> Result<(Option<String>, Vec<Item>), String> {
    let feed = feed_rs::parser::parse(bytes).map_err(|e| e.to_string())?;
    let cutoff = now - chrono::Duration::hours(i64::from(max_age_hours));
    let mut items: Vec<Item> = feed
        .entries
        .into_iter()
        .filter_map(|e| {
            let title = clean_line(&e.title.map(|t| t.content).unwrap_or_default());
            if title.is_empty() {
                return None;
            }
            let published = e.published.or(e.updated);
            if published.is_some_and(|p| p < cutoff) {
                return None;
            }
            let html = e
                .content
                .and_then(|c| c.body)
                .or(e.summary.map(|s| s.content))
                .unwrap_or_default();
            Some(Item {
                title,
                source: None,
                published,
                paragraphs: html_paragraphs(&html),
                link: e.links.first().map(|l| l.href.clone()),
            })
        })
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    Ok((feed.title.map(|t| clean_line(&t.content)), items))
}

/// Google News items: "Headline - Source", and a description that only repeats the link.
pub fn as_briefs(items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .map(|mut i| {
            if let Some((head, source)) = i.title.rsplit_once(" - ") {
                i.source = Some(source.trim().to_owned());
                i.title = head.trim().to_owned();
            }
            i.paragraphs.clear();
            i
        })
        .collect()
}

static DROP_BLOCKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(script|style|figure|iframe)\b.*?</(script|style|figure|iframe)>").unwrap()
});
static BREAKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)<br\s*/?>|</?(p|div|li|ul|ol|h[1-6]|blockquote|tr|table|section|article)\b[^>]*>",
    )
    .unwrap()
});
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\r\f\u{a0}]+").unwrap());
static PARAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n|\n").unwrap());

/// Feed HTML → plain-text paragraphs.
pub fn html_paragraphs(html: &str) -> Vec<String> {
    let s = DROP_BLOCKS.replace_all(html, "");
    let s = BREAKS.replace_all(&s, "\n\n");
    let s = TAGS.replace_all(&s, "");
    let s = html_escape::decode_html_entities(&s);
    PARAS
        .split(&s)
        .map(|p| SPACES.replace_all(p, " ").trim().to_owned())
        .filter(|p| !p.is_empty())
        .take(MAX_PARAGRAPHS)
        .collect()
}

fn clean_line(s: &str) -> String {
    let s = html_escape::decode_html_entities(s);
    SPACES
        .replace_all(&s.replace('\n', " "), " ")
        .trim()
        .to_owned()
}

fn key(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn when(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local)
        .format("%b %-d, %-I:%M %p")
        .to_string()
}

pub fn article(item: &Item) -> Article {
    let meta = [item.source.clone(), item.published.map(when)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let mut body: String = item
        .paragraphs
        .iter()
        .map(|p| format!("<p>{}</p>", esc(p)))
        .collect();
    if let Some(link) = &item.link {
        let host = link
            .split("://")
            .nth(1)
            .and_then(|r| r.split('/').next())
            .unwrap_or(link);
        body.push_str(&format!(
            "<p class=\"link\">Read online: <a href=\"{}\">{}</a></p>",
            esc(link),
            esc(host)
        ));
    }
    Article {
        title: item.title.clone(),
        meta: (!meta.is_empty()).then_some(meta),
        body,
        teaser: teaser(&item.paragraphs),
    }
}

/// The first paragraph that reads like prose (not "Article URL: …" link lists).
fn teaser(paragraphs: &[String]) -> Option<String> {
    paragraphs
        .iter()
        .find(|p| p.len() >= 80 && !p.contains("URL:"))
        .cloned()
}

fn section_of(title: &str, kicker: Option<String>, items: &[Item], errors: &[String]) -> Section {
    if items.is_empty() && !errors.is_empty() {
        return super::unavailable(title, errors.join("; "));
    }
    let mut section = Section {
        title: title.into(),
        kicker,
        articles: items.iter().map(article).collect(),
        unavailable: None,
    };
    if items.is_empty() {
        section.unavailable = Some("No new stories today.".into());
    }
    section
}

/// The Local section: the Google News search and local feeds, merged, deduplicated, newest
/// first.
pub fn local_section(cfg: &Config, now: DateTime<Utc>, get: &mut Get<'_>) -> Section {
    let title = "Local";
    let mut items = Vec::new();
    let mut errors = Vec::new();
    if !cfg.local.google_news.trim().is_empty() {
        match get(&google_news_url(
            &cfg.local.google_news,
            cfg.local.max_age_hours,
        ))
        .and_then(|b| parse(&b, now, cfg.local.max_age_hours))
        {
            Ok((_, found)) => items.extend(as_briefs(found)),
            Err(e) => errors.push(format!("Google News: {e}")),
        }
    }
    for url in &cfg.local.feeds {
        match get(url).and_then(|b| parse(&b, now, cfg.local.max_age_hours)) {
            Ok((name, found)) => items.extend(found.into_iter().map(|mut i| {
                i.source = i.source.or(name.clone());
                i
            })),
            Err(e) => errors.push(format!("{url}: {e}")),
        }
    }
    if cfg.local.google_news.trim().is_empty() && cfg.local.feeds.is_empty() {
        return super::unavailable(title, "add a Google News search or local feeds to [local]");
    }
    let mut seen = HashSet::new();
    items.retain(|i| seen.insert(key(&i.title)));
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    items.truncate(cfg.local.max_items);
    let kicker = (!cfg.place.is_empty()).then(|| format!("News from around {}", cfg.place));
    section_of(title, kicker, &items, &errors)
}

/// One `[[feeds]]` entry as its own section.
pub fn feed_section(
    feed: &FeedConfig,
    cfg: &Config,
    now: DateTime<Utc>,
    get: &mut Get<'_>,
) -> Section {
    match get(&feed.url).and_then(|b| parse(&b, now, cfg.max_age_hours)) {
        Ok((_, mut items)) => {
            items.truncate(feed.max_items);
            section_of(&feed.name, None, &items, &[])
        }
        Err(e) => super::unavailable(&feed.name, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_paragraphs() {
        let p = html_paragraphs(
            "<p>One &amp; two</p><script>x()</script><p>Three<br/>Four</p><figure><img src=x></figure>",
        );
        assert_eq!(p, ["One & two", "Three", "Four"]);
    }

    #[test]
    fn briefs_split_the_source() {
        let items = as_briefs(vec![Item {
            title: "Standoff in Candor - WXII".into(),
            source: None,
            published: None,
            paragraphs: vec!["noise".into()],
            link: None,
        }]);
        assert_eq!(items[0].title, "Standoff in Candor");
        assert_eq!(items[0].source.as_deref(), Some("WXII"));
        assert!(items[0].paragraphs.is_empty());
    }

    #[test]
    fn query_is_encoded() {
        assert_eq!(
            google_news_url("Moore County NC", 72),
            "https://news.google.com/rss/search?q=Moore+County+NC+when%3A3d&hl=en-US&gl=US&ceid=US:en"
        );
        assert!(google_news_url("x when:1d", 72).contains("x+when%3A1d&"));
    }
}
