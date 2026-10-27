// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle cookbook add / list / move / remove / note / picture / chapters` (cookbook design
//! §8.1). A saved page stands in for the web, so nothing touches the network.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// A recipe page with a picture beside it, in `dir`. Returns the page's path.
fn soup_page(dir: &Path) -> PathBuf {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(900, 600, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    }))
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    .unwrap();
    std::fs::create_dir_all(dir.join("soup_files")).unwrap();
    std::fs::write(dir.join("soup_files/soup.png"), png).unwrap();
    let page = dir.join("Lentil Soup.html");
    std::fs::write(
        &page,
        r#"<!doctype html><html><head><title>Lentil Soup</title>
<script type="application/ld+json">
{"@context": "https://schema.org", "@type": "Recipe", "name": "Lentil Soup",
  "recipeCategory": "Soup", "image": "soup_files/soup.png",
  "recipeIngredient": ["1 cup lentils"], "recipeInstructions": ["Simmer them."]}
</script></head><body><h1>Lentil Soup</h1></body></html>"#,
    )
    .unwrap();
    page
}

/// A page with no recipe data, which is read from its text.
fn text_page() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-digest/tests/fixtures/cookbook/no-jsonld.html")
}

struct Book {
    dir: tempfile::TempDir,
}

impl Book {
    fn new() -> Self {
        Book {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn cookbook(&self) -> PathBuf {
        self.dir.path().join("cookbook")
    }

    fn page(&self) -> PathBuf {
        soup_page(&self.dir.path().join("site"))
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::cargo_bin("deckle").unwrap();
        // A home of its own, so `list` doesn't read the owner's `digest.toml` for Kindles.
        let home = self.dir.path().join("home");
        c.env("DECKLE_COOKBOOK_DIR", self.cookbook())
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &home)
            .env("APPDATA", &home)
            .arg("cookbook")
            .args(args);
        c
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.cmd(args).output().unwrap();
        assert!(
            out.status.success(),
            "{args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn fails(&self, args: &[&str]) -> String {
        let out = self.cmd(args).output().unwrap();
        assert!(!out.status.success(), "{args:?} should fail");
        String::from_utf8(out.stderr).unwrap()
    }

    /// Adds the soup page and returns its id.
    fn add_soup(&self) -> String {
        let page = self.page();
        self.ok(&["add", &page.to_string_lossy()]);
        self.list_json()[0]["id"].as_str().unwrap().to_owned()
    }

    /// `list --json`'s `recipes`.
    fn list_json(&self) -> Vec<serde_json::Value> {
        recipes_of(&self.ok(&["list", "--json"]))
    }
}

/// The `recipes` of a `list --json` listing (`{recipes, kindles}`).
fn recipes_of(listing: &str) -> Vec<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(listing).unwrap();
    v["recipes"].as_array().unwrap().clone()
}

#[test]
fn add_prints_chapter_and_picture() {
    let book = Book::new();
    let page = book.page();
    let out = book.ok(&["add", &page.to_string_lossy()]);
    assert_eq!(out.trim(), "Added to Soups and stews · site photo");
}

#[test]
fn add_again_prints_updated() {
    let book = Book::new();
    let page = book.page();
    book.ok(&["add", &page.to_string_lossy()]);
    let out = book.ok(&["add", &page.to_string_lossy()]);
    assert_eq!(out.trim(), "Updated · Soups and stews · site photo");
}

#[test]
fn add_without_a_picture_says_why() {
    let book = Book::new();
    let page = book.page();
    std::fs::remove_file(page.parent().unwrap().join("soup_files/soup.png")).unwrap();
    let out = book.ok(&["add", &page.to_string_lossy()]);
    assert_eq!(
        out.trim(),
        "Added to Soups and stews · no picture\nNo picture on the page"
    );
}

#[test]
fn add_again_without_the_photo_keeps_the_earlier_picture() {
    let book = Book::new();
    let page = book.page();
    book.ok(&["add", &page.to_string_lossy()]);
    std::fs::remove_file(page.parent().unwrap().join("soup_files/soup.png")).unwrap();
    let out = book.ok(&["add", &page.to_string_lossy()]);
    assert!(
        out.contains("Updated · Soups and stews · site photo\nKept the earlier picture"),
        "{out}"
    );
}

#[test]
fn add_with_an_empty_note_stores_none() {
    let book = Book::new();
    let page = book.page();
    book.ok(&["add", &page.to_string_lossy(), "--note", ""]);
    assert!(book.list_json()[0]["note"].is_null());
}

#[test]
fn add_takes_a_chapter_and_a_note() {
    let book = Book::new();
    let page = book.page();
    let out = book.ok(&[
        "add",
        &page.to_string_lossy(),
        "--chapter",
        "mains",
        "--note",
        "Half the salt",
    ]);
    assert!(out.starts_with("Added to Mains · "), "{out}");
    let rows = book.list_json();
    assert_eq!(rows[0]["chapter"], "mains");
    assert_eq!(rows[0]["note"], "Half the salt");

    let err = book.fails(&["add", &page.to_string_lossy(), "--chapter", "nope"]);
    assert!(err.contains("nope") && err.contains("soups"), "{err}");
}

#[test]
fn move_refiles() {
    let book = Book::new();
    let id = book.add_soup();
    let out = book.ok(&["move", &id, "mains"]);
    assert_eq!(out.trim(), "Moved to Mains");
    assert_eq!(book.list_json()[0]["chapter"], "mains");
}

#[test]
fn move_to_unknown_chapter_errors_listing_ids() {
    let book = Book::new();
    let id = book.add_soup();
    let err = book.fails(&["move", &id, "brunchy"]);
    assert!(err.contains("brunchy"), "{err}");
    for valid in ["breakfast", "soups", "mains", "desserts", "other"] {
        assert!(err.contains(valid), "{valid} missing from: {err}");
    }
    assert_eq!(book.list_json()[0]["chapter"], "soups");
}

#[test]
fn remove_by_url() {
    let book = Book::new();
    let page = book.page();
    book.add_soup();
    // `who` is an id or the address it was saved from (a saved page's is its path), given
    // however the shell has it: here a relative path.
    let site = page.parent().unwrap();
    let relative = "./Lentil Soup.html";
    let out = book
        .cmd(&["remove", relative])
        .current_dir(site)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Removed");
    assert!(book.list_json().is_empty());
    let err = book.fails(&["remove", &page.to_string_lossy()]);
    assert!(err.contains("nothing saved"), "{err}");
}

#[test]
fn who_as_a_saved_page_path_reaches_every_command() {
    let book = Book::new();
    let page = book.page();
    book.add_soup();
    let site = page.parent().unwrap();
    for args in [
        vec!["move", "./Lentil Soup.html", "mains"],
        vec!["note", "./Lentil Soup.html", "Good cold"],
        vec!["picture", "./Lentil Soup.html", "none"],
    ] {
        let out = book.cmd(&args).current_dir(site).output().unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let row = &book.list_json()[0];
    assert_eq!(row["chapter"], "mains");
    assert_eq!(row["note"], "Good cold");
    assert_eq!(row["picture"], "none");
}

#[test]
fn note_empty_clears() {
    let book = Book::new();
    let id = book.add_soup();
    book.ok(&["note", &id, "Half the sugar works"]);
    assert_eq!(book.list_json()[0]["note"], "Half the sugar works");
    book.ok(&["note", &id, ""]);
    assert!(book.list_json()[0]["note"].is_null());
}

#[test]
fn picture_records_the_choice() {
    let book = Book::new();
    let id = book.add_soup();
    assert_eq!(book.list_json()[0]["picture"], "default");
    for choice in ["illustration", "none", "site", "default"] {
        book.ok(&["picture", &id, choice]);
        assert_eq!(book.list_json()[0]["picture"], choice);
    }
    book.cmd(&["picture", &id, "sepia"]).assert().failure();
}

#[test]
fn list_json_shape() {
    let book = Book::new();
    let page = book.page();
    book.ok(&["add", &page.to_string_lossy(), "--note", "Good cold"]);
    let rows = book.list_json();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["id"].as_str().unwrap().len(), 12);
    assert_eq!(row["title"], "Lentil Soup");
    assert_eq!(row["chapter"], "soups");
    assert_eq!(row["structured"], true);
    assert_eq!(row["picture"], "default");
    assert_eq!(row["note"], "Good cold");
    assert!(row["added"].as_str().unwrap().starts_with("20"));
    assert!(row["url"].as_str().unwrap().starts_with("file://"));

    let filtered = |chapter: &str| {
        let out = book
            .cmd(&["list", "--chapter", chapter, "--json"])
            .output()
            .unwrap();
        assert!(out.status.success());
        recipes_of(&String::from_utf8(out.stdout).unwrap())
    };
    assert_eq!(filtered("soups").len(), 1);
    assert!(filtered("mains").is_empty());
    book.fails(&["list", "--chapter", "nope"]);
}

#[test]
fn list_text_has_a_row_per_recipe() {
    let book = Book::new();
    assert_eq!(book.ok(&["list"]).trim(), "No recipes yet");
    let id = book.add_soup();
    let out = book.ok(&["list"]);
    assert!(out.contains(&id), "{out}");
    assert!(out.contains("Lentil Soup"), "{out}");
    assert!(out.contains("Soups and stews"), "{out}");
    assert!(out.contains("recipe data"), "{out}");
}

#[test]
fn text_fallback_says_so() {
    let book = Book::new();
    let out = book.ok(&["add", &text_page().to_string_lossy()]);
    assert!(
        out.contains("Read from the page text, not recipe data"),
        "{out}"
    );
    assert_eq!(book.list_json()[0]["structured"], false);
    assert!(book.ok(&["list"]).contains("page text"));
}

#[test]
fn chapters_lists_ids_names_and_counts() {
    let book = Book::new();
    book.add_soup();
    let out = book.ok(&["chapters"]);
    assert!(
        out.contains("soups") && out.contains("Soups and stews"),
        "{out}"
    );
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&book.ok(&["chapters", "--json"])).unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0]["id"], "breakfast");
    assert_eq!(rows[0]["recipes"], 0);
    let soups = rows.iter().find(|r| r["id"] == "soups").unwrap();
    assert_eq!(soups["name"], "Soups and stews");
    assert_eq!(soups["recipes"], 1);
    assert!(
        soups["match"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k == "soup")
    );
}

#[test]
fn who_that_is_unknown_is_an_error() {
    let book = Book::new();
    for args in [
        vec!["move", "0123456789ab", "mains"],
        vec!["note", "0123456789ab", "hi"],
        vec!["picture", "0123456789ab", "none"],
    ] {
        let err = book.fails(&args);
        assert!(err.contains("nothing saved"), "{args:?}: {err}");
    }
}

#[test]
fn a_deleted_chapter_files_under_other_in_list_and_chapters() {
    let book = Book::new();
    let id = book.add_soup();
    book.ok(&["move", &id, "mains"]);
    // Delete "mains" from the config by hand.
    let toml = book.cookbook().join("cookbook.toml");
    std::fs::write(
        &toml,
        "[[chapters]]\nid = \"soups\"\nname = \"Soups\"\n\n[[chapters]]\nid = \"other\"\nname = \"Everything else\"\n",
    )
    .unwrap();
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&book.ok(&["chapters", "--json"])).unwrap();
    let other = rows.iter().find(|r| r["id"] == "other").unwrap();
    assert_eq!(other["recipes"], 1);
    let listed = recipes_of(&book.ok(&["list", "--chapter", "other", "--json"]));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], id);
    let soups = recipes_of(&book.ok(&["list", "--chapter", "soups", "--json"]));
    assert!(soups.is_empty());
}

#[test]
fn build_writes_epub_offline() {
    let book = Book::new();
    let id = book.add_soup();
    book.ok(&["move", &id, "soups"]);
    let text = text_page();
    book.ok(&["add", &text.to_string_lossy(), "--chapter", "mains"]);
    let dirty = book.cookbook().join("dirty");
    assert!(dirty.exists(), "adding marks the book dirty");

    let out = book.dir.path().join("out");
    let said = book.ok(&["build", "-o", &out.to_string_lossy()]);
    let epub = out.join("cookbook.epub");
    assert!(epub.is_file(), "no EPUB at {}", epub.display());
    let mut lines = said.lines();
    let count = lines.next().unwrap();
    assert_eq!(count, "2 recipes in 2 chapters");
    assert_eq!(lines.next().unwrap(), epub.to_string_lossy());
    let id = lines
        .next()
        .unwrap()
        .strip_prefix("build ")
        .expect("a build line");
    assert_eq!(id.len(), 16, "{id}");
    assert!(id.bytes().all(|c| c.is_ascii_hexdigit()), "{id}");
    // An export, not a delivery: the flag stays as it was.
    assert!(dirty.exists(), "build must not clear the dirty flag");
}

#[test]
fn build_counts_only_chapters_with_recipes() {
    let book = Book::new();
    let id = book.add_soup();
    book.ok(&["move", &id, "mains"]);
    let out = book.dir.path().join("out");
    let said = book.ok(&["build", "-o", &out.to_string_lossy()]);
    assert_eq!(
        said.lines().next().unwrap(),
        "1 recipe in 1 chapter",
        "{said}"
    );
}

#[test]
fn build_of_an_empty_book_still_writes_one() {
    let book = Book::new();
    let out = book.dir.path().join("out");
    let said = book.ok(&["build", "-o", &out.to_string_lossy()]);
    assert_eq!(said.lines().next().unwrap(), "0 recipes in 0 chapters");
    assert!(out.join("cookbook.epub").is_file());
}

/// Deckle's config folder under `home`, as the CLI finds it on this OS.
fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

#[test]
fn a_one_shot_build_leaves_the_recipes_folder_to_the_watcher() {
    // A backlog of recipe fetches must not delay the morning paper: only `digest watch`
    // adds what is in Inbox/Recipes.
    let book = Book::new();
    let home = book.dir.path().join("home");
    let dir = config_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        "sections = [\"horoscope\"]\n\n[horoscope]\nsigns = [\"leo\"]\n\n[reading]\ninbox = \"~/Inbox\"\n",
    )
    .unwrap();
    let recipes = home.join("Inbox/Recipes");
    std::fs::create_dir_all(&recipes).unwrap();
    let page = book.page();
    let dropped = recipes.join("soup.html");
    std::fs::copy(&page, &dropped).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(120);
    std::fs::File::options()
        .write(true)
        .open(&dropped)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let out = book.dir.path().join("out");
    Command::cargo_bin("deckle")
        .unwrap()
        .env("DECKLE_COOKBOOK_DIR", book.cookbook())
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("APPDATA", &home)
        .env("LOCALAPPDATA", &home)
        .args(["digest", "build", "--date", "2026-10-01", "-o"])
        .arg(&out)
        .assert()
        .success();
    assert!(dropped.exists(), "still waiting for the watcher");
    assert!(!recipes.join("Sent").exists());
    assert!(book.list_json().is_empty(), "nothing was added to the book");
}
