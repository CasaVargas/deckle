// SPDX-License-Identifier: GPL-3.0-or-later
//! The cookbook's plain-file store (cookbook design §5).

use chrono::{DateTime, TimeZone, Utc};
use deckle_digest::cookbook::store::{
    BookConfig, ImageKind, Picture, Saved, Store, Stored, recipe_id,
};
use deckle_digest::cookbook::{Group, Recipe};

fn recipe(title: &str, url: &str, category: &[&str]) -> Recipe {
    Recipe {
        title: title.to_owned(),
        description: Some("A good one.".to_owned()),
        yield_: Some("4 servings".to_owned()),
        prep: Some(10),
        cook: Some(20),
        total: None,
        ingredients: vec![Group {
            heading: None,
            items: vec!["1 egg".to_owned()],
        }],
        steps: vec![Group {
            heading: None,
            items: vec!["Cook it.".to_owned()],
        }],
        category: category.iter().map(|c| c.to_string()).collect(),
        cuisine: None,
        author: Some("Ann".to_owned()),
        image_url: None,
        source_url: url.to_owned(),
        site_name: "fetch.test".to_owned(),
        structured: true,
        body_xhtml: None,
    }
}

fn at(day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap()
}

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("cookbook"));
    (dir, store)
}

fn added(stored: Stored) -> Saved {
    match stored {
        Stored::Added(s) => s,
        Stored::Updated(_) => panic!("expected Added"),
    }
}

const URL: &str = "https://fetch.test/soup";

#[test]
fn put_writes_json_and_files_chapter() {
    let (_d, store) = store();
    let r = recipe("Tomato soup", URL, &["Soup"]);
    let saved = added(store.put(r.clone(), URL, None, None, at(1)).unwrap());
    assert_eq!(saved.id, recipe_id(URL));
    assert_eq!(saved.id.len(), 12);
    assert_eq!(saved.chapter, "soups");
    assert_eq!(saved.picture, Picture::Default);
    assert_eq!(saved.added, at(1));
    let file = store.dir.join("recipes").join(format!("{}.json", saved.id));
    let text = std::fs::read_to_string(file).unwrap();
    // The flattened recipe keeps `yield`, and the owner's fields sit beside it.
    assert!(text.contains("\"yield\""), "{text}");
    assert!(text.contains("\"chapter\": \"soups\""), "{text}");
    assert!(text.contains("\"picture\": \"default\""), "{text}");
    // Round trip.
    let back = store.find(&saved.id).unwrap().unwrap();
    assert_eq!(back, saved);
    assert_eq!(back.recipe, r);
    assert_eq!(store.all().unwrap(), vec![saved]);
}

#[test]
fn resave_keeps_owner_fields() {
    let (_d, store) = store();
    let first = added(
        store
            .put(
                recipe("Tomato soup", URL, &["Soup"]),
                URL,
                None,
                None,
                at(1),
            )
            .unwrap(),
    );
    let mut edited = first.clone();
    edited.chapter = "mains".to_owned();
    edited.note = Some("Less salt.".to_owned());
    edited.title_override = Some("Gran's soup".to_owned());
    edited.picture = Picture::None;
    store.update(&edited).unwrap();

    let again = store
        .put(
            recipe("Tomato soup, improved", URL, &["Soup"]),
            URL,
            None,
            None,
            at(9),
        )
        .unwrap();
    let Stored::Updated(saved) = again else {
        panic!("expected Updated");
    };
    assert_eq!(saved.id, first.id);
    assert_eq!(saved.chapter, "mains");
    assert_eq!(saved.picture, Picture::None);
    assert_eq!(saved.note.as_deref(), Some("Less salt."));
    assert_eq!(saved.title_override.as_deref(), Some("Gran's soup"));
    assert_eq!(saved.added, at(1));
    assert_eq!(saved.recipe.title, "Tomato soup, improved");
    assert_eq!(store.all().unwrap().len(), 1);
    assert_eq!(
        std::fs::read_dir(store.dir.join("recipes"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn resave_with_explicit_chapter_or_note_overrides() {
    let (_d, store) = store();
    added(
        store
            .put(
                recipe("Tomato soup", URL, &["Soup"]),
                URL,
                None,
                Some("old"),
                at(1),
            )
            .unwrap(),
    );
    let Stored::Updated(saved) = store
        .put(
            recipe("Tomato soup", URL, &["Soup"]),
            URL,
            Some("sides"),
            Some("new"),
            at(2),
        )
        .unwrap()
    else {
        panic!("expected Updated");
    };
    assert_eq!(saved.chapter, "sides");
    assert_eq!(saved.note.as_deref(), Some("new"));
}

#[test]
fn same_url_with_utm_is_same_id() {
    assert_eq!(
        recipe_id("https://fetch.test/soup?utm_source=feed#top"),
        recipe_id(URL)
    );
    assert_ne!(recipe_id("https://fetch.test/stew"), recipe_id(URL));
    let (_d, store) = store();
    added(
        store
            .put(recipe("Soup", URL, &["Soup"]), URL, None, None, at(1))
            .unwrap(),
    );
    let tracked = "https://fetch.test/soup?utm_medium=x";
    let stored = store
        .put(
            recipe("Soup", tracked, &["Soup"]),
            tracked,
            None,
            None,
            at(2),
        )
        .unwrap();
    assert!(matches!(stored, Stored::Updated(_)));
    assert_eq!(store.all().unwrap().len(), 1);
    // `find` takes a URL as well as an id.
    assert!(store.find(tracked).unwrap().is_some());
    assert!(store.find(&recipe_id(URL)).unwrap().is_some());
    assert!(store.find("https://fetch.test/none").unwrap().is_none());
}

#[test]
fn explicit_chapter_wins_over_filing() {
    let (_d, store) = store();
    let saved = added(
        store
            .put(
                recipe("Tomato soup", URL, &["Soup"]),
                URL,
                Some("sides"),
                None,
                at(1),
            )
            .unwrap(),
    );
    assert_eq!(saved.chapter, "sides");
}

#[test]
fn unknown_chapter_kept_on_disk() {
    let (_d, store) = store();
    let saved = added(
        store
            .put(
                recipe("Tomato soup", URL, &["Soup"]),
                URL,
                Some("deleted-chapter"),
                None,
                at(1),
            )
            .unwrap(),
    );
    assert_eq!(saved.chapter, "deleted-chapter");
    assert_eq!(
        store.find(&saved.id).unwrap().unwrap().chapter,
        "deleted-chapter"
    );
}

#[test]
fn remove_deletes_json_and_images() {
    let (_d, store) = store();
    let saved = added(
        store
            .put(recipe("Soup", URL, &["Soup"]), URL, None, None, at(1))
            .unwrap(),
    );
    let other = added(
        store
            .put(
                recipe("Stew", "https://fetch.test/stew", &[]),
                "https://fetch.test/stew",
                None,
                None,
                at(1),
            )
            .unwrap(),
    );
    std::fs::create_dir_all(store.dir.join("images")).unwrap();
    for kind in [ImageKind::Printed, ImageKind::Orig, ImageKind::Ill] {
        std::fs::write(store.image_path(&saved.id, kind), b"x").unwrap();
    }
    std::fs::write(store.image_path(&other.id, ImageKind::Printed), b"x").unwrap();
    assert!(store.remove(URL).unwrap());
    assert!(store.find(&saved.id).unwrap().is_none());
    let left: Vec<String> = std::fs::read_dir(store.dir.join("images"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, vec![format!("{}.jpg", other.id)]);
    assert!(!store.remove(URL).unwrap());
}

#[test]
fn image_paths() {
    let (_d, store) = store();
    let img = store.dir.join("images");
    assert_eq!(
        store.image_path("abc", ImageKind::Printed),
        img.join("abc.jpg")
    );
    assert_eq!(
        store.image_path("abc", ImageKind::Orig),
        img.join("abc.orig.jpg")
    );
    assert_eq!(
        store.image_path("abc", ImageKind::Ill),
        img.join("abc.ill.jpg")
    );
}

#[test]
fn missing_toml_gives_defaults() {
    let (_d, store) = store();
    let cfg = store.config().unwrap();
    assert_eq!(cfg, BookConfig::default());
    assert_eq!(cfg.title, "The Kitchen Book");
    assert_eq!(cfg.subtitle, "Recipes, gathered");
    assert_eq!(cfg.picture, Picture::Site);
    assert_eq!(cfg.chapters, deckle_digest::cookbook::default_chapters());
    assert!(!cfg.illustrations.enabled);
    assert!(cfg.illustrations.endpoint.is_empty() && cfg.illustrations.model.is_empty());
}

#[test]
fn partial_toml_fills_defaults() {
    let (_d, store) = store();
    std::fs::create_dir_all(&store.dir).unwrap();
    std::fs::write(store.dir.join("cookbook.toml"), "title = \"Ours\"\n").unwrap();
    let cfg = store.config().unwrap();
    assert_eq!(cfg.title, "Ours");
    assert_eq!(cfg.subtitle, "Recipes, gathered");
    assert_eq!(cfg.chapters.len(), 10);
}

#[test]
fn bad_toml_is_an_error() {
    let (_d, store) = store();
    std::fs::create_dir_all(&store.dir).unwrap();
    std::fs::write(store.dir.join("cookbook.toml"), "title = [\n").unwrap();
    assert!(store.config().is_err());
}

#[test]
fn save_config_keeps_comments() {
    let (_d, store) = store();
    std::fs::create_dir_all(&store.dir).unwrap();
    let path = store.dir.join("cookbook.toml");
    std::fs::write(
        &path,
        "# my book, my rules\ntitle = \"Old\" # the name\nsubtitle = \"Recipes, gathered\"\n",
    )
    .unwrap();
    let mut cfg = store.config().unwrap();
    cfg.title = "The Vargas Kitchen Book".to_owned();
    cfg.chapters[0].name = "Morning".to_owned();
    store.save_config(&cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# my book, my rules"), "{text}");
    assert!(text.contains("# the name"), "{text}");
    assert!(text.contains("The Vargas Kitchen Book"), "{text}");
    assert_eq!(store.config().unwrap(), cfg);
}

#[test]
fn save_config_creates_the_file() {
    let (_d, store) = store();
    let mut cfg = BookConfig::default();
    cfg.illustrations.enabled = true;
    cfg.illustrations.endpoint = "http://localhost:8080".to_owned();
    cfg.picture = Picture::Illustration;
    store.save_config(&cfg).unwrap();
    assert_eq!(store.config().unwrap(), cfg);
    let text = std::fs::read_to_string(store.dir.join("cookbook.toml")).unwrap();
    assert!(text.contains("picture = \"illustration\""), "{text}");
    // Written the way the owner would: one [[chapters]] table per chapter.
    assert_eq!(text.matches("[[chapters]]").count(), 10, "{text}");
    assert!(text.contains("[illustrations]"), "{text}");
}

#[test]
fn note_capped_at_1000() {
    let (_d, store) = store();
    let long = "é".repeat(1500);
    let saved = added(
        store
            .put(recipe("Soup", URL, &[]), URL, None, Some(&long), at(1))
            .unwrap(),
    );
    assert_eq!(saved.note.as_ref().unwrap().chars().count(), 1000);
    // `update` caps too.
    let mut edited = saved.clone();
    edited.note = Some("x".repeat(1200));
    store.update(&edited).unwrap();
    assert_eq!(
        store
            .find(&saved.id)
            .unwrap()
            .unwrap()
            .note
            .unwrap()
            .chars()
            .count(),
        1000
    );
}

#[test]
fn dirty_set_by_put_update_remove() {
    let (_d, store) = store();
    assert!(!store.is_dirty());
    let saved = added(
        store
            .put(recipe("Soup", URL, &[]), URL, None, None, at(1))
            .unwrap(),
    );
    assert!(store.is_dirty());
    store.clear_dirty().unwrap();
    assert!(!store.is_dirty());
    store.update(&saved).unwrap();
    assert!(store.is_dirty());
    store.clear_dirty().unwrap();
    store.remove(URL).unwrap();
    assert!(store.is_dirty());
    // Clearing what isn't there is fine.
    store.clear_dirty().unwrap();
    store.clear_dirty().unwrap();
}

#[test]
fn all_is_ordered_and_skips_junk() {
    let (_d, store) = store();
    for (i, u) in ["https://a.test/1", "https://a.test/2", "https://a.test/3"]
        .iter()
        .enumerate()
    {
        store
            .put(
                recipe(&format!("R{i}"), u, &[]),
                u,
                None,
                None,
                at(3 - i as u32),
            )
            .unwrap();
    }
    std::fs::write(store.dir.join("recipes").join("notes.txt"), "hi").unwrap();
    let all = store.all().unwrap();
    assert_eq!(all.len(), 3);
    // Oldest first, so the book order never depends on the directory listing.
    assert!(all.windows(2).all(|w| w[0].added <= w[1].added));
}

#[test]
fn default_dir_follows_env_then_config_folder() {
    // Passed in rather than set with `set_var`: the process environment is shared by every test.
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Store::dir_for(Some(dir.path().into())), dir.path());
    assert!(Store::dir_for(Some("".into())).ends_with("deckle/cookbook"));
    assert!(Store::dir_for(None).ends_with("deckle/cookbook"));
}

#[test]
fn save_config_keeps_chapter_comments() {
    let (_d, store) = store();
    store.save_config(&BookConfig::default()).unwrap();
    let path = store.dir.join("cookbook.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let text = text.replacen(
        "[[chapters]]\nid = \"salads\"",
        "# kids won't eat these\n[[chapters]]\nid = \"salads\"",
        1,
    );
    std::fs::write(&path, text).unwrap();
    let mut cfg = store.config().unwrap();
    cfg.subtitle = "Gathered".to_owned();
    store.save_config(&cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# kids won't eat these"), "{text}");
    assert!(text.contains("subtitle = \"Gathered\""), "{text}");
    assert_eq!(store.config().unwrap(), cfg);
}

#[test]
fn save_config_keeps_comment_on_renamed_chapter() {
    let (_d, store) = store();
    store.save_config(&BookConfig::default()).unwrap();
    let path = store.dir.join("cookbook.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let text = text.replacen(
        "[[chapters]]\nid = \"salads\"",
        "# kids won't eat these\n[[chapters]]\nid = \"salads\"",
        1,
    );
    std::fs::write(&path, text).unwrap();
    let mut cfg = store.config().unwrap();
    let salads = cfg.chapters.iter_mut().find(|c| c.id == "salads").unwrap();
    salads.name = "Greens".to_owned();
    store.save_config(&cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("# kids won't eat these\n[[chapters]]\nid = \"salads\"\nname = \"Greens\""),
        "{text}"
    );
    assert_eq!(store.config().unwrap(), cfg);
}

#[test]
fn all_skips_appledouble_and_junk_json() {
    let (_d, store) = store();
    let saved = added(
        store
            .put(
                recipe("Tomato soup", URL, &["Soup"]),
                URL,
                None,
                None,
                at(1),
            )
            .unwrap(),
    );
    let dir = store.dir.join("recipes");
    let text = std::fs::read_to_string(dir.join(format!("{}.json", saved.id))).unwrap();
    // An AppleDouble file, a sync-conflict copy, and a stray config: none is a recipe.
    std::fs::write(dir.join(format!("._{}.json", saved.id)), [0u8, 5, 22, 7]).unwrap();
    std::fs::write(dir.join(format!("{} (conflict).json", saved.id)), &text).unwrap();
    std::fs::write(dir.join("settings.json"), "{}").unwrap();
    let all = store.all().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, saved.id);
}

#[test]
fn all_errors_on_corrupt_recipe_naming_it() {
    let (_d, store) = store();
    let dir = store.dir.join("recipes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("0123456789ab.json"), "{ not json").unwrap();
    let err = store.all().unwrap_err();
    assert!(err.contains("0123456789ab.json"), "{err}");
}

#[test]
fn empty_note_is_no_note() {
    let (_d, store) = store();
    let s = added(
        store
            .put(
                recipe("Tomato soup", URL, &[]),
                URL,
                None,
                Some("  "),
                at(1),
            )
            .unwrap(),
    );
    assert_eq!(s.note, None);
    assert_eq!(store.find(URL).unwrap().unwrap().note, None);
}

#[test]
fn remove_marks_dirty_even_if_an_image_will_not_delete() {
    let (_d, store) = store();
    let saved = added(
        store
            .put(recipe("Tomato soup", URL, &[]), URL, None, None, at(1))
            .unwrap(),
    );
    store.clear_dirty().unwrap();
    // A non-empty directory where the picture should be: deleting it fails.
    let blocker = store.image_path(&saved.id, ImageKind::Printed);
    std::fs::create_dir_all(blocker.join("x")).unwrap();
    assert!(store.remove(URL).is_err());
    assert!(store.find(URL).unwrap().is_none());
    assert!(store.is_dirty());
}
