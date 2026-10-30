// SPDX-License-Identifier: GPL-3.0-or-later
//! The reader server's one account (KOReader hub design §A.2). KOReader's sync signs in with
//! `md5(password)`, so that is what's kept: in `reader-server.json` beside `digest.toml`,
//! readable only by its owner. The catalog takes HTTP Basic and compares the MD5 of what it's
//! given.

use std::path::{Path, PathBuf};

use axum::http::HeaderMap;
use base64::Engine;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::Settings;

/// `reader-server.json` beside `config`.
pub fn key_path(config: &Path) -> PathBuf {
    config.with_file_name("reader-server.json")
}

#[derive(Serialize, Deserialize)]
struct KeyFile {
    /// `md5(password)`, lowercase hex.
    key: String,
}

/// `md5(text)` as lowercase hex.
pub fn md5_hex(text: &[u8]) -> String {
    Md5::digest(text)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The stored key, when a password has been set.
pub fn load_key(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let f: KeyFile = serde_json::from_slice(&bytes).ok()?;
    (f.key.len() == 32 && f.key.bytes().all(|b| b.is_ascii_hexdigit())).then_some(f.key)
}

/// Keeps `md5(password)`, owner-only, through a temporary file renamed into place.
pub fn save_password(path: &Path, password: &str) -> std::io::Result<()> {
    if password.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the password is empty",
        ));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = serde_json::to_vec_pretty(&KeyFile {
        key: md5_hex(password.as_bytes()),
    })?;
    let tmp = path.with_extension("json.tmp");
    write_private(&tmp, &body)?;
    std::fs::rename(&tmp, path)
}

#[cfg(unix)]
fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(body)
}

#[cfg(not(unix))]
fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, body)
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

/// `Authorization: Basic …` with the account's username and password.
pub fn basic_ok(headers: &HeaderMap, s: &Settings) -> bool {
    let Some(raw) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
    else {
        return false;
    };
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(raw.trim()) else {
        return false;
    };
    let Ok(text) = String::from_utf8(decoded) else {
        return false;
    };
    let Some((user, pass)) = text.split_once(':') else {
        return false;
    };
    // Both compared, so a wrong name takes as long as a wrong password.
    let user_ok = same(user, &s.username);
    let key_ok = same(&md5_hex(pass.as_bytes()), &s.key);
    user_ok & key_ok
}

/// KOReader sync's `x-auth-user` and `x-auth-key` (the MD5 of the password).
pub fn kosync_ok(headers: &HeaderMap, s: &Settings) -> bool {
    let get = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).unwrap_or("");
    let user_ok = same(get("x-auth-user"), &s.username);
    let key_ok = same(&get("x-auth-key").to_ascii_lowercase(), &s.key);
    user_ok & key_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            bind: "127.0.0.1".into(),
            port: 0,
            username: "deckle".into(),
            key: md5_hex(b"hunter2"),
        }
    }

    #[test]
    fn md5_matches_the_known_vector() {
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn basic_auth_needs_both_name_and_password() {
        let s = settings();
        let with = |cred: &str| {
            let mut h = HeaderMap::new();
            let b = base64::engine::general_purpose::STANDARD.encode(cred);
            h.insert("authorization", format!("Basic {b}").parse().unwrap());
            h
        };
        assert!(basic_ok(&with("deckle:hunter2"), &s));
        assert!(!basic_ok(&with("deckle:hunter3"), &s));
        assert!(!basic_ok(&with("other:hunter2"), &s));
        assert!(!basic_ok(&HeaderMap::new(), &s));
    }

    #[test]
    fn kosync_takes_the_md5_in_either_case() {
        let s = settings();
        let mut h = HeaderMap::new();
        h.insert("x-auth-user", "deckle".parse().unwrap());
        h.insert(
            "x-auth-key",
            md5_hex(b"hunter2").to_uppercase().parse().unwrap(),
        );
        assert!(kosync_ok(&h, &s));
        h.insert("x-auth-key", md5_hex(b"nope").parse().unwrap());
        assert!(!kosync_ok(&h, &s));
    }

    #[test]
    fn the_key_file_is_owner_only_and_reads_back() {
        let tmp = tempfile::tempdir().unwrap();
        let p = key_path(&tmp.path().join("digest.toml"));
        assert_eq!(load_key(&p), None);
        save_password(&p, "hunter2").unwrap();
        assert_eq!(load_key(&p), Some(md5_hex(b"hunter2")));
        assert!(save_password(&p, "").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
