// SPDX-License-Identifier: GPL-3.0-or-later
//! The folders the web UI's server may use (`/config`, `/data` and `DECKLE_WEB_ROOTS`). On the
//! desktop nothing is confined: the owner chose every path in a file dialog. In the web UI a
//! path comes from a browser, so each one, given now or saved earlier, must lie inside them.
//!
//! The server sets them once at start ([`set_roots`]). The CLI processes it starts get them in
//! [`ENV`], set on each child's command ([`child`]), never in the server's own environment, so a
//! CLI run by hand in the container is not confined.

use std::path::{Component, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// How the server tells the processes it starts which folders they may use.
pub const ENV: &str = "DECKLE_CONFINE_ROOTS";

/// What a refusal says.
pub const OUTSIDE: &str = "outside the folders this server may use";

static ROOTS: OnceLock<Option<Vec<PathBuf>>> = OnceLock::new();

/// The roots, canonical; those that don't exist are left out (they can hold nothing).
fn canonical(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots.iter().filter_map(|r| r.canonicalize().ok()).collect()
}

/// Confines this process to `roots`. Call before any path is checked; a second call is
/// ignored.
pub fn set_roots(roots: &[PathBuf]) {
    let _ = ROOTS.set(Some(canonical(roots)));
}

/// The roots this process is confined to: those set by the server, or those its parent passed
/// in [`ENV`]. `None` on the desktop and for a CLI run by hand.
pub fn roots() -> Option<&'static [PathBuf]> {
    ROOTS
        .get_or_init(|| {
            let v = std::env::var_os(ENV)?;
            let list: Vec<PathBuf> = v
                .to_string_lossy()
                .split('\n')
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .collect();
            Some(canonical(&list))
        })
        .as_deref()
}

/// Passes this process's roots on to a CLI it starts.
pub fn child(cmd: &mut Command) -> &mut Command {
    if let Some(r) = roots() {
        let joined: Vec<String> = r.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        cmd.env(ENV, joined.join("\n"));
    }
    cmd
}

/// `~` and `~/…` as the home folder.
fn expand(raw: &str) -> PathBuf {
    let raw = raw.trim();
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if raw == "~" {
        return home().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(h) = home()
    {
        return h.join(rest);
    }
    PathBuf::from(raw)
}

/// `raw` resolved and checked against `roots`: absolute (after `~`), with no `..`, and inside a
/// root once links are followed. A path that doesn't exist yet is judged by its deepest
/// existing folder, so a new file or folder inside a root is fine.
pub fn confine_to(roots: &[PathBuf], raw: &str) -> Result<PathBuf, String> {
    let path = expand(raw);
    if !path.is_absolute() {
        return Err(format!("{raw}: not an absolute path"));
    }
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(format!("{raw}: {OUTSIDE}"));
    }
    let mut existing = path.as_path();
    let mut rest: Vec<&std::ffi::OsStr> = Vec::new();
    let real = loop {
        match existing.canonicalize() {
            Ok(real) => break real,
            Err(_) => {
                let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
                    return Err(format!("{raw}: {OUTSIDE}"));
                };
                rest.push(name);
                existing = parent;
            }
        }
    };
    let full = rest.iter().rev().fold(real, |p, n| p.join(n));
    if roots.iter().any(|r| full.starts_with(r)) {
        Ok(full)
    } else {
        Err(format!("{raw}: {OUTSIDE}"))
    }
}

/// `raw` checked against this process's roots, if it has any; otherwise `raw` with `~`
/// expanded, as it always was.
pub fn check(raw: &str) -> Result<PathBuf, String> {
    match roots() {
        Some(r) => confine_to(r, raw),
        None => Ok(expand(raw)),
    }
}

/// [`check`] for a setting saved earlier: the error names the setting, since the owner has to
/// change it in the file.
pub fn check_saved(setting: &str, raw: &str) -> Result<PathBuf, String> {
    check(raw).map_err(|e| format!("{setting}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn paths_inside_a_root_pass_and_others_dont() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let outside = tmp.path().join("private");
        std::fs::create_dir_all(root.join("books")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.epub"), b"x").unwrap();
        let roots = canonical(std::slice::from_ref(&root));
        let s = |p: &Path| p.to_string_lossy().into_owned();

        assert!(confine_to(&roots, &s(&root.join("books"))).is_ok());
        // Not there yet, three folders deep: judged by `books`.
        let new = root.join("books/a/b/c.epub");
        assert_eq!(
            confine_to(&roots, &s(&new)).unwrap(),
            roots[0].join("books/a/b/c.epub")
        );
        for bad in [
            s(&outside.join("secret.epub")),
            s(&outside),
            format!("{}/../private/secret.epub", s(&root)),
            format!("{}/books/..", s(&root)),
            "relative/path".into(),
            "/".into(),
        ] {
            assert!(confine_to(&roots, &bad).is_err(), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_a_root_that_points_out_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let outside = tmp.path().join("private");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("way-out")).unwrap();
        let roots = canonical(std::slice::from_ref(&root));
        let p = root.join("way-out/new.epub");
        assert!(confine_to(&roots, &p.to_string_lossy()).is_err());
    }
}
