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

const PARSED_FIXTURES: usize = 19;

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
        // A unit word after the first real word is part of the name.
        ("prime rib", "prime rib"),
        ("1 tsp ground cloves", "ground clove"),
        ("fish sticks", "fish stick"),
        ("1 head garlic", "garlic"),
        ("loaf pan", "loaf pan"),
        ("1 cinnamon stick", "cinnamon stick"),
        ("1 (9-inch) pie crust", "pie crust"),
        // "hot" and "extra" are not prep words.
        ("1 tbsp hot sauce", "hot sauce"),
        ("2 hot dogs", "hot dog"),
        ("2 tbsp extra virgin olive oil", "olive oil"),
        ("2 tbsp extra-virgin olive oil", "olive oil"),
        // Serving phrases come off the end first.
        ("Tortillas for serving", "tortilla"),
        ("Parsley as needed", "parsley"),
        ("Parsley, chopped, for garnish", "parsley"),
        ("1 cup flour or more", "flour"),
        ("Butter for the pan", "butter"),
        ("Cilantro if desired", "cilantro"),
        // Seasoning modifiers do not make a different key unless something else does.
        ("1 bell pepper", "bell pepper"),
        ("1 tsp red pepper flakes", "red pepper flake"),
        ("1 jalapeño pepper", "jalapeño pepper"),
        ("1 tsp celery salt", "celery salt"),
        ("1 cup cream of tartar", "cream of tartar"),
        // "extra" before a size or texture word is skipped.
        ("2 extra large eggs", "egg"),
        ("1 block extra firm tofu", "tofu"),
        ("2 sourdough loaves", "sourdough loaf"),
        ("2 bay leaves", "bay leaf"),
        ("3 sprigs thyme", "thyme"),
        ("2 thyme sprigs", "thyme sprig"),
        ("4 garlic cloves", "garlic"),
        ("1 tbsp butter, plus more", "butter"),
        ("1 tsp vanilla (optional)", "vanilla"),
        // Plurals: es after ch, sh, x and z; the others keep the plain rule.
        ("3 ripe peaches", "peach"),
        ("6 radishes", "radish"),
        ("2 cheese boxes", "cheese box"),
        ("2 jumbo quizzes", "jumbo quiz"),
        ("2 tbsp apricot glazes", "apricot glaze"),
        ("1 slice brioche", "brioche"),
        ("2 tomatoes", "tomato"),
        // Chili in its spellings.
        ("2 chilies", "chili"),
        ("2 chillies", "chili"),
        ("1 chilly", "chili"),
        ("1 chily", "chili"),
        // Juice and zest of a fruit is the fruit.
        ("juice of 1 lemon", "lemon"),
        ("Zest of 2 limes", "lime"),
        ("zest and juice of 1 orange", "orange"),
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
        "Salt to taste",
        "Salt and pepper to taste",
        "Freshly ground black pepper",
        "Coarse kosher salt",
        "Fine sea salt",
        "Diamond Crystal kosher salt",
        "Cracked black pepper",
        "Flaky salt",
        "Kosher salt and freshly ground black pepper to taste",
        "salt and black pepper",
        "sea salt and cracked pepper",
        "Oil for frying",
        "Sugar substitute",
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
    // The fixtures that are not recipes at all fail to parse and are skipped.
    assert_eq!(all.len(), PARSED_FIXTURES, "fixtures that parsed");
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
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(text, want, "regenerate with UPDATE_SNAPSHOT=1");
}
