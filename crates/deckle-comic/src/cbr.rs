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

/// Unpacks `archive` into `dir` with `tool`, each page at most
/// [`MAX_COMIC_PAGE_BYTES`](deckle_limits::MAX_COMIC_PAGE_BYTES) and all of them
/// [`MAX_COMIC_BYTES`](deckle_limits::MAX_COMIC_BYTES) (size-limits design, step 3).
///
/// bsdtar converts the archive once to a pax tar on its output (`-cf - --format pax @archive`:
/// `ustar` would silently leave out a path over 100 characters), which is read here member by
/// member, so a size the archive only declares is never trusted. A member with an absolute path,
/// a `..`, or a link is refused, since bsdtar's own `-x` no longer stands guard.
pub fn extract(tool: &Path, archive: &Path, dir: &Path) -> Result<(), ComicError> {
    extract_with(
        tool,
        archive,
        dir,
        deckle_limits::MAX_COMIC_PAGE_BYTES,
        deckle_limits::MAX_COMIC_BYTES,
    )
}

fn extract_with(
    tool: &Path,
    archive: &Path,
    dir: &Path,
    page_limit: u64,
    total_limit: u64,
) -> Result<(), ComicError> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut source = std::ffi::OsString::from("@");
    source.push(archive);
    let mut child = Command::new(tool)
        .args(["-c", "-f", "-", "--format", "pax"])
        .arg(source)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ComicError::NoBsdtar)?;
    // Drained while the tar is read, so bsdtar never blocks on a full stderr; only its start
    // is kept.
    let mut stderr = child.stderr.take().expect("piped");
    let errors = std::thread::spawn(move || {
        let mut head = Vec::new();
        let _ = (&mut stderr).take(64 * 1024).read_to_end(&mut head);
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        head
    });
    let read = unpack(
        child.stdout.take().expect("piped"),
        dir,
        page_limit,
        total_limit,
    );
    if read.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let stderr = errors.join().unwrap_or_default();
    let failed = || {
        let why = String::from_utf8_lossy(&stderr);
        let why = why
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("bsdtar failed");
        ComicError::NotArchive(why.trim().to_owned())
    };
    match read {
        // A tar cut short is how bsdtar failing shows here; its reason is the better one.
        Err(ComicError::Io(_)) if !status.success() => Err(failed()),
        Err(e) => Err(e),
        // A truncated archive gives exit 1 and a valid, empty tar.
        Ok(()) if !status.success() => Err(failed()),
        Ok(()) => Ok(()),
    }
}

/// Writes the members of the tar stream `r` into `dir`, each through `page_limit` and all of
/// them through `total_limit`.
fn unpack(
    r: impl std::io::Read,
    dir: &Path,
    page_limit: u64,
    total_limit: u64,
) -> Result<(), ComicError> {
    use std::path::Component;
    let mut total = deckle_limits::Budget::new(total_limit);
    let mut tar = tar::Archive::new(r);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        let path = entry.path()?.into_owned();
        let name = path.to_string_lossy().into_owned();
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(ComicError::NotArchive(format!(
                "the archive has an unsafe path: {name}"
            )));
        }
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(ComicError::NotArchive(format!(
                "the archive has a link, which a comic doesn't need: {name}"
            )));
        }
        if kind.is_dir() {
            std::fs::create_dir_all(dir.join(&path))?;
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let bytes =
            total.read(&mut entry, page_limit).map_err(
                |e| match deckle_limits::TooLarge::in_io(&e) {
                    Some(t) if t.limit == page_limit => ComicError::PageTooLarge(name.clone(), t),
                    Some(t) => ComicError::TooLarge(t),
                    None => e.into(),
                },
            )?;
        let out = dir.join(&path);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out, bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tar of `(name, type flag, data)` members, headers written by hand so any name goes.
    fn tar_of(members: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for &(name, kind, data) in members {
            let mut h = [0u8; 512];
            let mut put = |at: usize, v: &[u8]| h[at..at + v.len()].copy_from_slice(v);
            put(0, name.as_bytes());
            put(100, b"0000644\0");
            put(108, b"0000000\0");
            put(116, b"0000000\0");
            put(124, format!("{:011o}\0", data.len()).as_bytes());
            put(136, b"00000000000\0");
            put(148, b"        ");
            put(156, &[kind]);
            if kind == b'2' {
                put(157, b"elsewhere");
            }
            put(257, b"ustar\0");
            put(263, b"00");
            let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
            h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
            out.extend(h);
            out.extend(data);
            out.resize(out.len().next_multiple_of(512), 0);
        }
        out.extend([0u8; 1024]);
        out
    }

    fn unpack_into(
        tar: &[u8],
        page: u64,
        total: u64,
    ) -> (tempfile::TempDir, Result<(), ComicError>) {
        let dir = tempfile::tempdir().unwrap();
        let r = unpack(tar, dir.path(), page, total);
        (dir, r)
    }

    #[test]
    fn members_are_written_inside_the_page_and_archive_limits() {
        let page = [7u8; 600];
        let tar = tar_of(&[("a/1.png", b'0', &page), ("2.png", b'0', &page)]);
        let (dir, r) = unpack_into(&tar, 1000, 2000);
        r.unwrap();
        assert_eq!(std::fs::read(dir.path().join("a/1.png")).unwrap(), page);
        let (_, r) = unpack_into(&tar, 500, 2000);
        assert_eq!(
            r.unwrap_err().to_string(),
            "page a/1.png is more than 500 bytes"
        );
        let (_, r) = unpack_into(&tar, 1000, 1100);
        assert_eq!(
            r.unwrap_err().to_string(),
            "this comic unpacks to more than 1100 bytes"
        );
    }

    #[test]
    fn escaping_paths_and_links_are_refused() {
        for (name, kind) in [("../up.png", b'0'), ("/abs.png", b'0'), ("l.png", b'2')] {
            let (dir, r) = unpack_into(&tar_of(&[(name, kind, b"x")]), 1000, 1000);
            assert!(matches!(r, Err(ComicError::NotArchive(_))), "{name}");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "{name}");
        }
    }

    #[test]
    fn bsdtar_streams_a_long_path_and_its_limits_hold() {
        let Ok(tool) = bsdtar() else {
            eprintln!("skipped: no bsdtar");
            return;
        };
        // A path past ustar's 100 characters comes through in a pax header.
        let long = format!("{}/1.png", "d".repeat(120));
        let src = tempfile::tempdir().unwrap();
        let archive = src.path().join("book.cbr");
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(600);
        h.set_mode(0o644);
        b.append_data(&mut h, &long, &[1u8; 600][..]).unwrap();
        std::fs::write(&archive, b.into_inner().unwrap()).unwrap();

        let out = tempfile::tempdir().unwrap();
        extract_with(&tool, &archive, out.path(), 1000, 1000).unwrap();
        assert_eq!(std::fs::read(out.path().join(&long)).unwrap().len(), 600);
        let out = tempfile::tempdir().unwrap();
        let e = extract_with(&tool, &archive, out.path(), 500, 1000).unwrap_err();
        assert!(matches!(e, ComicError::PageTooLarge(..)), "{e}");

        // Cut short, it fails the import, whatever tar bsdtar still wrote.
        let bytes = std::fs::read(&archive).unwrap();
        std::fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();
        let out = tempfile::tempdir().unwrap();
        let e = extract_with(&tool, &archive, out.path(), 1000, 1000).unwrap_err();
        assert!(matches!(e, ComicError::NotArchive(_)), "{e}");
    }

    #[test]
    fn a_missing_bsdtar_says_how_to_get_it() {
        let dir = tempfile::tempdir().unwrap();
        let e = extract(Path::new("/no/such/bsdtar"), Path::new("x.cbr"), dir.path()).unwrap_err();
        assert!(matches!(e, ComicError::NoBsdtar));
        assert!(e.to_string().contains("libarchive-tools"));
    }
}
