// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader plugins on a reader's drive (docs/superpowers/specs/2026-10-31-koreader-plugins-design.md
//! part A).
use std::io::Write;
use std::path::Path;

use deckle_core::koreader_backup::{Options, Outcome, backup, restore};
use deckle_core::koreader_plugins::{From, data_dir, install, list, prepare, remove, rollback};

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A Kobo running KOReader: News downloader (built in), Zen (added, switched off), two patches
/// switched off as a folder.
fn kobo(root: &Path) -> std::path::PathBuf {
    let ko = root.join(".adds/koreader");
    write(
        &ko.join("plugins/newsdownloader.koplugin/_meta.lua"),
        "local _ = require(\"gettext\")\nreturn {\n    fullname = _(\"News downloader\"),\n    description = _([[Retrieves RSS and Atom news.]]),\n}\n",
    );
    write(
        &ko.join("plugins/newsdownloader.koplugin/main.lua"),
        "-- built in",
    );
    write(
        &ko.join("plugins/zen.koplugin/_meta.lua"),
        "local _ = require(\"gettext\")\nreturn { name = \"zen\", fullname = _(\"Zen UI\"), version = \"1.0\" }\n",
    );
    write(&ko.join("plugins/zen.koplugin/main.lua"), "-- zen 1.0");
    write(
        &ko.join("settings.reader.lua"),
        "-- we can read Lua syntax here!\nreturn {\n    [\"plugins_disabled\"] = {\n        [\"zen\"] = true,\n    },\n}\n",
    );
    write(&ko.join("patches/2-dim-status.lua"), "-- patch");
    write(&ko.join("patches/.patches_disabled"), "");
    ko
}

/// A zip of `files` (path, text).
fn zip(files: &[(&str, &str)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (p, t) in files {
        z.start_file(*p, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(t.as_bytes()).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn no_network(_: &str) -> Result<Vec<u8>, String> {
    Err("offline".into())
}

#[test]
fn every_plugin_and_patch_is_listed_with_its_state() {
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    assert_eq!(data_dir(tmp.path()).unwrap(), ko);
    let l = list(&ko);
    let rows: Vec<_> = l
        .plugins
        .iter()
        .map(|p| (p.name.as_str(), p.fullname.as_str(), p.builtin, p.enabled))
        .collect();
    assert_eq!(
        rows,
        [
            ("newsdownloader", "News downloader", true, true),
            ("zen", "Zen UI", false, false),
        ]
    );
    assert_eq!(
        l.plugins[0].description.as_deref(),
        Some("Retrieves RSS and Atom news.")
    );
    assert_eq!(l.plugins[1].version.as_deref(), Some("1.0"));
    assert_eq!(l.patches, ["2-dim-status.lua"]);
    assert!(l.patches_disabled);
}

#[test]
fn an_install_replaces_through_a_backup_and_rollback_puts_it_back() {
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    let archive = tmp.path().join("zen-2.0.zip");
    std::fs::write(
        &archive,
        zip(&[
            ("zen.koplugin/main.lua", "-- zen 2.0"),
            (
                "zen.koplugin/_meta.lua",
                "return { fullname = \"Zen UI\", version = \"2.0\" }",
            ),
            ("zen.koplugin/icons/a.svg", "<svg/>"),
        ]),
    )
    .unwrap();
    let from = From::parse(archive.to_str().unwrap()).unwrap();
    let p = prepare(&ko, &from, &mut no_network).unwrap();
    assert_eq!(
        (p.name.as_str(), p.version.as_deref()),
        ("zen", Some("2.0"))
    );
    assert_eq!(p.replaces.as_deref(), Some("zen"));
    let done = install(&ko, &p).unwrap();
    let main = std::fs::read_to_string(ko.join("plugins/zen.koplugin/main.lua")).unwrap();
    assert_eq!(main, "-- zen 2.0");
    assert!(ko.join("plugins/zen.koplugin/icons/a.svg").is_file());
    let kept = done.kept.unwrap();
    assert!(kept.starts_with(ko.join("deckle-backup/plugins")));
    assert_eq!(
        std::fs::read_to_string(kept.join("main.lua")).unwrap(),
        "-- zen 1.0"
    );
    // no temporary folder left behind, and the source remembered
    assert!(!ko.join("plugins/.zen.koplugin.tmp").exists());
    let listed = list(&ko);
    let zen = listed.plugins.iter().find(|x| x.name == "zen").unwrap();
    assert!(zen.source.as_ref().unwrap().from.ends_with("zen-2.0.zip"));

    rollback(&ko, "zen").unwrap();
    let main = std::fs::read_to_string(ko.join("plugins/zen.koplugin/main.lua")).unwrap();
    assert_eq!(main, "-- zen 1.0");
    // and the 2.0 that was rolled back is kept in turn
    rollback(&ko, "zen").unwrap();
    let main = std::fs::read_to_string(ko.join("plugins/zen.koplugin/main.lua")).unwrap();
    assert_eq!(main, "-- zen 2.0");
}

#[test]
fn unsafe_or_ambiguous_archives_are_refused_before_anything_is_written() {
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    let cases: [(&str, Vec<u8>, &str); 5] = [
        (
            "two.zip",
            zip(&[("a.koplugin/main.lua", "a"), ("b.koplugin/main.lua", "b")]),
            "2 plugins",
        ),
        (
            "nomain.zip",
            zip(&[("x.koplugin/_meta.lua", "return {}")]),
            "no main.lua",
        ),
        (
            "escape.zip",
            zip(&[("x.koplugin/main.lua", "x"), ("../../evil.lua", "x")]),
            "unsafe path",
        ),
        (
            "builtin.zip",
            zip(&[("kosync.koplugin/main.lua", "x")]),
            "built-in",
        ),
        ("notzip.zip", b"PK nope".to_vec(), "not a zip"),
    ];
    for (name, bytes, why) in cases {
        let path = tmp.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        let err = prepare(&ko, &From::Zip(path), &mut no_network).unwrap_err();
        assert!(err.contains(why), "{name}: {err}");
    }
    let big = tmp.path().join("big.zip");
    let filler = "0".repeat(21 * 1024 * 1024);
    std::fs::write(&big, zip(&[("x.koplugin/main.lua", &filler)])).unwrap();
    let err = prepare(&ko, &From::Zip(big), &mut no_network).unwrap_err();
    assert!(err.contains("20 MB"), "{err}");
    // nothing reached the reader
    let names: Vec<_> = list(&ko).plugins.into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["newsdownloader", "zen"]);
}

#[test]
fn a_github_repository_installs_its_newest_release_or_else_its_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    let asset = zip(&[
        ("appstore.koplugin/main.lua", "-- store"),
        (
            "appstore.koplugin/_meta.lua",
            "return { fullname = \"App Store\" }",
        ),
    ]);
    let mut asked = Vec::new();
    let mut fetch = |url: &str| -> Result<Vec<u8>, String> {
        asked.push(url.to_owned());
        match url {
            "https://api.github.com/repos/omer-faruq/appstore.koplugin/releases/latest" => Ok(
                br#"{"tag_name":"v1.4","assets":[{"name":"appstore.koplugin.zip","browser_download_url":"https://dl/appstore.zip"}]}"#.to_vec(),
            ),
            "https://dl/appstore.zip" => Ok(asset.clone()),
            _ => Err(format!("unexpected {url}")),
        }
    };
    let from = From::parse("https://github.com/omer-faruq/appstore.koplugin").unwrap();
    let p = prepare(&ko, &from, &mut fetch).unwrap();
    assert_eq!(p.name, "appstore");
    assert_eq!(p.source.release.as_deref(), Some("v1.4"));
    assert_eq!(p.source.from, "omer-faruq/appstore.koplugin");
    install(&ko, &p).unwrap();
    assert!(ko.join("plugins/appstore.koplugin/main.lua").is_file());

    // No release: the default branch, whose top folder is the plugin itself.
    let branch = zip(&[
        ("someone-zen.koplugin-abc123/main.lua", "-- zen main"),
        ("someone-zen.koplugin-abc123/_meta.lua", "return {}"),
    ]);
    let mut fetch = |url: &str| -> Result<Vec<u8>, String> {
        match url {
            "https://api.github.com/repos/someone/zen.koplugin/releases/latest" => {
                Err("404".into())
            }
            "https://api.github.com/repos/someone/zen.koplugin/zipball" => Ok(branch.clone()),
            _ => Err(format!("unexpected {url}")),
        }
    };
    let p = prepare(
        &ko,
        &From::GitHub("someone/zen.koplugin".into()),
        &mut fetch,
    )
    .unwrap();
    assert_eq!(p.name, "zen");
    assert_eq!(p.source.release, None);
}

#[test]
fn removing_keeps_a_copy_and_never_touches_a_built_in() {
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    assert!(
        remove(&ko, "newsdownloader")
            .unwrap_err()
            .contains("built-in")
    );
    assert!(
        remove(&ko, "nothing")
            .unwrap_err()
            .contains("no such plugin")
    );
    let done = remove(&ko, "zen").unwrap();
    assert!(!ko.join("plugins/zen.koplugin").exists());
    assert!(done.kept.unwrap().join("main.lua").is_file());
    rollback(&ko, "zen").unwrap();
    assert!(ko.join("plugins/zen.koplugin/main.lua").is_file());
}

#[test]
fn a_backup_carries_added_plugins_and_patches_but_not_built_ins() {
    let tmp = tempfile::tempdir().unwrap();
    kobo(&tmp.path().join("Kobo"));
    let saved = backup(
        &tmp.path().join("Kobo"),
        &tmp.path().join("b"),
        "Kobo",
        "2026-10-31 090000",
    )
    .unwrap();
    let paths: Vec<String> = saved
        .manifest
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    assert!(
        paths
            .iter()
            .any(|p| p == ".adds/koreader/plugins/zen.koplugin/main.lua"),
        "{paths:?}"
    );
    assert!(
        paths
            .iter()
            .any(|p| p == ".adds/koreader/patches/2-dim-status.lua"),
        "{paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.contains("newsdownloader")),
        "{paths:?}"
    );

    // onto a fresh Kobo: only with --settings
    let fresh = tmp.path().join("New");
    std::fs::create_dir_all(fresh.join(".adds/koreader/plugins")).unwrap();
    let r = restore(
        &saved.dir,
        &fresh,
        Options {
            settings: false,
            force: false,
        },
    )
    .unwrap();
    assert!(!r.settings.iter().any(|i| i.book.starts_with("plugins/")));
    assert!(!fresh.join(".adds/koreader/plugins/zen.koplugin").exists());
    let r = restore(
        &saved.dir,
        &fresh,
        Options {
            settings: true,
            force: false,
        },
    )
    .unwrap();
    let zen = r
        .settings
        .iter()
        .find(|i| i.book == "plugins/zen.koplugin")
        .unwrap();
    assert_eq!(zen.outcome, Outcome::Restored);
    assert!(
        fresh
            .join(".adds/koreader/plugins/zen.koplugin/main.lua")
            .is_file()
    );
    assert!(
        fresh
            .join(".adds/koreader/patches/2-dim-status.lua")
            .is_file()
    );
}

#[test]
fn the_deckle_plugin_installs_with_its_settings_and_keeps_the_owners() {
    use deckle_core::koreader::lua::{self, Value};
    use deckle_core::koreader_plugins::{DeckleSettings, deckle_plugin, write_deckle_settings};
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    let p = deckle_plugin(&ko);
    assert_eq!(p.name, "deckle");
    install(&ko, &p).unwrap();
    let meta = std::fs::read_to_string(ko.join("plugins/deckle.koplugin/_meta.lua")).unwrap();
    assert!(meta.contains(env!("CARGO_PKG_VERSION")), "{meta}");
    assert!(!meta.contains("DECKLE_VERSION"));
    for f in ["main.lua", "logic.lua"] {
        assert!(ko.join("plugins/deckle.koplugin").join(f).is_file(), "{f}");
    }
    let listed = list(&ko);
    let row = listed.plugins.iter().find(|x| x.name == "deckle").unwrap();
    assert_eq!(row.fullname, "Deckle");
    assert!(!row.builtin);

    // The owner already chose to keep 7 and the plugin has a fetched list; a new install
    // changes the server and the key, and keeps those.
    write(
        &ko.join("settings/deckle.lua"),
        "return { [\"keep\"] = 7, [\"fetched\"] = { [1] = \"/mnt/onboard/Deckle/a.epub\" }, [\"key\"] = \"old\" }",
    );
    let s = DeckleSettings {
        server: "http://192.168.1.5:47220".into(),
        username: "deckle \"home\"".into(),
        key: "2ab96390c7dbe3439de74d0c9b0b1767".into(),
        format: "kepub".into(),
    };
    let path = write_deckle_settings(&ko, &s).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let v = lua::parse(&text).unwrap();
    let get = |k: &str| v.get(k).cloned();
    assert_eq!(
        get("server"),
        Some(Value::Str("http://192.168.1.5:47220".into()))
    );
    assert_eq!(get("username"), Some(Value::Str("deckle \"home\"".into())));
    assert_eq!(
        get("key"),
        Some(Value::Str("2ab96390c7dbe3439de74d0c9b0b1767".into()))
    );
    assert_eq!(get("format"), Some(Value::Str("kepub".into())));
    assert_eq!(get("keep"), Some(Value::Num(7.0)), "the owner's keep");
    assert_eq!(get("auto"), Some(Value::Bool(true)));
    assert_eq!(get("wifi_on_wake"), Some(Value::Bool(false)));
    let Some(Value::Table(fetched)) = get("fetched") else {
        panic!("fetched kept: {text}")
    };
    assert_eq!(fetched.len(), 1);
    assert!(!text.contains("hunter2"));
}

#[test]
fn switching_waits_for_the_deckle_plugin_and_never_edits_koreaders_settings() {
    use deckle_core::koreader_plugins::{
        PENDING, deckle_plugin, pending, set_enabled, set_patches,
    };
    let tmp = tempfile::tempdir().unwrap();
    let ko = kobo(tmp.path());
    let before = std::fs::read_to_string(ko.join("settings.reader.lua")).unwrap();

    let err = set_enabled(&ko, "zen", true).unwrap_err();
    assert!(err.contains("Deckle plugin"), "{err}");
    assert!(!ko.join(PENDING).exists());

    install(&ko, &deckle_plugin(&ko)).unwrap();
    set_enabled(&ko, "zen", true).unwrap();
    set_enabled(&ko, "newsdownloader", false).unwrap();
    assert_eq!(
        pending(&ko).into_iter().collect::<Vec<_>>(),
        [
            ("newsdownloader".to_owned(), false),
            ("zen".to_owned(), true)
        ]
    );
    let l = list(&ko);
    let zen = l.plugins.iter().find(|p| p.name == "zen").unwrap();
    assert_eq!((zen.enabled, zen.pending), (false, Some(true)));
    // Switching back to how it is clears the wait; the last one removes the file.
    set_enabled(&ko, "zen", false).unwrap();
    set_enabled(&ko, "newsdownloader", true).unwrap();
    assert!(!ko.join(PENDING).exists());
    assert!(
        set_enabled(&ko, "nothing", true)
            .unwrap_err()
            .contains("no such plugin")
    );
    assert!(
        set_enabled(&ko, "deckle", false)
            .unwrap_err()
            .contains("itself")
    );
    assert_eq!(
        std::fs::read_to_string(ko.join("settings.reader.lua")).unwrap(),
        before
    );

    // Patches are a marker KOReader only reads: set directly.
    set_patches(&ko, true).unwrap();
    assert!(!list(&ko).patches_disabled);
    set_patches(&ko, false).unwrap();
    assert!(list(&ko).patches_disabled);
}
