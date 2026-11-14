// SPDX-License-Identifier: GPL-3.0-or-later
//! In the web UI every path a command takes, given now or saved before, lies in the server's
//! folders (web UI path design 6.1).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use deckle_app::commands::AppState;
use deckle_app::dispatch::{COMMANDS, Ctx, dispatch};
use deckle_app::events::NoEvents;
use serde_json::json;

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    outside: PathBuf,
    ctx: Ctx,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let root = base.join("data");
    let outside = base.join("private");
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.epub"), b"not yours").unwrap();
    let ctx = Ctx {
        state: AppState {
            settings_file: root.join("config/settings.json"),
            devices_file: root.join("config/devices.json"),
            preload: Mutex::new(Vec::new()),
            preload_target: Mutex::new(None),
            start_tab: "books",
        },
        events: Arc::new(NoEvents),
        exe: PathBuf::from("/opt/deckle/deckle"),
        web: true,
        roots: Some(vec![root.clone()]),
    };
    Fixture {
        _tmp: tmp,
        root,
        outside,
        ctx,
    }
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn refused(f: &Fixture, command: &str, args: serde_json::Value) {
    let e = dispatch(&f.ctx, command, args).expect_err(command);
    assert!(
        e.contains("outside the folders this server may use"),
        "{command}: {e}"
    );
}

#[test]
fn commands_refuse_paths_outside_the_roots_and_touch_nothing() {
    let f = fixture();
    let secret = s(&f.outside.join("secret.epub"));
    let folder = json!({ "kind": "folder", "path": s(&f.outside) });
    refused(&f, "device_books", json!({ "target": folder }));
    refused(
        &f,
        "device_remove",
        json!({ "target": folder, "file": "secret.epub" }),
    );
    refused(
        &f,
        "device_backup",
        json!({ "target": { "kind": "folder", "path": s(&f.root) }, "into": s(&f.outside) }),
    );
    refused(&f, "inspect_books", json!({ "paths": [secret] }));
    refused(&f, "book_preflight", json!({ "path": secret }));
    refused(&f, "library_read_details", json!({ "dir": s(&f.outside) }));
    refused(
        &f,
        "wireless_queue",
        json!({ "reader": "x", "paths": [secret] }),
    );
    refused(
        &f,
        "words_anki",
        json!({ "target": folder, "to": s(&f.root.join("w.csv")) }),
    );
    refused(
        &f,
        "words_anki",
        json!({ "target": { "kind": "folder", "path": s(&f.root) }, "to": s(&f.outside.join("w.csv")) }),
    );
    let book = json!({ "id": "1", "path": secret, "cde": null });
    refused(
        &f,
        "run_books",
        json!({ "books": [book], "target": { "kind": "folder", "path": s(&f.root) } }),
    );
    refused(
        &f,
        "plan_books",
        json!({ "books": [book], "target": { "kind": "folder", "path": s(&f.root) } }),
    );
    refused(
        &f,
        "cookbook_preview",
        json!({ "target": s(&f.outside.join("pie.html")) }),
    );
    refused(
        &f,
        "import_articles",
        json!({ "targets": [s(&f.outside.join("a.html"))] }),
    );
    refused(&f, "koreader_status", json!({ "folder": s(&f.outside) }));
    refused(
        &f,
        "koreader_preview",
        json!({ "folder": null, "source": s(&f.outside) }),
    );
    refused(
        &f,
        "library_set",
        json!({ "root": s(&f.outside.join("Library")), "autoAdd": true }),
    );
    // `..` and links that lead out don't count as inside.
    refused(
        &f,
        "inspect_books",
        json!({ "paths": [format!("{}/../private/secret.epub", s(&f.root))] }),
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&f.outside, f.root.join("way-out")).unwrap();
        refused(
            &f,
            "inspect_books",
            json!({ "paths": [s(&f.root.join("way-out/secret.epub"))] }),
        );
    }
    assert!(f.outside.join("secret.epub").exists());
    assert!(!f.outside.join("Library").exists());
    assert!(!f.outside.join("w.csv").exists());
}

#[test]
fn a_path_inside_the_roots_is_used() {
    let f = fixture();
    std::fs::write(f.root.join("book.epub"), b"PK").unwrap();
    // Reaches the command, which says what it thinks of the file.
    let r = dispatch(
        &f.ctx,
        "inspect_books",
        json!({ "paths": [s(&f.root.join("book.epub"))] }),
    );
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn nothing_is_written_into_the_settings_folder() {
    let f = fixture();
    let e = dispatch(
        &f.ctx,
        "words_anki",
        json!({
            "target": { "kind": "folder", "path": s(&f.root) },
            "to": s(&f.root.join("config/settings.json")),
        }),
    )
    .unwrap_err();
    assert!(e.contains("settings folder"), "{e}");
    let e = dispatch(
        &f.ctx,
        "device_books",
        json!({ "target": { "kind": "folder", "path": s(&f.root.join("config")) } }),
    )
    .unwrap_err();
    assert!(e.contains("settings folder"), "{e}");
}

#[test]
fn a_daily_save_with_a_new_outside_folder_is_refused() {
    let f = fixture();
    let mut cfg = deckle_digest::Config::parse(deckle_digest::config::STARTER).unwrap();
    cfg.reading.inbox = s(&f.outside);
    let e = dispatch(
        &f.ctx,
        "daily_save",
        json!({ "config": cfg, "paper": null }),
    )
    .unwrap_err();
    assert!(e.contains("[reading] inbox"), "{e}");
    let mut cfg = deckle_digest::Config::parse(deckle_digest::config::STARTER).unwrap();
    cfg.morgue.folder = s(&f.outside);
    let e = dispatch(
        &f.ctx,
        "daily_save",
        json!({ "config": cfg, "paper": null }),
    )
    .unwrap_err();
    assert!(e.contains("[morgue] folder"), "{e}");
}

/// Commands that take no path from the browser. A new command fails here until someone has
/// looked at its arguments (nested ones too) and put it in one list or the other.
const NO_PATHS: &[&str] = &[
    "app_version",
    "bookmarklet",
    "bookmarklet_page_open",
    "calibre_create_columns",
    "calibre_set_password",
    "calibre_writeback_confirm",
    "calibre_writeback_set",
    "calibre_writeback_status",
    "calibre_writeback_write",
    "capabilities",
    "cookbook_book",
    "cookbook_delivery",
    "cookbook_illustrate",
    "cookbook_illustrate_stop",
    "cookbook_preview_book",
    "cookbook_remove",
    "cookbook_save_config",
    "cookbook_send_now",
    "cookbook_set_kindle",
    "cookbook_update",
    "cover_candidates",
    "cover_fetch",
    "daily_day_preview",
    "daily_next",
    "daily_next_refresh",
    "daily_run",
    "daily_setup",
    "daily_state",
    "edition_preview",
    "email_set_password",
    "email_status",
    "email_test",
    "geocode",
    "get_settings",
    "highlights_status",
    "interests_accept",
    "interests_dismiss",
    "koreader_store_list",
    "koreader_store_readme",
    "koreader_store_search",
    "library_catalog",
    "library_search",
    "library_sent_kindles",
    "library_sources",
    "library_status",
    "library_watch_scan",
    "list_devices",
    "llm_check",
    "llm_pull",
    "llm_pull_stop",
    "llm_recommended",
    "masthead_missing",
    "morgue_month",
    "morgue_open",
    "morgue_search",
    "morgue_send",
    "morgue_show",
    "name_kindle",
    "paper_create",
    "papers_list",
    "press_setup",
    "press_stop",
    "previewer_status",
    "puzzles_bundle_targets",
    "puzzles_kinds",
    "puzzles_list",
    "puzzles_sources",
    "puzzles_sunday_get",
    "puzzles_sunday_set",
    "reader_server_set_password",
    "reader_server_status",
    "readers_edit",
    "readers_last_backup",
    "readers_library",
    "readers_list",
    "readers_remove",
    "readership_apply",
    "readership_dismiss",
    "readership_report",
    "readership_set",
    "reading_list",
    "reading_remove",
    "reading_save",
    "recipe_bookmarklet",
    "schedule_get",
    "schedule_set",
    "set_device_options",
    "set_illustrations_key",
    "set_kindle_paper",
    "set_settings",
    "shelf_forget",
    "shelf_set",
    "shelf_status",
    "shelves_apply",
    "shelves_kobo_ok",
    "shelves_set",
    "shelves_status",
    "start_tab",
    "subscription_forget",
    "subscription_set_password",
    "subscription_test",
    "subscriptions_status",
    "take_preloaded_books",
    "take_preloaded_target",
    "wireless_cancel",
    "wireless_remove",
    "wireless_set_password",
    "wireless_status",
];

/// Commands that take a path (or a target folder, or a saved folder) and check it.
const CHECKED: &[&str] = &[
    "bind_books",
    "book_preflight",
    "cookbook_add",
    "cookbook_preview",
    "daily_save",
    "device_backup",
    "device_books",
    "device_edit_source",
    "device_put_back",
    "device_rebuild",
    "device_remove",
    "device_remove_many",
    "device_restore",
    "device_upgrades",
    "highlights_set",
    "highlights_write",
    "import_articles",
    "inspect_books",
    "koreader_install",
    "koreader_preview",
    "koreader_remove",
    "koreader_rollback",
    "koreader_set_enabled",
    "koreader_set_patches",
    "koreader_status",
    "koreader_store_updates",
    "koreader_update",
    "library_covers",
    "library_fix_details",
    "library_read_details",
    "library_set",
    "library_sources_set",
    "library_suggest_details",
    "library_watch_set",
    "plan_books",
    "puzzles_make",
    "puzzles_preview",
    "puzzles_send",
    "readers_backup",
    "reveal",
    "run_books",
    "wireless_queue",
    "words_anki",
];

#[test]
fn every_command_has_been_looked_at_for_paths() {
    let unreviewed: Vec<&&str> = COMMANDS
        .iter()
        .filter(|c| !NO_PATHS.contains(c) && !CHECKED.contains(c))
        .collect();
    assert!(
        unreviewed.is_empty(),
        "new commands: check their arguments for paths (in the web UI they must go through \
         UiPath or Ctx::path), then add them to NO_PATHS or CHECKED: {unreviewed:?}"
    );
    let gone: Vec<&&str> = NO_PATHS
        .iter()
        .chain(CHECKED)
        .filter(|c| !COMMANDS.contains(c))
        .collect();
    assert!(gone.is_empty(), "no such commands any more: {gone:?}");
}
