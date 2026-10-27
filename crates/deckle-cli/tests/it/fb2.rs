// SPDX-License-Identifier: GPL-3.0-or-later
#[test]
#[cfg(unix)]
fn cli_runs_fb2_and_zipped_fb2() {
    use assert_cmd::Command;
    use std::path::PathBuf;
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fb2");
    let fake =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../deckle-core/tests/fake_previewer/ok.sh");
    for (source, title) in [
        ("mini.fb2", "The Deckle FB2 Mini"),
        ("mini.fb2.zip", "The Deckle FB2 Mini"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let res = Command::cargo_bin("deckle")
            .unwrap()
            .env("HOME", temp.path())
            .env("APPDATA", temp.path())
            .env("XDG_CONFIG_HOME", temp.path())
            .args(["--json", "run"])
            .arg(fixtures.join(source))
            .arg("--to")
            .arg(temp.path().join("out"))
            .arg("--previewer")
            .arg(&fake)
            .output()
            .unwrap();
        assert!(
            res.status.success(),
            "{}",
            String::from_utf8_lossy(&res.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
        assert_eq!(report[0]["info"]["title"], title);
        assert!(report[0]["delivered"].as_str().unwrap().ends_with(".kfx"));
    }
}
