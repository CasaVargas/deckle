// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader's plugins on a reader's drive (docs/superpowers/specs/2026-10-31-koreader-plugins-design.md
//! part A): listed, installed, updated, removed and rolled back. A plugin is a folder
//! `<name>.koplugin` with a `main.lua`; on an e-ink reader KOReader's data folder is its own
//! install folder, so built-in and added plugins share `plugins/`, and only KOReader's list of
//! built-in names tells them apart.
//!
//! Every replacement keeps the old folder in `deckle-backup/plugins/`, so it can be put back.
//! Switching a plugin on or off is phase 3: KOReader rewrites `settings.reader.lua` when it
//! closes, so Deckle doesn't edit it.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::koreader::lua::{self, Value};
use crate::koreader_backup::DATA_DIRS;

/// KOReader's built-in plugins (`BUILTIN_PLUGINS`, frontend/pluginloader.lua, master on
/// 2026-10-31). Deckle never replaces or removes one of these.
pub const BUILTIN: &[&str] = &[
    "archiveviewer",
    "autodim",
    "autostandby",
    "autosuspend",
    "autoturn",
    "autowarmth",
    "batterystat",
    "bookshortcuts",
    "calibre",
    "cloudstorage",
    "coverbrowser",
    "coverimage",
    "docsettingtweak",
    "exporter",
    "externalkeyboard",
    "gestures",
    "hello",
    "hotkeys",
    "httpinspector",
    "japanese",
    "keepalive",
    "kosync",
    "movetoarchive",
    "newsdownloader",
    "opds",
    "perceptionexpander",
    "profiles",
    "qrclipboard",
    "readtimer",
    "statistics",
    "systemstat",
    "terminal",
    "texteditor",
    "timesync",
    "vocabbuilder",
    "wallabag",
];

/// The largest plugin archive Deckle takes, unpacked.
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// Where replaced and removed plugins are kept, in KOReader's folder.
pub const BACKUP_DIR: &str = "deckle-backup/plugins";
/// Where Deckle remembers each plugin's source, in KOReader's folder.
pub const SOURCES: &str = "deckle-plugins.json";

pub fn is_builtin(name: &str) -> bool {
    BUILTIN.contains(&name)
}

/// KOReader's folder on a reader's drive (or a copy of one): the first of [`DATA_DIRS`] there
/// that holds `plugins/` or `settings.reader.lua`.
pub fn data_dir(root: &Path) -> Option<PathBuf> {
    DATA_DIRS
        .iter()
        .map(|d| root.join(d))
        .find(|d| d.join("plugins").is_dir() || d.join("settings.reader.lua").is_file())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Plugin {
    /// The folder's name less `.koplugin`: what KOReader calls it.
    pub name: String,
    /// `_meta.lua`'s `fullname`, else the name.
    pub fullname: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub builtin: bool,
    /// Off in `plugins_disabled`, or an added plugin with `plugins_disable_external` on.
    pub enabled: bool,
    /// Where Deckle installed it from, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Listing {
    pub data_dir: PathBuf,
    pub plugins: Vec<Plugin>,
    /// The `.lua` files in `patches/`.
    pub patches: Vec<String>,
    /// `patches/.patches_disabled` is there: every patch is off.
    pub patches_disabled: bool,
}

/// Every plugin and patch in KOReader's folder `data`, built-in first, then by name.
pub fn list(data: &Path) -> Listing {
    let settings = std::fs::read_to_string(data.join("settings.reader.lua"))
        .ok()
        .and_then(|t| lua::parse(&t).ok());
    let disabled: Vec<String> = match settings.as_ref().and_then(|s| s.get("plugins_disabled")) {
        Some(Value::Table(t)) => t
            .iter()
            .filter(|(_, v)| *v == Value::Bool(true))
            .filter_map(|(k, _)| match k {
                lua::Key::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let external_off = settings
        .as_ref()
        .and_then(|s| s.get("plugins_disable_external"))
        == Some(&Value::Bool(true));
    let sources = load_sources(data);
    let mut plugins: Vec<Plugin> = std::fs::read_dir(data.join("plugins"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            let name = folder.strip_suffix(".koplugin")?.to_owned();
            let meta = read_meta(&e.path());
            let builtin = is_builtin(&name);
            Some(Plugin {
                fullname: meta.fullname.unwrap_or_else(|| name.clone()),
                description: meta.description,
                version: meta.version,
                builtin,
                enabled: !disabled.contains(&name) && (builtin || !external_off),
                source: sources.get(&name).cloned(),
                name,
            })
        })
        .collect();
    plugins.sort_by(|a, b| (!a.builtin, &a.name).cmp(&(!b.builtin, &b.name)));
    let patches_dir = data.join("patches");
    let mut patches: Vec<String> = std::fs::read_dir(&patches_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".lua"))
        .collect();
    patches.sort();
    Listing {
        data_dir: data.to_path_buf(),
        plugins,
        patches,
        patches_disabled: patches_dir.join(".patches_disabled").is_file(),
    }
}

#[derive(Default)]
struct Meta {
    fullname: Option<String>,
    description: Option<String>,
    version: Option<String>,
}

/// `_meta.lua`: `local _ = require("gettext")` then `return { fullname = _("…"), … }`. The
/// `_()` calls are unwrapped and the table read with [`lua::parse`]; nothing is run.
fn read_meta(plugin: &Path) -> Meta {
    std::fs::read_to_string(plugin.join("_meta.lua"))
        .map(|t| meta_text(&t))
        .unwrap_or_default()
}

/// [`read_meta`] on the file's text.
fn meta_text(text: &str) -> Meta {
    let Some(at) = text.find("return") else {
        return Meta::default();
    };
    let Ok(v) = lua::parse(&unwrap_gettext(&text[at..])) else {
        return Meta::default();
    };
    let s = |k: &str| {
        v.get(k).and_then(|x| match x {
            Value::Str(s) => Some(s.trim().to_owned()),
            Value::Num(n) => Some(n.to_string()),
            _ => None,
        })
    };
    Meta {
        fullname: s("fullname").filter(|f| !f.is_empty()),
        description: s("description").filter(|d| !d.is_empty()),
        version: s("version"),
    }
}

/// `_("text")` and `_([[text]])` as the bare string: KOReader's translation call.
fn unwrap_gettext(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        let call = b[i] == b'_'
            && b.get(i + 1) == Some(&b'(')
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'));
        if call && let Some((lit, end)) = string_then_paren(src, i + 2) {
            out.push_str(lit);
            i = end;
            continue;
        }
        let c = src[i..].chars().next().unwrap_or(' ');
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// At `from` (after `_(`): a string literal, then `)`. Returns the literal's text in the source
/// and the byte after the `)`.
fn string_then_paren(src: &str, from: usize) -> Option<(&str, usize)> {
    let rest = &src[from..];
    let lead = rest.len() - rest.trim_start().len();
    let start = from + lead;
    let body = &src[start..];
    let len = if body.starts_with("[[") {
        body.find("]]")? + 2
    } else {
        let q = body.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let mut esc = false;
        let mut end = None;
        for (j, c) in body.char_indices().skip(1) {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == q {
                end = Some(j + 1);
                break;
            }
        }
        end?
    };
    let after = &src[start + len..];
    let gap = after.len() - after.trim_start().len();
    after
        .trim_start()
        .starts_with(')')
        .then(|| (&src[start..start + len], start + len + gap + 1))
}

/// Where a plugin came from, as Deckle installed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// A GitHub repository (`owner/repo`), a URL, or the local path it was installed from.
    pub from: String,
    /// The GitHub release tag, when it came from a release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    /// When Deckle installed it, ISO 8601 UTC.
    pub installed: String,
}

fn load_sources(data: &Path) -> BTreeMap<String, Source> {
    std::fs::read(data.join(SOURCES))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_sources(data: &Path, all: &BTreeMap<String, Source>) -> Result<(), String> {
    let path = data.join(SOURCES);
    let tmp = data.join(format!(".{SOURCES}.tmp"));
    let body = serde_json::to_vec_pretty(all).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

// ---------------------------------------------------------------------------------------------
// Sources

/// Files relative to a plugin (or archive) root, with their bytes.
type Files = Vec<(String, Vec<u8>)>;

/// `(url) -> bytes`: the network, injected so tests never touch it.
pub type Fetch<'a> = dyn FnMut(&str) -> Result<Vec<u8>, String> + 'a;

/// What an install takes: a zip or a folder on this computer, or a GitHub repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum From {
    Zip(PathBuf),
    Folder(PathBuf),
    /// `owner/repo`, from `https://github.com/owner/repo` or the bare pair.
    GitHub(String),
}

impl From {
    /// A path that exists is a zip or a folder; anything else that looks like a GitHub
    /// repository is one.
    pub fn parse(s: &str) -> Result<From, String> {
        let p = Path::new(s);
        if p.is_dir() {
            return Ok(From::Folder(p.to_path_buf()));
        }
        if p.is_file() {
            return Ok(From::Zip(p.to_path_buf()));
        }
        let repo = s
            .trim()
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.")
            .trim_start_matches("github.com/");
        let parts: Vec<&str> = repo.split('/').collect();
        let ok = |x: &str| {
            !x.is_empty()
                && x.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if parts.len() == 2 && ok(parts[0]) && ok(parts[1]) {
            return Ok(From::GitHub(format!("{}/{}", parts[0], parts[1])));
        }
        Err(format!(
            "{s}: not a zip, a folder or a GitHub repository (owner/repo)"
        ))
    }

    fn label(&self) -> String {
        match self {
            From::Zip(p) | From::Folder(p) => p.display().to_string(),
            From::GitHub(r) => r.clone(),
        }
    }
}

/// A plugin ready to install: checked, unpacked in memory, and what it would replace.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub name: String,
    pub fullname: String,
    pub version: Option<String>,
    /// The plugin already there that this would replace.
    pub replaces: Option<String>,
    pub source: Source,
    /// Files relative to the plugin folder, with their bytes.
    files: Files,
}

/// The real network: GitHub's API and downloads over HTTPS.
pub fn http_fetch(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .user_agent("Deckle (+https://github.com/CasaVargas/deckle)")
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent();
    let mut resp = agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(MAX_BYTES * 2)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// Fetches and checks a plugin from `from`, without writing anything to the reader. `data` is
/// KOReader's folder, for what it would replace.
pub fn prepare(data: &Path, from: &From, fetch: &mut Fetch<'_>) -> Result<Prepared, String> {
    let now = crate::devices::now_iso();
    let (files, hint, release) = match from {
        From::Folder(dir) => (read_folder(dir)?, folder_hint(dir), None),
        From::Zip(path) => {
            let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            (unzip(&bytes)?, None, None)
        }
        From::GitHub(repo) => {
            let (bytes, release) = github(repo, fetch)?;
            let hint = repo
                .rsplit('/')
                .next()
                .map(|r| r.trim_end_matches(".koplugin").to_owned());
            (unzip(&bytes)?, hint, release)
        }
    };
    let (name, files) = pick_plugin(files, hint.as_deref())?;
    if is_builtin(&name) {
        return Err(format!(
            "{name} is one of KOReader's built-in plugins; Deckle won't replace it"
        ));
    }
    if !files.iter().any(|(p, _)| p == "main.lua") {
        return Err(format!("{name}.koplugin has no main.lua"));
    }
    let meta = meta_of(&files);
    let replaces = data
        .join("plugins")
        .join(format!("{name}.koplugin"))
        .is_dir()
        .then(|| name.clone());
    Ok(Prepared {
        fullname: meta.fullname.unwrap_or_else(|| name.clone()),
        version: meta.version,
        replaces,
        source: Source {
            from: from.label(),
            release,
            installed: now,
        },
        name,
        files,
    })
}

/// A repository's newest release asset that's a plugin zip, else its default branch's zip.
fn github(repo: &str, fetch: &mut Fetch<'_>) -> Result<(Vec<u8>, Option<String>), String> {
    let api = format!("https://api.github.com/repos/{repo}/releases/latest");
    if let Ok(body) = fetch(&api)
        && let Ok(rel) = serde_json::from_slice::<serde_json::Value>(&body)
        && let Some(assets) = rel["assets"].as_array()
    {
        let zip = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .is_some_and(|n| n.to_ascii_lowercase().ends_with(".zip"))
        });
        if let Some(url) = zip.and_then(|a| a["browser_download_url"].as_str()) {
            let tag = rel["tag_name"].as_str().map(str::to_owned);
            return Ok((fetch(url)?, tag));
        }
    }
    let zipball = format!("https://api.github.com/repos/{repo}/zipball");
    Ok((fetch(&zipball)?, None))
}

fn folder_hint(dir: &Path) -> Option<String> {
    let n = dir.file_name()?.to_string_lossy();
    Some(n.trim_end_matches(".koplugin").to_owned())
}

/// The files under `dir`, relative and `/`-separated, under the size cap.
fn read_folder(dir: &Path) -> Result<Files, String> {
    let mut out = Vec::new();
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))? {
            let e = e.map_err(|e| e.to_string())?;
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            total += bytes.len() as u64;
            if total > MAX_BYTES {
                return Err(format!("{} is over 20 MB", dir.display()));
            }
            let rel = p
                .strip_prefix(dir.parent().unwrap_or(dir))
                .unwrap_or(&p)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel, bytes));
        }
    }
    Ok(out)
}

/// A zip's files, refusing absolute paths, `..` and anything over the size cap.
fn unzip(bytes: &[u8]) -> Result<Files, String> {
    let mut z =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("not a zip: {e}"))?;
    let mut out = Vec::new();
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().map_err(|e| e.to_string())?.replace('\\', "/");
        if name.starts_with('/') || name.split('/').any(|p| p == "..") || name.contains(':') {
            return Err(format!("the archive has an unsafe path: {name}"));
        }
        total += f.size();
        if total > MAX_BYTES {
            return Err("the archive is over 20 MB unpacked".into());
        }
        let mut data = Vec::with_capacity(f.size() as usize);
        (&mut f)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        out.push((name, data));
    }
    Ok(out)
}

/// The one plugin in `files`: the single `<name>.koplugin/` folder, or, for a repository that is
/// the plugin (its `main.lua` at the top), the top folder named after `hint`. Returns its name and
/// its files relative to it.
fn pick_plugin(files: Files, hint: Option<&str>) -> Result<(String, Files), String> {
    let mut roots: Vec<String> = files
        .iter()
        .filter_map(|(p, _)| {
            let parts: Vec<&str> = p.split('/').collect();
            let i = parts.iter().position(|s| s.ends_with(".koplugin"))?;
            Some(parts[..=i].join("/"))
        })
        .collect();
    roots.sort();
    roots.dedup();
    let (root, name) = match roots.as_slice() {
        [one] => {
            let name = one
                .rsplit('/')
                .next()
                .unwrap_or(one)
                .trim_end_matches(".koplugin")
                .to_owned();
            (one.clone(), name)
        }
        [] => {
            // A repository whose top level is the plugin: `<repo-sha>/main.lua`.
            let tops: Vec<&str> = files
                .iter()
                .filter(|(p, _)| p.ends_with("/main.lua") && p.matches('/').count() == 1)
                .map(|(p, _)| p.split('/').next().unwrap_or(""))
                .collect();
            match (tops.as_slice(), hint) {
                ([top], Some(h)) if !h.is_empty() => (top.to_string(), h.to_owned()),
                _ => return Err("the archive holds no .koplugin folder".into()),
            }
        }
        many => {
            return Err(format!(
                "the archive holds {} plugins; install one at a time",
                many.len()
            ));
        }
    };
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !ok {
        return Err(format!("{name}: not a plugin name KOReader would load"));
    }
    let prefix = format!("{root}/");
    let files = files
        .into_iter()
        .filter_map(|(p, d)| p.strip_prefix(&prefix).map(|r| (r.to_owned(), d)))
        .filter(|(p, _)| !p.is_empty())
        .collect();
    Ok((name, files))
}

fn meta_of(files: &[(String, Vec<u8>)]) -> Meta {
    files
        .iter()
        .find(|(p, _)| p == "_meta.lua")
        .map(|(_, b)| meta_text(&String::from_utf8_lossy(b)))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Changes

/// What a change did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Done {
    pub name: String,
    /// Where the plugin it replaced or removed now is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept: Option<PathBuf>,
}

/// Writes `p` into KOReader's `plugins/`, through a temporary folder, keeping what it replaces.
pub fn install(data: &Path, p: &Prepared) -> Result<Done, String> {
    let plugins = data.join("plugins");
    std::fs::create_dir_all(&plugins).map_err(|e| format!("{}: {e}", plugins.display()))?;
    let target = plugins.join(format!("{}.koplugin", p.name));
    let tmp = plugins.join(format!(".{}.koplugin.tmp", p.name));
    let _ = std::fs::remove_dir_all(&tmp);
    for (rel, bytes) in &p.files {
        let to = tmp.join(rel);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&to, bytes).map_err(|e| {
            let _ = std::fs::remove_dir_all(&tmp);
            format!("{}: {e}", to.display())
        })?;
    }
    let kept = if target.exists() {
        Some(keep(data, &p.name, &target)?)
    } else {
        None
    };
    std::fs::rename(&tmp, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    let mut sources = load_sources(data);
    sources.insert(p.name.clone(), p.source.clone());
    save_sources(data, &sources)?;
    Ok(Done {
        name: p.name.clone(),
        kept,
    })
}

/// Moves an added plugin to `deckle-backup/plugins/`.
pub fn remove(data: &Path, name: &str) -> Result<Done, String> {
    if is_builtin(name) {
        return Err(format!(
            "{name} is one of KOReader's built-in plugins; it can only be switched off"
        ));
    }
    let target = data.join("plugins").join(format!("{name}.koplugin"));
    if !target.is_dir() {
        return Err(format!("{name}: no such plugin"));
    }
    let kept = keep(data, name, &target)?;
    let mut sources = load_sources(data);
    sources.remove(name);
    save_sources(data, &sources)?;
    Ok(Done {
        name: name.to_owned(),
        kept: Some(kept),
    })
}

/// Puts the newest kept copy of `name` back; the plugin there now is kept in its place.
pub fn rollback(data: &Path, name: &str) -> Result<Done, String> {
    let backups = data.join(BACKUP_DIR);
    let prefix = format!("{name}.koplugin.");
    let newest = std::fs::read_dir(&backups)
        .map_err(|_| format!("{name}: nothing kept to put back"))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.strip_prefix(&prefix)
                .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
        })
        .max()
        .ok_or_else(|| format!("{name}: nothing kept to put back"))?;
    let from = backups.join(&newest);
    let target = data.join("plugins").join(format!("{name}.koplugin"));
    let kept = if target.exists() {
        Some(keep(data, name, &target)?)
    } else {
        None
    };
    std::fs::rename(&from, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(Done {
        name: name.to_owned(),
        kept,
    })
}

/// Moves `folder` to `deckle-backup/plugins/<name>.koplugin.<stamp>`, a stamp later than any
/// already there.
fn keep(data: &Path, name: &str, folder: &Path) -> Result<PathBuf, String> {
    let backups = data.join(BACKUP_DIR);
    std::fs::create_dir_all(&backups).map_err(|e| format!("{}: {e}", backups.display()))?;
    let mut stamp: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut to = backups.join(format!("{name}.koplugin.{stamp}"));
    while to.exists() {
        stamp += 1;
        to = backups.join(format!("{name}.koplugin.{stamp}"));
    }
    std::fs::rename(folder, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gettext_calls_unwrap_to_their_strings() {
        let src = r#"return { fullname = _("News downloader"), description = _([[Saves
feeds.]]), version = "1.2", other = foo_("x") }"#;
        let out = unwrap_gettext(src);
        assert!(out.contains(r#"fullname = "News downloader""#), "{out}");
        assert!(out.contains("description = [[Saves\nfeeds.]]"), "{out}");
        assert!(out.contains(r#"foo_("x")"#), "{out}");
    }

    #[test]
    fn sources_parse_as_a_path_or_a_repository() {
        assert_eq!(
            From::parse("https://github.com/omer-faruq/appstore.koplugin").unwrap(),
            From::GitHub("omer-faruq/appstore.koplugin".into())
        );
        assert_eq!(From::parse("a/b.git").unwrap(), From::GitHub("a/b".into()));
        assert!(From::parse("not a thing").is_err());
        assert!(From::parse("https://example.com/x/y/z").is_err());
    }

    #[test]
    fn exactly_one_plugin_is_picked() {
        let f = |p: &str| (p.to_owned(), b"x".to_vec());
        let (name, files) =
            pick_plugin(vec![f("x/zen.koplugin/main.lua"), f("README.md")], None).unwrap();
        assert_eq!(name, "zen");
        assert_eq!(files[0].0, "main.lua");
        let two = vec![f("a.koplugin/main.lua"), f("b.koplugin/main.lua")];
        assert!(pick_plugin(two, None).unwrap_err().contains("2 plugins"));
        let repo = vec![
            f("owner-zen-abc123/main.lua"),
            f("owner-zen-abc123/_meta.lua"),
        ];
        assert_eq!(pick_plugin(repo.clone(), Some("zen")).unwrap().0, "zen");
        assert!(pick_plugin(repo, None).is_err());
    }
}
