// SPDX-License-Identifier: GPL-3.0-or-later
//! The bookmarklet (save-to-paper design §4): `deckle digest watch` listens on
//! `127.0.0.1:47219`; the bookmarklet opens `/save?token=…&url=…&title=…` in a small popup,
//! which avoids the browser's mixed-content and CORS rules. The token keeps other local pages
//! from adding to the queue blindly.

use std::path::Path;

pub const PORT: u16 = 47219;

/// The token in `bookmarklet.token` in `dir`, created on first use: 32 hex characters.
pub fn token(dir: &Path) -> std::io::Result<String> {
    let path = dir.join("bookmarklet.token");
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim().to_owned();
        if t.len() == 32 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(t);
        }
    }
    let t = format!("{:032x}", rand::random::<u128>());
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, &t)?;
    Ok(t)
}

/// The bookmarklet to drag to the bookmarks bar.
pub fn javascript(token: &str) -> String {
    format!(
        "javascript:(()=>{{window.open('http://127.0.0.1:{PORT}/save?token={token}&url='+\
         encodeURIComponent(location.href)+'&title='+encodeURIComponent(document.title),\
         'deckle','width=420,height=200')}})()"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_created_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let a = token(dir.path()).unwrap();
        assert_eq!(a.len(), 32);
        assert_eq!(token(dir.path()).unwrap(), a);
        assert!(javascript(&a).contains(&format!("127.0.0.1:47219/save?token={a}&url=")));
    }
}
