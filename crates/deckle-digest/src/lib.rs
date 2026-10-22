// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle Daily: a newspaper EPUB built from free, no-key sources
//! (docs/superpowers/specs/2026-10-20-daily-digest-design.md).
//!
//! `gather` turns a [`Config`] into an [`Edition`]; a failing source becomes an "unavailable
//! today" note in its section and never fails the edition. `build` writes the EPUB.

pub mod article;
pub mod bookmarklet;
pub mod config;
pub mod cover;
pub mod day;
pub mod epub;
pub mod fetch;
pub mod frost;
pub mod geocode;
pub mod horoscope;
pub mod inbox;
pub mod longread;
pub mod mail;
pub mod moon;
pub mod reading;
pub mod render;
pub mod schedule;
pub mod seasons;
pub mod secret;
pub mod showers;
pub mod sources;
pub mod sun;
pub mod tz;
pub mod web_book;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::Serialize;

pub use chrono;
pub use config::{Config, SectionKind};
pub use fetch::{Http, Net};
pub use zeroize;

/// The author stamped on every edition.
pub const AUTHOR: &str = "Deckle";

/// Editions land on the Kindle as `deckle-daily-YYYY-MM-DD.kfx`: sortable by name, so pruning
/// works over MTP too (which reports no dates). The Kindle shows the title from the metadata.
pub const EDITION_PREFIX: &str = "deckle-daily-";

pub fn edition_filename(date: NaiveDate) -> String {
    format!("{EDITION_PREFIX}{date}.kfx")
}

#[derive(Clone, Debug, Default)]
pub struct Article {
    pub title: String,
    /// Small line under the headline: source, time, drawing date.
    pub meta: Option<String>,
    /// XHTML fragment, already escaped.
    pub body: String,
    /// A plain-text opening paragraph for the cover's lead story.
    pub teaser: Option<String>,
    /// E-ink JPEGs; the body refers to photo `n` as [`sources::feeds::photo_token`]`(n)`.
    pub photos: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
pub struct Section {
    /// What the section is across editions: `config::section_key` (readership design §4).
    pub key: String,
    pub title: String,
    /// One line under the section heading.
    pub kicker: Option<String>,
    pub articles: Vec<Article>,
    /// Why the section is empty today, when a source failed.
    pub unavailable: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Edition {
    pub paper: String,
    pub date: NaiveDate,
    pub place: String,
    /// "Clear · 74°/40°F · 0% rain", for the front page and the cover.
    pub weather_line: Option<String>,
    pub sections: Vec<Section>,
}

impl Edition {
    /// `<paper> — Tuesday, October 20, 2026`
    pub fn full_title(&self) -> String {
        format!("{} — {}", self.paper, long_date(self.date))
    }

    /// The edition number: the day of the year.
    pub fn number(&self) -> u32 {
        self.date.ordinal()
    }

    /// Lead headlines: the first article of each news section, then the seconds, and so on.
    /// News sections are the ones whose articles are stories (local and feeds).
    pub fn headlines(&self, news: &[usize], max: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let depth = news
            .iter()
            .map(|&s| self.sections[s].articles.len())
            .max()
            .unwrap_or(0);
        for a in 0..depth {
            for &s in news {
                if a < self.sections[s].articles.len() && out.len() < max {
                    out.push((s, a));
                }
            }
        }
        out
    }
}

pub fn long_date(d: NaiveDate) -> String {
    d.format("%A, %B %-d, %Y").to_string()
}

#[derive(Clone, Debug, Serialize)]
pub struct SectionSummary {
    pub key: String,
    pub title: String,
    pub items: usize,
    pub unavailable: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BuildReport {
    pub epub: PathBuf,
    pub title: String,
    pub sections: Vec<SectionSummary>,
    /// Long-read EPUBs written beside the paper.
    pub long_reads: Vec<PathBuf>,
    pub reading_outcomes: Vec<(String, reading::Outcome)>,
    pub notes: Vec<String>,
}

/// A gathered edition plus which of its sections carry news stories (for the headlines).
pub struct Gathered {
    pub edition: Edition,
    pub news: Vec<usize>,
    /// The Your Reading section's index, when it has articles.
    pub reading: Option<usize>,
    pub long_reads: Vec<sources::reading::LongRead>,
    /// What this edition did with each saved entry (committed by `digest run` only).
    pub reading_outcomes: Vec<(String, reading::Outcome)>,
    /// One-line notices for the person running the build.
    pub notes: Vec<String>,
}

/// Fetches every configured source through `get`. Never fails: a source error becomes the
/// section's `unavailable` note.
pub fn gather(cfg: &Config, date: NaiveDate, now: DateTime<Utc>, net: &dyn Net) -> Gathered {
    let mut edition = Edition {
        paper: cfg.title.clone(),
        date,
        place: cfg.place.clone(),
        weather_line: None,
        sections: Vec::new(),
    };
    let mut news = Vec::new();
    let mut g_reading = None;
    let mut long_reads = Vec::new();
    let mut reading_outcomes = Vec::new();
    let mut notes = Vec::new();
    let queue = reading::ReadingQueue::new(
        cfg.reading
            .queue_path
            .clone()
            .unwrap_or_else(reading::ReadingQueue::default_path),
    );
    let saved = queue.for_edition(date, cfg.reading.max_items);
    let mut kinds = cfg.sections.clone();
    if !saved.is_empty() && !kinds.contains(&SectionKind::Reading) {
        // Saving is an explicit ask: print it even if the config predates the section.
        kinds.insert(0, SectionKind::Reading);
        notes.push(
            "Your Reading added first: you have saved articles and \"reading\" isn't in \
             sections (add it to digest.toml to choose its place)"
                .into(),
        );
    }
    for kind in &kinds {
        let first_new = edition.sections.len();
        match kind {
            SectionKind::Reading => {
                if saved.is_empty() {
                    continue;
                }
                let (section, lrs, outcomes) = sources::reading::section(&saved, cfg, net, date);
                long_reads.extend(lrs);
                reading_outcomes.extend(outcomes);
                if !section.articles.is_empty() {
                    g_reading = Some(edition.sections.len());
                    edition.sections.push(section);
                }
            }
            SectionKind::Weather => {
                let (section, line) = sources::weather::section(cfg, date, net);
                edition.weather_line = line;
                edition.sections.push(section);
            }
            SectionKind::Local => {
                news.push(edition.sections.len());
                edition
                    .sections
                    .push(sources::feeds::local_section(cfg, now, net));
            }
            SectionKind::Feeds => {
                for feed in &cfg.feeds {
                    news.push(edition.sections.len());
                    edition
                        .sections
                        .push(sources::feeds::feed_section(feed, cfg, now, net));
                }
            }
            SectionKind::Lottery => edition.sections.push(sources::lottery::section(cfg, net)),
            SectionKind::Horoscope => edition.sections.push(horoscope::section(cfg, date)),
            SectionKind::Onthisday => edition
                .sections
                .push(sources::onthisday::section(date, net)),
        }
        // Each `[[feeds]]` entry is its own section, in file order.
        for (i, s) in edition.sections[first_new..].iter_mut().enumerate() {
            let feed = (*kind == SectionKind::Feeds).then(|| cfg.feeds[i].name.as_str());
            s.key = config::section_key(*kind, feed);
        }
    }
    Gathered {
        edition,
        news,
        reading: g_reading,
        long_reads,
        reading_outcomes,
        notes,
    }
}

/// Gathers and writes `<out_dir>/deckle-daily-<date>.epub`.
pub fn build(
    cfg: &Config,
    date: NaiveDate,
    now: DateTime<Utc>,
    out_dir: &Path,
    net: &dyn Net,
) -> Result<BuildReport, String> {
    let gathered = gather(cfg, date, now, net);
    let path = out_dir.join(format!("deckle-daily-{date}.epub"));
    write_edition(&gathered, &path)?;
    let mut long_reads = Vec::new();
    for lr in &gathered.long_reads {
        let p = out_dir.join(format!("{}.epub", lr.file_stem));
        longread::write_long_read(lr, date, &p)?;
        long_reads.push(p);
    }
    let mut r = report(&gathered.edition, path);
    r.long_reads = long_reads;
    r.reading_outcomes = gathered.reading_outcomes;
    r.notes = gathered.notes;
    Ok(r)
}

/// Renders and writes one gathered edition as an EPUB.
pub fn write_edition(gathered: &Gathered, path: &Path) -> Result<(), String> {
    let cover = cover::render(gathered).map_err(|e| format!("cover: {e}"))?;
    let files = render::render(gathered, cover);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    epub::write(path, &files).map_err(|e| format!("{}: {e}", path.display()))
}

fn report(edition: &Edition, epub: PathBuf) -> BuildReport {
    BuildReport {
        epub,
        title: edition.full_title(),
        sections: edition
            .sections
            .iter()
            .map(|s| SectionSummary {
                key: s.key.clone(),
                title: s.title.clone(),
                items: s.articles.len(),
                unavailable: s.unavailable.clone(),
            })
            .collect(),
        long_reads: Vec::new(),
        reading_outcomes: Vec::new(),
        notes: Vec::new(),
    }
}

/// XML-escapes text for element content and attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\t') => {}
            c => out.push(c),
        }
    }
    out
}
