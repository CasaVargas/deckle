// SPDX-License-Identifier: GPL-3.0-or-later
use assert_cmd::Command;

#[test]
fn help_lists_all_subcommands() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    for sub in ["inspect", "normalize", "convert", "send", "devices", "run"] {
        assert!(s.contains(sub), "missing subcommand {sub} in:\n{s}");
    }
}

#[test]
fn devices_without_a_kindle_succeeds_with_a_message() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["devices"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("Kindle") || text.contains("thumbnails"),
        "{text}"
    );
}

#[test]
fn convert_from_kpf_matches_the_golden() {
    let golden = std::fs::read(deckle_kfx::fixtures::path(
        "deckle-mini-pdoc-nopages.golden.kfx",
    ))
    .unwrap();
    let doc = deckle_kfx::container::read_kfx(&golden).unwrap();
    let out = tempfile::tempdir().unwrap();
    let kfx = out.path().join("out.kfx");
    Command::cargo_bin("deckle")
        .unwrap()
        .args(["convert"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.kpf"))
        .args(["-o"])
        .arg(&kfx)
        .args([
            "--cde",
            "pdoc",
            "--asin",
            "B0DECK000002",
            "--no-authors",
            "--container-id",
            &doc.container_id,
            "--creator",
            &doc.creator,
        ])
        .assert()
        .success();
    assert_eq!(std::fs::read(&kfx).unwrap(), golden);
}

#[test]
fn inspect_json_reports_the_title() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["--json", "inspect"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["title"], "Deckle Mini Fixture");
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn run_with_fake_previewer_from_the_cli() {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let out = tempfile::tempdir().unwrap();
    let res = Command::cargo_bin("deckle")
        .unwrap()
        .args(["--json", "run"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .args(["--to"])
        .arg(out.path())
        .args(["--previewer"])
        .arg(&fake)
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
    assert!(
        v[0]["delivered"]
            .as_str()
            .unwrap()
            .ends_with("Deckle Mini Fixture.kfx")
    );
    // The Open Library metadata fill is off unless asked for: no lookup, no outcome.
    assert!(v[0]["metadata_fill"].is_null(), "{}", v[0]);
    assert!(v[0]["stage_ms"].get("fill").is_none(), "{}", v[0]);
    // So is the typography pass.
    assert!(v[0]["normalize"]["typography"].is_null(), "{}", v[0]);
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn run_with_typography_reports_what_it_changed() {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let out = tempfile::tempdir().unwrap();
    let res = Command::cargo_bin("deckle")
        .unwrap()
        .args(["run"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .args(["--to"])
        .arg(out.path())
        .args(["--previewer"])
        .arg(&fake)
        .arg("--typography")
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let s = String::from_utf8(res.stdout).unwrap();
    assert!(s.contains("Typography: ") && s.contains(" quotes, "), "{s}");
}

#[test]
fn run_help_offers_typography_off_by_default() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["run", "--help"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    let line = s
        .lines()
        .find(|l| l.contains("--typography"))
        .unwrap_or_else(|| panic!("no --typography in:\n{s}"));
    assert!(!line.contains('<'), "{line}");
    assert!(s.contains("curly quotes"), "{s}");
}

#[test]
fn run_help_offers_the_metadata_fill_off_by_default() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["run", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    let line = s
        .lines()
        .find(|l| l.contains("--fill-metadata"))
        .unwrap_or_else(|| panic!("no --fill-metadata in:\n{s}"));
    // A plain switch: it takes no value and has no default that turns it on.
    assert!(!line.contains('<'), "{line}");
    assert!(!s.contains("[default: true]"), "{s}");
    assert!(
        s.contains("Open Library") && s.contains("off by default"),
        "{s}"
    );
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn run_takes_a_word_document() {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let docx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/docx/deckle-mini.docx");
    let out = tempfile::tempdir().unwrap();
    let res = Command::cargo_bin("deckle")
        .unwrap()
        .args(["--json", "run"])
        .arg(&docx)
        .args(["--to"])
        .arg(out.path())
        .args(["--previewer"])
        .arg(&fake)
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
    assert_eq!(v[0]["info"]["title"], "Deckle Mini Document");
    assert!(v[0]["delivered"].as_str().unwrap().ends_with(".kfx"));
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn run_takes_a_saved_web_page_and_reports_a_stub() {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let tmp = tempfile::tempdir().unwrap();
    let para = "The council met again tonight to argue about the new library and its roof. ";
    let page = tmp.path().join("essay.html");
    std::fs::write(
        &page,
        format!(
            "<html><head><title>The Roof Question</title></head><body><article>\
             <h1>The Roof Question</h1>{}</article></body></html>",
            format!("<p>{para}</p>").repeat(40)
        ),
    )
    .unwrap();
    let stub = tmp.path().join("paywall.htm");
    std::fs::write(&stub, "<html><body><p>Subscribe.</p></body></html>").unwrap();
    let out = tmp.path().join("out");
    // Deckle's cache (the built EPUBs) and config live under this HOME, not the real one.
    let home = tmp.path().join("home");
    let res = Command::cargo_bin("deckle")
        .unwrap()
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .args(["--json", "run"])
        .arg(&page)
        .arg(&stub)
        .args(["--xray", "off", "--word-wise", "off", "--to"])
        .arg(&out)
        .args(["--previewer"])
        .arg(&fake)
        .output()
        .unwrap();
    assert!(!res.status.success(), "the stub fails the run");
    let v: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
    assert_eq!(v[0]["input"], page.to_string_lossy().as_ref());
    assert_eq!(v[0]["info"]["title"], "The Roof Question");
    assert!(
        v[0]["delivered"]
            .as_str()
            .unwrap()
            .ends_with("The Roof Question.kfx"),
        "{}",
        v[0]
    );
    assert!(
        v[1]["error"].as_str().unwrap().contains("under 120 words"),
        "{}",
        v[1]
    );
}

#[test]
fn run_checks_previewer_before_creating_the_output_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("new-folder");
    let res = Command::cargo_bin("deckle")
        .unwrap()
        .args(["run"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .args(["--to"])
        .arg(&target)
        .args(["--previewer", "/nonexistent/KindlePreviewer4CLI"])
        .output()
        .unwrap();
    assert!(!res.status.success());
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&res.stdout),
        String::from_utf8_lossy(&res.stderr)
    );
    assert!(text.contains("Kindle Previewer is not installed"), "{text}");
    assert!(
        !target.exists(),
        "output folder must not be created before the Previewer check"
    );
}

fn save(queue: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .env("DECKLE_READING_QUEUE", queue)
        .arg("save")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr),
    )
}

#[test]
fn save_list_remove_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let q = dir.path().join("reading.jsonl");
    let (ok, text) = save(&q, &["https://ex.com/a"]);
    assert!(ok && text.contains("saved r-"), "{text}");
    let id = text
        .split_whitespace()
        .find(|w| w.starts_with("r-"))
        .unwrap()
        .to_owned();
    let (_, list) = save(&q, &["--list"]);
    assert!(
        list.contains(&id) && list.contains("pending") && list.contains("https://ex.com/a"),
        "{list}"
    );
    let (ok, _) = save(&q, &["--remove", &id]);
    assert!(ok);
    let (_, list) = save(&q, &["--list"]);
    assert!(!list.contains(&id), "{list}");
}

#[test]
fn save_duplicate_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let q = dir.path().join("reading.jsonl");
    save(&q, &["https://ex.com/a"]);
    let (ok, text) = save(&q, &["https://ex.com/a#again"]);
    assert!(ok && text.contains("already saved"), "{text}");
}

#[test]
fn bookmarklet_prints_javascript_with_token() {
    let dir = tempfile::tempdir().unwrap();
    let q = dir.path().join("reading.jsonl");
    let (ok, text) = save(&q, &["--bookmarklet"]);
    assert!(ok, "{text}");
    let js = text
        .lines()
        .find(|l| l.starts_with("javascript:"))
        .unwrap_or_default();
    assert!(js.contains("127.0.0.1:47219/save?token="), "{text}");
    // The token is stable across calls.
    let (_, again) = save(&q, &["--bookmarklet"]);
    assert!(again.contains(js));
}

#[test]
fn digest_email_has_password_test_and_forget() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["digest", "email", "--help"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    for sub in ["password", "test", "forget"] {
        assert!(s.contains(sub), "missing {sub} in:\n{s}");
    }
}

/// The starter config's `[email]` is commented out, so these stop before the keychain or the
/// network: no test touches either.
fn email_with_starter(args: &[&str], stdin: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("digest.toml");
    std::fs::write(&cfg, deckle_digest::config::STARTER).unwrap();
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["digest", "--config"])
        .arg(&cfg)
        .arg("email")
        .args(args)
        .write_stdin(stdin)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn digest_email_needs_the_email_table_first() {
    let (ok, err) = email_with_starter(&["test"], "");
    assert!(!ok);
    assert!(
        err.contains("[email] needs to, from, smtp_host, username"),
        "{err}"
    );
    for args in [&["password", "--stdin"][..], &["forget"]] {
        let (ok, err) = email_with_starter(args, "not-a-real-password\n");
        assert!(!ok, "{args:?}");
        assert!(err.contains("username and smtp_host"), "{args:?}: {err}");
        assert!(!err.contains("not-a-real-password"), "{err}");
    }
}

/// `deckle` with HOME (so the config and cache folders) under `home`.
fn deckle_at(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", home.join("AppData"));
    c
}

#[test]
fn library_init_add_and_list() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let root = tmp.path().join("Library");
    let none = deckle_at(&home).args(["library", "list"]).output().unwrap();
    assert!(!none.status.success());
    assert!(String::from_utf8_lossy(&none.stderr).contains("library init"));
    deckle_at(&home)
        .args(["library", "init"])
        .arg(&root)
        .assert()
        .success();
    deckle_at(&home)
        .args(["library", "add"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .assert()
        .success();
    let out = deckle_at(&home).args(["library", "list"]).output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("Testwright, Jane  Deckle Mini Fixture  [Fixture Series #2]"),
        "{text}"
    );
    let out = deckle_at(&home)
        .args(["--json", "library", "list"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v[0]["book"]["asin"].as_str().unwrap().ends_with("DECK"));
    let scan = deckle_at(&home).args(["library", "scan"]).output().unwrap();
    assert!(scan.status.success());
    assert!(String::from_utf8_lossy(&scan.stdout).contains("1 books in"));
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn run_adds_the_book_to_the_library_and_records_the_send() {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let out = tmp.path().join("out");
    deckle_at(&home)
        .args(["library", "init"])
        .arg(tmp.path().join("Library"))
        .assert()
        .success();
    deckle_at(&home)
        .args(["run"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .args(["--xray", "off", "--word-wise", "off", "--to"])
        .arg(&out)
        .args(["--previewer"])
        .arg(&fake)
        .assert()
        .success();
    let list = deckle_at(&home)
        .args(["--json", "library", "list"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let send = &v[0]["book"]["sends"][0];
    assert!(
        send["device"].as_str().unwrap().starts_with("folder:"),
        "{v}"
    );
    assert!(
        send["file"]
            .as_str()
            .unwrap()
            .ends_with("Deckle Mini Fixture.kfx")
    );
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn kindle_books_lists_what_run_sent_and_remove_takes_it_off() {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let out = tmp.path().join("out");
    deckle_at(&home)
        .args(["library", "init"])
        .arg(tmp.path().join("Library"))
        .assert()
        .success();
    deckle_at(&home)
        .args(["run"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .args(["--xray", "off", "--word-wise", "off", "--to"])
        .arg(&out)
        .args(["--previewer"])
        .arg(&fake)
        .assert()
        .success();
    std::fs::write(out.join("paper.pdf"), b"%PDF").unwrap();
    let books = deckle_at(&home)
        .args(["--json", "kindle", "books", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        books.status.success(),
        "{}",
        String::from_utf8_lossy(&books.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&books.stdout).unwrap();
    let rows = v["books"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{v}");
    let kfx = rows.iter().find(|r| r["book"]["format"] == "kfx").unwrap();
    assert_eq!(kfx["book"]["title"], "Deckle Mini Fixture");
    assert!(kfx["library"].is_string(), "matched to the library: {kfx}");
    let file = kfx["book"]["file"].as_str().unwrap().to_owned();
    let text = deckle_at(&home)
        .args(["kindle", "books", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&text.stdout).contains("in library"));
    // No terminal to ask in: refused without --yes.
    let refused = deckle_at(&home)
        .args(["kindle", "remove", &file, "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(out.join(&file).exists());
    deckle_at(&home)
        .args(["kindle", "remove", &file, "--yes", "--to"])
        .arg(&out)
        .assert()
        .success();
    assert!(!out.join(&file).exists());
    let list = deckle_at(&home)
        .args(["--json", "library", "list"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(
        v[0]["book"]["sends"],
        serde_json::json!([]),
        "send forgotten"
    );
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn library_reprocess_updates_books_sent_before_engine_stamps() {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let out = tmp.path().join("out");
    let root = tmp.path().join("Library");
    deckle_at(&home)
        .args(["library", "init"])
        .arg(&root)
        .assert()
        .success();
    deckle_at(&home)
        .args(["run"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .args(["--xray", "off", "--word-wise", "off", "--to"])
        .arg(&out)
        .args(["--previewer"])
        .arg(&fake)
        .assert()
        .success();
    // As a Deckle from before engine stamps would have written it.
    let sidecar = root.join("Testwright, Jane/Deckle Mini Fixture/deckle.json");
    let mut v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
    assert_eq!(v["sends"][0]["stamped"], true);
    v["sends"][0].as_object_mut().unwrap().remove("stamped");
    std::fs::write(&sidecar, serde_json::to_vec(&v).unwrap()).unwrap();

    let dry = deckle_at(&home)
        .args(["library", "reprocess", "--dry-run", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&dry.stdout).into_owned();
    assert!(dry.status.success(), "{text}");
    assert!(
        text.contains("sent before Deckle tracked versions"),
        "{text}"
    );
    assert!(text.contains("1 can be updated"), "{text}");
    let unchanged: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
    assert!(unchanged["sends"][0].get("stamped").is_none());

    let done = deckle_at(&home)
        .args(["library", "reprocess", "--yes", "--previewer"])
        .arg(&fake)
        .args(["--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "{}{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr)
    );
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
    assert_eq!(after["sends"][0]["stamped"], true);
    let kfx: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "kfx"))
        .collect();
    assert_eq!(kfx.len(), 1, "one copy");
    let again = deckle_at(&home)
        .args(["library", "reprocess", "--dry-run", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&again.stdout).contains("is current"));
}

/// `deckle run` on the DOCX fixture (no cover) through the fake Previewer, with `extra` flags.
#[cfg(unix)] // the fake Previewer is a shell script
fn run_docx(extra: &[&str]) -> std::process::Output {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let docx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/docx/deckle-mini.docx");
    let out = tempfile::tempdir().unwrap();
    Command::cargo_bin("deckle")
        .unwrap()
        .arg("run")
        .arg(docx)
        .arg("--to")
        .arg(out.path())
        .arg("--previewer")
        .arg(&fake)
        .args(extra)
        .output()
        .unwrap()
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn run_generates_a_cover_for_a_coverless_book_unless_told_not_to() {
    let res = run_docx(&["--cover-style", "rule"]);
    let s = String::from_utf8_lossy(&res.stdout);
    assert!(res.status.success(), "{s}");
    assert!(s.contains("Cover: generated (rule)"), "{s}");

    let res = run_docx(&["--json"]);
    let v: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
    assert!(v[0]["normalize"]["cover_generated"].is_string(), "{}", v[0]);

    let res = run_docx(&["--no-generated-cover"]);
    let s = String::from_utf8_lossy(&res.stdout);
    assert!(res.status.success(), "{s}");
    assert!(!s.contains("Cover:"), "{s}");
}

#[test]
fn run_rejects_an_unknown_cover_style() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["run", "x.epub", "--cover-style", "fancy"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("classic") && err.contains("band"), "{err}");
}
