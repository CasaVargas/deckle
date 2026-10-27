// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kitchen Book: a cookbook built from saved recipe links
//! (docs/superpowers/specs/2026-10-26-cookbook-design.md).

pub mod add;
pub mod book;
pub mod filing;
pub mod index;
pub(crate) mod ornament;
pub mod picture;
pub mod recipe;
pub mod store;

pub use filing::{Chapter, EVERYTHING_ELSE, default_chapters, file_into};
pub use recipe::{Group, Recipe, iso_minutes, parse_recipe};

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// Lowercase words for 1..=999 ("three hundred forty-two").
fn below_thousand(n: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    if n >= 100 {
        parts.push(format!("{} hundred", ONES[n / 100]));
    }
    match n % 100 {
        0 => {}
        r if r < 20 => parts.push(ONES[r].to_owned()),
        r if r % 10 == 0 => parts.push(TENS[r / 10].to_owned()),
        r => parts.push(format!("{}-{}", TENS[r / 10], ONES[r % 10])),
    }
    parts.join(" ")
}

/// A recipe count for the cover, spelled out and capitalised: "No", "One", "Forty-seven",
/// "One hundred". From 1,000 on it is digits with a thousands comma ("1,500"): a book that
/// size reads better as a number.
pub fn count_words(n: usize) -> String {
    if n == 0 {
        return "No".to_owned();
    }
    if n >= 1_000 {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        return out;
    }
    let mut words = below_thousand(n);
    if let Some(first) = words.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    words
}

/// The cover's subtitle line: the count in words, then the configured subtitle with its
/// first letter lowercased ("Recipes, gathered" -> "Forty-seven recipes, gathered"). A
/// subtitle that starts with "Recipes" gives "recipe" when there is exactly one. An empty
/// subtitle is just the count and "recipes".
pub fn subtitle_line(subtitle: &str, count: usize) -> String {
    let subtitle = subtitle.trim();
    let subtitle = if subtitle.is_empty() {
        "Recipes"
    } else {
        subtitle
    };
    let mut chars = subtitle.chars();
    let first = chars.next().expect("not empty");
    let mut rest: String = first.to_lowercase().chain(chars).collect();
    if count == 1
        && let Some(tail) = rest.strip_prefix("recipes")
    {
        rest = format!("recipe{tail}");
    }
    format!("{} {rest}", count_words(count))
}
