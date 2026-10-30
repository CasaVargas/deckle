// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle reader-server` (KOReader hub design §A.2): the password's MD5 kept, owner-only, and
//! the status the app's card shows.

use assert_cmd::Command;

fn deckle(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("DECKLE_READER_SERVER_URL", "http://tower.local:47220");
    c
}

#[test]
fn a_password_is_kept_as_its_md5_and_status_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let st = || {
        let o = deckle(home)
            .args(["--json", "reader-server", "status"])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice::<serde_json::Value>(&o.stdout).unwrap()
    };
    deckle(home).args(["digest", "init"]).assert().success();
    let before = st();
    assert_eq!(before["enabled"], false);
    assert_eq!(before["password_set"], false);
    assert_eq!(before["opds"], "http://tower.local:47220/opds");

    deckle(home)
        .args(["reader-server", "password", "--stdin"])
        .write_stdin("hunter2\n")
        .assert()
        .success();
    assert_eq!(st()["password_set"], true);
    let key = walk(home, "reader-server.json").expect("reader-server.json");
    let text = std::fs::read_to_string(&key).unwrap();
    assert!(text.contains("2ab96390c7dbe3439de74d0c9b0b1767"), "{text}");
    assert!(!text.contains("hunter2"));
}

fn walk(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n == name) {
            return Some(p);
        }
        if p.is_dir()
            && let Some(f) = walk(&p, name)
        {
            return Some(f);
        }
    }
    None
}
