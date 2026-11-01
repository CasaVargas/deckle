// SPDX-License-Identifier: GPL-3.0-or-later
//! A CBR unpacked by the system's bsdtar (libarchive), which reads RAR4, RAR5, 7z and tar. Deckle
//! bundles no unRAR: its licence sits badly with GPL-3 (fixed-layout spec §10). macOS and
//! Windows 10+ ship bsdtar as `tar`; on Linux it's `bsdtar` from libarchive-tools.

use std::path::{Path, PathBuf};

use crate::ComicError;

/// The bsdtar to use: `DECKLE_BSDTAR`, else `bsdtar`, else `tar` when it is bsdtar (not GNU tar,
/// which can't read RAR).
pub fn bsdtar() -> Result<PathBuf, ComicError> {
    if let Some(p) = std::env::var_os("DECKLE_BSDTAR").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    ["bsdtar", "tar"]
        .into_iter()
        .find(|tool| {
            std::process::Command::new(tool)
                .arg("--version")
                .output()
                .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("bsdtar"))
        })
        .map(PathBuf::from)
        .ok_or(ComicError::NoBsdtar)
}

/// Unpacks `archive` into `dir` with `tool`. bsdtar refuses absolute paths and `..` unless told
/// otherwise (`-P`), so nothing lands outside `dir`.
pub fn extract(tool: &Path, archive: &Path, dir: &Path) -> Result<(), ComicError> {
    let out = std::process::Command::new(tool)
        .arg("-x")
        .arg("-f")
        .arg(archive)
        .arg("-C")
        .arg(dir)
        .output()
        .map_err(|_| ComicError::NoBsdtar)?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("bsdtar failed");
        return Err(ComicError::NotArchive(why.trim().to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_bsdtar_says_how_to_get_it() {
        let dir = tempfile::tempdir().unwrap();
        let e = extract(Path::new("/no/such/bsdtar"), Path::new("x.cbr"), dir.path()).unwrap_err();
        assert!(matches!(e, ComicError::NoBsdtar));
        assert!(e.to_string().contains("libarchive-tools"));
    }
}
