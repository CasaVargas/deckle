// SPDX-License-Identifier: GPL-3.0-or-later
//! What every bundle on disk shares (edition bundle design §5, puzzle book bundle design §2):
//! its files gathered in memory, hashed and listed, then written into a hidden `.<name>.tmp`
//! folder with the manifest last, and renamed into place. A folder iCloud Drive has only partly
//! delivered is never read as whole, and a write that fails leaves no package behind.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One file's line in a manifest's `files`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// The files a package holds, by their path inside it, before they're written.
#[derive(Default)]
pub struct Files {
    pub(crate) by_path: BTreeMap<String, Vec<u8>>,
}

impl Files {
    /// Adds `bytes` at `path` (forward slashes, no leading slash), replacing what was there.
    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) {
        self.by_path.insert(path.into(), bytes);
    }

    /// Adds `bytes` as `media/<12 hex digits of its SHA-256>.<ext>`, once however often it's
    /// added, and returns that path.
    pub fn media(&mut self, bytes: &[u8], ext: &str) -> String {
        let hash = hex(&Sha256::digest(bytes));
        let path = format!("media/{}.{ext}", &hash[..12]);
        self.by_path
            .entry(path.clone())
            .or_insert_with(|| bytes.to_vec());
        path
    }

    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    /// Every file with its size and SHA-256, by path: the manifest's `files`.
    pub fn entries(&self) -> Vec<FileEntry> {
        self.by_path
            .iter()
            .map(|(path, bytes)| FileEntry {
                path: path.clone(),
                size: bytes.len() as u64,
                sha256: hex(&Sha256::digest(bytes)),
            })
            .collect()
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What to do when the package, or its staging folder, is already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Existing {
    /// A stale staging folder is cleared and a package of the same name replaced.
    Replace,
    /// A stale staging folder is cleared, but a package of the same name is kept and the
    /// write is an error: the caller has checked, and checks again here, that none is there.
    Keep,
    /// The staging folder must not exist (it's created exclusively, which refuses a stale one
    /// and a symlink), and the package is never removed first. The caller has checked that
    /// no package is there.
    Exclusive,
}

/// Writes `files` and then `manifest` (named `manifest_name`, its bytes `json`) into
/// `dir/.<name>.tmp`, and renames that to `dir/<name>`, returning it. On any error the
/// staging folder is removed and nothing is left at `dir/<name>` that wasn't already there.
pub fn write_package(
    dir: &Path,
    name: &str,
    files: &Files,
    manifest_name: &str,
    json: &[u8],
    existing: Existing,
) -> Result<PathBuf, String> {
    let io = |p: &Path, err: std::io::Error| format!("{}: {err}", p.display());
    std::fs::create_dir_all(dir).map_err(|err| io(dir, err))?;
    let dest = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    match existing {
        Existing::Exclusive => std::fs::create_dir(&tmp).map_err(|err| io(&tmp, err))?,
        Existing::Replace | Existing::Keep => {
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }
    let result = (|| {
        for (path, bytes) in &files.by_path {
            let to = tmp.join(path);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|err| io(parent, err))?;
            }
            std::fs::write(&to, bytes).map_err(|err| io(&to, err))?;
        }
        let manifest = tmp.join(manifest_name);
        std::fs::write(&manifest, json).map_err(|err| io(&manifest, err))?;
        match existing {
            Existing::Replace if dest.exists() => {
                std::fs::remove_dir_all(&dest).map_err(|err| io(&dest, err))?;
            }
            Existing::Keep if dest.exists() => {
                return Err(format!("{}: already there", dest.display()));
            }
            _ => {}
        }
        std::fs::rename(&tmp, &dest).map_err(|err| io(&dest, err))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result?;
    Ok(dest)
}

/// Whether the package at `dir` is whole by its manifest's `files`: every listed file is there
/// at its size, and, with `hashes`, at its SHA-256; and every file in the folder but the
/// manifest is listed. `Err` says what isn't.
pub fn verify(dir: &Path, manifest_name: &str, hashes: bool) -> Result<(), String> {
    let manifest = dir.join(manifest_name);
    let bytes = std::fs::read(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let listed: Vec<FileEntry> =
        serde_json::from_value(v.get("files").cloned().unwrap_or_default())
            .map_err(|e| format!("{manifest_name}: files: {e}"))?;
    for f in &listed {
        if f.path.split(['/', '\\']).any(|seg| seg == "..") || f.path.starts_with('/') {
            return Err(format!("{}: not a path inside the package", f.path));
        }
        let p = dir.join(&f.path);
        let meta = std::fs::metadata(&p).map_err(|e| format!("{}: {e}", f.path))?;
        if !meta.is_file() || meta.len() != f.size {
            return Err(format!(
                "{}: {} bytes, listed as {}",
                f.path,
                meta.len(),
                f.size
            ));
        }
        if hashes {
            let got = hex(&Sha256::digest(
                std::fs::read(&p).map_err(|e| format!("{}: {e}", f.path))?,
            ));
            if got != f.sha256 {
                return Err(format!("{}: SHA-256 {got}, listed as {}", f.path, f.sha256));
            }
        }
    }
    let mut on_disk = Vec::new();
    walk(dir, "", &mut on_disk)?;
    for path in on_disk {
        if path != manifest_name && !listed.iter().any(|f| f.path == path) {
            return Err(format!("{path}: in the package but not in {manifest_name}"));
        }
    }
    Ok(())
}

/// Every file under `dir`, as `prefix` + its path with forward slashes.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    let io = |e: std::io::Error| format!("{}: {e}", dir.display());
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}{name}");
        if entry.file_type().map_err(io)?.is_dir() {
            walk(&entry.path(), &format!("{path}/"), out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_is_stored_once_by_content() {
        let mut f = Files::default();
        let a = f.media(b"photo", "jpg");
        let b = f.media(b"photo", "jpg");
        let c = f.media(b"other", "jpg");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(f.len(), 2);
        assert!(a.starts_with("media/") && a.len() == "media/".len() + 12 + 4);
    }

    #[test]
    fn a_package_is_staged_renamed_and_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let mut files = Files::default();
        files.insert("cover.jpg", b"jpeg".to_vec());
        files.insert("media/a.png", b"png".to_vec());
        let json = serde_json::to_vec(&serde_json::json!({ "files": files.entries() })).unwrap();
        let dest = write_package(
            tmp.path(),
            "x.pkg",
            &files,
            "m.json",
            &json,
            Existing::Exclusive,
        )
        .unwrap();
        assert_eq!(dest, tmp.path().join("x.pkg"));
        assert!(!tmp.path().join(".x.pkg.tmp").exists());
        verify(&dest, "m.json", true).unwrap();
        // a stray file, a short file and a changed file are each found out
        std::fs::write(dest.join("stray.txt"), b"x").unwrap();
        assert!(
            verify(&dest, "m.json", false)
                .unwrap_err()
                .contains("stray.txt")
        );
        std::fs::remove_file(dest.join("stray.txt")).unwrap();
        std::fs::write(dest.join("cover.jpg"), b"JPEG").unwrap();
        verify(&dest, "m.json", false).unwrap();
        assert!(
            verify(&dest, "m.json", true)
                .unwrap_err()
                .contains("SHA-256")
        );
        std::fs::write(dest.join("cover.jpg"), b"jp").unwrap();
        assert!(
            verify(&dest, "m.json", false)
                .unwrap_err()
                .contains("2 bytes")
        );
        // exclusive refuses a stale staging folder; replace clears it and the old package
        std::fs::create_dir(tmp.path().join(".x.pkg.tmp")).unwrap();
        assert!(
            write_package(
                tmp.path(),
                "x.pkg",
                &files,
                "m.json",
                &json,
                Existing::Exclusive
            )
            .is_err()
        );
        write_package(
            tmp.path(),
            "x.pkg",
            &files,
            "m.json",
            &json,
            Existing::Replace,
        )
        .unwrap();
        verify(&dest, "m.json", true).unwrap();
        // keep clears stale staging too, but never the package
        std::fs::create_dir(tmp.path().join(".x.pkg.tmp")).unwrap();
        let err = write_package(tmp.path(), "x.pkg", &files, "m.json", &json, Existing::Keep)
            .unwrap_err();
        assert!(err.contains("already there"), "{err}");
        assert!(!tmp.path().join(".x.pkg.tmp").exists());
        verify(&dest, "m.json", true).unwrap();
    }

    #[test]
    fn a_failed_write_leaves_no_package_or_staging_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let mut files = Files::default();
        // a path that can't be made: its parent is a file
        files.insert("a", b"file".to_vec());
        files.insert("a/b", b"under a file".to_vec());
        let err = write_package(
            tmp.path(),
            "y.pkg",
            &files,
            "m.json",
            b"{}",
            Existing::Replace,
        )
        .unwrap_err();
        assert!(err.contains("a/b") || err.contains("a:"), "{err}");
        assert!(!tmp.path().join("y.pkg").exists());
        assert!(!tmp.path().join(".y.pkg.tmp").exists());
    }
}
