// SPDX-License-Identifier: GPL-3.0-or-later
//! The dispatch table: what the web UI calls, by the names and argument keys api.ts sends.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use deckle_app::capabilities::capabilities;
use deckle_app::commands::AppState;
use deckle_app::dispatch::{COMMANDS, Ctx, dispatch};
use deckle_app::events::NoEvents;
use serde_json::{Value, json};

fn ctx_in(dir: &Path, web: bool) -> Ctx {
    Ctx {
        state: AppState {
            settings_file: dir.join("settings.json"),
            devices_file: dir.join("devices.json"),
            preload: Mutex::new(vec!["/books/a.epub".into()]),
            preload_target: Mutex::new(None),
            start_tab: "books",
        },
        events: Arc::new(NoEvents),
        exe: PathBuf::from("/opt/deckle/deckle"),
        web,
    }
}

/// Every name in `invoke<…>("name"` and `invoke("name"` in `src`.
fn invoked(src: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (i, _) in src.match_indices("invoke") {
        let mut rest = &src[i + "invoke".len()..];
        // Skip the result type, which may hold `<`, `>` and quotes of its own.
        if rest.starts_with('<') {
            let mut depth = 0;
            let end = rest.char_indices().find(|&(_, c)| {
                match c {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    _ => {}
                }
                depth == 0
            });
            let Some((end, _)) = end else { continue };
            rest = &rest[end + 1..];
        }
        if let Some(name) = rest.strip_prefix("(\"").and_then(|r| r.split('"').next()) {
            names.insert(name.to_owned());
        }
    }
    names
}

#[test]
fn every_api_ts_command_is_dispatchable() {
    let api = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/api.ts");
    let names = invoked(&std::fs::read_to_string(api).unwrap());
    let table: BTreeSet<String> = COMMANDS.iter().map(|c| c.to_string()).collect();
    assert_eq!(names, table);
    let mut sorted = COMMANDS.to_vec();
    sorted.sort_unstable();
    assert_eq!(COMMANDS, sorted.as_slice(), "COMMANDS is kept sorted");
}

#[test]
fn the_api_ts_parser_reads_both_call_shapes() {
    let src = r#"a: () => invoke<"books" | "daily">("start_tab"), b: () => invoke("cookbook_illustrate_stop"), c: invoke<[string, string | null][]>("library_covers", { paths })"#;
    let want: BTreeSet<String> = ["start_tab", "cookbook_illustrate_stop", "library_covers"]
        .map(String::from)
        .into();
    assert_eq!(invoked(src), want);
}

#[test]
fn dispatch_app_version_returns_string() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [Value::Null, json!({})] {
        let v = dispatch(&ctx_in(tmp.path(), false), "app_version", args).unwrap();
        assert_eq!(v, json!(env!("CARGO_PKG_VERSION")));
    }
}

#[test]
fn dispatch_unknown_command_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let e = dispatch(&ctx_in(tmp.path(), false), "rm_rf", json!({})).unwrap_err();
    assert!(e.contains("unknown command: rm_rf"), "{e}");
}

#[test]
fn dispatch_camel_case_args() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx_in(tmp.path(), false);
    // An unknown device key is refused only after the arguments were read.
    let e = dispatch(
        &ctx,
        "set_device_options",
        json!({"key": "nope", "cdeOverride": "PDOC", "verifiedRealAsin": true}),
    );
    assert!(
        !e.as_ref()
            .is_err_and(|e| e.starts_with("set_device_options:")),
        "{e:?}"
    );
    // snake_case keys are not what Tauri takes, so they are not taken here either.
    let e = dispatch(
        &ctx,
        "set_device_options",
        json!({"key": "nope", "cde_override": "PDOC", "verified_real_asin": true}),
    )
    .unwrap_err();
    assert!(e.contains("verifiedRealAsin"), "{e}");
    // An omitted optional argument is `None`, as Tauri allows; a required one is an error.
    let v = dispatch(&ctx, "take_preloaded_target", json!({})).unwrap();
    assert_eq!(v, Value::Null);
    let e = dispatch(&ctx, "morgue_show", json!({})).unwrap_err();
    assert!(e.contains("date"), "{e}");
}

#[test]
fn web_mode_reveal_is_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx_in(tmp.path(), true);
    for (cmd, args) in [
        ("reveal", json!({"path": "/data/x.epub"})),
        ("morgue_open", json!({"date": "2026-10-29"})),
    ] {
        let e = dispatch(&ctx, cmd, args).unwrap_err();
        assert_eq!(e, "not available in the web app");
    }
}

#[test]
fn web_mode_password_set_names_env() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx_in(tmp.path(), true);
    let e = dispatch(&ctx, "email_set_password", json!({"password": "pw"})).unwrap_err();
    assert_eq!(e, "no keychain in the container: set DECKLE_SMTP_PASSWORD");
    let e = dispatch(
        &ctx,
        "subscription_set_password",
        json!({"site": "gazette.example", "password": "pw"}),
    )
    .unwrap_err();
    assert_eq!(
        e,
        "no keychain in the container: set DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE"
    );
}

#[test]
fn web_mode_starts_on_daily_with_nothing_preloaded() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx_in(tmp.path(), true);
    assert_eq!(
        dispatch(&ctx, "start_tab", Value::Null).unwrap(),
        json!("daily")
    );
    assert_eq!(
        dispatch(&ctx, "take_preloaded_books", Value::Null).unwrap(),
        json!([])
    );
    assert_eq!(
        dispatch(&ctx, "take_preloaded_target", Value::Null).unwrap(),
        Value::Null
    );
    let desktop = ctx_in(tmp.path(), false);
    assert_eq!(
        dispatch(&desktop, "start_tab", Value::Null).unwrap(),
        json!("books")
    );
    assert_eq!(
        dispatch(&desktop, "take_preloaded_books", Value::Null).unwrap(),
        json!(["/books/a.epub"])
    );
}

#[test]
fn web_mode_schedule_set_writes_config() {
    let tmp = tempfile::tempdir().unwrap();
    // The config folder, wherever this OS keeps it, under the temp dir. nextest runs each
    // test in its own process, so the environment is this test's alone.
    unsafe {
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("APPDATA", tmp.path());
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
    }
    let main = deckle_digest::config::default_path();
    std::fs::create_dir_all(main.parent().unwrap()).unwrap();
    std::fs::write(
        &main,
        "title = \"The Gazette\"\n\n[schedule]\ndays = [\"sat\"]\n",
    )
    .unwrap();
    let ctx = ctx_in(tmp.path(), true);

    let v = dispatch(&ctx, "schedule_set", json!({"at": "06:15"})).unwrap();
    assert_eq!(v["at"], json!("06:15"));
    assert_eq!(v["papers"][0]["days"], json!(["sat"]));
    let toml = std::fs::read_to_string(&main).unwrap();
    assert!(toml.contains("at = \"06:15\""), "{toml}");
    assert_eq!(dispatch(&ctx, "schedule_get", Value::Null).unwrap(), v);

    let e = dispatch(&ctx, "schedule_set", json!({"at": "6pm"})).unwrap_err();
    assert!(e.contains("24-hour"), "{e}");
    let v = dispatch(&ctx, "schedule_set", json!({"at": null})).unwrap();
    assert_eq!(v["at"], Value::Null);
    let toml = std::fs::read_to_string(&main).unwrap();
    assert!(!toml.contains("06:15") && toml.contains("sat"), "{toml}");
}

#[test]
fn web_mode_schedule_shows_as_on_everywhere() {
    let tmp = tempfile::tempdir().unwrap();
    // nextest runs each test in its own process, so the environment is this test's alone.
    // Dry-run makes any attempt to install a job an error.
    unsafe {
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("APPDATA", tmp.path());
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        std::env::set_var("DECKLE_DAEMON", "1");
        std::env::set_var("DECKLE_SCHEDULE_DRY_RUN", "1");
    }
    let main = deckle_digest::config::default_path();
    std::fs::create_dir_all(main.parent().unwrap()).unwrap();
    std::fs::write(&main, "title = \"The Gazette\"\n").unwrap();
    let ctx = ctx_in(tmp.path(), true);

    let v = dispatch(&ctx, "schedule_set", json!({"at": "06:15"})).unwrap();
    assert_eq!(v["watch"], json!(true), "the daemon owns the watcher");
    assert_eq!(dispatch(&ctx, "schedule_get", Value::Null).unwrap(), v);
    let next = dispatch(&ctx, "daily_next", Value::Null).unwrap();
    assert_eq!(next["press"]["at"], json!("06:15"));
    let papers = dispatch(&ctx, "papers_list", Value::Null).unwrap();
    assert_eq!(papers["papers"][0]["installed"], json!(true), "{papers}");
    let stopped = dispatch(&ctx, "press_stop", json!({"on": true})).unwrap();
    assert!(!stopped["press"]["stopped"].is_null(), "{stopped}");

    std::fs::write(&main, "title = [").unwrap();
    assert!(dispatch(&ctx, "schedule_get", Value::Null).is_err());
}

#[test]
fn web_mode_settings_mask_the_api_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let web = ctx_in(tmp.path(), true);
    let stored = deckle_app::commands::Settings {
        llm_api_key: "sk-ant-secret".into(),
        illustrations_api_key: "sk-image-secret".into(),
        ..Default::default()
    };
    deckle_app::commands::save_settings_to(&web.state.settings_file, &stored).unwrap();
    let load = || deckle_app::commands::load_settings_from(&web.state.settings_file);

    let v = dispatch(&web, "get_settings", Value::Null).unwrap();
    assert_eq!(v["llm_api_key"], json!(deckle_app::commands::MASKED_KEY));
    assert_eq!(
        v["illustrations_api_key"],
        json!(deckle_app::commands::MASKED_KEY)
    );
    assert!(!v.to_string().contains("secret"), "{v}");

    // The page sends the mask back with another change: the stored keys stay.
    let mut back = v.clone();
    back["xray"] = json!(false);
    dispatch(&web, "set_settings", json!({ "settings": back })).unwrap();
    let s = load();
    assert!(!s.xray);
    assert_eq!(s.llm_api_key, "sk-ant-secret");
    assert_eq!(s.illustrations_api_key, "sk-image-secret");
    dispatch(
        &web,
        "set_illustrations_key",
        json!({ "key": deckle_app::commands::MASKED_KEY }),
    )
    .unwrap();
    assert_eq!(load().illustrations_api_key, "sk-image-secret");

    // A new value replaces the key; an empty one clears it.
    back["llm_api_key"] = json!("sk-ant-new");
    back["illustrations_api_key"] = json!("");
    dispatch(&web, "set_settings", json!({ "settings": back })).unwrap();
    let s = load();
    assert_eq!(s.llm_api_key, "sk-ant-new");
    assert_eq!(s.illustrations_api_key, "");
    let v = dispatch(&web, "get_settings", Value::Null).unwrap();
    assert_eq!(v["illustrations_api_key"], json!(""), "no key isn't masked");

    // The desktop reads its own keys in clear.
    let desktop = ctx_in(tmp.path(), false);
    let v = dispatch(&desktop, "get_settings", Value::Null).unwrap();
    assert_eq!(v["llm_api_key"], json!("sk-ant-new"));
}

#[test]
fn capabilities_web_has_no_updater_or_keychain() {
    let tmp = tempfile::tempdir().unwrap();
    let state = ctx_in(tmp.path(), false).state;
    let web = capabilities(true, &state);
    assert!(web.web && !web.updater && !web.keychain && !web.reveal);
    assert_eq!(web.schedule, "config");
    let desktop = capabilities(false, &state);
    assert!(!desktop.web && desktop.updater && desktop.keychain && desktop.reveal && desktop.usb);
    assert_eq!(desktop.schedule, "install");
    let v = dispatch(&ctx_in(tmp.path(), true), "capabilities", Value::Null).unwrap();
    assert_eq!(v["schedule"], json!("config"));
    assert_eq!(v["keychain"], json!(false));
}

// The fake Previewer is a shell script.
#[cfg(unix)]
#[test]
fn capabilities_previewer_honours_settings_path() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx_in(tmp.path(), true);
    let fake =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../deckle-core/tests/fake_previewer/ok.sh");
    let settings = deckle_app::commands::Settings {
        previewer_path: Some(fake.to_string_lossy().into_owned()),
        ..Default::default()
    };
    deckle_app::commands::save_settings_to(&ctx.state.settings_file, &settings).unwrap();
    assert!(capabilities(true, &ctx.state).previewer);
    let v = dispatch(&ctx, "capabilities", Value::Null).unwrap();
    assert_eq!(v["previewer"], json!(true));
}
