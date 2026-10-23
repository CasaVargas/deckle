// SPDX-License-Identifier: GPL-3.0-or-later
//! Export folders (other readers design §7): `digest run` puts the paper in each `[[export]]`
//! folder, and needs no Kindle Previewer to do it.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn deckle(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home);
    c
}

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

/// A paper that needs no network (the horoscope is written here), with `extra` appended.
fn setup(extra: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let dir = config_dir(home.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        format!("sections = [\"horoscope\"]\n{extra}"),
    )
    .unwrap();
    home
}

fn ipad(home: &Path) -> PathBuf {
    home.join("iPad")
}

fn export_block(home: &Path) -> String {
    format!(
        "\n[[export]]\nname = \"iPad\"\ndir = {:?}\nkeep = 2\n",
        ipad(home).display().to_string()
    )
}

#[test]
fn without_previewer_the_paper_still_goes_to_the_export_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let home = setup_in(tmp.path());
    std::fs::create_dir_all(ipad(&home)).unwrap();
    for old in ["2026-10-01", "2026-10-02", "2026-10-03"] {
        std::fs::write(ipad(&home).join(format!("deckle-daily-{old}.epub")), b"old").unwrap();
    }
    let o = deckle(&home)
        .args([
            "--json",
            "digest",
            "run",
            "--date",
            "2026-10-23",
            "--previewer",
        ])
        .arg(home.join("no-previewer"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["run"], serde_json::Value::Null);
    let x = &v["exports"][0];
    assert_eq!(x["name"], "iPad");
    assert!(x.get("error").is_none(), "{x}");
    let paper = ipad(&home).join("deckle-daily-2026-10-23.epub");
    assert_eq!(x["written"][0], paper.display().to_string());
    assert_eq!(x["pruned"].as_array().unwrap().len(), 2);
    assert!(paper.is_file());
    assert!(ipad(&home).join("deckle-daily-2026-10-03.epub").is_file());
    let notes = v["build"]["notes"].to_string();
    assert!(notes.contains("export folders only"), "{notes}");
    // A color export builds a color edition beside the paper.
    let color = v["build"]["color_epub"].as_str().unwrap();
    assert!(color.ends_with("deckle-daily-2026-10-23.epub"), "{color}");
    assert_eq!(
        std::fs::read(&paper).unwrap(),
        std::fs::read(color).unwrap()
    );
}

fn setup_in(home: &Path) -> PathBuf {
    let dir = config_dir(home);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        format!("sections = [\"horoscope\"]\n{}", export_block(home)),
    )
    .unwrap();
    home.to_path_buf()
}

#[test]
fn the_text_report_names_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let home = setup_in(tmp.path());
    let o = deckle(&home)
        .args(["digest", "run", "--date", "2026-10-23", "--previewer"])
        .arg(home.join("no-previewer"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(
        text.contains(&format!("export iPad -> {}", ipad(&home).display())),
        "{text}"
    );
    assert!(!text.contains("outbox"), "{text}");
}

#[test]
fn without_previewer_or_an_export_the_run_still_fails() {
    let home = setup("");
    let o = deckle(home.path())
        .args(["digest", "run", "--date", "2026-10-23", "--previewer"])
        .arg(home.path().join("no-previewer"))
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("Previewer"));
}

#[test]
fn an_export_folder_that_cannot_be_reached_is_a_line_not_a_failure() {
    let home =
        setup("\n[[export]]\nname = \"iPad\"\ndir = \"/nonexistent-deckle/iCloud/Deckle Daily\"\n");
    let o = deckle(home.path())
        .args([
            "--json",
            "digest",
            "run",
            "--date",
            "2026-10-23",
            "--previewer",
        ])
        .arg(home.path().join("no-previewer"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(
        v["exports"][0]["error"]
            .as_str()
            .unwrap()
            .contains("no such folder")
    );
}

#[test]
fn digest_build_writes_a_bundle_beside_the_epub() {
    let home = setup("");
    let out = home.path().join("out");
    let o = deckle(home.path())
        .args([
            "--json",
            "digest",
            "build",
            "--bundle",
            "--date",
            "2026-10-23",
            "-o",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let bundle = out.join("deckle-daily-2026-10-23.deckle");
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("edition.json")).unwrap()).unwrap();
    assert_eq!(json["format"], "deckle-edition");
    assert_eq!(json["sections"][0]["kind"], "horoscope");
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["bundles"][0]["path"], bundle.display().to_string());
}
