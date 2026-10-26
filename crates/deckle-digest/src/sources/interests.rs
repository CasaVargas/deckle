// SPDX-License-Identifier: GPL-3.0-or-later
//! Your interests (interests design §4): one section per `[[interests]]` entry, hunted across
//! Google News searches and feeds and kept only when a story actually names the interest.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::config::{Config, FullText, InterestConfig, SectionKind, section_key};
use crate::fetch::Net;
use crate::{Article, Section};

use super::feeds::{
    LEAD_TRIES, UNAVAILABLE, article, fetch_merged, full_article, section_of, title_key,
};

/// The line under every interest section's heading.
pub const KICKER: &str = "Your interests";
/// The runner's note for an interest with interests but no `"interests"` in `sections`.
pub const NOT_IN_SECTIONS: &str = "You have interests but \"interests\" isn't in sections";
/// How much of a fetched lead is checked against the interest's phrases.
const LEAD_CHECK_CHARS: usize = 600;

/// The runner's note for a quiet day: nothing relevant, nothing broken.
pub fn quiet_note(name: &str) -> String {
    format!("Nothing new about {name} today.")
}

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

/// A story's body as plain text, for the lead check.
fn plain(body: &str) -> String {
    let re = regex::Regex::new(r"(?s)<[^>]*>").unwrap();
    html_escape::decode_html_entities(&re.replace_all(body, " "))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The first `LEAD_CHECK_CHARS` of a fetched story.
fn opening(a: &Article) -> String {
    plain(&a.body).chars().take(LEAD_CHECK_CHARS).collect()
}

/// One `[[interests]]` entry as its own section, keyed and kickered (interests design §4).
/// `None` when nothing relevant was found and no source failed: a quiet day, left out of the
/// paper. `taken` holds the titles earlier sections printed; what this one keeps is added.
pub fn interest_section(
    interest: &InterestConfig,
    cfg: &Config,
    now: DateTime<Utc>,
    net: &dyn Net,
    taken: &mut HashSet<String>,
) -> Option<Section> {
    let name = interest.name.trim();
    let keyed = |mut s: Section| {
        s.key = section_key(SectionKind::Interests, Some(name));
        s.kicker = Some(KICKER.into());
        s
    };
    if name.is_empty() {
        return Some(keyed(super::unavailable("Interest", "give a name")));
    }
    let (mut items, errors) = fetch_merged(
        &interest.searches(),
        &interest.feeds,
        interest.max_age_hours,
        now,
        net,
    );
    items.retain(|i| relevant(interest, &i.title, &i.paragraphs.join(" ")));
    items.retain(|i| !taken.contains(&title_key(&i.title)));
    if items.is_empty() {
        if errors.is_empty() {
            return None;
        }
        return Some(keyed(super::unavailable(name, errors.join("; "))));
    }
    items.truncate(interest.max_items);
    let section = match interest.full() {
        FullText::Lead => {
            // The top story whole, re-checked on its own text: Google matched the headline, the
            // story may still be about the wrong Rust. A failing lead is dropped, not demoted.
            let mut whole = None;
            let mut n = 0;
            let mut tries = 0;
            while n < items.len() && tries < LEAD_TRIES {
                tries += 1;
                let a = full_article(&items[n], cfg, net);
                if a.body.contains(UNAVAILABLE) {
                    n += 1;
                    continue;
                }
                if !relevant(interest, &a.title, &opening(&a)) {
                    items.remove(n);
                    continue;
                }
                whole = Some((n, a));
                break;
            }
            if items.is_empty() {
                return None;
            }
            let brief = |i: &super::feeds::Item| {
                let mut i = i.clone();
                i.paragraphs.clear();
                article(&i, None)
            };
            let articles: Vec<Article> = match whole {
                Some((n, a)) => std::iter::once(a)
                    .chain(
                        items
                            .iter()
                            .enumerate()
                            .filter(|(k, _)| *k != n)
                            .map(|(_, i)| brief(i)),
                    )
                    .collect(),
                None => items.iter().map(brief).collect(),
            };
            Section {
                title: name.into(),
                articles,
                ..Default::default()
            }
        }
        mode => section_of(name, None, &items, &errors, cfg, net, mode),
    };
    taken.extend(items.iter().map(|i| title_key(&i.title)));
    Some(keyed(section))
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
