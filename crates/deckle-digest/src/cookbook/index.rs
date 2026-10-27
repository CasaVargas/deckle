// SPDX-License-Identifier: GPL-3.0-or-later
//! The index by ingredient (cookbook design §6.4): each ingredient line reduces to a key
//! ingredient with a small deterministic normalizer, and the book lists keys alphabetically
//! with the recipes that use them. The word lists are plain text in `words/`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use super::store::Saved;

struct Lists {
    units: HashSet<String>,
    prep: HashSet<String>,
    stop: HashSet<String>,
    aliases: HashMap<String, String>,
}

/// The non-blank, non-comment lines of a word list.
fn entries(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
}

fn set(text: &str) -> HashSet<String> {
    entries(text).map(str::to_lowercase).collect()
}

fn lists() -> &'static Lists {
    static LISTS: OnceLock<Lists> = OnceLock::new();
    LISTS.get_or_init(|| Lists {
        units: set(include_str!("words/units.txt")),
        prep: set(include_str!("words/prep.txt")),
        stop: set(include_str!("words/stop.txt")),
        aliases: entries(include_str!("words/aliases.txt"))
            .filter_map(|l| l.split_once('='))
            .map(|(from, to)| (from.trim().to_lowercase(), to.trim().to_lowercase()))
            .collect(),
    })
}

/// A quantity word: digits, decimals, ASCII or Unicode fractions, ranges ("1-2", "1–2").
fn is_quantity(token: &str) -> bool {
    !token.is_empty()
        && token.chars().any(char::is_numeric)
        && token
            .chars()
            .all(|c| c.is_numeric() || matches!(c, '.' | ',' | '/' | '⁄' | '-' | '–' | '—'))
}

/// Singularizes a word: `-ies` to `-y`, `-oes` to `-o`, a trailing `-s` (but not `-ss` or `-us`)
/// dropped.
fn singular(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies").filter(|s| !s.is_empty()) {
        return format!("{stem}y");
    }
    if let Some(stem) = word.strip_suffix("oes").filter(|s| !s.is_empty()) {
        return format!("{stem}o");
    }
    if word.len() > 2 && word.ends_with('s') && !word.ends_with("ss") && !word.ends_with("us") {
        return word[..word.len() - 1].to_owned();
    }
    word.to_owned()
}

fn listed(list: &HashSet<String>, token: &str) -> bool {
    list.contains(token) || list.contains(&singular(token))
}

/// Drops everything inside parentheses (nested ones too).
fn without_parens(line: &str) -> String {
    let mut depth = 0u32;
    line.chars()
        .filter(|&c| match c {
            '(' => {
                depth += 1;
                false
            }
            ')' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect()
}

/// Cuts the line at its first comma (not one inside a number such as "1,000") or semicolon.
fn before_comma(line: &str) -> &str {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    for (i, &(at, c)) in chars.iter().enumerate() {
        let digit_before = i > 0 && chars[i - 1].1.is_ascii_digit();
        let digit_after = chars.get(i + 1).is_some_and(|n| n.1.is_ascii_digit());
        if c == ';' || (c == ',' && !(digit_before && digit_after)) {
            return &line[..at];
        }
    }
    line
}

/// Reduces an ingredient line to the key ingredient it is filed under, or `None` when nothing
/// worth indexing is left (a heading, a bare quantity, or only stop words such as "salt").
pub fn key_ingredient(line: &str) -> Option<String> {
    let lists = lists();
    // 1. Lowercase; drop anything in parentheses.
    let line = without_parens(&line.to_lowercase().replace('&', " and "));
    // A heading such as "For the dressing:" is not an ingredient.
    let line = line.trim();
    if line.ends_with(':') || line.starts_with("for the ") || line.starts_with("for ") {
        return None;
    }
    // 4a. Anything after the first comma goes.
    let line = before_comma(line);

    let mut words: Vec<String> = Vec::new();
    let mut leading = true;
    for raw in line.split_whitespace() {
        let token = raw.trim_matches(|c: char| !c.is_alphanumeric());
        if token.is_empty() {
            continue;
        }
        // 2. Leading quantities, fractions and ranges, with the "to" or "or" joining a range.
        if is_quantity(token) {
            continue;
        }
        if leading && matches!(token, "to" | "or" | "x") {
            continue;
        }
        leading = false;
        // "400g", "2tbsp", "9-inch": a quantity stuck to its unit.
        let bare = token.trim_start_matches(|c: char| {
            c.is_numeric() || matches!(c, '.' | ',' | '/' | '⁄' | '-' | '–' | '—')
        });
        let token = if bare.len() < token.len() && !bare.is_empty() && listed(&lists.units, bare) {
            bare
        } else {
            token
        };
        // 3 and 4b. Units and preparation words.
        if listed(&lists.units, token) || listed(&lists.prep, token) {
            continue;
        }
        words.push(token.to_owned());
    }
    // "flour or cornstarch": the first alternative is the key; and/or never lead or trail.
    if let Some(at) = words.iter().position(|w| w == "or") {
        words.truncate(at);
    }
    while words.first().is_some_and(|w| w == "and") {
        words.remove(0);
    }
    while words.last().is_some_and(|w| w == "and") {
        words.pop();
    }
    // 5. Singularize the last word.
    let last = words.pop()?;
    words.push(singular(&last));
    let key = words.join(" ");
    if lists.stop.contains(&key) {
        return None;
    }
    // 6. Aliases.
    Some(lists.aliases.get(&key).cloned().unwrap_or(key))
}

/// The ingredient index: (key, ids of the recipes that use it), keys alphabetical and each
/// key's recipes sorted by displayed title, case-insensitively. Unstructured recipes have no
/// ingredient list and are skipped.
pub fn index(recipes: &[&Saved]) -> Vec<(String, Vec<String>)> {
    let mut keys: BTreeMap<String, Vec<&Saved>> = BTreeMap::new();
    for saved in recipes.iter().filter(|s| s.recipe.structured) {
        for line in saved.recipe.ingredients.iter().flat_map(|g| &g.items) {
            let Some(key) = key_ingredient(line) else {
                continue;
            };
            let users = keys.entry(key).or_default();
            if !users.iter().any(|u| u.id == saved.id) {
                users.push(saved);
            }
        }
    }
    let shown = |s: &Saved| {
        s.title_override
            .clone()
            .unwrap_or_else(|| s.recipe.title.clone())
            .to_lowercase()
    };
    keys.into_iter()
        .map(|(key, mut users)| {
            users.sort_by_cached_key(|s| (shown(s), s.id.clone()));
            (key, users.into_iter().map(|s| s.id.clone()).collect())
        })
        .collect()
}
