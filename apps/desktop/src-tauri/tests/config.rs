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
    for t in ["app", "dmg", "nsis", "msi"] {
        assert!(targets.contains(&t), "missing bundle target {t}");
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
fn no_updater_plugin() {
    // design §12: no auto-updater (it phones home); the About section links to Releases.
    assert!(
        config()
            .get("plugins")
            .and_then(|p| p.get("updater"))
            .is_none()
    );
    assert!(!include_str!("../Cargo.toml").contains("tauri-plugin-updater"));
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
