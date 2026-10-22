// SPDX-License-Identifier: GPL-3.0-or-later
//! Headlines for tomorrow's front page, fetched politely: the feeds' RSS only, each URL at most
//! once per [`REFRESH_EVERY_MIN`] minutes, failures included (tomorrow's paper design §5.1).

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::fetch::Net;
use crate::sources::feeds::{as_briefs, google_news_url, parse};

/// Minutes a URL rests between fetches.
pub const REFRESH_EVERY_MIN: i64 = 30;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveItem {
    pub title: String,
    /// The outlet, for Google News briefs and local feeds.
    pub source: Option<String>,
    pub published: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fetched {
    pub at: DateTime<Utc>,
    /// The feed's own title.
    pub title: Option<String>,
    pub items: Vec<LiveItem>,
    pub error: Option<String>,
}

/// `next.json`: what each feed URL said when last asked. Disposable.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveCache {
    pub feeds: BTreeMap<String, Fetched>,
}

impl LiveCache {
    /// A missing or unreadable file is an empty cache.
    pub fn load(path: &Path) -> LiveCache {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// What a URL is for: how old an item may be, how many are kept, and whether it's a Google
/// News search (whose titles carry the outlet).
struct Source {
    url: String,
    max_age_hours: u32,
    max_items: usize,
    briefs: bool,
}

fn sources(cfg: &Config) -> Vec<Source> {
    let mut out: Vec<Source> = Vec::new();
    let mut push = |s: Source| {
        if !out.iter().any(|o| o.url == s.url) {
            out.push(s);
        }
    };
    if !cfg.local.google_news.trim().is_empty() {
        push(Source {
            url: google_news_url(&cfg.local.google_news, cfg.local.max_age_hours),
            max_age_hours: cfg.local.max_age_hours,
            max_items: cfg.local.max_items,
            briefs: true,
        });
    }
    for url in &cfg.local.feeds {
        push(Source {
            url: url.clone(),
            max_age_hours: cfg.local.max_age_hours,
            max_items: cfg.local.max_items,
            briefs: false,
        });
    }
    for f in &cfg.feeds {
        push(Source {
            url: f.url.clone(),
            max_age_hours: cfg.max_age_hours,
            max_items: f.max_items,
            briefs: false,
        });
    }
    out
}

/// The URLs the front page reads: the Google News search, `[local] feeds`, then `[[feeds]]`.
pub fn live_urls(cfg: &Config) -> Vec<String> {
    sources(cfg).into_iter().map(|s| s.url).collect()
}

/// Fetches each URL whose last fetch is [`REFRESH_EVERY_MIN`] or more minutes old, one at a
/// time, and forgets URLs no longer in the config. Returns the requests made.
pub fn refresh(cfg: &Config, cache: &mut LiveCache, now: DateTime<Utc>, net: &dyn Net) -> usize {
    let wanted = sources(cfg);
    cache
        .feeds
        .retain(|url, _| wanted.iter().any(|s| s.url == *url));
    let mut requests = 0;
    for s in wanted {
        if cache
            .feeds
            .get(&s.url)
            .is_some_and(|f| now - f.at < Duration::minutes(REFRESH_EVERY_MIN))
        {
            continue;
        }
        requests += 1;
        let fetched = match net
            .get(&s.url)
            .and_then(|b| parse(&b, now, s.max_age_hours))
        {
            Ok((title, items)) => {
                let mut items = if s.briefs { as_briefs(items) } else { items };
                items.truncate(s.max_items);
                Fetched {
                    at: now,
                    title,
                    items: items
                        .into_iter()
                        .map(|i| LiveItem {
                            title: i.title,
                            source: i.source,
                            published: i.published,
                        })
                        .collect(),
                    error: None,
                }
            }
            Err(e) => Fetched {
                at: now,
                title: None,
                items: Vec::new(),
                error: Some(e),
            },
        };
        cache.feeds.insert(s.url, fetched);
    }
    requests
}

/// When headlines were last checked: the newest fetch.
pub fn checked(cache: &LiveCache) -> Option<DateTime<Utc>> {
    cache.feeds.values().map(|f| f.at).max()
}
