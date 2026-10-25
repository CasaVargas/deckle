// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle Daily: a newspaper EPUB built from free, no-key sources
//! (docs/superpowers/specs/2026-10-20-daily-digest-design.md).
//!
//! `gather` turns a [`Config`] into an [`Edition`]; a failing source becomes an "unavailable
//! today" note in its section and never fails the edition. `build` writes the EPUB.

pub mod article;
pub mod bookmarklet;
pub mod bundle;
pub mod config;
pub mod cover;
pub mod day;
pub mod editor;
pub mod epub;
pub mod export;
pub mod fetch;
pub mod front;
pub mod frost;
pub mod geocode;
pub mod horoscope;
pub mod inbox;
pub mod longread;
pub mod mail;
pub mod masthead;
pub mod moon;
pub mod next;
pub mod papers;
pub mod pdf;
pub mod press;
pub mod publisher;
pub mod puzzle;
pub mod reading;
pub mod render;
pub mod schedule;
pub mod seasons;
pub mod secret;
pub mod session;
pub mod shelf;
pub mod showers;
pub mod sources;
pub mod sun;
pub mod tz;
pub mod web_book;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::Serialize;

pub use chrono;
pub use chrono_tz;
pub use config::{Config, SectionKind};
pub use fetch::{Http, Net};
pub use publisher::Access;
pub use session::SessionNet;
pub use url;
pub use zeroize;

/// The author stamped on every edition.
pub const AUTHOR: &str = "Deckle";

/// Editions land on the Kindle as `deckle-daily-YYYY-MM-DD.kfx`: sortable by name, so pruning
/// works over MTP too (which reports no dates). The Kindle shows the title from the metadata.
pub const EDITION_PREFIX: &str = "deckle-daily-";

pub fn edition_filename(date: NaiveDate) -> String {
    format!("{EDITION_PREFIX}{date}.kfx")
}

/// The paper's edition prefix: `deckle-daily-` for the main paper, `deckle-daily-<slug>-` for
/// another (many papers §4).
pub fn edition_prefix(cfg: &Config) -> String {
    match &cfg.paper {
        None => EDITION_PREFIX.to_owned(),
        Some(p) => format!("{EDITION_PREFIX}{}-", p.slug),
    }
}

/// `<prefix>YYYY-MM-DD`: the paper's edition on `date`, for every file it's written as.
pub fn edition_stem(cfg: &Config, date: NaiveDate) -> String {
    format!("{}{date}", edition_prefix(cfg))
}

/// `<stem>.kfx`: the edition on the Kindle.
pub fn edition_file(cfg: &Config, date: NaiveDate) -> String {
    format!("{}.kfx", edition_stem(cfg, date))
}

/// `<prefix>YYYY-MM-DD.kfx` or `.epub`, exactly: one paper's edition, never another paper's
/// (`deckle-daily-kid-courier-…` for prefix `deckle-daily-`) or a test file.
pub fn is_edition_name(name: &str, prefix: &str) -> bool {
    deckle_core::deliver::is_dated(name, prefix)
        && (name.ends_with(".kfx") || name.ends_with(".epub"))
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
    /// The story's own page, when it has one (edition bundle design §9). The EPUB doesn't
    /// link out; a native reader may.
    pub url: Option<String>,
    /// Who published it: the feed's source, else the page's host.
    pub outlet: Option<String>,
    /// What only a native reader uses: photo masters and captions, the byline, when it was
    /// published (Books and Kindle design §9).
    pub extra: Extra,
}

/// The parts of a story the edition bundle carries and the EPUB doesn't.
#[derive(Clone, Debug, Default)]
pub struct Extra {
    /// One per photo, in [`Article::photos`] order (or empty: none known).
    pub photos: Vec<PhotoExtra>,
    pub byline: Option<String>,
    /// RFC 3339.
    pub published: Option<String>,
    /// How the page was fetched: open, through a subscription, or walled.
    pub access: publisher::Access,
}

#[derive(Clone, Debug, Default)]
pub struct PhotoExtra {
    /// The photo in color at up to 2400 px, when a bundle wanted it.
    pub master: Option<Vec<u8>>,
    pub caption: Option<String>,
    pub credit: Option<String>,
    pub alt: Option<String>,
}

impl Article {
    /// Removes the photos `drop(n, jpeg, is_lead)` picks: their `<img>`, any figure left empty,
    /// and renumbers the tokens of the photos after them.
    fn drop_photos_where(&mut self, drop: impl Fn(usize, &[u8], bool) -> bool) {
        use sources::feeds::photo_token;
        let mut n = 0;
        while n < self.photos.len() {
            let lead = self.body.contains(&format!(
                "<div class=\"figure lead\"><img src=\"{}\"",
                photo_token(n)
            ));
            if !drop(n, &self.photos[n], lead) {
                n += 1;
                continue;
            }
            self.photos.remove(n);
            if n < self.extra.photos.len() {
                self.extra.photos.remove(n);
            }
            let img = format!("<img src=\"{}\" alt=\"\"/>", photo_token(n));
            self.body = article::strip_empty_figures(&self.body.replace(&img, ""));
            for m in n + 1..=self.photos.len() {
                self.body = self.body.replace(&photo_token(m), &photo_token(m - 1));
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Section {
    /// AI-written news brief; empty unless the optional editor succeeded.
    pub brief: Vec<String>,
    /// What the section is across editions: `config::section_key` (readership design §4).
    pub key: String,
    pub title: String,
    /// One line under the section heading.
    pub kicker: Option<String>,
    pub articles: Vec<Article>,
    /// Why the section is empty today, when a source failed.
    pub unavailable: Option<String>,
    /// The weather page's numbers, beside the XHTML they print (edition bundle design §6.5).
    pub weather: Option<sources::weather::WeatherData>,
    /// Today's puzzle and yesterday's, beside the XHTML they print (edition bundle §6.6).
    pub puzzle: Option<puzzle::PuzzleData>,
}

#[derive(Clone, Debug)]
pub struct Edition {
    pub paper: String,
    pub date: NaiveDate,
    pub place: String,
    /// "Clear · 74°/40°F · 0% rain", for the front page and the cover.
    pub weather_line: Option<String>,
    /// The face the paper's name is drawn in on the cover and the PDF.
    pub masthead: masthead::Masthead,
    pub sections: Vec<Section>,
    /// Pictures pages show, `(href, PNG)` inside `OEBPS/`: the weather icons.
    pub assets: Vec<(String, Vec<u8>)>,
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

    /// Drops a site's template picture (a default author avatar, a logo) from the stories: a
    /// photo whose bytes show up in two or more articles. A story's lead photo stays, and so
    /// does a photo repeated only within one story.
    pub fn drop_template_photos(&mut self) {
        use std::collections::{HashMap, HashSet};
        let mut stories: HashMap<&[u8], HashSet<(usize, usize)>> = HashMap::new();
        for (i, s) in self.sections.iter().enumerate() {
            for (j, a) in s.articles.iter().enumerate() {
                for jpeg in &a.photos {
                    stories.entry(jpeg).or_default().insert((i, j));
                }
            }
        }
        let template: HashSet<Vec<u8>> = stories
            .into_iter()
            .filter(|(_, at)| at.len() > 1)
            .map(|(jpeg, _)| jpeg.to_vec())
            .collect();
        if template.is_empty() {
            return;
        }
        for a in self.sections.iter_mut().flat_map(|s| &mut s.articles) {
            a.drop_photos_where(|_, jpeg, lead| !lead && template.contains(jpeg));
        }
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
    /// The paper with its photos in color, in `color/` beside the paper, when an export wants
    /// it (other readers design §6.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_epub: Option<PathBuf>,
    /// The long reads in color, in the same order as `long_reads`, beside `color_epub`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub color_long_reads: Vec<PathBuf>,
    /// The PDF editions, one per screen and photos asked for (PDF edition design §7).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pdfs: Vec<PdfBuild>,
    /// The edition bundles, with and without the long reads, as asked for (edition bundle §4).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bundles: Vec<BundleBuild>,
    pub reading_outcomes: Vec<(String, reading::Outcome)>,
    pub notes: Vec<String>,
    /// The paper's edition prefix (`edition_prefix`): what exports name and prune by.
    pub prefix: String,
}

/// One edition bundle, `books` saying whether it carries the long reads.
#[derive(Clone, Debug, Serialize)]
pub struct BundleBuild {
    pub books: bool,
    pub path: PathBuf,
}

/// One PDF edition: the paper laid out for `screen`, its photos `photos`.
#[derive(Clone, Debug, Serialize)]
pub struct PdfBuild {
    pub screen: pdf::Screen,
    pub photos: config::Photos,
    pub path: PathBuf,
}

impl BuildReport {
    /// The edition bundle with (or without) the long reads, when the build made one.
    pub fn bundle(&self, books: bool) -> Option<&Path> {
        self.bundles
            .iter()
            .find(|b| b.books == books)
            .map(|b| b.path.as_path())
    }

    /// The PDF edition for `screen` and `photos`, when the build made one.
    pub fn pdf(&self, screen: pdf::Screen, photos: config::Photos) -> Option<&Path> {
        self.pdfs
            .iter()
            .find(|p| p.screen == screen && p.photos == photos)
            .map(|p| p.path.as_path())
    }
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

/// What a caller knows that the config doesn't.
#[derive(Clone, Copy, Debug, Default)]
pub struct GatherOptions {
    /// The Kindle's own progress fraction may print as "about N%" on From Your Shelf (set from
    /// `deckle_shelf::KINDLE_FRACTION_OK`, which this crate can't see).
    pub shelf_fraction_ok: bool,
}

/// Fetches every configured source through `get`. Never fails: a source error becomes the
/// section's `unavailable` note.
pub fn gather(cfg: &Config, date: NaiveDate, now: DateTime<Utc>, net: &dyn Net) -> Gathered {
    gather_with(cfg, date, now, net, &GatherOptions::default())
}

/// [`gather`] with the options a caller supplies.
pub fn gather_with(
    cfg: &Config,
    date: NaiveDate,
    now: DateTime<Utc>,
    net: &dyn Net,
    opts: &GatherOptions,
) -> Gathered {
    let key = std::env::var(cfg.llm.provider.env_key()).unwrap_or_default();
    let key = zeroize::Zeroizing::new(key);
    gather_with_editor(
        cfg,
        date,
        now,
        net,
        opts,
        &key,
        &mut deckle_xray::llm::http_post,
    )
}

/// Deterministic/offline-test entry point: provider HTTP and credentials are injected.
#[allow(clippy::too_many_arguments)]
pub fn gather_with_editor(
    cfg: &Config,
    date: NaiveDate,
    now: DateTime<Utc>,
    net: &dyn Net,
    opts: &GatherOptions,
    key: &str,
    post: &mut deckle_xray::llm::Post<'_>,
) -> Gathered {
    let mut edition = Edition {
        paper: cfg.title.clone(),
        date,
        place: cfg.place.clone(),
        weather_line: None,
        masthead: masthead::Masthead::default(),
        sections: Vec::new(),
        assets: Vec::new(),
    };
    let mut news = Vec::new();
    // Titles already printed by Local, places and earlier feed sections (Your Reading is neither).
    let mut printed = std::collections::HashSet::<String>::new();
    let mut g_reading = None;
    let mut long_reads = Vec::new();
    let mut reading_outcomes = Vec::new();
    let mut notes = Vec::new();
    match cfg.masthead.resolve(&cfg.title) {
        Ok(m) => edition.masthead = m,
        Err(why) => notes.push(format!("Masthead font: {why}; using the blackletter.")),
    }
    // Loaded when the paper has a puzzle; saved with today's headlines once all is gathered.
    let mut puzzles: Option<puzzle::store::Store> = None;
    let queue = reading::ReadingQueue::new(
        cfg.reading
            .queue_path
            .clone()
            .unwrap_or_else(reading::ReadingQueue::default_path),
    );
    // Saved articles are the main paper's (many papers §3): another paper never prints them.
    let saved = match cfg.paper {
        None => queue.for_edition(date, cfg.reading.max_items),
        Some(_) => Vec::new(),
    };
    let mut kinds = cfg.sections.clone();
    if cfg.paper.is_some() && kinds.contains(&SectionKind::Reading) {
        kinds.retain(|k| *k != SectionKind::Reading);
        notes.push("Saved articles go to the main paper (digest.toml).".into());
    }
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
                let (section, line, assets) =
                    sources::weather::section(cfg, date, tz::local_tz(), net);
                edition.weather_line = line;
                for (href, bytes) in assets {
                    if !edition.assets.iter().any(|(h, _)| *h == href) {
                        edition.assets.push((href, bytes));
                    }
                }
                edition.sections.push(section);
            }
            SectionKind::Local => {
                news.push(edition.sections.len());
                let local = sources::feeds::local_section(cfg, now, net);
                printed.extend(
                    local
                        .articles
                        .iter()
                        .map(|a| sources::feeds::title_key(&a.title)),
                );
                edition.sections.push(local);
                // then other places' local news, each its own section (#102)
                for p in &cfg.local.places {
                    news.push(edition.sections.len());
                    let place = sources::feeds::place_section(p, cfg, now, net);
                    printed.extend(
                        place
                            .articles
                            .iter()
                            .map(|a| sources::feeds::title_key(&a.title)),
                    );
                    edition.sections.push(place);
                }
            }
            SectionKind::Feeds => {
                for feed in &cfg.feeds {
                    news.push(edition.sections.len());
                    edition.sections.push(sources::feeds::feed_section(
                        feed,
                        cfg,
                        now,
                        net,
                        &mut printed,
                    ));
                }
            }
            SectionKind::Lottery => edition.sections.push(sources::lottery::section(cfg, net)),
            SectionKind::Horoscope => edition.sections.push(horoscope::section(cfg, date)),
            SectionKind::Puzzle => {
                let store =
                    puzzles.get_or_insert_with(|| puzzle::store::Store::load(&cfg.puzzle_store()));
                edition.sections.push(puzzle::section(
                    date,
                    cfg,
                    store,
                    puzzle::words::Words::embedded(),
                ));
            }
            SectionKind::Shelf => {
                let path = cfg.shelf_store();
                let mut store = shelf::store::Shelf::load(&path);
                let before = store.clone();
                match shelf::section(
                    &mut store,
                    cfg,
                    date,
                    tz::local_tz(),
                    opts.shelf_fraction_ok,
                ) {
                    Some(section) => edition.sections.push(section),
                    None if store.kindles.is_empty() => notes.push(
                        "From Your Shelf left out: plug your Kindle in once to fill it".into(),
                    ),
                    None => {}
                }
                store.prune_shown(date);
                if store != before
                    && let Err(e) = store.save(&path)
                {
                    notes.push(format!(
                        "From Your Shelf wasn't saved ({e}); a rebuild today prints the same page"
                    ));
                }
            }
            SectionKind::Onthisday => edition
                .sections
                .push(sources::onthisday::section(date, net)),
        }
        // Each `[[feeds]]` entry is its own section, in file order; so is each place after Local.
        for (i, s) in edition.sections[first_new..].iter_mut().enumerate() {
            let name = match kind {
                SectionKind::Feeds => Some(cfg.feeds[i].name.as_str()),
                SectionKind::Local if i > 0 => Some(cfg.local.places[i - 1].name.as_str()),
                _ => None,
            };
            s.key = config::section_key(*kind, name);
        }
    }
    // Only the paper of the day (or the next one, built late) is kept: a preview of another date
    // prints a puzzle but neither stores it nor prunes the real ones.
    let today = now.with_timezone(&tz::local_tz()).date_naive();
    if let Some(mut store) = puzzles.filter(|_| (date - today).num_days().abs() <= 1) {
        store.prune(today);
        // tomorrow's theme: today's headlines from the news and Your Reading
        let mut from = news.clone();
        from.extend(g_reading);
        let found = puzzle::theme::harvest(&edition, &from, puzzle::words::Words::embedded());
        if let Some(day) = store.days.get_mut(&date) {
            day.headlines = found;
        }
        let path = cfg.puzzle_store();
        if let Err(e) = store.save(&path) {
            notes.push(format!(
                "The puzzle wasn't saved ({}: {e}); a rebuild today makes it again",
                path.display()
            ));
        }
    }
    edition.drop_template_photos();
    let mut gathered = Gathered {
        edition,
        news,
        reading: g_reading,
        long_reads,
        reading_outcomes,
        notes,
    };
    editor::edit(&mut gathered, &cfg.llm, key, post);
    gathered
}

/// Gathers and writes `<out_dir>/deckle-daily-<date>.epub`.
pub fn build(
    cfg: &Config,
    date: NaiveDate,
    now: DateTime<Utc>,
    out_dir: &Path,
    net: &dyn Net,
) -> Result<BuildReport, String> {
    build_with(cfg, date, now, out_dir, net, &GatherOptions::default())
}

/// [`build`] with the options a caller supplies.
pub fn build_with(
    cfg: &Config,
    date: NaiveDate,
    now: DateTime<Utc>,
    out_dir: &Path,
    net: &dyn Net,
    opts: &GatherOptions,
) -> Result<BuildReport, String> {
    build_gathered(cfg, date, now, out_dir, net, opts).map(|(report, _)| report)
}

/// [`build_with`], also handing back what was gathered (the Morgue keeps its text).
pub fn build_gathered(
    cfg: &Config,
    date: NaiveDate,
    now: DateTime<Utc>,
    out_dir: &Path,
    net: &dyn Net,
    opts: &GatherOptions,
) -> Result<(BuildReport, Gathered), String> {
    let gathered = gather_with(cfg, date, now, net, opts);
    let stem = edition_stem(cfg, date);
    let path = out_dir.join(format!("{stem}.epub"));
    write_edition(&gathered, &path)?;
    let mut long_reads = Vec::new();
    for lr in &gathered.long_reads {
        let p = out_dir.join(format!("{}.epub", lr.file_stem));
        longread::write_long_read(lr, date, &p)?;
        long_reads.push(p);
    }
    let mut r = report(&gathered.edition, path);
    r.prefix = edition_prefix(cfg);
    r.long_reads = long_reads;
    // The fetch kept the photos in color for an export; the paper above grayed them.
    if cfg.wants_color() {
        let dir = out_dir.join("color");
        let color = dir.join(format!("{stem}.epub"));
        write_edition_as(&gathered, &color, config::Photos::Color)?;
        r.color_epub = Some(color);
        for lr in &gathered.long_reads {
            let p = dir.join(format!("{}.epub", lr.file_stem));
            longread::write_long_read_as(lr, date, &p, config::Photos::Color)?;
            r.color_long_reads.push(p);
        }
    }
    for (screen, photos) in cfg.pdfs_wanted() {
        let p = out_dir
            .join("pdf")
            .join(format!("{screen:?}-{photos:?}").to_lowercase())
            .join(format!("{stem}.pdf"));
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, pdf::render(&gathered, screen, photos))
            .map_err(|e| format!("{}: {e}", p.display()))?;
        r.pdfs.push(PdfBuild {
            screen,
            photos,
            path: p,
        });
    }
    for books in cfg.bundles_wanted() {
        let dir = out_dir
            .join("bundle")
            .join(if books { "books" } else { "paper" });
        let path = bundle::write(&gathered, &r, &dir, books, now)?;
        r.bundles.push(BundleBuild { books, path });
    }
    r.reading_outcomes = gathered.reading_outcomes.clone();
    r.notes = gathered.notes.clone();
    Ok((r, gathered))
}

/// Renders and writes one gathered edition as an EPUB.
pub fn write_edition(gathered: &Gathered, path: &Path) -> Result<(), String> {
    write_edition_as(gathered, path, config::Photos::Gray)
}

/// [`write_edition`] with the photos grayscale for e-ink, or as they were fetched.
pub fn write_edition_as(
    gathered: &Gathered,
    path: &Path,
    photos: config::Photos,
) -> Result<(), String> {
    let cover = cover::render(gathered).map_err(|e| format!("cover: {e}"))?;
    let files = render::render_as(gathered, cover, photos);
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
        color_epub: None,
        color_long_reads: Vec::new(),
        pdfs: Vec::new(),
        bundles: Vec::new(),
        reading_outcomes: Vec::new(),
        notes: Vec::new(),
        prefix: EDITION_PREFIX.to_owned(),
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
