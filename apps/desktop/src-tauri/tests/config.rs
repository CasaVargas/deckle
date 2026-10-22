// SPDX-License-Identifier: GPL-3.0-or-later
//! Guards on the shipping config: the things a release must not lose by accident.

fn config() -> serde_json::Value {
    serde_json::from_str(include_str!("../tauri.conf.json")).unwrap()
}

#[test]
fn bundle_targets_macos_14_and_installs_per_user_on_windows() {
    let c = config();
    assert_eq!(c["bundle"]["macOS"]["minimumSystemVersion"], "14.0");
    assert_eq!(c["bundle"]["windows"]["nsis"]["installMode"], "currentUser");
    assert_eq!(c["bundle"]["license"], "GPL-3.0-or-later");
    let targets: Vec<&str> = c["bundle"]["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t.as_str())
        .collect();
    for t in ["app", "dmg", "nsis"] {
        assert!(targets.contains(&t), "missing bundle target {t}");
    }
    // NSIS is the one Windows installer: it's what the updater installs.
    assert!(!targets.contains(&"msi"), "the MSI target is dropped");
    for t in ["appimage", "deb"] {
        assert!(!targets.contains(&t), "Linux bundles are dropped: {t}");
    }
}

#[test]
fn csp_allows_data_cover_thumbnails_and_nothing_remote() {
    let csp = config()["app"]["security"]["csp"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(csp.contains("img-src 'self' data:"), "{csp}");
    assert!(!csp.contains("http"), "{csp}");
}

#[test]
fn updater_points_at_latest_release_and_is_signed() {
    // auto-update design §4: the Latest GitHub release's latest.json, minisign-verified.
    let c = config();
    let u = &c["plugins"]["updater"];
    assert_eq!(
        u["endpoints"],
        serde_json::json!([
            "https://github.com/CasaVargas/deckle/releases/latest/download/latest.json"
        ])
    );
    assert!(!u["pubkey"].as_str().unwrap_or("").is_empty());
    assert_eq!(u["windows"]["installMode"], "passive");
    assert_eq!(c["bundle"]["createUpdaterArtifacts"], true);
    let cargo = include_str!("../Cargo.toml");
    assert!(cargo.contains("tauri-plugin-updater"));
    assert!(cargo.contains("tauri-plugin-process"));
}

#[test]
fn versions_agree() {
    let c = config();
    let pkg: serde_json::Value = serde_json::from_str(include_str!("../../package.json")).unwrap();
    assert_eq!(c["version"], pkg["version"]);
    let ws = include_str!("../../../../Cargo.toml");
    assert!(
        ws.contains(&format!("version = \"{}\"", c["version"].as_str().unwrap())),
        "workspace version differs from tauri.conf.json"
    );
}
