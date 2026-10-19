// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 6: put the KFX on a folder or a mass-storage Kindle.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::inspect::BookInfo;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Device {
    pub root: PathBuf,
    /// First line of `system/version.txt`, else the volume name.
    pub model_hint: String,
    /// `system/thumbnails` exists and is writable: EBOK books can get a cover thumbnail from us.
    pub thumbnails_writable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Target {
    Folder(PathBuf),
    MassStorage(Device),
}

#[derive(Debug, thiserror::Error)]
pub enum DeliverError {
    #[error("I/O error writing {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

const MAX_FILENAME_LEN: usize = 120;

/// Sort-form author name: `file-as` when present, else `Last, First`.
fn author_sort(info: &BookInfo) -> Option<String> {
    let c = info.creators.first()?;
    if let Some(fa) = &c.file_as
        && !fa.trim().is_empty()
    {
        return Some(fa.trim().to_owned());
    }
    Some(deckle_kfx::metadata::author_sort(&c.name))
}

fn sanitize(s: &str) -> String {
    let ascii = deunicode::deunicode(s);
    ascii
        .chars()
        .map(|c| {
            if c.is_ascii_control()
                || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// `<Author sort> - <Title>.kfx`, ASCII-folded, filesystem-safe, at most 120 characters.
pub fn device_filename(info: &BookInfo) -> String {
    let title = sanitize(&info.title);
    let stem = match author_sort(info)
        .map(|a| sanitize(&a))
        .filter(|a| !a.is_empty())
    {
        Some(author) => format!("{author} - {title}"),
        None => title,
    };
    let stem = if stem.is_empty() {
        "book".to_owned()
    } else {
        stem
    };
    let max_stem = MAX_FILENAME_LEN - ".kfx".len();
    let stem: String = stem.chars().take(max_stem).collect();
    format!("{}.kfx", stem.trim_end())
}

/// Kindle detection over explicit mount roots (the public `detect_mass_storage` wraps it).
pub fn detect_in(roots: &[PathBuf]) -> Vec<Device> {
    let mut out = Vec::new();
    for root in roots {
        let documents = root.join("documents");
        let system = root.join("system");
        if !(documents.is_dir() && system.is_dir()) {
            continue;
        }
        let model_hint = std::fs::read_to_string(system.join("version.txt"))
            .ok()
            .and_then(|s| s.lines().next().map(|l| l.trim().to_owned()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                root.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Kindle".into())
            });
        let thumbs = system.join("thumbnails");
        let thumbnails_writable = thumbs.is_dir() && {
            let probe = thumbs.join(".deckle-probe");
            let ok = std::fs::write(&probe, b"").is_ok();
            let _ = std::fs::remove_file(&probe);
            ok
        };
        out.push(Device {
            root: root.clone(),
            model_hint,
            thumbnails_writable,
        });
    }
    out
}

/// Connected mass-storage Kindles: `/Volumes/*` on macOS, drive letters on Windows,
/// `/media/$USER/*` and `/run/media/$USER/*` on Linux.
pub fn detect_mass_storage() -> Vec<Device> {
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            roots.extend(rd.flatten().map(|e| e.path()));
        }
    }
    #[cfg(target_os = "windows")]
    {
        for letter in b'D'..=b'Z' {
            roots.push(PathBuf::from(format!("{}:\\", letter as char)));
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(user) = std::env::var_os("USER") {
            for base in ["/media", "/run/media"] {
                if let Ok(rd) = std::fs::read_dir(std::path::Path::new(base).join(&user)) {
                    roots.extend(rd.flatten().map(|e| e.path()));
                }
            }
        }
    }
    detect_in(&roots)
}

/// Writes the KFX and returns its path. Mass storage gets `documents/<name>`, fsynced.
pub fn deliver(kfx: &[u8], info: &BookInfo, target: &Target) -> Result<PathBuf, DeliverError> {
    let name = device_filename(info);
    let path = match target {
        Target::Folder(dir) => dir.join(name),
        Target::MassStorage(dev) => dev.root.join("documents").join(name),
    };
    let io = |source: std::io::Error| DeliverError::Io {
        path: path.clone(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let mut f = std::fs::File::create(&path).map_err(io)?;
    f.write_all(kfx).map_err(io)?;
    f.sync_all().map_err(io)?;
    Ok(path)
}
