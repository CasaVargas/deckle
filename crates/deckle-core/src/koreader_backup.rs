// SPDX-License-Identifier: GPL-3.0-or-later
//! Backup and restore of KOReader's state (docs/superpowers/specs/2026-10-30-koreader-hub-design.md
//! §D): every sidecar and KOReader's own files, copied as plain files with a `manifest.json`, and
//! put back beside each book on the same reader or another one.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::koreader::{self, lua};

pub const MANIFEST: &str = "manifest.json";
/// KOReader's own folder, per kind of reader: a Kobo, a Kindle, a PocketBook.
pub const DATA_DIRS: &[&str] = &[".adds/koreader", "koreader", "applications/koreader"];
/// The files in KOReader's folder that `--settings` puts back.
pub const SETTINGS: &[&str] = &[
    "settings.reader.lua",
    "history.lua",
    "settings/statistics.sqlite3",
    "settings/vocabulary_builder.sqlite3",
];
/// Folders in KOReader's folder backed up whole. Their sidecars go back beside the book.
const DATA_FOLDERS: &[&str] = &["docsettings", "hashdocsettings"];
/// Seconds a target file must be past the backup's copy to count as newer: FAT keeps 2 s.
const SLACK: u64 = 2;
/// How deep a copy or a search for books goes, and how many entries a search looks at.
const MAX_DEPTH: usize = 12;
const MAX_ENTRIES: usize = 50_000;

/// `manifest.json`: what a backup holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// The reader it came from: `Kobo`, `PocketBook`, `Kindle` or a folder's name.
    pub device: String,
    /// When it was made, ISO 8601 UTC.
    pub when: String,
    /// KOReader's folder on the source, relative to its root; `None` when it had none.
    pub koreader_dir: Option<String>,
    /// Every file copied, at the same path in the backup as on the source.
    pub files: Vec<File>,
    pub sidecars: Vec<Saved>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    /// Relative to the root, `/`-separated.
    pub path: String,
    pub size: u64,
    /// Seconds since 1970.
    pub modified: u64,
}

/// One sidecar in a backup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    /// The `.sdr` folder, relative to the root.
    pub sdr: String,
    /// The `metadata.<ext>.lua` file in it.
    pub metadata: String,
    /// The book, as [`koreader::Sidecar::book`] gives it.
    pub book: String,
    /// The book's partial MD5: the sidecar's `partial_md5_checksum`, else worked out from the
    /// book, else a `hashdocsettings/` folder's name.
    pub partial_md5: Option<String>,
    /// The `metadata.<ext>.lua` file's modification time, seconds since 1970.
    pub modified: u64,
}

/// What a backup did.
#[derive(Clone, Debug, Serialize)]
pub struct Backup {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// `(file, why)` for files that couldn't be copied.
    pub failed: Vec<(String, String)>,
}

/// Copies KOReader's state on `root` into `into`, a folder that mustn't exist yet.
pub fn backup(root: &Path, into: &Path, device: &str, when: &str) -> Result<Backup, String> {
    if into.exists() {
        return Err(format!("{} already exists", into.display()));
    }
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    let mut copy = Copier {
        root,
        into,
        files: Vec::new(),
        seen: HashSet::new(),
        failed: Vec::new(),
    };
    let mut sidecars = Vec::new();
    for s in koreader::find(root) {
        let Some(sdr) = s.file.parent() else {
            continue;
        };
        copy.tree(sdr, 0);
        let checksum = std::fs::read(&s.file)
            .ok()
            .and_then(|b| lua::parse(&String::from_utf8_lossy(&b)).ok())
            .and_then(|v| v.get("partial_md5_checksum")?.as_str().map(str::to_owned));
        let partial_md5 = checksum.or_else(|| match s.book.strip_prefix("koreader:") {
            // `hashdocsettings/<xx>/<md5>.sdr`
            Some(stem) => is_md5(stem).then(|| stem.to_ascii_lowercase()),
            None => koreader::partial_md5(&root.join(&s.book)),
        });
        sidecars.push(Saved {
            sdr: rel(root, sdr),
            metadata: name(&s.file),
            book: s.book,
            partial_md5,
            modified: modified(&s.file).unwrap_or(0),
        });
    }
    let data = DATA_DIRS.iter().find(|d| root.join(d).is_dir());
    if let Some(d) = data {
        let dir = root.join(d);
        for f in SETTINGS {
            if dir.join(f).is_file() {
                copy.file(&dir.join(f));
            }
        }
        for f in DATA_FOLDERS {
            if dir.join(f).is_dir() {
                copy.tree(&dir.join(f), 0);
            }
        }
        // Added plugins and patches; KOReader's own plugins come with KOReader (plugins
        // design §4).
        for e in std::fs::read_dir(dir.join("plugins"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let n = e.file_name().to_string_lossy().into_owned();
            if let Some(name) = n.strip_suffix(".koplugin")
                && e.path().is_dir()
                && !crate::koreader_plugins::is_builtin(name)
            {
                copy.tree(&e.path(), 0);
            }
        }
        if dir.join("patches").is_dir() {
            copy.tree(&dir.join("patches"), 0);
        }
    }
    let mut files = copy.files;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        device: device.to_owned(),
        when: when.to_owned(),
        koreader_dir: data.map(|d| (*d).to_owned()),
        files,
        sidecars,
    };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(into.join(MANIFEST), json).map_err(|e| format!("{MANIFEST}: {e}"))?;
    Ok(Backup {
        dir: into.to_path_buf(),
        manifest,
        failed: copy.failed,
    })
}

/// Copies files from under `root` to the same path under `into`, each once.
struct Copier<'a> {
    root: &'a Path,
    into: &'a Path,
    files: Vec<File>,
    seen: HashSet<String>,
    failed: Vec<(String, String)>,
}

impl Copier<'_> {
    fn tree(&mut self, dir: &Path, depth: usize) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            self.failed
                .push((rel(self.root, dir), "can't be read".into()));
            return;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            match e.file_type() {
                Ok(t) if t.is_dir() && depth < MAX_DEPTH => self.tree(&e.path(), depth + 1),
                Ok(t) if t.is_file() => self.file(&e.path()),
                _ => {}
            }
        }
    }

    fn file(&mut self, path: &Path) {
        let r = rel(self.root, path);
        if !self.seen.insert(r.clone()) {
            return;
        }
        let when = modified(path).unwrap_or(0);
        match write_atomic(path, &self.into.join(&r), when) {
            Ok(size) => self.files.push(File {
                path: r,
                size,
                modified: when,
            }),
            Err(e) => self.failed.push((r, e)),
        }
    }
}

/// How a sidecar or a settings file fared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Restored,
    /// The target's copy is newer (or another sidecar from the backup already went there).
    Kept,
    NoBook,
    Failed,
}

/// One line of a restore's report.
#[derive(Clone, Debug, Serialize)]
pub struct Item {
    /// The book as the backup knows it, or the settings file.
    pub book: String,
    /// Where it went on the target, relative to its root: the book, or the settings file.
    pub to: Option<String>,
    /// How the book was found: `path` or `md5`.
    pub by: Option<&'static str>,
    pub outcome: Outcome,
    pub error: Option<String>,
}

/// What a restore did.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Restore {
    pub sidecars: Vec<Item>,
    pub settings: Vec<Item>,
}

impl Restore {
    pub fn count(&self, o: Outcome) -> usize {
        self.sidecars
            .iter()
            .chain(&self.settings)
            .filter(|i| i.outcome == o)
            .count()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Also put back KOReader's settings, history, statistics and vocabulary.
    pub settings: bool,
    /// Overwrite a target file that's newer than the backup's.
    pub force: bool,
}

/// The manifest of the backup in `dir`.
pub fn load(dir: &Path) -> Result<Manifest, String> {
    let bytes = std::fs::read(dir.join(MANIFEST))
        .map_err(|e| format!("{}: {e}", dir.join(MANIFEST).display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{MANIFEST}: {e}"))
}

/// Puts the backup in `dir` back on `target`: each sidecar beside its book, found by its path
/// or else by its partial MD5, and with `settings`, KOReader's own files.
pub fn restore(dir: &Path, target: &Path, opts: Options) -> Result<Restore, String> {
    let manifest = load(dir)?;
    let mut out = Restore::default();
    // Newest first, so when two sidecars of the backup find one book the newer goes back.
    let mut sidecars = manifest.sidecars.clone();
    sidecars.sort_by_key(|s| std::cmp::Reverse(s.modified));
    let mut index: Option<HashMap<String, Vec<String>>> = None;
    let mut done: HashSet<String> = HashSet::new();
    for s in &sidecars {
        let item = |to: Option<String>, by, outcome, error: Option<String>| Item {
            book: s.book.clone(),
            to,
            by,
            outcome,
            error,
        };
        if !safe(&s.sdr) || !safe(&s.book) {
            out.sidecars.push(item(
                None,
                None,
                Outcome::Failed,
                Some("a path in the manifest leaves the folder".into()),
            ));
            continue;
        }
        let by_path = (!s.book.starts_with("koreader:") && target.join(&s.book).is_file())
            .then(|| (s.book.clone(), "path"));
        let found = by_path.or_else(|| {
            let md5 = s.partial_md5.as_deref()?;
            let index = index.get_or_insert_with(|| books(target, &extensions(&manifest)));
            let book = index.get(&md5.to_ascii_lowercase())?.first()?;
            Some((book.clone(), "md5"))
        });
        let Some((book, by)) = found else {
            out.sidecars.push(item(None, None, Outcome::NoBook, None));
            continue;
        };
        let (stem, ext) = split_name(&book);
        let parent = book.rsplit_once('/').map(|(p, _)| p);
        let sdr = match parent {
            Some(p) => format!("{p}/{stem}.sdr"),
            None => format!("{stem}.sdr"),
        };
        let metadata = format!("metadata.{ext}.lua");
        let dest = target.join(&sdr);
        let newer = modified(&dest.join(&metadata)).is_some_and(|t| t > s.modified + SLACK);
        if !done.insert(sdr.clone()) || (newer && !opts.force) {
            out.sidecars
                .push(item(Some(book), Some(by), Outcome::Kept, None));
            continue;
        }
        // The backup's `metadata.<its ext>.lua…` files take the book's extension here.
        let old = s.metadata.strip_suffix(".lua").unwrap_or(&s.metadata);
        let new = format!("metadata.{ext}");
        let prefix = format!("{}/", s.sdr);
        let mut error = None;
        for f in manifest
            .files
            .iter()
            .filter(|f| f.path.starts_with(&prefix))
        {
            let inside = &f.path[prefix.len()..];
            let inside = match inside.strip_prefix(old) {
                Some(rest) => format!("{new}{rest}"),
                None => inside.to_owned(),
            };
            if !safe(&f.path) || !safe(&inside) {
                error = Some("a path in the manifest leaves the folder".into());
                break;
            }
            if let Err(e) = write_atomic(&dir.join(&f.path), &dest.join(&inside), f.modified) {
                error = Some(format!("{inside}: {e}"));
                break;
            }
        }
        let outcome = if error.is_some() {
            Outcome::Failed
        } else {
            Outcome::Restored
        };
        out.sidecars
            .push(item(Some(book), Some(by), outcome, error));
    }
    if opts.settings {
        out.settings = restore_settings(dir, &manifest, target, opts.force);
    }
    Ok(out)
}

/// KOReader's own files from the backup into the target's KOReader folder.
fn restore_settings(dir: &Path, manifest: &Manifest, target: &Path, force: bool) -> Vec<Item> {
    let Some(from) = manifest.koreader_dir.as_deref() else {
        return Vec::new();
    };
    let to = DATA_DIRS.iter().find(|d| target.join(d).is_dir());
    let mut out = Vec::new();
    for name in SETTINGS {
        let path = format!("{from}/{name}");
        let Some(f) = manifest.files.iter().find(|f| f.path == path) else {
            continue;
        };
        let mut item = Item {
            book: (*name).to_owned(),
            to: None,
            by: None,
            outcome: Outcome::Failed,
            error: None,
        };
        let Some(to) = to else {
            item.error = Some("no KOReader folder on the target".into());
            out.push(item);
            continue;
        };
        let rel = format!("{to}/{name}");
        let dest = target.join(&rel);
        item.to = Some(rel);
        if !safe(from) {
            item.error = Some("a path in the manifest leaves the folder".into());
        } else if modified(&dest).is_some_and(|t| t > f.modified + SLACK) && !force {
            item.outcome = Outcome::Kept;
        } else {
            match write_atomic(&dir.join(&path), &dest, f.modified) {
                Ok(_) => item.outcome = Outcome::Restored,
                Err(e) => item.error = Some(e),
            }
        }
        out.push(item);
    }
    out.extend(restore_plugins(
        dir,
        manifest,
        from,
        to.copied(),
        target,
        force,
    ));
    out
}

/// The backup's added plugins and patches back into KOReader's folder, one item per plugin
/// folder or patch file: kept when the target's copy is newer, unless `force`.
fn restore_plugins(
    dir: &Path,
    manifest: &Manifest,
    from: &str,
    to: Option<&str>,
    target: &Path,
    force: bool,
) -> Vec<Item> {
    let mut groups: std::collections::BTreeMap<String, Vec<&File>> = Default::default();
    for f in &manifest.files {
        let Some(rest) = f.path.strip_prefix(&format!("{from}/")) else {
            continue;
        };
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        let key = match parts.as_slice() {
            ["plugins", p, _] if p.ends_with(".koplugin") => format!("plugins/{p}"),
            ["patches", p] => format!("patches/{p}"),
            ["patches", p, _] => format!("patches/{p}"),
            _ => continue,
        };
        groups.entry(key).or_default().push(f);
    }
    let mut out = Vec::new();
    for (key, files) in groups {
        let mut item = Item {
            book: key.clone(),
            to: None,
            by: None,
            outcome: Outcome::Failed,
            error: None,
        };
        let Some(to) = to else {
            item.error = Some("no KOReader folder on the target".into());
            out.push(item);
            continue;
        };
        item.to = Some(format!("{to}/{key}"));
        let newer = files.iter().any(|f| {
            let rel = f.path.strip_prefix(&format!("{from}/")).unwrap_or(&f.path);
            modified(&target.join(to).join(rel)).is_some_and(|t| t > f.modified + SLACK)
        });
        if !safe(from) || files.iter().any(|f| !safe(&f.path)) {
            item.error = Some("a path in the manifest leaves the folder".into());
        } else if newer && !force {
            item.outcome = Outcome::Kept;
        } else {
            item.outcome = Outcome::Restored;
            for f in files {
                let rel = f.path.strip_prefix(&format!("{from}/")).unwrap_or(&f.path);
                if let Err(e) =
                    write_atomic(&dir.join(&f.path), &target.join(to).join(rel), f.modified)
                {
                    item.outcome = Outcome::Failed;
                    item.error = Some(e);
                    break;
                }
            }
        }
        out.push(item);
    }
    out
}

/// The book extensions the backup's sidecars name (`metadata.<ext>.lua`), lowercase.
fn extensions(m: &Manifest) -> HashSet<String> {
    m.sidecars
        .iter()
        .filter_map(|s| {
            let ext = s.metadata.strip_prefix("metadata.")?.strip_suffix(".lua")?;
            Some(ext.to_ascii_lowercase())
        })
        .collect()
}

/// The books under `root` with one of `exts`, by partial MD5: paths relative to `root`,
/// `/`-separated and sorted.
fn books(root: &Path, exts: &HashSet<String>) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut seen = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                stack.clear();
                break;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(t) = e.file_type() else {
                continue;
            };
            if t.is_dir() {
                if !name.starts_with('.') && !name.ends_with(".sdr") && depth < MAX_DEPTH {
                    stack.push((e.path(), depth + 1));
                }
            } else if t.is_file()
                && exts.contains(&split_name(&name).1.to_ascii_lowercase())
                && let Some(md5) = koreader::partial_md5(&e.path())
            {
                out.entry(md5).or_default().push(rel(root, &e.path()));
            }
        }
    }
    for v in out.values_mut() {
        v.sort();
    }
    out
}

/// A book's file name without its last extension, and that extension, as KOReader names the
/// `.sdr` folder and its `metadata.<ext>.lua`.
fn split_name(path: &str) -> (&str, &str) {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.').unwrap_or((file, ""))
}

/// Copies `from` to `to` through a hidden temporary file and a rename, with `modified` as its
/// time. Returns the size.
fn write_atomic(from: &Path, to: &Path, modified: u64) -> Result<u64, String> {
    let dir = to.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".deckle-")
        .suffix(".tmp")
        .tempfile_in(dir)
        .map_err(|e| e.to_string())?;
    let mut src = std::fs::File::open(from).map_err(|e| e.to_string())?;
    let size = std::io::copy(&mut src, tmp.as_file_mut()).map_err(|e| e.to_string())?;
    let f = tmp.as_file();
    f.set_modified(UNIX_EPOCH + Duration::from_secs(modified))
        .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    tmp.persist(to).map_err(|e| e.error.to_string())?;
    Ok(size)
}

fn modified(path: &Path) -> Option<u64> {
    let t: SystemTime = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

/// `path` relative to `root`, `/`-separated.
fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A relative path that stays inside its folder.
fn safe(path: &str) -> bool {
    let p = PathBuf::from(path.strip_prefix("koreader:").unwrap_or(path));
    !path.is_empty()
        && p.components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

fn is_md5(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_split_as_koreader_does() {
        assert_eq!(
            split_name("Books/Walden.kepub.epub"),
            ("Walden.kepub", "epub")
        );
        assert_eq!(split_name("Walden.pdf"), ("Walden", "pdf"));
        assert_eq!(split_name("README"), ("README", ""));
    }

    #[test]
    fn manifest_paths_stay_inside() {
        assert!(safe("Books/Walden.sdr"));
        assert!(safe("koreader:abcdef"));
        assert!(!safe("../Walden.sdr"));
        assert!(!safe("/etc/passwd"));
        assert!(!safe("Books/../../x"));
        assert!(!safe(""));
    }

    #[test]
    fn a_hash_folder_name_is_an_md5() {
        assert!(is_md5("0123456789abcdef0123456789ABCDEF"));
        assert!(!is_md5("Walden"));
    }
}
