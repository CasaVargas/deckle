// SPDX-License-Identifier: GPL-3.0-or-later
//! The KOReader plugin store (docs/superpowers/specs/2026-10-31-koreader-plugins-design.md
//! part C). **Recommended:** a short list in Deckle's repository, each pinned to a release asset
//! and its SHA-256, so an install is what was reviewed. **Discover:** GitHub's `koreader-plugin`
//! topic, searched live and labeled unreviewed. Installs go through part A's checks; the store
//! only chooses the source. It goes online only when asked, and caches GitHub's answers for an
//! hour.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::koreader_plugins::{self as kp, Fetch, Prepared, Source};

/// Where the live list is: the main branch of Deckle's repository.
pub const RECOMMENDED_URL: &str = "https://raw.githubusercontent.com/CasaVargas/deckle/main/crates/deckle-core/koplugin/recommended.json";
/// The list as this Deckle was built, for when the live one can't be had.
const BUILT_IN: &str = include_str!("../koplugin/recommended.json");
/// How long a GitHub answer is reused.
pub const CACHE_FOR: Duration = Duration::from_secs(3600);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommended {
    pub version: u32,
    pub updated: String,
    pub plugins: Vec<Pick>,
}

/// One plugin Deckle vouches for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    /// `owner/repo` on GitHub.
    pub repo: String,
    /// Its name in KOReader (the folder less `.koplugin`).
    pub name: String,
    pub kind: String,
    /// One line: why it's on the list.
    pub why: String,
    pub license: String,
    pub release: Pin,
}

/// The reviewed release: installed only when its asset has this SHA-256.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub tag: String,
    pub asset: String,
    pub sha256: String,
}

impl Pick {
    pub fn asset_url(&self) -> String {
        format!(
            "https://github.com/{}/releases/download/{}/{}",
            self.repo, self.release.tag, self.release.asset
        )
    }
}

/// The list as built into this Deckle.
pub fn built_in() -> Recommended {
    serde_json::from_str(BUILT_IN).expect("recommended.json is checked in tests")
}

/// The live list, else the built-in one (and whether it's live). A live list that doesn't parse
/// is ignored.
pub fn recommended(fetch: &mut Fetch<'_>) -> (Recommended, bool) {
    match fetch(RECOMMENDED_URL)
        .ok()
        .and_then(|b| serde_json::from_slice::<Recommended>(&b).ok())
    {
        Some(r) if r.version == 1 => (r, true),
        _ => (built_in(), false),
    }
}

/// A pinned plugin, downloaded and checked against its SHA-256, then through part A's checks.
pub fn prepare_pick(data: &Path, pick: &Pick, fetch: &mut Fetch<'_>) -> Result<Prepared, String> {
    let bytes = fetch(&pick.asset_url())?;
    let got: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !got.eq_ignore_ascii_case(&pick.release.sha256) {
        return Err(format!(
            "{} {} isn't the file Deckle reviewed (its SHA-256 differs); nothing was installed",
            pick.repo, pick.release.tag
        ));
    }
    let p = kp::prepare_zip(
        data,
        &bytes,
        Some(&pick.name),
        Source {
            from: pick.repo.clone(),
            release: Some(pick.release.tag.clone()),
            installed: crate::devices::now_iso(),
        },
    )?;
    if p.name != pick.name {
        return Err(format!("{} holds {}, not {}", pick.repo, p.name, pick.name));
    }
    Ok(p)
}

/// The pick `wanted` names, by plugin name, `owner/repo` or a GitHub URL.
pub fn find_pick<'a>(list: &'a Recommended, wanted: &str) -> Option<&'a Pick> {
    let w = wanted.trim().trim_end_matches('/').to_ascii_lowercase();
    list.plugins.iter().find(|p| {
        let repo = p.repo.to_ascii_lowercase();
        p.name == w || repo == w || w.ends_with(&format!("github.com/{repo}"))
    })
}

/// Any install, reviewed when it can be: a plugin on the list gets its pinned release, checked
/// by hash; anything else (a repository, a .zip, a folder) goes through part A as it is.
/// Returns whether it's the reviewed release.
pub fn prepare_any(
    data: &Path,
    wanted: &str,
    list: &Recommended,
    fetch: &mut Fetch<'_>,
) -> Result<(Prepared, bool), String> {
    if let Some(pick) = find_pick(list, wanted) {
        return Ok((prepare_pick(data, pick, fetch)?, true));
    }
    let from = kp::From::parse(wanted)?;
    Ok((kp::prepare(data, &from, fetch)?, false))
}

/// A repository found by Discover.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Found {
    pub repo: String,
    pub description: Option<String>,
    pub stars: u64,
    /// Last push, `YYYY-MM-DD`.
    pub pushed: String,
    /// SPDX id, or `None` for no license.
    pub license: Option<String>,
    pub url: String,
}

/// How Discover sorts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Stars,
    Updated,
}

/// GitHub repositories with the `koreader-plugin` topic matching `text` (all of them when
/// empty), at most 30.
pub fn search(text: &str, sort: Sort, fetch: &mut Fetch<'_>) -> Result<Vec<Found>, String> {
    let q = format!("{} topic:koreader-plugin", text.trim());
    let url = format!(
        "https://api.github.com/search/repositories?q={}&sort={}&order=desc&per_page=30",
        encode(q.trim()),
        match sort {
            Sort::Stars => "stars",
            Sort::Updated => "updated",
        }
    );
    let body = fetch(&url)?;
    let v: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("GitHub's answer: {e}"))?;
    let items = v["items"]
        .as_array()
        .ok_or_else(|| v["message"].as_str().unwrap_or("GitHub said no").to_owned())?;
    Ok(items
        .iter()
        .filter_map(|i| {
            Some(Found {
                repo: i["full_name"].as_str()?.to_owned(),
                description: i["description"].as_str().map(str::to_owned),
                stars: i["stargazers_count"].as_u64().unwrap_or(0),
                pushed: i["pushed_at"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(10)
                    .collect(),
                license: i["license"]["spdx_id"]
                    .as_str()
                    .filter(|l| *l != "NOASSERTION")
                    .map(str::to_owned),
                url: i["html_url"].as_str().unwrap_or("").to_owned(),
            })
        })
        .collect())
}

/// A repository's README, as text.
pub fn readme(repo: &str, fetch: &mut Fetch<'_>) -> Result<String, String> {
    let body = fetch(&format!("https://api.github.com/repos/{repo}/readme"))?;
    let v: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("GitHub's answer: {e}"))?;
    let b64: String = v["content"]
        .as_str()
        .ok_or("no README")?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// A repository's newest release tag, if it has releases.
pub fn latest_release(repo: &str, fetch: &mut Fetch<'_>) -> Option<String> {
    let body = fetch(&format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    ))
    .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
    v["tag_name"].as_str().map(str::to_owned)
}

/// A newer release of an installed plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Update {
    pub name: String,
    pub repo: String,
    /// The release installed, when Deckle recorded one.
    pub installed: Option<String>,
    pub latest: String,
    /// On the Recommended list: the update is the newly pinned release.
    pub pinned: bool,
}

/// Each installed plugin with a recorded GitHub source that has a newer release: for a
/// Recommended one, its pin; for any other, its newest release.
pub fn updates(listing: &kp::Listing, list: &Recommended, fetch: &mut Fetch<'_>) -> Vec<Update> {
    let mut out = Vec::new();
    for p in &listing.plugins {
        let Some(src) = &p.source else { continue };
        let Ok(kp::From::GitHub(repo)) = kp::From::parse(&src.from) else {
            continue;
        };
        let pick = list
            .plugins
            .iter()
            .find(|x| x.repo.eq_ignore_ascii_case(&repo));
        let (latest, pinned) = match pick {
            Some(x) => (Some(x.release.tag.clone()), true),
            None => (latest_release(&repo, fetch), false),
        };
        if let Some(latest) = latest
            && src.release.as_deref() != Some(latest.as_str())
        {
            out.push(Update {
                name: p.name.clone(),
                repo,
                installed: src.release.clone(),
                latest,
                pinned,
            });
        }
    }
    out
}

/// `fetch`, with GitHub API answers kept in `dir` for [`CACHE_FOR`]. Downloads of release
/// assets aren't cached: they're checked by hash, and fetched once.
pub fn cached<'a>(
    dir: PathBuf,
    mut fetch: impl FnMut(&str) -> Result<Vec<u8>, String> + 'a,
) -> impl FnMut(&str) -> Result<Vec<u8>, String> + 'a {
    move |url: &str| {
        let api = url.starts_with("https://api.github.com/") || url == RECOMMENDED_URL;
        if !api {
            return fetch(url);
        }
        let key: String = Sha256::digest(url.as_bytes())
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect();
        let path = dir.join(format!("{key}.json"));
        let fresh = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < CACHE_FOR);
        if fresh && let Ok(b) = std::fs::read(&path) {
            return Ok(b);
        }
        let body = fetch(url)?;
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(&path, &body);
        }
        Ok(body)
    }
}

/// Where the store keeps GitHub's answers.
pub fn cache_dir() -> PathBuf {
    crate::wordwise::default_cache_dir().join("koreader-store")
}

/// Percent-encodes a query value.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_list_parses_and_every_pin_is_complete() {
        let r = built_in();
        assert_eq!(r.version, 1);
        assert!(!r.plugins.is_empty());
        for p in &r.plugins {
            assert_eq!(p.release.sha256.len(), 64, "{}", p.repo);
            assert!(p.release.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(p.repo.contains('/') && !p.why.is_empty() && !p.license.is_empty());
            assert!(!kp::is_builtin(&p.name), "{} is a built-in name", p.name);
            assert!(p.asset_url().ends_with(&p.release.asset));
        }
    }

    #[test]
    fn queries_are_encoded() {
        assert_eq!(
            encode("anki topic:koreader-plugin"),
            "anki+topic%3Akoreader-plugin"
        );
    }
}
