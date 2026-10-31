// SPDX-License-Identifier: GPL-3.0-or-later
//! The plugin store (docs/superpowers/specs/2026-10-31-koreader-plugins-design.md part C), with
//! GitHub stood in for.
use std::cell::Cell;
use std::io::Write;
use std::path::Path;

use deckle_core::koreader_plugins::{install, list};
use deckle_core::koreader_store::{
    Pick, Pin, RECOMMENDED_URL, Recommended, Sort, built_in, cached, prepare_pick, recommended,
    search, updates,
};
use sha2::{Digest, Sha256};

fn zip(files: &[(&str, &str)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (p, t) in files {
        z.start_file(*p, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(t.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

fn kobo(root: &Path) -> std::path::PathBuf {
    let ko = root.join(".adds/koreader");
    std::fs::create_dir_all(ko.join("plugins")).unwrap();
    std::fs::write(ko.join("settings.reader.lua"), "return {}").unwrap();
    ko
}

fn pick(asset: &[u8]) -> Pick {
    Pick {
        repo: "someone/zen.koplugin".into(),
        name: "zen".into(),
        kind: "plugin".into(),
        why: "Calm.".into(),
        license: "GPL-3.0".into(),
        release: Pin {
            tag: "v2.0".into(),
            asset: "zen.koplugin.zip".into(),
            sha256: sha(asset),
        },
    }
}

#[test]
fn a_pinned_install_takes_only_the_reviewed_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    let asset = zip(&[
        ("zen.koplugin/main.lua", "-- zen 2"),
        ("zen.koplugin/_meta.lua", "return { version = \"2.0\" }"),
    ]);
    let p = pick(&asset);
    let mut good = |url: &str| -> Result<Vec<u8>, String> {
        assert_eq!(
            url,
            "https://github.com/someone/zen.koplugin/releases/download/v2.0/zen.koplugin.zip"
        );
        Ok(asset.clone())
    };
    let prepared = prepare_pick(&ko, &p, &mut good).unwrap();
    assert_eq!(prepared.source.from, "someone/zen.koplugin");
    assert_eq!(prepared.source.release.as_deref(), Some("v2.0"));
    install(&ko, &prepared).unwrap();
    assert!(ko.join("plugins/zen.koplugin/main.lua").is_file());

    // The same release, but a byte changed upstream: refused, nothing written.
    let tmp2 = tempfile::tempdir().unwrap();
    let ko2 = kobo(tmp2.path());
    let mut tampered =
        |_: &str| -> Result<Vec<u8>, String> { Ok(zip(&[("zen.koplugin/main.lua", "-- evil")])) };
    let err = prepare_pick(&ko2, &p, &mut tampered).unwrap_err();
    assert!(err.contains("SHA-256"), "{err}");
    assert!(list(&ko2).plugins.is_empty());
}

#[test]
fn the_live_list_is_used_and_the_built_in_one_stands_in() {
    let live = Recommended {
        version: 1,
        updated: "2026-11-01".into(),
        plugins: vec![pick(b"x")],
    };
    let body = serde_json::to_vec(&live).unwrap();
    let mut ok = |url: &str| -> Result<Vec<u8>, String> {
        assert_eq!(url, RECOMMENDED_URL);
        Ok(body.clone())
    };
    assert_eq!(recommended(&mut ok), (live, true));
    let mut offline = |_: &str| -> Result<Vec<u8>, String> { Err("offline".into()) };
    assert_eq!(recommended(&mut offline), (built_in(), false));
    let mut junk = |_: &str| -> Result<Vec<u8>, String> { Ok(b"<html>".to_vec()) };
    assert!(!recommended(&mut junk).1);
}

#[test]
fn discover_reads_githubs_search() {
    let body = br#"{"total_count":2,"items":[
        {"full_name":"a/zen.koplugin","description":"Calm","stargazers_count":99,"pushed_at":"2026-10-30T10:00:00Z","license":{"spdx_id":"GPL-3.0"},"html_url":"https://github.com/a/zen.koplugin"},
        {"full_name":"b/raw","description":null,"stargazers_count":3,"pushed_at":"2026-01-02T00:00:00Z","license":null,"html_url":"https://github.com/b/raw"}
    ]}"#;
    let asked = Cell::new(String::new());
    let mut fetch = |url: &str| -> Result<Vec<u8>, String> {
        asked.set(url.to_owned());
        Ok(body.to_vec())
    };
    let found = search("calm ui", Sort::Updated, &mut fetch).unwrap();
    assert!(
        asked
            .take()
            .contains("q=calm+ui+topic%3Akoreader-plugin&sort=updated")
    );
    assert_eq!(found.len(), 2);
    assert_eq!(
        (
            found[0].repo.as_str(),
            found[0].stars,
            found[0].pushed.as_str()
        ),
        ("a/zen.koplugin", 99, "2026-10-30")
    );
    assert_eq!(found[1].license, None);
    let mut limited = |_: &str| -> Result<Vec<u8>, String> {
        Ok(br#"{"message":"API rate limit exceeded"}"#.to_vec())
    };
    assert!(
        search("", Sort::Stars, &mut limited)
            .unwrap_err()
            .contains("rate limit")
    );
}

#[test]
fn updates_are_the_new_pin_or_the_newest_release() {
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    // Two plugins installed from GitHub releases: one on the list, one not.
    for (repo, name, tag) in [
        ("someone/zen.koplugin", "zen", "v1.0"),
        ("other/tool.koplugin", "tool", "v3"),
    ] {
        let asset = zip(&[(&format!("{name}.koplugin/main.lua"), "--")]);
        let mut fetch = |url: &str| -> Result<Vec<u8>, String> {
            if url.contains("/releases/latest") {
                Ok(format!(r#"{{"tag_name":"{tag}","assets":[{{"name":"p.zip","browser_download_url":"https://dl/p.zip"}}]}}"#).into_bytes())
            } else {
                Ok(asset.clone())
            }
        };
        let p = deckle_core::koreader_plugins::prepare(
            &ko,
            &deckle_core::koreader_plugins::From::GitHub(repo.into()),
            &mut fetch,
        )
        .unwrap();
        install(&ko, &p).unwrap();
    }
    let list_ = Recommended {
        version: 1,
        updated: "x".into(),
        plugins: vec![pick(b"x")],
    };
    let mut gh = |url: &str| -> Result<Vec<u8>, String> {
        assert_eq!(
            url,
            "https://api.github.com/repos/other/tool.koplugin/releases/latest"
        );
        Ok(br#"{"tag_name":"v4"}"#.to_vec())
    };
    let mut u = updates(&list(&ko), &list_, &mut gh);
    u.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(u.len(), 2);
    assert_eq!(
        (u[0].name.as_str(), u[0].latest.as_str(), u[0].pinned),
        ("tool", "v4", false)
    );
    assert_eq!(
        (
            u[1].name.as_str(),
            u[1].installed.as_deref(),
            u[1].latest.as_str(),
            u[1].pinned
        ),
        ("zen", Some("v1.0"), "v2.0", true)
    );
}

#[test]
fn githubs_answers_are_reused_for_an_hour_but_downloads_arent_cached() {
    let tmp = tempfile::tempdir().unwrap();
    let calls = Cell::new(0);
    let mut f = cached(tmp.path().to_path_buf(), |url: &str| {
        calls.set(calls.get() + 1);
        Ok(url.as_bytes().to_vec())
    });
    let api = "https://api.github.com/search/repositories?q=x";
    assert_eq!(f(api).unwrap(), api.as_bytes());
    assert_eq!(f(api).unwrap(), api.as_bytes());
    assert_eq!(calls.get(), 1, "the second came from the cache");
    let dl = "https://github.com/a/b/releases/download/v1/a.zip";
    f(dl).unwrap();
    f(dl).unwrap();
    assert_eq!(calls.get(), 3, "downloads go to the network each time");
}
