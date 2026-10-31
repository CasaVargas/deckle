// SPDX-License-Identifier: GPL-3.0-or-later
//! The edition bundle (docs/superpowers/specs/2026-10-23-edition-bundle-design.md): the paper
//! as data for native readers. A `deckle-daily-YYYY-MM-DD.deckle` folder holds `edition.json`,
//! the cover, the photos and icons by content hash, and the edition's EPUB.
//!
//! It is written into a hidden `.….deckle.tmp` folder and renamed into place, and
//! `edition.json` is its last file, listing every other file with its size and SHA-256: a
//! bundle that iCloud Drive has only partly delivered is never read as whole.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::Units;
use crate::print::PrintPlan;
use crate::sources::feeds::photo_token;
use crate::{BuildReport, EDITION_PREFIX, Gathered};

/// The format's name and the version this writer writes (§6.1, §10).
pub const FORMAT: &str = "deckle-edition";
pub const VERSION: &str = "1.3";

/// The bundle's folder name for `date`.
pub fn bundle_name(date: NaiveDate) -> String {
    format!("{EDITION_PREFIX}{date}.deckle")
}

#[derive(Serialize)]
struct EditionJson {
    format: &'static str,
    version: &'static str,
    generator: String,
    edition: EditionInfo,
    front: Front,
    sections: Vec<SectionJson>,
    files: Vec<FileEntry>,
}

#[derive(Serialize)]
struct EditionInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<crate::press::Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    paper_id: Option<crate::press::Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    paper: String,
    date: String,
    number: u32,
    place: String,
    timezone: String,
    built_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    weather_line: Option<String>,
    cover: &'static str,
}

#[derive(Serialize)]
struct Front {
    lead: Option<String>,
    tiles: Vec<String>,
    // 1.3: the print plan (edition bundle design §6.7).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skybox: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lead_layout: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    jump: Option<JumpJson>,
}

#[derive(Serialize)]
struct JumpJson {
    story: String,
    slug: String,
    /// How many of the body's top-level paragraphs the front page prints, from the first.
    opening: usize,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
struct PullJson {
    text: String,
    /// The top-level paragraph it follows, counted over the story's whole body.
    after: usize,
}

#[derive(Serialize)]
struct SectionJson {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ai_brief: Vec<String>,
    // 1.3: the book the section prints in.
    #[serde(skip_serializing_if = "Option::is_none")]
    book: Option<String>,
    key: String,
    kind: &'static str,
    title: String,
    kicker: Option<String>,
    unavailable: Option<String>,
    stories: Vec<StoryJson>,
    weather: Option<WeatherJson>,
    puzzle: Option<PuzzleJson>,
}

#[derive(Serialize)]
struct StoryJson {
    id: String,
    title: String,
    meta: Option<String>,
    teaser: Option<String>,
    url: Option<String>,
    outlet: Option<String>,
    words: usize,
    body: Body,
    photos: Vec<PhotoJson>,
    long_read: Option<String>,
    // 1.1 (Books and Kindle design §9):
    byline: Option<String>,
    published: Option<String>,
    minutes: usize,
    prominence: &'static str,
    partial: bool,
    pull_quote: Option<String>,
    // 1.2: how the story was fetched.
    access: &'static str,
    // 1.3: the print plan's pull quote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pull: Option<PullJson>,
}

#[derive(Serialize)]
struct Body {
    format: &'static str,
    text: String,
}

#[derive(Serialize)]
struct PhotoJson {
    file: String,
    width: u32,
    height: u32,
    lead: bool,
    /// Moving pictures are made by the reader from the still (parallax): always null.
    motion: Option<()>,
    // 1.1 (Books and Kindle design §9):
    caption: Option<String>,
    credit: Option<String>,
    alt: Option<String>,
    focus: Option<Focus>,
    color: Option<String>,
}

/// Where the photo's subject is, 0–1 from the top left: crops keep it in frame.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
struct Focus {
    x: f64,
    y: f64,
}

#[derive(Serialize)]
struct WeatherJson {
    units: Units,
    current: Option<NowJson>,
    days: Vec<DayJson>,
    sun: Option<SunJson>,
    moon: MoonJson,
    source: &'static str,
}

#[derive(Serialize)]
struct NowJson {
    temperature: i64,
    feels_like: i64,
    humidity: i64,
    wind: i64,
    condition: String,
    icon: Option<String>,
}

#[derive(Serialize)]
struct DayJson {
    date: String,
    high: i64,
    low: i64,
    rain: i64,
    condition: String,
    icon: Option<String>,
}

#[derive(Serialize)]
struct SunJson {
    rise: String,
    set: String,
}

#[derive(Serialize)]
struct MoonJson {
    phase: String,
    /// 0–1.
    illumination: f64,
}

#[derive(Serialize)]
struct PuzzleJson {
    number: u32,
    grid: Option<Vec<String>>,
    numbers: Vec<[u32; 3]>,
    clues: Vec<ClueJson>,
    ladder: Option<LadderJson>,
    yesterday: Option<YesterdayJson>,
    /// The word search (word search design §5): optional and additive, in any version.
    #[serde(skip_serializing_if = "Option::is_none")]
    search: Option<SearchJson>,
}

#[derive(Serialize)]
struct SearchJson {
    size: usize,
    grid: Vec<String>,
    words: Vec<HiddenJson>,
}

#[derive(Serialize)]
struct HiddenJson {
    word: String,
    row: u8,
    col: u8,
    dir: &'static str,
    theme: bool,
}

fn search_json(s: &crate::puzzle::search::Search) -> SearchJson {
    use crate::puzzle::search::Way;
    SearchJson {
        size: crate::puzzle::search::SIZE,
        grid: s.rows.clone(),
        words: s
            .words
            .iter()
            .map(|h| HiddenJson {
                word: h.word.to_uppercase(),
                row: h.row,
                col: h.col,
                dir: match h.way {
                    Way::Across => "across",
                    Way::Down => "down",
                    Way::DownRight => "down-right",
                    Way::UpRight => "up-right",
                },
                theme: h.theme,
            })
            .collect(),
    }
}

#[derive(Serialize)]
struct ClueJson {
    number: u8,
    dir: &'static str,
    clue: String,
    answer: String,
    theme: bool,
}

#[derive(Serialize)]
struct LadderJson {
    from: String,
    to: String,
    /// Rungs between the two ends.
    steps: usize,
    answer: Vec<String>,
}

#[derive(Serialize)]
struct YesterdayJson {
    grid: Option<Vec<String>>,
    ladder: Option<Vec<String>>,
    /// Yesterday's word search, or `null`.
    search: Option<SearchJson>,
}

#[derive(Serialize)]
struct FileEntry {
    path: String,
    size: u64,
    sha256: String,
}

/// The files a bundle holds, by their path inside it, before they're written.
#[derive(Default)]
struct Files {
    by_path: BTreeMap<String, Vec<u8>>,
}

impl Files {
    /// Adds `bytes` as `media/<12 hex digits of its SHA-256>.<ext>`, once however often it's
    /// added, and returns that path.
    fn media(&mut self, bytes: &[u8], ext: &str) -> String {
        let hash = hex(&Sha256::digest(bytes));
        let path = format!("media/{}.{ext}", &hash[..12]);
        self.by_path
            .entry(path.clone())
            .or_insert_with(|| bytes.to_vec());
        path
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn kind_name(key: &str) -> &'static str {
    let head = key.split(':').next().unwrap_or(key);
    match head {
        "reading" => "reading",
        "weather" => "weather",
        "local" => "local",
        "feeds" => "feeds",
        "interests" => "interests",
        "shelf" => "shelf",
        "lottery" => "lottery",
        "horoscope" => "horoscope",
        "puzzle" => "puzzle",
        "onthisday" => "onthisday",
        "subscribed" => "subscribed",
        _ => "other",
    }
}

/// The bundle's name for how a story was fetched (1.2).
fn access_name(a: &crate::publisher::Access) -> &'static str {
    use crate::publisher::Access;
    match a {
        Access::Open => "open",
        Access::Subscription { .. } => "subscription",
        Access::Walled { .. } => "walled",
    }
}

/// Words in an XHTML fragment's text: what's outside its tags, split on whitespace.
fn words(xhtml: &str) -> usize {
    plain(xhtml).split_whitespace().count()
}

/// Reading pace for a story's minutes (Books and Kindle design §9).
const WORDS_A_MINUTE: usize = 230;

/// An XHTML fragment's text: what's outside its tags, entities decoded, whitespace collapsed.
fn plain(xhtml: &str) -> String {
    let mut text = String::with_capacity(xhtml.len());
    let mut in_tag = false;
    for c in xhtml.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = html_escape::decode_html_entities(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The story's own paragraphs, one a line: not its captions, notes or source line.
fn paragraphs(xhtml: &str) -> String {
    static PARA: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)<p>(.*?)</p>").unwrap());
    PARA.captures_iter(xhtml)
        .map(|c| plain(&c[1]))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where a photo's subject is: the weighted middle of its detail, skin tones and saturation,
/// on a small copy, pulled a quarter of the way back to the center (smartcrop's idea, without
/// a model). A reader may refine it with its own vision API.
fn focus(img: &image::DynamicImage) -> Focus {
    let small = img.thumbnail(64, 64).to_rgb8();
    let (w, h) = small.dimensions();
    let luma = |x: u32, y: u32| {
        let p = small.get_pixel(x, y).0;
        0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2])
    };
    let (mut sx, mut sy, mut total) = (0.0, 0.0, 0.0);
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let edge = (4.0 * luma(x, y)
                - luma(x - 1, y)
                - luma(x + 1, y)
                - luma(x, y - 1)
                - luma(x, y + 1))
            .abs()
                / 255.0;
            let [r, g, b] = small.get_pixel(x, y).0.map(f64::from);
            let max = r.max(g).max(b);
            let sat = if max > 0.0 {
                (max - r.min(g).min(b)) / max
            } else {
                0.0
            };
            let skin = r > 95.0 && g > 40.0 && b > 20.0 && r > g && r > b && r - g > 15.0;
            let score = edge + 0.4 * sat + if skin { 0.8 } else { 0.0 };
            let wgt = score * score;
            sx += wgt * f64::from(x);
            sy += wgt * f64::from(y);
            total += wgt;
        }
    }
    let round = |v: f64| (v.clamp(0.0, 1.0) * 100.0).round() / 100.0;
    if total <= f64::EPSILON || w < 3 || h < 3 {
        return Focus { x: 0.5, y: 0.5 };
    }
    let cx = (sx / total + 0.5) / f64::from(w);
    let cy = (sy / total + 0.5) / f64::from(h);
    Focus {
        x: round(0.5 + (cx - 0.5) * 0.75),
        y: round(0.5 + (cy - 0.5) * 0.75),
    }
}

/// A photo's average color, `#rrggbb`: the page behind it while it loads.
fn dominant_color(img: &image::DynamicImage) -> String {
    let small = img.thumbnail(16, 16).to_rgb8();
    let n = u64::from(small.width() * small.height()).max(1);
    let mut sum = [0u64; 3];
    for p in small.pixels() {
        for (s, v) in sum.iter_mut().zip(p.0) {
            *s += u64::from(v);
        }
    }
    format!("#{:02x}{:02x}{:02x}", sum[0] / n, sum[1] / n, sum[2] / n)
}

/// `[row, column, number]` for each numbered square of a 5×5 grid, numbered as crosswords are:
/// a square starts a word across or down.
fn numbers(rows: &[String]) -> Vec<[u32; 3]> {
    let cell = |r: usize, c: usize| {
        rows.get(r)
            .and_then(|row| row.as_bytes().get(c))
            .is_some_and(|b| *b != b'#')
    };
    let mut out = Vec::new();
    let mut n = 0;
    for (r, row) in rows.iter().enumerate() {
        for c in 0..row.len() {
            if !cell(r, c) {
                continue;
            }
            let across = (c == 0 || !cell(r, c - 1)) && cell(r, c + 1);
            let down = (r == 0 || !cell(r - 1, c)) && cell(r + 1, c);
            if across || down {
                n += 1;
                out.push([r as u32, c as u32, n]);
            }
        }
    }
    out
}

/// How the front page prints the lead (§6.7): its opening and a jump, all of it, or its teaser
/// when its body has no paragraph to open with.
fn lead_layout(print: &PrintPlan) -> &'static str {
    if print.jump.is_some() {
        "jump"
    } else if print.whole.is_some() {
        "whole"
    } else {
        "teaser"
    }
}

/// The story's pull quote, its position counted over the whole body: the print plan counts the
/// lead's in the continuation, after the opening's paragraphs.
fn pull(print: &PrintPlan, section: usize, article: usize) -> Option<PullJson> {
    let p = print.pulls.get(&(section, article))?;
    let opening = match &print.jump {
        Some(j) if (j.section, j.article) == (section, article) => {
            crate::print::paragraph_ends(&j.opening).len()
        }
        _ => 0,
    };
    Some(PullJson {
        text: p.text.clone(),
        after: p.after + opening,
    })
}

/// Writes the bundle of `gathered` (built as `built`) into `dir`, as `bundle_name(date)`, and
/// returns its path. With `books`, the long reads' EPUBs go in `epub/` too.
pub fn write(
    gathered: &Gathered,
    built: &BuildReport,
    dir: &Path,
    books: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<PathBuf, String> {
    write_inner(gathered, built, dir, books, now, None)
}

/// Writes a fresh 1.4 bundle named by its immutable edition UUID. The destination must
/// not exist; callers own the private build directory and publish separately.
pub fn write_fresh(
    gathered: &Gathered,
    built: &BuildReport,
    dir: &Path,
    books: bool,
    now: chrono::DateTime<chrono::Utc>,
    identity: &crate::press::FreshIdentity,
) -> Result<PathBuf, String> {
    write_inner(gathered, built, dir, books, now, Some(identity))
}

fn write_inner(
    gathered: &Gathered,
    built: &BuildReport,
    dir: &Path,
    books: bool,
    now: chrono::DateTime<chrono::Utc>,
    identity: Option<&crate::press::FreshIdentity>,
) -> Result<PathBuf, String> {
    let e = &gathered.edition;
    let name = identity.map_or_else(
        || bundle_name(e.date),
        |i| format!("{}.deckle", i.edition_id),
    );
    let dest = dir.join(&name);
    if identity.is_some() {
        match std::fs::symlink_metadata(&dest) {
            Ok(_) => return Err("fresh bundle already exists".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    let mut files = Files::default();

    let cover = crate::cover::render(gathered).map_err(|err| format!("cover: {err}"))?;
    files.by_path.insert("cover.jpg".into(), cover);

    // The edition's pictures (the weather icons), by their EPUB href.
    let assets: BTreeMap<&str, String> = e
        .assets
        .iter()
        .map(|(href, png)| (href.as_str(), files.media(png, "png")))
        .collect();
    let asset = |href: &str| assets.get(href).cloned();

    let epub_name = format!(
        "epub/{}",
        built.epub.file_name().unwrap_or_default().to_string_lossy()
    );
    files.by_path.insert(
        epub_name,
        std::fs::read(&built.epub).map_err(|err| format!("{}: {err}", built.epub.display()))?,
    );
    let mut long_reads: BTreeMap<String, String> = BTreeMap::new();
    if books {
        for (lr, p) in gathered.long_reads.iter().zip(&built.long_reads) {
            let path = format!(
                "epub/{}",
                p.file_name().unwrap_or_default().to_string_lossy()
            );
            files.by_path.insert(
                path.clone(),
                std::fs::read(p).map_err(|err| format!("{}: {err}", p.display()))?,
            );
            long_reads.insert(lr.title.clone(), path);
        }
    }

    let id = |s: usize, a: usize| format!("{}/{}/{}", e.date, e.sections[s].key, a + 1);
    let plan = crate::front::front_plan(gathered);
    let print = crate::print::print_plan(gathered, &plan);
    let mut sections = Vec::new();
    for (i, s) in e.sections.iter().enumerate() {
        let mut stories = Vec::new();
        for (j, a) in s.articles.iter().enumerate() {
            let mut body = a.body.clone();
            let mut photos = Vec::new();
            for (n, jpeg) in a.photos.iter().enumerate() {
                let extra = a.extra.photos.get(n);
                // The master in color at full size, when the story has one.
                let bytes = extra.and_then(|x| x.master.as_deref()).unwrap_or(jpeg);
                let path = files.media(bytes, "jpg");
                let img = image::load_from_memory(bytes).ok();
                let (width, height) = img.as_ref().map_or((0, 0), |i| (i.width(), i.height()));
                let token = photo_token(n);
                let lead =
                    body.contains(&format!("<div class=\"figure lead\"><img src=\"{token}\""));
                body = body.replace(&token, &path);
                photos.push(PhotoJson {
                    file: path,
                    width,
                    height,
                    lead,
                    motion: None,
                    caption: extra.and_then(|x| x.caption.clone()),
                    credit: extra.and_then(|x| x.credit.clone()),
                    alt: extra.and_then(|x| x.alt.clone()),
                    focus: img.as_ref().map(focus),
                    color: img.as_ref().map(dominant_color),
                });
            }
            for (href, path) in &assets {
                body = body.replace(&format!("src=\"{href}\""), &format!("src=\"{path}\""));
            }
            let n_words = words(&body);
            let text = plain(&body);
            let prominence = if plan.lead == Some((i, j)) {
                "lead"
            } else if plan.tiles.contains(&(i, j)) {
                "front"
            } else {
                "inside"
            };
            stories.push(StoryJson {
                id: id(i, j),
                title: a.title.clone(),
                meta: a.meta.clone(),
                teaser: a.teaser.clone(),
                url: a.url.clone(),
                outlet: a.outlet.clone(),
                byline: a.extra.byline.clone(),
                published: a.extra.published.clone(),
                minutes: n_words.div_ceil(WORDS_A_MINUTE).max(1),
                prominence,
                partial: crate::article::looks_cut_off(&text),
                pull_quote: crate::article::pull_quote(&paragraphs(&body), n_words),
                access: access_name(&a.extra.access),
                pull: pull(&print, i, j),
                words: n_words,
                body: Body {
                    format: "xhtml",
                    text: body,
                },
                photos,
                long_read: long_reads.get(&a.title).cloned(),
            });
        }
        let weather = s.weather.as_ref().map(|w| WeatherJson {
            units: w.units,
            current: w.current.as_ref().map(|c| NowJson {
                temperature: c.temperature,
                feels_like: c.feels_like,
                humidity: c.humidity,
                wind: c.wind,
                condition: c.condition.clone(),
                icon: asset(&c.icon),
            }),
            days: w
                .days
                .iter()
                .map(|d| DayJson {
                    date: d.date.clone(),
                    high: d.high,
                    low: d.low,
                    rain: d.rain,
                    condition: d.condition.clone(),
                    icon: asset(&d.icon),
                })
                .collect(),
            sun: match (&w.sunrise, &w.sunset) {
                (Some(rise), Some(set)) => Some(SunJson {
                    rise: rise.clone(),
                    set: set.clone(),
                }),
                _ => None,
            },
            moon: MoonJson {
                phase: w.moon.phase.clone(),
                illumination: f64::from(w.moon.illumination) / 100.0,
            },
            source: "Open-Meteo",
        });
        let puzzle = s.puzzle.as_ref().map(|p| {
            let t = &p.today;
            let grid = t.rows.as_ref().map(|r| r.to_vec());
            PuzzleJson {
                number: t.number,
                numbers: grid.as_deref().map(numbers).unwrap_or_default(),
                grid,
                clues: t
                    .clues
                    .iter()
                    .map(|c| ClueJson {
                        number: c.number,
                        dir: match c.dir {
                            crate::puzzle::grid::Dir::Across => "across",
                            crate::puzzle::grid::Dir::Down => "down",
                        },
                        clue: c.clue.clone(),
                        answer: c.answer.to_uppercase(),
                        theme: c.theme,
                    })
                    .collect(),
                ladder: t.ladder.as_ref().map(|l| LadderJson {
                    from: l.from.to_uppercase(),
                    to: l.to.to_uppercase(),
                    steps: l.path.len().saturating_sub(2),
                    answer: l.path.iter().map(|w| w.to_uppercase()).collect(),
                }),
                yesterday: p.yesterday.as_ref().map(|y| YesterdayJson {
                    grid: y.rows.as_ref().map(|r| r.to_vec()),
                    ladder: y
                        .ladder
                        .as_ref()
                        .map(|l| l.path.iter().map(|w| w.to_uppercase()).collect()),
                    search: y.search.as_ref().map(search_json),
                }),
                search: t.search.as_ref().map(search_json),
            }
        });
        sections.push(SectionJson {
            book: print.book.get(i).copied().flatten().map(String::from),
            key: s.key.clone(),
            kind: kind_name(&s.key),
            title: s.title.clone(),
            kicker: s.kicker.clone(),
            ai_brief: s.brief.clone(),
            unavailable: s.unavailable.clone(),
            stories,
            weather,
            puzzle,
        });
    }

    let entries: Vec<FileEntry> = files
        .by_path
        .iter()
        .map(|(path, bytes)| FileEntry {
            path: path.clone(),
            size: bytes.len() as u64,
            sha256: hex(&Sha256::digest(bytes)),
        })
        .collect();
    let json = EditionJson {
        format: FORMAT,
        version: if identity.is_some() { "1.4" } else { VERSION },
        generator: format!("Deckle {}", env!("CARGO_PKG_VERSION")),
        edition: EditionInfo {
            id: identity.map(|i| i.edition_id),
            paper_id: identity.map(|i| i.paper_id),
            kind: identity.map(|_| "fresh"),
            paper: e.paper.clone(),
            date: e.date.to_string(),
            number: e.number(),
            place: e.place.clone(),
            timezone: crate::tz::local_tz().name().to_owned(),
            built_at: now.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            weather_line: e.weather_line.clone(),
            cover: "cover.jpg",
        },
        front: Front {
            lead: plan.lead.map(|(s, a)| id(s, a)),
            tiles: plan.tiles.iter().map(|&(s, a)| id(s, a)).collect(),
            skybox: print
                .skybox
                .iter()
                .map(|t| id(t.section, t.article))
                .collect(),
            lead_layout: plan.lead.map(|_| lead_layout(&print)),
            jump: print.jump.as_ref().map(|j| JumpJson {
                story: id(j.section, j.article),
                slug: j.slug.clone(),
                opening: crate::print::paragraph_ends(&j.opening).len(),
            }),
        },
        sections,
        files: entries,
    };
    let json = serde_json::to_vec_pretty(&json).map_err(|err| err.to_string())?;

    // Into a hidden folder, `edition.json` last, then renamed into place.
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let tmp = dir.join(format!(".{name}.tmp"));
    if identity.is_some() {
        // Exclusive creation refuses stale staging and symlinks; a fresh attempt owns it.
        std::fs::create_dir(&tmp).map_err(|err| format!("{}: {err}", tmp.display()))?;
    } else {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    let io = |p: &Path, err: std::io::Error| format!("{}: {err}", p.display());
    for (path, bytes) in &files.by_path {
        let to = tmp.join(path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|err| io(parent, err))?;
        }
        std::fs::write(&to, bytes).map_err(|err| io(&to, err))?;
    }
    let edition = tmp.join("edition.json");
    std::fs::write(&edition, json).map_err(|err| io(&edition, err))?;
    if identity.is_none() && dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|err| io(&dest, err))?;
    }
    std::fs::rename(&tmp, &dest).map_err(|err| io(&dest, err))?;
    Ok(dest)
}

/// Whether the bundle at `dir` is whole: `edition.json` parses and every file it lists is
/// there at its size (§5). What a reader checks, and what a prune trusts.
pub fn is_complete(dir: &Path) -> bool {
    let Ok(bytes) = std::fs::read(dir.join("edition.json")) else {
        return false;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    let Some(files) = v.get("files").and_then(|f| f.as_array()) else {
        return false;
    };
    files.iter().all(|f| {
        let (Some(path), Some(size)) = (
            f.get("path").and_then(|p| p.as_str()),
            f.get("size").and_then(|s| s.as_u64()),
        ) else {
            return false;
        };
        !path.contains("..")
            && std::fs::metadata(dir.join(path)).is_ok_and(|m| m.is_file() && m.len() == size)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_numbers_follow_crossword_rules() {
        let rows: Vec<String> = ["#BAN#", "ERROR", "EERIE", "LEAST", "#DYE#"]
            .iter()
            .map(|r| r.to_string())
            .collect();
        assert_eq!(
            numbers(&rows),
            [
                [0, 1, 1],
                [0, 2, 2],
                [0, 3, 3],
                [1, 0, 4],
                [1, 4, 5],
                [2, 0, 6],
                [3, 0, 7],
                [4, 1, 8]
            ]
        );
    }

    fn picture(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> image::DynamicImage {
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| image::Rgb(f(x, y))))
    }

    #[test]
    fn the_focus_follows_the_detail_and_a_flat_photo_centers() {
        let flat = picture(400, 300, |_, _| [90, 90, 90]);
        assert_eq!(focus(&flat), Focus { x: 0.5, y: 0.5 });
        // A busy, colorful patch in the top right of a gray field.
        let busy = picture(400, 300, |x, y| {
            if x > 280 && y < 90 {
                if (x / 4 + y / 4) % 2 == 0 {
                    [230, 40, 40]
                } else {
                    [20, 20, 200]
                }
            } else {
                [120, 120, 120]
            }
        });
        let f = focus(&busy);
        assert!(f.x > 0.65 && f.y < 0.35, "{f:?}");
        assert_eq!(dominant_color(&flat), "#5a5a5a");
    }

    #[test]
    fn a_pull_quote_comes_from_paragraphs_only() {
        let body = "<p>One.</p><div class=\"figure\"><span class=\"caption\">The yard (Photo: Jane)</span></div><p class=\"source\">Source: x</p><p>Two &amp; three.</p>";
        assert_eq!(paragraphs(body), "One.\nTwo & three.");
    }

    #[test]
    fn plain_text_decodes_and_collapses() {
        assert_eq!(
            plain("<p>Fish &amp; chips</p>\n<p>  at   six</p>"),
            "Fish & chips at six"
        );
        assert_eq!(words("<p>Fish &amp; chips</p>"), 3);
    }

    #[test]
    fn media_is_stored_once_by_content() {
        let mut f = Files::default();
        let a = f.media(b"photo", "jpg");
        let b = f.media(b"photo", "jpg");
        let c = f.media(b"other", "jpg");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(f.by_path.len(), 2);
        assert!(a.starts_with("media/") && a.len() == "media/".len() + 12 + 4);
    }

    #[test]
    fn the_leads_pull_counts_its_opening_and_others_count_as_they_are() {
        use crate::print::{Jump, Pull};
        let mut print = PrintPlan::default();
        print.pulls.insert(
            (0, 0),
            Pull {
                text: "“Lead”".into(),
                after: 3,
            },
        );
        print.pulls.insert(
            (1, 2),
            Pull {
                text: "“Other”".into(),
                after: 4,
            },
        );
        assert_eq!(lead_layout(&print), "teaser");
        print.whole = Some((0, 0));
        assert_eq!(lead_layout(&print), "whole");
        print.whole = None;
        print.jump = Some(Jump {
            section: 0,
            article: 0,
            slug: "MILL".into(),
            // a figure among the opening's paragraphs isn't one of them
            opening: "<p>one</p>\n<div class=\"figure\"><img src=\"x\"/></div>\n<p>two</p>".into(),
            rest: String::new(),
        });
        assert_eq!(lead_layout(&print), "jump");
        assert_eq!(
            pull(&print, 0, 0),
            Some(PullJson {
                text: "“Lead”".into(),
                after: 5
            })
        );
        assert_eq!(
            pull(&print, 1, 2),
            Some(PullJson {
                text: "“Other”".into(),
                after: 4
            })
        );
        assert_eq!(pull(&print, 1, 0), None);
    }

    #[test]
    fn section_kinds() {
        assert_eq!(kind_name("feeds:Hacker News"), "feeds");
        assert_eq!(kind_name("local:Asheville, NC"), "local");
        assert_eq!(kind_name("weather"), "weather");
        assert_eq!(kind_name("subscribed:gazette.example:news"), "subscribed");
    }

    #[test]
    fn access_names_the_three_ways_a_story_was_fetched() {
        use crate::publisher::{Access, Walled};
        assert_eq!(access_name(&Access::Open), "open");
        let sub = Access::Subscription {
            site: "gazette.example".into(),
            service: "BLOX".into(),
        };
        assert_eq!(access_name(&sub), "subscription");
        let walled = Access::Walled {
            site: "gazette.example".into(),
            why: Walled::NotInPlan,
        };
        assert_eq!(access_name(&walled), "walled");
    }
}
