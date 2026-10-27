// SPDX-License-Identifier: GPL-3.0-or-later
//! The ingredient index (cookbook design §6.4): the key-ingredient normalizer and the
//! key -> recipes listing, plus a snapshot over the fixture pages that a reviewer reads for junk.

use std::path::Path;

use chrono::{TimeZone, Utc};
use deckle_digest::cookbook::index::{index, key_ingredient};
use deckle_digest::cookbook::store::{Picture, Saved, recipe_id};
use deckle_digest::cookbook::{Group, parse_recipe};

fn saved(id: &str, title: &str, structured: bool, lines: &[&str]) -> Saved {
    let mut recipe = blank_recipe();
    recipe.title = title.to_owned();
    recipe.structured = structured;
    recipe.ingredients = vec![Group {
        heading: None,
        items: lines.iter().map(|s| s.to_string()).collect(),
    }];
    Saved {
        id: id.to_owned(),
        recipe,
        chapter: "mains".to_owned(),
        picture: Picture::Default,
        note: None,
        title_override: None,
        added: Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap(),
    }
}

fn blank_recipe() -> deckle_digest::cookbook::Recipe {
    deckle_digest::cookbook::Recipe {
        title: String::new(),
        description: None,
        yield_: None,
        prep: None,
        cook: None,
        total: None,
        ingredients: vec![],
        steps: vec![],
        category: vec![],
        cuisine: None,
        author: None,
        image_url: None,
        source_url: "https://x.test/".to_owned(),
        site_name: "x.test".to_owned(),
        structured: true,
        body_xhtml: None,
    }
}

#[test]
fn key_ingredient_table() {
    let cases = [
        ("1 butternut squash", "butternut squash"),
        ("2 tbsp olive oil", "olive oil"),
        ("3 cloves garlic, minced", "garlic"),
        ("½ cup heavy cream", "heavy cream"),
        ("1-2 large onions, diced", "onion"),
        ("4 scallions", "green onion"),
        ("1 (14 oz) can chickpeas", "chickpea"),
        // Quantities in every spelling.
        ("1/2 cup sugar", "sugar"),
        ("1 1/2 cups flour", "flour"),
        ("1½ cups milk", "milk"),
        ("2.5 lb potatoes", "potato"),
        ("1–2 tomatoes", "tomato"),
        ("1 to 2 lemons", "lemon"),
        ("400g spaghetti", "spaghetti"),
        ("3 ripe berries", "berry"),
        ("hummus", "hummus"),
    ];
    for (line, want) in cases {
        assert_eq!(key_ingredient(line).as_deref(), Some(want), "{line:?}");
    }
    // The stop list matches whole keys only.
    assert_eq!(key_ingredient("1 tbsp oil"), None);
}

#[test]
fn key_ingredient_rejects_junk() {
    for line in [
        "Salt and pepper, to taste",
        "2 (14 oz) cans",
        "For the dressing:",
        "",
        "   ",
        "and",
        "1/2",
    ] {
        assert_eq!(key_ingredient(line), None, "{line:?}");
    }
}

#[test]
fn index_skips_unstructured() {
    let a = saved("aaa", "Soup", true, &["1 onion"]);
    let b = saved("bbb", "Pasted text", false, &["1 onion", "2 leeks"]);
    let got = index(&[&a, &b]);
    assert_eq!(got, vec![("onion".to_owned(), vec!["aaa".to_owned()])]);
}

#[test]
fn index_sorted_with_links() {
    let a = saved("a1", "zucchini bread", true, &["2 eggs", "1 onion"]);
    let b = saved("b2", "Apple cake", true, &["3 eggs", "3 eggs", "2 apples"]);
    let mut c = saved("c3", "Plain", true, &["1 egg"]);
    c.title_override = Some("Breakfast egg".to_owned());
    let got = index(&[&a, &b, &c]);
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        got,
        vec![
            ("apple".to_owned(), ids(&["b2"])),
            // Sorted by displayed title, case-insensitively; a recipe is listed once per key.
            ("egg".to_owned(), ids(&["b2", "c3", "a1"])),
            ("onion".to_owned(), ids(&["a1"])),
        ]
    );
}

#[test]
fn fixture_index_snapshot() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cookbook");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".html"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 20);

    let all: Vec<Saved> = names
        .iter()
        .filter_map(|name| {
            let url = format!("https://fixture.test/{name}");
            let html = std::fs::read_to_string(dir.join(name)).unwrap();
            let recipe = parse_recipe(&html, &url).ok()?;
            Some(Saved {
                id: recipe_id(&url),
                recipe,
                chapter: "mains".to_owned(),
                picture: Picture::Default,
                note: None,
                title_override: None,
                added: Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap(),
            })
        })
        .collect();
    let refs: Vec<&Saved> = all.iter().collect();
    let title = |id: &str| {
        all.iter()
            .find(|s| s.id == id)
            .map(|s| s.recipe.title.clone())
            .unwrap()
    };
    let mut text = String::new();
    for (key, ids) in index(&refs) {
        let titles: Vec<String> = ids.iter().map(|id| title(id)).collect();
        text.push_str(&format!("{key}: {}\n", titles.join(", ")));
    }
    let path = dir.join("index.txt");
    if std::env::var_os("UPDATE_SNAPSHOT").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(text, want, "regenerate with UPDATE_SNAPSHOT=1");
}
