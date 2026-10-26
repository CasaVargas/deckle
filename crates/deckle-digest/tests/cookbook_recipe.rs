// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading a recipe from a page's schema.org data, and falling back to its text
//! (cookbook design §4), against saved fixture pages (no network).

use std::path::Path;

use deckle_digest::cookbook::{Group, Recipe, iso_minutes, parse_recipe};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/cookbook")
            .join(name),
    )
    .unwrap()
}

const URL: &str = "https://fetch.test/recipes/page?utm_source=feed";

fn parse(name: &str) -> Recipe {
    parse_recipe(&fixture(name), URL).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn plain(items: &[&str]) -> Vec<Group> {
    vec![group(None, items)]
}

fn group(heading: Option<&str>, items: &[&str]) -> Group {
    Group {
        heading: heading.map(str::to_owned),
        items: items.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[test]
fn top_level_recipe() {
    // The page also carries a broken JSON-LD block ahead of the good one; it is skipped.
    let r = parse("top-level.html");
    assert_eq!(
        r,
        Recipe {
            title: "Brown Butter Oat Cookies".into(),
            description: Some("Chewy, nutty cookies with crisp edges.".into()),
            yield_: Some("24 cookies".into()),
            prep: Some(20),
            cook: Some(12),
            total: Some(45),
            ingredients: plain(&["1 cup butter", "2 cups rolled oats", "1 cup brown sugar"]),
            // A step with no text falls back to its name.
            steps: plain(&[
                "Brown the butter and let it cool.",
                "Stir in the sugar and oats.",
                "Bake",
            ]),
            category: vec!["Dessert".into()],
            cuisine: Some("American".into()),
            author: Some("Marta Quill".into()),
            image_url: Some("https://www.panhandle.test/img/cookies.jpg".into()),
            source_url: "https://www.panhandle.test/cookies/brown-butter-oat".into(),
            site_name: "Panhandle Kitchen".into(),
            structured: true,
            body_xhtml: None,
        }
    );
}

#[test]
fn recipe_in_array() {
    let r = parse("in-array.html");
    assert!(r.structured);
    assert_eq!(r.title, "Tomato Toast");
    assert_eq!(
        r.ingredients,
        plain(&["2 slices sourdough", "1 ripe tomato", "Flaky salt"])
    );
    assert_eq!(
        r.steps,
        plain(&["Toast the bread.", "Rub with tomato.", "Salt it."])
    );
}

#[test]
fn graph_recipe_ignores_webpage_name() {
    let r = parse("graph.html");
    assert_eq!(r.title, "Lentil Soup");
    assert_eq!(
        r.ingredients,
        plain(&["1 cup red lentils", "1 onion", "4 cups stock"])
    );
    assert_eq!(
        r.steps,
        plain(&["Sweat the onion.", "Add lentils and stock; simmer."])
    );
}

#[test]
fn type_array_contains_recipe() {
    let r = parse("type-array.html");
    assert!(r.structured);
    assert_eq!(r.title, "Pickled Onions");
    assert_eq!(r.steps[0].items.len(), 3);
}

#[test]
fn howto_sections_become_groups() {
    let r = parse("sections.html");
    assert_eq!(
        r.steps,
        vec![
            group(None, &["Read the whole recipe first."]),
            group(
                Some("Make the custard"),
                &["Warm the milk.", "Whisk in the yolks and thicken."]
            ),
            group(
                Some("Assemble"),
                &["Layer cake and berries.", "Top with custard and chill."]
            ),
        ]
    );
    // Ingredients stay one plain group.
    assert_eq!(r.ingredients.len(), 1);
    assert_eq!(r.ingredients[0].heading, None);
}

#[test]
fn instructions_as_one_string_split_on_newlines() {
    let r = parse("instructions-string.html");
    assert_eq!(
        r.steps,
        plain(&[
            "Fry the garlic until pale gold.",
            "Add the rice and stir.",
            "Cover with water and cook until tender.",
        ])
    );
}

#[test]
fn yield_array_prefers_entry_with_digit() {
    assert_eq!(
        parse("yield-array.html").yield_.as_deref(),
        Some("30 crackers")
    );
}

#[test]
fn iso_durations() {
    assert_eq!(iso_minutes("PT1H30M"), Some(90));
    assert_eq!(iso_minutes("P1DT2H"), Some(1560));
    assert_eq!(iso_minutes("PT45M"), Some(45));
    assert_eq!(iso_minutes("garbage"), None);
    assert_eq!(iso_minutes(""), None);
    assert_eq!(iso_minutes("P"), None);
    assert_eq!(iso_minutes("PT"), None);

    // total is prep + cook when the page gives none.
    let r = parse("durations.html");
    assert_eq!((r.prep, r.cook, r.total), (Some(90), Some(45), Some(135)));

    // Unparseable values are dropped; a stated total is kept as given.
    let r = parse("durations-days.html");
    assert_eq!((r.prep, r.cook, r.total), (None, Some(1560), Some(45)));
}

#[test]
fn image_string_array_and_imageobject() {
    // An array of ImageObjects: the largest width wins, even when written as a string.
    assert_eq!(
        parse("image-array.html").image_url.as_deref(),
        Some("https://img.test/galette-large.jpg")
    );
    // An array of plain strings: the first.
    assert_eq!(
        parse("image-strings.html").image_url.as_deref(),
        Some("https://img.test/fritters-1x1.jpg")
    );
    // A single ImageObject, with a relative url resolved against the page.
    assert_eq!(
        parse("image-object.html").image_url.as_deref(),
        Some("https://fetch.test/photos/crumble.jpg")
    );
    // A plain string, and no image at all.
    assert_eq!(
        parse("top-level.html").image_url.as_deref(),
        Some("https://www.panhandle.test/img/cookies.jpg")
    );
    assert_eq!(parse("in-array.html").image_url, None);
}

#[test]
fn entities_decoded_and_tags_stripped() {
    let r = parse("entities.html");
    assert_eq!(r.title, "Mom's Best Mac & Cheese");
    assert_eq!(
        r.description.as_deref(),
        Some("Creamy & sharp \u{2014} the real thing.")
    );
    assert_eq!(r.author.as_deref(), Some("Rosa & Dale"));
    assert_eq!(
        r.ingredients,
        plain(&["2 cups macaroni", "1 cup caf\u{e9} cream", "Salt & pepper"])
    );
    assert_eq!(
        r.steps,
        plain(&["Boil the pasta & drain.", "Fold in the cheese\u{2026}"])
    );
}

#[test]
fn two_recipe_nodes_takes_first_complete() {
    let r = parse("two-recipes.html");
    // The first node has no instructions; the second is the first complete one.
    assert_eq!(r.title, "First Complete Recipe");
    assert_eq!(r.ingredients, plain(&["2 eggs"]));
    assert_eq!(r.steps, plain(&["Whisk."]));
}

#[test]
fn description_capped_at_300_on_sentence_end() {
    let r = parse("long-description.html");
    let d = r.description.expect("description");
    assert!(d.chars().count() <= 300, "{} chars", d.chars().count());
    assert_eq!(
        d,
        "This honey cake is dark, spiced and keeps for a week. It is the cake we make every \
         autumn when the first cold snap arrives and the kitchen windows steam up for the whole \
         afternoon. Serve it with strong tea."
    );
    // A short description is left alone.
    assert_eq!(
        parse("top-level.html").description.as_deref(),
        Some("Chewy, nutty cookies with crisp edges.")
    );
}

#[test]
fn no_jsonld_falls_back_to_text() {
    let url = "https://blog.test/frost-stew";
    let r = parse_recipe(&fixture("no-jsonld.html"), url).unwrap();
    assert!(!r.structured);
    assert_eq!(r.title, "First Frost Squash Stew");
    assert!(r.ingredients.is_empty());
    assert!(r.steps.is_empty());
    assert_eq!((r.prep, r.cook, r.total), (None, None, None));
    assert_eq!(r.source_url, url);
    assert_eq!(r.site_name, "blog.test");
    let body = r.body_xhtml.expect("the extractor's body");
    assert!(body.contains("squash"), "{body}");
    assert!(body.contains("<p>"), "{body}");
}

#[test]
fn incomplete_recipe_data_falls_back_to_text() {
    // A Recipe node with no instructions is not usable (§4.1).
    let r = parse("incomplete.html");
    assert!(!r.structured);
    assert!(r.ingredients.is_empty());
    assert!(r.body_xhtml.is_some());
}

#[test]
fn stub_page_fails_with_120_word_reason() {
    let e = parse_recipe(&fixture("stub.html"), URL).unwrap_err();
    assert!(e.contains("no readable article"), "{e}");
    assert!(e.contains("under 120 words"), "{e}");
}

#[test]
fn canonical_url_and_og_site_name() {
    let url = "https://chickpea.test/some/page?utm=1";
    let r = parse_recipe(&fixture("canonical-og.html"), url).unwrap();
    // A relative canonical link resolves against the page.
    assert_eq!(
        r.source_url,
        "https://chickpea.test/recipes/spiced-chickpeas"
    );
    assert_eq!(r.site_name, "Chickpea Weekly");

    // Neither: the fetched address and its host.
    let url = "https://www.toast.test/t/tomato-toast";
    let r = parse_recipe(&fixture("in-array.html"), url).unwrap();
    assert_eq!(r.source_url, url);
    assert_eq!(r.site_name, "toast.test");
}

#[test]
fn legacy_ingredients_key_is_read() {
    let r = parse("legacy-ingredients.html");
    assert_eq!(
        r.ingredients,
        plain(&["200 g noodles", "2 tbsp sesame paste"])
    );
    assert_eq!(r.site_name, "Noodle Notes");
}

#[test]
fn recipe_serializes_yield_under_its_plain_name() {
    let r = parse("top-level.html");
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["yield"], "24 cookies");
    assert!(json.get("yield_").is_none());
    let back: Recipe = serde_json::from_value(json).unwrap();
    assert_eq!(back, r);
}
