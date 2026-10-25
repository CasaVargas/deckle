// SPDX-License-Identifier: GPL-3.0-or-later
//! Your interests (interests design §4): one section per `[[interests]]` entry, hunted across
//! Google News searches and feeds and kept only when a story actually names the interest.

use crate::config::InterestConfig;

/// `text` contains `phrase` case-insensitively at word boundaries: a letter or digit on either
/// side of a match rules it out, so "Rust" is in "Rust's" and "e-ink" but not in "trust".
pub fn contains_phrase(text: &str, phrase: &str) -> bool {
    let phrase = phrase.trim().to_lowercase();
    if phrase.is_empty() {
        return false;
    }
    let text = text.to_lowercase();
    let mut from = 0;
    while let Some(at) = text[from..].find(&phrase) {
        let start = from + at;
        let end = start + phrase.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric) {
            return true;
        }
        from = start + phrase.chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// The title (or a feed item's description) names the interest or one of its `also` phrases,
/// and none of its `not` phrases.
pub fn relevant(interest: &InterestConfig, title: &str, description: &str) -> bool {
    let has = |p: &str| contains_phrase(title, p) || contains_phrase(description, p);
    interest.phrases().iter().any(|p| has(p)) && !interest.not.iter().any(|n| has(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_match_at_word_boundaries() {
        assert!(contains_phrase("Why Rust is winning", "rust"));
        assert!(!contains_phrase("Trust me", "Rust"));
        assert!(!contains_phrase("Rusty nails", "Rust"));
        assert!(contains_phrase(
            "Carolina Panthers win",
            "carolina panthers"
        ));
        assert!(contains_phrase("e-ink readers compared", "e-ink"));
        assert!(contains_phrase("Panthers' QB", "Panthers"));
    }

    #[test]
    fn relevance_needs_a_phrase_and_no_not() {
        let i = InterestConfig {
            name: "Rust".into(),
            also: vec!["Cargo".into()],
            not: vec!["Rust Belt".into(), "corrosion".into()],
            ..Default::default()
        };
        assert!(relevant(&i, "Rust 2.0 lands", ""));
        assert!(relevant(&i, "Cargo gets workspaces", ""));
        assert!(relevant(&i, "New release", "Rust 2.0 is here"));
        assert!(!relevant(&i, "Rust Belt voters", ""));
        assert!(!relevant(&i, "Trusted sources", ""));
        assert!(!relevant(&i, "Rust", "corrosion study"));
    }
}
