// SPDX-License-Identifier: GPL-3.0-or-later
//! Chapters and filing (cookbook design §6.2): the default chapter table, and the rule that
//! files a recipe into one of them once, when it is first saved.

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// The id of the catch-all chapter. It has no keywords, can be renamed and cannot be deleted
/// (§6.2).
pub const EVERYTHING_ELSE: &str = "other";

/// One chapter of the book, as written to `cookbook.toml` (§5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub name: String,
    /// Keywords matched, case-insensitively and as whole words, against a recipe's categories
    /// and then its title. Written as `match` in the config.
    #[serde(rename = "match", default)]
    pub match_: Vec<String>,
}

/// The default chapters, in book order (§6.2).
pub fn default_chapters() -> Vec<Chapter> {
    const TABLE: &[(&str, &str, &[&str])] = &[
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
        (EVERYTHING_ELSE, "Everything else", &[]),
    ];
    TABLE
        .iter()
        .map(|(id, name, keywords)| Chapter {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            match_: keywords.iter().map(|k| (*k).to_owned()).collect(),
        })
        .collect()
}

/// A chapter's keywords as one whole-word, case-insensitive pattern; `None` when it has none.
fn keyword_pattern(chapter: &Chapter) -> Option<Regex> {
    let alternatives: Vec<String> = chapter
        .match_
        .iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .map(regex::escape)
        .collect();
    if alternatives.is_empty() {
        return None;
    }
    RegexBuilder::new(&format!(r"\b(?:{})\b", alternatives.join("|")))
        .case_insensitive(true)
        .build()
        .ok()
}

/// The id of the chapter a recipe belongs in (§6.2). The first chapter, in book order, whose
/// keywords match a category wins; failing that, the first whose keywords match the title;
/// failing that, [`EVERYTHING_ELSE`]. Sites often write several categories as one
/// comma-separated string ("Dinner, Main Course"), so each category is split on commas first.
pub fn file_into(categories: &[String], title: &str, chapters: &[Chapter]) -> String {
    let patterns: Vec<(&Chapter, Regex)> = chapters
        .iter()
        .filter_map(|c| keyword_pattern(c).map(|re| (c, re)))
        .collect();
    let first_match = |text: &str| {
        patterns
            .iter()
            .find(|(_, re)| re.is_match(text))
            .map(|(c, _)| c.id.clone())
    };
    let pieces = categories
        .iter()
        .flat_map(|c| c.split(','))
        .map(str::trim)
        .filter(|p| !p.is_empty());
    // A chapter earlier in the book beats one that merely matched an earlier category.
    let by_category = patterns
        .iter()
        .find(|(_, re)| pieces.clone().any(|p| re.is_match(p)))
        .map(|(c, _)| c.id.clone());
    by_category
        .or_else(|| first_match(title))
        .unwrap_or_else(|| EVERYTHING_ELSE.to_owned())
}
