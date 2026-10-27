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

/// Singularizes a word: `-ies` to `-y`, `-ches`/`-shes`/`-xes` to the stem, `-zzes` to `-z`, `-oes` to `-o`, `-leaves` to `-leaf`, `-loaves` to `-loaf`, a trailing `-s` (but not `-ss` or `-us`)
/// dropped.
fn singular(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies").filter(|s| !s.is_empty()) {
        return format!("{stem}y");
    }
    for (plural, one) in [("leaves", "leaf"), ("loaves", "loaf")] {
        if let Some(stem) = word.strip_suffix(plural) {
            return format!("{stem}{one}");
        }
    }
    if let Some(stem) = word.strip_suffix("oes").filter(|s| !s.is_empty()) {
        return format!("{stem}o");
    }
    // peaches, radishes, boxes, quizzes: `es` goes after ch, sh, x and z. The French ones
    // that end in a silent `e` keep it.
    if !matches!(word, "quiches" | "brioches" | "cloches")
        && ["ches", "shes", "xes", "zzes"]
            .iter()
            .any(|e| word.ends_with(e))
    {
        let stem = &word[..word.len() - 2];
        // quizzes: the doubled z was only there for the plural. Single-z words (glazes, sizes)
        // take the plain rule below.
        return stem
            .strip_suffix("zz")
            .map_or(stem.to_owned(), |q| format!("{q}z"));
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

/// Words that may precede a stop key ("kosher salt", "freshly ground black pepper").
const SEASONING_MODIFIERS: [&str; 16] = [
    "black", "white", "ground", "cracked", "freshly", "coarse", "coarsely", "fine", "finely",
    "kosher", "sea", "flaky", "table", "diamond", "crystal", "morton",
];

/// Phrases that end a line without naming an ingredient.
const TRAILING_PHRASES: [&str; 12] = [
    " to taste",
    " for serving",
    " for garnish",
    " for the pan",
    " as needed",
    " or more",
    " plus more",
    " optional",
    " if desired",
    " for frying",
    " for greasing",
    " for brushing",
];

/// Size and texture words that make a preceding "extra" part of a phrase to skip ("extra large
/// eggs", "extra firm tofu"), not part of the name. "Extra virgin" is handled by an alias.
const EXTRA_BEFORE: [&str; 8] = [
    "large", "small", "firm", "soft", "lean", "fine", "thick", "thin",
];

/// Strips serving phrases from the end of a (lowercase) line, with or without a comma.
fn without_trailing_phrases(line: &str) -> &str {
    let mut line = line;
    loop {
        line = line.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | '.' | ';'));
        match TRAILING_PHRASES.iter().find_map(|p| line.strip_suffix(p)) {
            Some(rest) => line = rest,
            None => return line,
        }
    }
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
    let mut line = line.trim();
    // "juice of 1 lemon" is lemon.
    for lead in [
        "zest and juice of ",
        "juice and zest of ",
        "zest of ",
        "juice of ",
    ] {
        if let Some(rest) = line.strip_prefix(lead) {
            line = rest.trim_start();
            break;
        }
    }
    if line.ends_with(':') || line.starts_with("for the ") || line.starts_with("for ") {
        return None;
    }
    // Serving phrases at the end of the line go first, with or without a comma.
    let line = without_trailing_phrases(line);
    // 4a. Anything after the first comma goes.
    let line = before_comma(line);

    let mut words: Vec<String> = Vec::new();
    // The leading run: quantities, units and "of" before the first real word. A unit word
    // after that is part of the name ("prime rib", "ground cloves", "fish sticks").
    let mut leading = true;
    let tokens: Vec<&str> = line
        .split_whitespace()
        .map(|raw| raw.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .collect();
    for (i, &token) in tokens.iter().enumerate() {
        if token == "extra" && tokens.get(i + 1).is_some_and(|n| EXTRA_BEFORE.contains(n)) {
            continue;
        }
        // 2. Quantities, fractions and ranges, with the "to" or "or" joining a range.
        if is_quantity(token) {
            continue;
        }
        if leading {
            if matches!(token, "to" | "or" | "x" | "of") {
                continue;
            }
            // 3. Units: "400g", "2tbsp", "9-inch" (a quantity stuck to its unit), and "cloves".
            let bare = token.trim_start_matches(|c: char| {
                c.is_numeric() || matches!(c, '.' | ',' | '/' | '⁄' | '-' | '–' | '—')
            });
            if listed(&lists.units, token)
                || (bare.len() < token.len() && !bare.is_empty() && listed(&lists.units, bare))
            {
                continue;
            }
        }
        // 4b. Preparation words (anywhere; they do not end the leading run).
        if listed(&lists.prep, token) {
            continue;
        }
        leading = false;
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
    // Seasoning modifiers count only for the stop check: "freshly ground black pepper" is
    // pepper, but "bell pepper" and "celery salt" are not.
    // Each side of an "and" is checked on its own: "sea salt and cracked pepper".
    let plain = key
        .split(" and ")
        .map(|side| {
            side.split(' ')
                .skip_while(|w| SEASONING_MODIFIERS.contains(w))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" and ");
    if lists.stop.contains(&key) || lists.stop.contains(&plain) {
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
