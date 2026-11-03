// SPDX-License-Identifier: GPL-3.0-or-later
//! Returns (edition bundle design §12, `docs/formats/deckle-returns.schema.json`): what a
//! reader wrote back about an edition, one file per reader under
//! `.deckle-returns/<reader-id>/<edition date>.json`. Versions 1.0 to 1.2 are read; 1.2
//! (puzzle book bundle design §4.1) adds the daily word search's found words. A file this
//! Deckle can't read is skipped, never an error for the morning.

use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const FORMAT: &str = "deckle-returns";
/// The versions this reader accepts: a minor only adds.
pub const VERSIONS: [&str; 3] = ["1.0", "1.1", "1.2"];
/// The folder under an export where readers write.
pub const DIR: &str = ".deckle-returns";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Returns {
    pub format: String,
    pub version: String,
    pub reader: Reader,
    pub edition: NaiveDate,
    /// 1.1: the fresh edition's identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
    pub read: Vec<Read>,
    pub puzzle: Option<PuzzleReturn>,
    pub marks: Vec<Mark>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reader {
    pub id: String,
    pub name: String,
    pub app: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Read {
    pub story: String,
    pub fraction: f64,
    pub seconds: f64,
    pub last: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuzzleReturn {
    /// The mini's letters, `#` blacks, a space for an empty square.
    pub grid: Vec<String>,
    pub done: bool,
    pub ladder: Vec<String>,
    /// 1.2: the word search (puzzle book bundle design §4.1); absent from older readers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchReturn>,
}

/// The daily word search as a reader left it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchReturn {
    /// Upper-case words found.
    pub found: Vec<String>,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub story: String,
    pub quote: String,
    pub note: String,
    pub at: DateTime<Utc>,
}

/// A returns file's contents, when its format is ours and its version one this reads.
pub fn parse(bytes: &[u8]) -> Result<Returns, String> {
    let r: Returns = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if r.format != FORMAT {
        return Err(format!("format {:?} isn't {FORMAT}", r.format));
    }
    if !VERSIONS.contains(&r.version.as_str()) {
        return Err(format!(
            "version {} isn't one of {}",
            r.version,
            VERSIONS.join(", ")
        ));
    }
    Ok(r)
}

/// Every reader's returns for `date` under `root` (the export folder), by reader id. A file
/// that doesn't parse is noted on stderr and skipped.
pub fn read_dir(root: &Path, date: NaiveDate) -> Vec<Returns> {
    let dir = root.join(DIR);
    let Ok(readers) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut readers: Vec<_> = readers.flatten().map(|e| e.path()).collect();
    readers.sort();
    let mut out = Vec::new();
    for reader in readers.into_iter().filter(|p| p.is_dir()) {
        let file = reader.join(format!("{date}.json"));
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        match parse(&bytes) {
            Ok(r) => out.push(r),
            Err(e) => eprintln!("returns: skipping {}: {e}", file.display()),
        }
    }
    out
}

/// The word search's state across readers: the latest `updated_at` with one wins, as edition
/// returns are merged by the latest reader (§12).
pub fn search_of(returns: &[Returns]) -> Option<SearchReturn> {
    returns
        .iter()
        .filter_map(|r| {
            r.puzzle
                .as_ref()?
                .search
                .as_ref()
                .map(|s| (r.updated_at, s))
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, s)| s.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRESH: &str = include_str!("../../../fixtures/press/returns-fresh.json");
    const LEGACY: &str = include_str!("../../../fixtures/press/returns-legacy.json");
    const SEARCH: &str = include_str!("../../../fixtures/press/returns-1.2.json");

    #[test]
    fn every_version_parses_and_only_1_2_has_the_search() {
        let legacy = parse(LEGACY.as_bytes()).unwrap();
        assert_eq!(legacy.version, "1.0");
        assert!(legacy.edition_id.is_none());
        let fresh = parse(FRESH.as_bytes()).unwrap();
        assert!(fresh.edition_id.is_some() && fresh.paper_id.is_some());
        assert_eq!(fresh.puzzle.as_ref().unwrap().search, None);
        let s = parse(SEARCH.as_bytes()).unwrap();
        assert_eq!(s.version, "1.2");
        let search = s.puzzle.as_ref().unwrap().search.clone().unwrap();
        assert_eq!(search.found, ["COURT", "MILL"]);
        assert!(!search.done);
        // what a reader wrote survives a round trip, search and all
        let again: Returns = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(again, s);
    }

    #[test]
    fn the_wrong_format_or_a_newer_version_is_refused() {
        let v: serde_json::Value = serde_json::from_str(SEARCH).unwrap();
        let mut other = v.clone();
        other["format"] = "deckle-edition".into();
        assert!(
            parse(other.to_string().as_bytes())
                .unwrap_err()
                .contains("format")
        );
        let mut newer = v;
        newer["version"] = "2.0".into();
        assert!(
            parse(newer.to_string().as_bytes())
                .unwrap_err()
                .contains("version 2.0")
        );
        assert!(parse(b"nonsense").is_err());
    }

    #[test]
    fn a_folder_of_readers_merges_by_the_latest_and_skips_what_it_cant_read() {
        let tmp = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let write = |reader: &str, text: &str| {
            let dir = tmp.path().join(DIR).join(reader);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("2026-10-27.json"), text).unwrap();
        };
        assert!(read_dir(tmp.path(), date).is_empty());
        write("a", SEARCH);
        write("b", FRESH);
        write("c", "{ not json");
        let mut later: serde_json::Value = serde_json::from_str(SEARCH).unwrap();
        later["updated_at"] = "2026-10-27T20:00:00Z".into();
        later["puzzle"]["search"]["found"] = serde_json::json!(["COURT", "MILL", "RIVER"]);
        later["puzzle"]["search"]["done"] = true.into();
        write("d", &later.to_string());
        let all = read_dir(tmp.path(), date);
        assert_eq!(all.len(), 3, "the malformed one is skipped");
        let s = search_of(&all).unwrap();
        assert_eq!(s.found, ["COURT", "MILL", "RIVER"]);
        assert!(s.done);
        assert!(
            read_dir(tmp.path(), date + chrono::Days::new(1)).is_empty(),
            "another date has no file"
        );
    }
}
