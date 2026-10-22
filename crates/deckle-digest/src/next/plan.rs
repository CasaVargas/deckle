// SPDX-License-Identifier: GPL-3.0-or-later
//! Tomorrow's front page, page plan and page estimate, composed from what's known before press
//! time (tomorrow's paper design §5–§7). Pure: every input is passed in.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;

use super::live::{LiveCache, LiveItem};
use super::{History, WeekDay};
use crate::config::{Config, SectionKind, Units, section_key};
use crate::front::{Candidate, Tier, lead};
use crate::press::{Press, press};
use crate::reading::{Entry, State};
use crate::sources::feeds::{google_news_url, title_key};

/// KFX positions on one Paperwhite page at the default font.
pub const POSITIONS_PER_PAGE: f64 = 1800.0;

/// A share at or under this is a section you rarely reach (readership's `RARELY`).
const RARELY: u8 = 20;

pub struct Inputs<'a> {
    pub cfg: &'a Config,
    pub now: DateTime<Tz>,
    /// The schedule's press time.
    pub at: Option<(u8, u8)>,
    pub stopped: Option<NaiveDate>,
    /// The whole reading queue.
    pub queue: &'a [Entry],
    pub live: &'a LiveCache,
    pub history: &'a History,
    /// Readership's section shares, by key, in percent.
    pub shares: &'a BTreeMap<String, u8>,
    pub week: Vec<WeekDay>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    /// The section key it belongs to.
    pub key: String,
    pub kicker: String,
    pub title: Option<String>,
    pub reason: Option<String>,
    /// A slot that fills at press time.
    pub dashed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRow {
    pub key: String,
    pub kind: SectionKind,
    /// The `[[feeds]]` index of a feed row.
    pub feed: Option<usize>,
    /// The `[[local.places]]` index of another place's row.
    pub place: Option<usize>,
    pub name: String,
    pub detail: String,
    pub budget: String,
    /// The editable budget (`max_items`); `None` for fixed sections.
    pub items: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextPaper {
    pub press: Press,
    pub paper: String,
    pub place: String,
    pub date: NaiveDate,
    pub number: u32,
    /// "Weather · fills in at press time" under the masthead.
    pub weather: bool,
    pub lead: Option<Tile>,
    pub tiles: Vec<Tile>,
    /// When headlines were last fetched.
    pub checked: Option<DateTime<Utc>>,
    pub plan: Vec<PlanRow>,
    /// Sections that are off, to switch on.
    pub off: Vec<PlanRow>,
    pub pages: u32,
    pub week: Vec<WeekDay>,
}

/// Every kind, in the starter's order: where an off section is listed.
const KINDS: [SectionKind; 9] = [
    SectionKind::Reading,
    SectionKind::Weather,
    SectionKind::Local,
    SectionKind::Feeds,
    SectionKind::Lottery,
    SectionKind::Horoscope,
    SectionKind::Puzzle,
    SectionKind::Shelf,
    SectionKind::Onthisday,
];

/// A row with what the front page needs to know about it.
struct Row<'a> {
    row: PlanRow,
    /// Your Reading: the saved articles that fit.
    saved: Vec<&'a Entry>,
    /// Local and feeds: what was fetched, newest first; `None` when nothing has been yet.
    stories: Option<Vec<LiveItem>>,
    /// Items the paper will hold, for the estimate.
    planned: usize,
}

fn host(url: &str) -> Option<String> {
    let h = url::Url::parse(url).ok()?.host_str()?.to_owned();
    Some(h.strip_prefix("www.").unwrap_or(&h).to_owned())
}

fn pending(queue: &[Entry], date: NaiveDate) -> Vec<&Entry> {
    let mut out: Vec<&Entry> = queue
        .iter()
        .filter(|e| e.state == State::Pending || e.printed_in == Some(date))
        .collect();
    out.sort_by_key(|e| e.saved_at);
    out
}

/// Local's (or another place's) headlines: its Google News search and feeds, merged and
/// deduplicated, newest first, cut to the budget; `None` when none of them has been fetched
/// without an error.
fn local_stories(
    search: &str,
    feeds: &[String],
    max: usize,
    cfg: &Config,
    live: &LiveCache,
) -> Option<Vec<LiveItem>> {
    let mut urls: Vec<String> = feeds.to_vec();
    if !search.trim().is_empty() {
        urls.insert(0, google_news_url(search, cfg.local.max_age_hours));
    }
    let got: Vec<&super::live::Fetched> = urls
        .iter()
        .filter_map(|u| live.feeds.get(u))
        .filter(|f| f.error.is_none())
        .collect();
    if got.is_empty() {
        return None;
    }
    let mut seen = HashSet::new();
    let mut items: Vec<LiveItem> = got
        .into_iter()
        .flat_map(|f| f.items.iter().cloned())
        .filter(|i| seen.insert(i.title.to_lowercase()))
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    items.truncate(max);
    Some(items)
}

/// "wxii.com + Google News": a local section's sources.
fn local_detail(search: &str, feeds: &[String]) -> String {
    let mut parts: Vec<String> = feeds.iter().filter_map(|u| host(u)).collect();
    if !search.trim().is_empty() {
        parts.push(if parts.is_empty() {
            "Google News".into()
        } else {
            "+ Google News".into()
        });
    }
    if parts.is_empty() {
        "No sources yet".to_owned()
    } else {
        parts.join(", ").replace(", + ", " + ")
    }
}

fn rows<'a>(i: &Inputs<'a>, kind: SectionKind, date: NaiveDate) -> Vec<Row<'a>> {
    let cfg = i.cfg;
    let fixed = |name: &str, detail: String, budget: String| Row {
        row: PlanRow {
            key: section_key(kind, None),
            kind,
            feed: None,
            place: None,
            name: name.into(),
            detail,
            budget,
            items: None,
        },
        saved: Vec::new(),
        stories: None,
        planned: 0,
    };
    let stories_row =
        |key: String, feed, place, name: &str, detail: String, max: usize, stories| {
            let planned = match &stories {
                Some(s) => max.min(Vec::len(s)),
                None => max,
            };
            Row {
                row: PlanRow {
                    key,
                    kind,
                    feed,
                    place,
                    name: name.into(),
                    detail,
                    budget: format!("{max} stories"),
                    items: Some(max),
                },
                saved: Vec::new(),
                stories,
                planned,
            }
        };
    match kind {
        SectionKind::Reading => {
            let all = pending(i.queue, date);
            let max = cfg.reading.max_items;
            let today = i.now.date_naive();
            let n = all.len();
            let detail = if n == 0 {
                "Nothing saved yet".to_owned()
            } else if n > max {
                format!("{max} of {n} fit; the rest wait")
            } else if all
                .iter()
                .all(|e| e.saved_at.with_timezone(&i.now.timezone()).date_naive() == today)
            {
                format!("{n} saved today")
            } else {
                format!("{n} waiting")
            };
            let saved: Vec<&Entry> = all.into_iter().take(max).collect();
            vec![Row {
                row: PlanRow {
                    key: section_key(kind, None),
                    kind,
                    feed: None,
                    place: None,
                    name: "Your Reading".into(),
                    detail,
                    budget: if n <= max {
                        "all".into()
                    } else {
                        max.to_string()
                    },
                    items: Some(max),
                },
                planned: saved.len(),
                saved,
                stories: None,
            }]
        }
        SectionKind::Weather => {
            let mut detail = match cfg.place.trim() {
                "" => "5-day box and almanac".to_owned(),
                p => format!("{p} · 5-day box and almanac"),
            };
            if !cfg.day.calendars.is_empty() {
                detail.push_str(" · Your Day");
            }
            vec![fixed("Weather", detail, "1 page".into())]
        }
        SectionKind::Local => {
            let l = &cfg.local;
            let mut out = vec![stories_row(
                section_key(kind, None),
                None,
                None,
                "Local",
                local_detail(&l.google_news, &l.feeds),
                l.max_items,
                local_stories(&l.google_news, &l.feeds, l.max_items, cfg, i.live),
            )];
            // then each other place's local news (#102)
            for (n, p) in l.places.iter().enumerate() {
                let max = p.max_items.unwrap_or(l.max_items);
                out.push(stories_row(
                    section_key(kind, Some(&p.name)),
                    None,
                    Some(n),
                    &p.name,
                    local_detail(&p.google_news, &p.feeds),
                    max,
                    local_stories(&p.google_news, &p.feeds, max, cfg, i.live),
                ));
            }
            out
        }
        SectionKind::Feeds => cfg
            .feeds
            .iter()
            .enumerate()
            .map(|(n, f)| {
                let sources = f.sources();
                let got: Vec<&super::live::Fetched> = sources
                    .iter()
                    .filter_map(|u| i.live.feeds.get(u))
                    .filter(|x| x.error.is_none())
                    .collect();
                let detail = got
                    .iter()
                    .find_map(|x| x.title.clone())
                    .or_else(|| f.topic.map(|t| t.title().to_owned()))
                    .or_else(|| sources.first().and_then(|u| host(u)))
                    .unwrap_or_else(|| "No sources yet".to_owned());
                // merged, newest first, one copy of a repeated story; `plan` drops what earlier
                // rows show and cuts to the budget
                let stories = if got.is_empty() {
                    None
                } else {
                    let mut items: Vec<LiveItem> =
                        got.iter().flat_map(|x| x.items.iter().cloned()).collect();
                    items.sort_by_key(|i| std::cmp::Reverse(i.published));
                    let mut seen = HashSet::new();
                    items.retain(|i| seen.insert(title_key(&i.title)));
                    Some(items)
                };
                stories_row(
                    section_key(kind, Some(&f.name)),
                    Some(n),
                    None,
                    &f.name,
                    detail,
                    f.max_items,
                    stories,
                )
            })
            .collect(),
        SectionKind::Lottery => {
            let games: Vec<&str> = cfg
                .lottery
                .games
                .iter()
                .map(|g| match g.as_str() {
                    "powerball" => "Powerball",
                    "megamillions" => "Mega Millions",
                    other => other,
                })
                .collect();
            vec![fixed("Lottery", games.join(", "), "1 page".into())]
        }
        SectionKind::Horoscope => {
            let n = match cfg.horoscope.signs.len() {
                0 => 12,
                n => n,
            };
            let detail = if cfg.horoscope.signs.is_empty() {
                "all 12 signs".to_owned()
            } else {
                format!("{n} signs")
            };
            vec![fixed("Horoscope", detail, format!("{n} signs"))]
        }
        SectionKind::Puzzle => vec![fixed(
            "Puzzle",
            "Mini crossword from yesterday's headlines".into(),
            "1 page".into(),
        )],
        SectionKind::Shelf => vec![fixed(
            "From Your Shelf",
            "From your Kindle, as of its last plug-in".into(),
            "1 page".into(),
        )],
        SectionKind::Onthisday => vec![fixed(
            "On This Day",
            "From Wikipedia".into(),
            "1 page".into(),
        )],
    }
}

/// "Saved at 9:14 this morning." and its kin (tomorrow's paper design §5.3).
pub fn saved_line(saved: DateTime<Tz>, now: DateTime<Tz>, units: Units) -> String {
    let saved = saved.with_timezone(&now.timezone());
    let time = match units {
        Units::Imperial => saved.format("%-I:%M").to_string(),
        Units::Metric => saved.format("%H:%M").to_string(),
    };
    match (now.date_naive() - saved.date_naive()).num_days() {
        0 => {
            let part = match saved.hour() {
                h if h < 12 => "this morning",
                h if h < 17 => "this afternoon",
                _ => "this evening",
            };
            format!("Saved at {time} {part}.")
        }
        1 => format!("Saved yesterday at {time}."),
        2..=6 => format!("Saved {}.", saved.format("%A")),
        _ => format!("Saved {}.", saved.format("%b %-d")),
    }
}

fn saved_kicker(e: &Entry) -> String {
    match e.url.as_deref().and_then(host) {
        Some(h) => format!("YOUR READING · {}", h.to_uppercase()),
        None => "YOUR READING".to_owned(),
    }
}

fn saved_title(e: &Entry) -> String {
    e.title
        .clone()
        .or_else(|| e.url.clone())
        .or_else(|| {
            e.file
                .as_ref()
                .and_then(|f| f.file_name())
                .map(|f| f.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "A saved article".into())
}

/// "SCIENCE · 2 NEW SINCE 05:30", or "SCIENCE" when nothing is newer than the last press.
fn story_kicker(name: &str, items: &[LiveItem], since: DateTime<Tz>) -> String {
    let since_utc = since.with_timezone(&Utc);
    let fresh = items
        .iter()
        .filter(|i| i.published.is_some_and(|p| p > since_utc))
        .count();
    match fresh {
        0 => name.to_uppercase(),
        n => format!(
            "{} · {n} NEW SINCE {}",
            name.to_uppercase(),
            since.format("%H:%M")
        ),
    }
}

/// The page estimate in Kindle pages: your editions' sizes where Readership has them, else
/// defaults (tomorrow's paper design §6.3).
pub fn estimate(rows: &[PlanRow], planned: &[usize], history: &History, cfg: &Config) -> u32 {
    let page = POSITIONS_PER_PAGE;
    let whole = |key: &str, pages: f64| history.whole.get(key).copied().unwrap_or(pages * page);
    let story = if cfg.full_text { 1.5 } else { 0.3 };
    let mut total = whole("front", 2.0);
    for (r, &n) in rows.iter().zip(planned) {
        let per = |pages: f64| {
            history
                .per_item
                .get(&r.key)
                .copied()
                .unwrap_or(pages * page)
                * n as f64
        };
        total += match r.kind {
            SectionKind::Reading => per(3.0),
            SectionKind::Local | SectionKind::Feeds => per(story),
            SectionKind::Weather => whole(&r.key, 2.0),
            SectionKind::Lottery => whole(&r.key, 1.0),
            SectionKind::Horoscope => {
                let signs = match cfg.horoscope.signs.len() {
                    0 => 12,
                    n => n,
                };
                whole(&r.key, 0.25 * signs as f64)
            }
            SectionKind::Puzzle | SectionKind::Shelf => whole(&r.key, 1.0),
            SectionKind::Onthisday => whole(&r.key, 2.0),
        };
    }
    ((total / page).round() as u32).max(1)
}

/// The next paper as the Daily tab and `digest next` show it.
pub fn plan(i: &Inputs) -> NextPaper {
    let cfg = i.cfg;
    let press = press(i.at, i.stopped, i.now);
    let date = press.edition;
    let since = press.since.with_timezone(&i.now.timezone());
    // As `gather` does: saved articles print first even when Your Reading is off.
    let mut kinds = cfg.sections.clone();
    if !kinds.contains(&SectionKind::Reading) && !pending(i.queue, date).is_empty() {
        kinds.insert(0, SectionKind::Reading);
    }
    let mut on: Vec<Row> = kinds.iter().flat_map(|k| rows(i, *k, date)).collect();
    let off: Vec<PlanRow> = KINDS
        .iter()
        .filter(|k| !kinds.contains(k))
        .flat_map(|k| rows(i, *k, date))
        .map(|r| r.row)
        .collect();
    // As the press does: Local and places only record their titles; a feed row drops the titles
    // an earlier news row shows, before its cut, and records what it keeps.
    let mut taken: HashSet<String> = HashSet::new();
    for r in &mut on {
        match (r.row.kind, r.stories.as_mut()) {
            (SectionKind::Local, Some(s)) => taken.extend(s.iter().map(|i| title_key(&i.title))),
            (SectionKind::Feeds, Some(s)) => {
                s.retain(|i| !taken.contains(&title_key(&i.title)));
                s.truncate(r.row.items.unwrap_or(usize::MAX));
                taken.extend(s.iter().map(|i| title_key(&i.title)));
                r.planned = s.len();
            }
            _ => {}
        }
    }
    for r in &mut on {
        if let Some(share) = i.shares.get(&r.row.key).filter(|s| **s <= RARELY) {
            r.row
                .detail
                .push_str(&format!(" · you get to it about {share}% of days"));
        }
    }
    if let Some(last) = on.last_mut().filter(|r| r.row.items.is_none()) {
        last.row.budget = "back page".into();
    }

    // The lead, by the press's own rule; the preview can't see opening paragraphs.
    let mut candidates = Vec::new();
    for (s, r) in on.iter().enumerate() {
        let (tier, n) = match r.row.kind {
            SectionKind::Reading => (Tier::Reading, r.saved.len()),
            // only your own Local leads by rule; another place counts like a feed
            SectionKind::Local if r.row.place.is_none() => {
                (Tier::Local, r.stories.as_ref().map_or(0, Vec::len))
            }
            SectionKind::Local => (Tier::Feed, r.stories.as_ref().map_or(0, Vec::len)),
            SectionKind::Feeds => (Tier::Feed, r.stories.as_ref().map_or(0, Vec::len)),
            _ => continue,
        };
        candidates.extend((0..n).map(|article| Candidate {
            tier,
            section: s,
            article,
            teaser: true,
        }));
    }
    let lead_at = lead(&candidates).map(|c| (candidates[c].section, candidates[c].article));
    let now = i.now;
    let lead_tile = lead_at.map(|(s, a)| {
        let r = &on[s];
        match r.row.kind {
            SectionKind::Reading => {
                let e = r.saved[a];
                Tile {
                    key: r.row.key.clone(),
                    kicker: saved_kicker(e),
                    title: Some(saved_title(e)),
                    reason: Some(format!(
                        "{} Leads the paper because nothing local outranks it yet.",
                        saved_line(e.saved_at.with_timezone(&now.timezone()), now, cfg.units)
                    )),
                    dashed: false,
                }
            }
            kind => {
                let items = r.stories.as_deref().unwrap_or_default();
                let reason = if kind == SectionKind::Local && r.row.place.is_none() {
                    "Leads the paper: local news always leads.".to_owned()
                } else {
                    format!(
                        "Leads the paper: nothing local yet, and {} comes first in your page \
                         plan.",
                        r.row.name
                    )
                };
                Tile {
                    key: r.row.key.clone(),
                    kicker: story_kicker(&r.row.name, items, since),
                    title: Some(items[a].title.clone()),
                    reason: Some(reason),
                    dashed: false,
                }
            }
        }
    });

    let mut tiles = Vec::new();
    for (s, r) in on.iter().enumerate() {
        let is_lead = |a: usize| lead_at == Some((s, a));
        match r.row.kind {
            SectionKind::Reading => {
                for (_, e) in r.saved.iter().enumerate().filter(|(a, _)| !is_lead(*a)) {
                    tiles.push(Tile {
                        key: r.row.key.clone(),
                        kicker: saved_kicker(e),
                        title: Some(saved_title(e)),
                        reason: Some(saved_line(
                            e.saved_at.with_timezone(&now.timezone()),
                            now,
                            cfg.units,
                        )),
                        dashed: false,
                    });
                }
            }
            SectionKind::Local | SectionKind::Feeds => match r.stories.as_deref() {
                Some(items) if !items.is_empty() => {
                    // its newest headline, unless that one leads the paper
                    if let Some((_, item)) = items.iter().enumerate().find(|(a, _)| !is_lead(*a)) {
                        tiles.push(Tile {
                            key: r.row.key.clone(),
                            kicker: story_kicker(&r.row.name, items, since),
                            title: Some(item.title.clone()),
                            reason: None,
                            dashed: false,
                        });
                    }
                }
                _ => tiles.push(Tile {
                    key: r.row.key.clone(),
                    kicker: format!("{} · {} slots", r.row.name, r.row.items.unwrap_or(0)),
                    title: None,
                    reason: None,
                    dashed: true,
                }),
            },
            // the masthead says it: "Weather · fills in at press time"
            SectionKind::Weather => {}
            SectionKind::Puzzle => tiles.push(Tile {
                key: r.row.key.clone(),
                kicker: "Puzzle · yesterday's answers".into(),
                title: None,
                reason: None,
                dashed: true,
            }),
            _ => tiles.push(Tile {
                key: r.row.key.clone(),
                kicker: format!("{} · fills at press time", r.row.name),
                title: None,
                reason: None,
                dashed: true,
            }),
        }
    }

    let planned: Vec<usize> = on.iter().map(|r| r.planned).collect();
    let plan: Vec<PlanRow> = on.into_iter().map(|r| r.row).collect();
    let pages = estimate(&plan, &planned, i.history, cfg);
    NextPaper {
        paper: cfg.title.clone(),
        place: cfg.place.clone(),
        date,
        number: date.ordinal(),
        weather: cfg.sections.contains(&SectionKind::Weather),
        lead: lead_tile,
        tiles,
        checked: super::live::checked(i.live),
        plan,
        off,
        pages,
        week: i.week.clone(),
        press,
    }
}
