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
use crate::sources::feeds::photo_token;
use crate::{BuildReport, EDITION_PREFIX, Gathered};

/// The format's name and the version this writer writes (§6.1, §10).
pub const FORMAT: &str = "deckle-edition";
pub const VERSION: &str = "1.0";

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
}

#[derive(Serialize)]
struct SectionJson {
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
    /// Reserved for version 1.1 (§7).
    motion: Option<()>,
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
        "shelf" => "shelf",
        "lottery" => "lottery",
        "horoscope" => "horoscope",
        "puzzle" => "puzzle",
        "onthisday" => "onthisday",
        _ => "other",
    }
}

/// Words in an XHTML fragment's text: what's outside its tags, split on whitespace.
fn words(xhtml: &str) -> usize {
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
    text.split_whitespace().count()
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

/// Writes the bundle of `gathered` (built as `built`) into `dir`, as `bundle_name(date)`, and
/// returns its path. With `books`, the long reads' EPUBs go in `epub/` too.
pub fn write(
    gathered: &Gathered,
    built: &BuildReport,
    dir: &Path,
    books: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<PathBuf, String> {
    let e = &gathered.edition;
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
    let mut sections = Vec::new();
    for (i, s) in e.sections.iter().enumerate() {
        let mut stories = Vec::new();
        for (j, a) in s.articles.iter().enumerate() {
            let mut body = a.body.clone();
            let mut photos = Vec::new();
            for (n, jpeg) in a.photos.iter().enumerate() {
                let path = files.media(jpeg, "jpg");
                let (width, height) = image::load_from_memory(jpeg)
                    .map(|img| (img.width(), img.height()))
                    .unwrap_or((0, 0));
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
                });
            }
            for (href, path) in &assets {
                body = body.replace(&format!("src=\"{href}\""), &format!("src=\"{path}\""));
            }
            stories.push(StoryJson {
                id: id(i, j),
                title: a.title.clone(),
                meta: a.meta.clone(),
                teaser: a.teaser.clone(),
                url: a.url.clone(),
                outlet: a.outlet.clone(),
                words: words(&body),
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
                }),
            }
        });
        sections.push(SectionJson {
            key: s.key.clone(),
            kind: kind_name(&s.key),
            title: s.title.clone(),
            kicker: s.kicker.clone(),
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
        version: VERSION,
        generator: format!("Deckle {}", env!("CARGO_PKG_VERSION")),
        edition: EditionInfo {
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
        },
        sections,
        files: entries,
    };
    let json = serde_json::to_vec_pretty(&json).map_err(|err| err.to_string())?;

    // Into a hidden folder, `edition.json` last, then renamed into place.
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let name = bundle_name(e.date);
    let tmp = dir.join(format!(".{name}.tmp"));
    let _ = std::fs::remove_dir_all(&tmp);
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
    let dest = dir.join(&name);
    if dest.exists() {
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
    fn section_kinds() {
        assert_eq!(kind_name("feeds:Hacker News"), "feeds");
        assert_eq!(kind_name("local:Asheville, NC"), "local");
        assert_eq!(kind_name("weather"), "weather");
    }
}
