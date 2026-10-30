// SPDX-License-Identifier: GPL-3.0-or-later
//! One Mac, many papers (many papers design §3): `digest.toml` is the main paper and each
//! `papers/<slug>.toml` beside it is another. A paper file can't set the household settings
//! (`[sync]`, `[reading]`, `[[subscriptions]]`, the email account); it takes them from `digest.toml`.

use std::path::{Path, PathBuf};

use crate::config::{Config, PAPER_STARTER, PaperRef};

/// The main paper's slug: `digest.toml`.
pub const MAIN: &str = "main";

/// Keys of `[email]` that belong to the household's account, not to one paper.
const ACCOUNT_KEYS: [&str; 4] = ["from", "smtp_host", "smtp_port", "username"];

/// `^[a-z][a-z0-9-]{0,31}$`, and not `main`.
pub fn valid_slug(s: &str) -> bool {
    let mut chars = s.chars();
    s != MAIN
        && s.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A slug from a paper's name: "The Kid Courier" → `kid-courier`. A leading "the" goes, and the
/// result is cut to 32 characters; it may still need checking with [`valid_slug`].
pub fn slug_from_title(title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect();
    let words = match words.split_first() {
        Some((first, rest)) if first == "the" && !rest.is_empty() => rest.to_vec(),
        _ => words,
    };
    let mut slug = words.join("-");
    slug = slug
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
        .to_owned();
    slug.truncate(32);
    slug.trim_end_matches('-').to_owned()
}

/// `papers/` beside `digest.toml`.
pub fn papers_dir(main: &Path) -> PathBuf {
    main.with_file_name("papers")
}

/// `papers/<slug>.toml`.
pub fn paper_path(main: &Path, slug: &str) -> PathBuf {
    papers_dir(main).join(format!("{slug}.toml"))
}

/// Every paper, main first, then by slug, with notes on what was skipped.
#[derive(Debug, Default)]
pub struct Loaded {
    pub papers: Vec<Config>,
    pub notes: Vec<String>,
}

/// The household settings a paper file may not set, as `[sync]` or `[email] smtp_host`.
fn household_key(text: &str) -> Result<Option<String>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    for key in ["sync", "reading", "reader_server"] {
        if table.contains_key(key) {
            return Ok(Some(format!("[{key}]")));
        }
    }
    if table.contains_key("subscriptions") {
        return Ok(Some("[[subscriptions]]".to_owned()));
    }
    if let Some(email) = table.get("email").and_then(|e| e.as_table()) {
        for key in ACCOUNT_KEYS {
            if email.contains_key(key) {
                return Ok(Some(format!("[email] {key}")));
            }
        }
    }
    Ok(None)
}

/// Loads `papers/<slug>.toml`, refusing household settings and taking them from `main`.
fn load_extra(main_path: &Path, main: &Config, slug: &str) -> Result<Config, String> {
    let path = paper_path(main_path, slug);
    let name = format!("papers/{slug}.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
    if let Some(key) = household_key(&text).map_err(|e| format!("{name}: {e}"))? {
        return Err(format!(
            "{name}: {key} is set in digest.toml for every paper"
        ));
    }
    let mut cfg = Config::parse(&text).map_err(|e| format!("{name}: {e}"))?;
    cfg.sync = main.sync.clone();
    cfg.reading = main.reading.clone();
    cfg.subscriptions = main.subscriptions.clone();
    cfg.reader_server = main.reader_server.clone();
    cfg.email.from = main.email.from.clone();
    cfg.email.smtp_host = main.email.smtp_host.clone();
    cfg.email.smtp_port = main.email.smtp_port;
    cfg.email.username = main.email.username.clone();
    let state_dir = papers_dir(main_path).join(slug);
    cfg.puzzle_path = Some(state_dir.join("puzzles.json"));
    cfg.paper = Some(PaperRef {
        slug: slug.to_owned(),
        state_dir,
    });
    Ok(cfg)
}

/// The slugs in `papers/`, sorted, and notes for `.toml` files that aren't paper names.
fn extra_slugs(main: &Path) -> (Vec<String>, Vec<String>) {
    let mut slugs = Vec::new();
    let mut notes = Vec::new();
    let Ok(entries) = std::fs::read_dir(papers_dir(main)) else {
        return (slugs, notes);
    };
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_file() || path.extension().is_none_or(|x| x != "toml") {
            continue;
        }
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if valid_slug(&stem) {
            slugs.push(stem);
        } else {
            notes.push(format!(
                "papers/{stem}.toml: not a paper name (lowercase letters, digits, dashes)"
            ));
        }
    }
    slugs.sort();
    notes.sort();
    (slugs, notes)
}

/// The main paper at `main`, then each extra in slug order. A main paper that fails is an
/// error; an extra that fails is skipped with a note saying why.
pub fn load_all(main: &Path) -> Result<Loaded, String> {
    let first = Config::load(main)?;
    let (slugs, mut notes) = extra_slugs(main);
    let mut papers = Vec::new();
    for slug in slugs {
        match load_extra(main, &first, &slug) {
            Ok(c) => papers.push(c),
            Err(e) => notes.push(e),
        }
    }
    papers.insert(0, first);
    Ok(Loaded { papers, notes })
}

/// One paper by slug (`main` is `digest.toml`).
pub fn load_one(main: &Path, slug: &str) -> Result<Config, String> {
    let first = Config::load(main)?;
    if slug == MAIN {
        return Ok(first);
    }
    if !valid_slug(slug) {
        return Err(format!(
            "{slug}: not a paper name (lowercase letters, digits, dashes)"
        ));
    }
    load_extra(main, &first, slug)
}

/// Writes a new `papers/<slug>.toml` from [`PAPER_STARTER`], with the main paper's place and
/// location. Refuses a bad slug and an existing file.
pub fn create(main: &Path, slug: &str, title: &str, reader: &str) -> Result<PathBuf, String> {
    if !valid_slug(slug) {
        return Err(format!(
            "{slug}: not a paper name (lowercase letters, digits, dashes)"
        ));
    }
    let path = paper_path(main, slug);
    if path.exists() {
        return Err(format!("papers/{slug}.toml already exists"));
    }
    let base = Config::load(main).unwrap_or_default();
    let quote = |s: &str| toml::Value::String(s.to_owned()).to_string();
    let location = match (base.latitude, base.longitude) {
        (Some(lat), Some(lon)) => format!("latitude = {lat}\nlongitude = {lon}\n"),
        _ => String::new(),
    };
    let text = PAPER_STARTER
        .replace("{title}", &quote(title))
        .replace("{reader}", &quote(reader))
        .replace("{place}", &quote(&base.place))
        .replace("{location}", &location);
    Config::parse(&text).map_err(|e| format!("papers/{slug}.toml: {e}"))?;
    std::fs::create_dir_all(papers_dir(main)).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// The file `cfg` was loaded from: `main` for the main paper, `papers/<slug>.toml` for another.
pub fn file_of(main: &Path, cfg: &Config) -> PathBuf {
    match &cfg.paper {
        None => main.to_path_buf(),
        Some(p) => paper_path(main, &p.slug),
    }
}

/// Saves `cfg` to its own file, comments kept. Another paper's file never gets the household
/// settings it inherits.
pub fn save(main: &Path, cfg: &Config) -> Result<(), String> {
    crate::config::save(&file_of(main, cfg), cfg)
}
