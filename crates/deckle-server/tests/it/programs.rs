// SPDX-License-Identifier: GPL-3.0-or-later
//! The web UI can't choose a program for the server to run: the Previewer's location and
//! calibredb stay as the server's own files say.

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::{app, login, post, send};

/// A script that leaves `marker` behind when anything runs it.
#[cfg(unix)]
fn trap(dir: &std::path::Path, marker: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("not-previewer.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\necho 9.9.9\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[cfg(unix)]
#[tokio::test]
async fn set_settings_keeps_the_saved_previewer() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("ran");
    let script = trap(tmp.path(), &marker);
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;

    let r = send(&app, post("/api/invoke/get_settings", "{}", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let mut settings: Value = serde_json::from_str(&r.body).unwrap();
    settings["previewer_path"] = json!(script.to_string_lossy());
    // Another change in the same save still lands.
    settings["llm_model"] = json!("kept-model");
    let body = json!({ "settings": settings }).to_string();
    let r = send(&app, post("/api/invoke/set_settings", &body, Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);

    let r = send(
        &app,
        post("/api/invoke/previewer_status", "{}", Some(&cookie)),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(!marker.exists(), "the script ran: {}", r.body);

    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join("settings.json")).unwrap())
            .unwrap();
    assert!(saved["previewer_path"].is_null(), "{saved}");
    assert_eq!(saved["llm_model"], "kept-model", "{saved}");
}

#[tokio::test]
async fn a_saved_previewer_survives_a_save_from_the_browser() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;
    let r = send(&app, post("/api/invoke/get_settings", "{}", Some(&cookie))).await;
    let mut settings: Value = serde_json::from_str(&r.body).unwrap();
    settings["previewer_path"] = json!("/owner/chose/this");
    std::fs::write(
        tmp.path().join("settings.json"),
        serde_json::to_string(&settings).unwrap(),
    )
    .unwrap();

    settings["previewer_path"] = json!("/browser/chose/this");
    let body = json!({ "settings": settings }).to_string();
    let r = send(&app, post("/api/invoke/set_settings", &body, Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(saved["previewer_path"], "/owner/chose/this");
}

#[tokio::test]
async fn calibredb_cannot_be_chosen_from_the_browser() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;
    for calibredb in ["/tmp/evil", ""] {
        let body = json!({ "patch": { "calibredb": calibredb } }).to_string();
        let r = send(
            &app,
            post("/api/invoke/calibre_writeback_set", &body, Some(&cookie)),
        )
        .await;
        assert_eq!(
            r.status,
            StatusCode::BAD_REQUEST,
            "{calibredb:?}: {}",
            r.body
        );
        assert!(r.body.contains("calibre-writeback.json"), "{}", r.body);
    }
}
