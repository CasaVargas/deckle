// SPDX-License-Identifier: GPL-3.0-or-later
//! Filing a recipe into a chapter (cookbook design §6.2).

use deckle_digest::cookbook::{Chapter, EVERYTHING_ELSE, default_chapters, file_into};

fn file(categories: &[&str], title: &str) -> String {
    let categories: Vec<String> = categories.iter().map(|c| c.to_string()).collect();
    file_into(&categories, title, &default_chapters())
}

#[test]
fn filing_table() {
    let cases: &[(&[&str], &str, &str)] = &[
        (&["Soup"], "Anything", "soups"),
        (&["Main Course", "Dinner"], "Anything", "mains"),
        (&["Dessert", "Cake"], "Anything", "desserts"),
        (&[], "Banana bread", "baking"),
        (&["Appetizer"], "Crostini", "other"),
        // "soupy" is not the whole word "soup".
        (&["Soupy pasta"], "Anything", "other"),
        // Book order wins, not category order.
        (&["Breakfast", "Bread"], "Anything", "breakfast"),
        (&["Bread", "Breakfast"], "Anything", "breakfast"),
        // Case-insensitive; accented keyword.
        (&["SOUP"], "Anything", "soups"),
        (&["Entrée"], "Anything", "mains"),
        // Categories beat the title.
        (&["Dessert"], "Tomato soup", "desserts"),
    ];
    for (categories, title, want) in cases {
        assert_eq!(&file(categories, title), want, "{categories:?} / {title}");
    }
}

#[test]
fn comma_separated_category_string() {
    assert_eq!(file(&["Dinner, Main Course"], "Anything"), "mains");
    assert_eq!(file(&["Appetizer , Soup"], "Anything"), "soups");
}

#[test]
fn keywords_are_literal_not_regex() {
    let chapters = vec![
        Chapter {
            id: "odd".into(),
            name: "Odd".into(),
            match_: vec!["a.c".into()],
        },
        Chapter {
            id: EVERYTHING_ELSE.into(),
            name: "Everything else".into(),
            match_: vec![],
        },
    ];
    assert_eq!(file_into(&["abc".into()], "x", &chapters), "other");
    assert_eq!(file_into(&["a.c".into()], "x", &chapters), "odd");
}

#[test]
fn default_chapters_match_spec_table() {
    let got: Vec<(String, String, Vec<String>)> = default_chapters()
        .into_iter()
        .map(|c| (c.id, c.name, c.match_))
        .collect();
    let want: &[(&str, &str, &[&str])] = &[
        ("breakfast", "Breakfast", &["breakfast", "brunch"]),
        (
            "soups",
            "Soups and stews",
            &["soup", "stew", "chili", "chowder", "bisque"],
        ),
        ("salads", "Salads", &["salad"]),
        (
            "mains",
            "Mains",
            &["main", "dinner", "entree", "entrée", "supper"],
        ),
        ("sides", "Sides", &["side", "vegetable"]),
        (
            "baking",
            "Baking",
            &["bread", "baking", "loaf", "muffin", "scone", "biscuit"],
        ),
        (
            "desserts",
            "Desserts",
            &["dessert", "cake", "cookie", "pie", "tart", "pudding"],
        ),
        (
            "drinks",
            "Drinks",
            &["drink", "cocktail", "beverage", "smoothie"],
        ),
        (
            "sauces",
            "Sauces and basics",
            &["sauce", "dressing", "condiment", "stock", "spice"],
        ),
        ("other", "Everything else", &[]),
    ];
    let want: Vec<(String, String, Vec<String>)> = want
        .iter()
        .map(|(i, n, m)| {
            (
                i.to_string(),
                n.to_string(),
                m.iter().map(|k| k.to_string()).collect(),
            )
        })
        .collect();
    assert_eq!(got, want);
}

#[test]
fn catch_all_present() {
    let chapters = default_chapters();
    let last = chapters.last().unwrap();
    assert_eq!(last.id, EVERYTHING_ELSE);
    assert_eq!(EVERYTHING_ELSE, "other");
    assert!(last.match_.is_empty());
}

#[test]
fn chapter_serializes_match_key() {
    let chapter = Chapter {
        id: "soups".into(),
        name: "Soups".into(),
        match_: vec!["soup".into()],
    };
    let json = serde_json::to_value(&chapter).unwrap();
    assert_eq!(json["match"], serde_json::json!(["soup"]));
    assert!(json.get("match_").is_none());
    let back: Chapter = serde_json::from_value(json).unwrap();
    assert_eq!(back, chapter);
}
