// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle digest lint` on a saved page (Books and Kindle design §9).

use assert_cmd::Command;

#[test]
fn lint_shows_what_a_saved_page_loses_and_its_photo_credits() {
    let tmp = tempfile::tempdir().unwrap();
    let words = "The council met on Tuesday night to talk about the old rail yard and its future. "
        .repeat(20);
    let html = format!(
        "<html lang=\"en\"><head><title>Market</title></head><body><article><h1>Market</h1><p>{words}</p>\
         <figure><img src=\"yard.jpg\" alt=\"The yard\"/><figcaption>The yard. Photo: Jane Doe</figcaption></figure>\
         <p>Advertisement</p><p>{words}</p></article></body></html>"
    );
    let page = tmp.path().join("market.html");
    std::fs::write(&page, html).unwrap();
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .env("HOME", tmp.path())
        .args(["--json", "digest", "lint"])
        .arg(&page)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dropped"], serde_json::json!(["Advertisement"]));
    assert_eq!(v["photos"][0]["caption"], "The yard.");
    assert_eq!(v["photos"][0]["credit"], "Jane Doe");
    assert_eq!(v["photos"][0]["alt"], "The yard");
}
