// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle koreader backup` from a PocketBook found through `DECKLE_MOUNTS`, and `restore` onto
//! a folder standing in for a Kobo, where the book sits in another folder (KOReader hub design
//! §D).

use std::path::Path;

use assert_cmd::Command;

fn deckle(home: &Path, mounts: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("DECKLE_MOUNTS", mounts)
        .env("PATH", "/nonexistent");
    c
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_pocketbooks_koreader_is_backed_up_and_restored_beside_the_book_elsewhere() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let pb = tmp.path().join("PB632");
    std::fs::create_dir_all(pb.join("system/explorer-3")).unwrap();
    std::fs::create_dir_all(pb.join("system/config")).unwrap();
    let book: Vec<u8> = (0..5000u32).map(|i| (i % 241) as u8).collect();
    write(&pb.join("Books/Walden.epub"), &book);
    let md5 = deckle_core::koreader::partial_md5(&pb.join("Books/Walden.epub")).unwrap();
    let sidecar = format!(
        "return {{\n    [\"partial_md5_checksum\"] = \"{md5}\",\n    [\"percent_finished\"] = 0.5,\n}}\n"
    );
    write(&pb.join("Books/Walden.sdr/metadata.epub.lua"), &sidecar);
    write(
        &pb.join("applications/koreader/settings.reader.lua"),
        "return {}",
    );

    let o = deckle(&home, &pb)
        .args(["koreader", "backup"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(
        text.contains("1 sidecars and 2 files from PocketBook"),
        "{text}"
    );
    let base = home.join("Deckle/KOReader backups/PocketBook");
    let stamps: Vec<_> = std::fs::read_dir(&base).unwrap().flatten().collect();
    assert_eq!(stamps.len(), 1);
    let dir = stamps[0].path();
    // `YYYY-MM-DD HHMMSS`
    assert_eq!(stamps[0].file_name().len(), 17);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["device"], "PocketBook");
    assert_eq!(manifest["sidecars"][0]["partial_md5"], md5.as_str());
    assert_eq!(manifest["sidecars"][0]["book"], "Books/Walden.epub");

    // A Kobo copy with Walden under another folder.
    let kobo = tmp.path().join("kobo");
    write(&kobo.join("Library/Walden.epub"), &book);
    std::fs::create_dir_all(kobo.join(".adds/koreader")).unwrap();
    let o = deckle(&home, &pb)
        .args(["--json", "koreader", "restore"])
        .arg(&dir)
        .arg("--to")
        .arg(&kobo)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["sidecars"][0]["outcome"], "restored");
    assert_eq!(r["sidecars"][0]["by"], "md5");
    assert_eq!(r["sidecars"][0]["to"], "Library/Walden.epub");
    assert_eq!(r["settings"], serde_json::json!([]));
    assert_eq!(
        std::fs::read_to_string(kobo.join("Library/Walden.sdr/metadata.epub.lua")).unwrap(),
        sidecar
    );
    // Settings only with --settings.
    assert!(!kobo.join(".adds/koreader/settings.reader.lua").exists());
    let o = deckle(&home, &pb)
        .args(["koreader", "restore", "--settings", "--to"])
        .arg(&kobo)
        .arg(&dir)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(
        text.contains("restored       Library/Walden.epub (found by MD5"),
        "{text}"
    );
    assert!(text.contains("2 restored, 0 kept (newer)"), "{text}");
    assert!(kobo.join(".adds/koreader/settings.reader.lua").is_file());
}

#[test]
fn restore_with_no_reader_connected_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let o = deckle(tmp.path(), &tmp.path().join("no-drives"))
        .args(["koreader", "restore"])
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("no e-reader connected"));
}

/// A zip of `files` (path, text).
fn plugin_zip(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (p, t) in files {
        z.start_file(*p, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(t.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

#[test]
fn plugins_are_listed_installed_updated_removed_and_rolled_back() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let kobo = tmp.path().join("KOBOeReader");
    let ko = kobo.join(".adds/koreader");
    write(&ko.join("plugins/kosync.koplugin/main.lua"), "-- built in");
    write(&ko.join("settings.reader.lua"), "return {}");
    let mounts = tmp.path().join("no-mounts");
    let run = |args: &[&str]| {
        deckle(&home, &mounts)
            .args(["--json", "koreader", "plugins", "--from"])
            .arg(&kobo)
            .args(args)
            .output()
            .unwrap()
    };
    let json = |o: &std::process::Output| -> serde_json::Value {
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    };

    let l = json(&run(&["list"]));
    assert_eq!(l["plugins"][0]["name"], "kosync");
    assert_eq!(l["plugins"][0]["builtin"], true);

    let v1 = tmp.path().join("zen-1.zip");
    std::fs::write(
        &v1,
        plugin_zip(&[
            ("zen.koplugin/main.lua", "-- 1"),
            (
                "zen.koplugin/_meta.lua",
                "return { fullname = \"Zen\", version = \"1\" }",
            ),
        ]),
    )
    .unwrap();
    // No terminal to ask on, and no --yes: nothing written.
    let o = run(&["install", v1.to_str().unwrap()]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("--yes"));
    assert!(!ko.join("plugins/zen.koplugin").exists());
    json(&run(&["install", v1.to_str().unwrap(), "--yes"]));
    assert!(ko.join("plugins/zen.koplugin/main.lua").is_file());

    // `update` goes back to where it came from: overwrite the zip with version 2.
    std::fs::write(&v1, plugin_zip(&[("zen.koplugin/main.lua", "-- 2")])).unwrap();
    let done = json(&run(&["update", "zen", "--yes"]));
    assert!(done["kept"].is_string());
    assert_eq!(
        std::fs::read_to_string(ko.join("plugins/zen.koplugin/main.lua")).unwrap(),
        "-- 2"
    );

    json(&run(&["remove", "zen"]));
    assert!(!ko.join("plugins/zen.koplugin").exists());
    json(&run(&["rollback", "zen"]));
    assert_eq!(
        std::fs::read_to_string(ko.join("plugins/zen.koplugin/main.lua")).unwrap(),
        "-- 2"
    );

    let o = run(&["remove", "kosync"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("built-in"));
}
