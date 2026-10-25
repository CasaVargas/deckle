// SPDX-License-Identifier: GPL-3.0-or-later
//! Theme answers: words from yesterday's headlines, clued as the headline with the word blanked
//! (puzzle design §4, "Theme words").

use serde::{Deserialize, Serialize};

use super::words::Words;
use crate::Edition;

/// A word from a headline, kept for tomorrow's mini.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThemeWord {
    pub word: String,
    pub headline: String,
    /// The section's title: "Local", a feed's name.
    pub section: String,
}

/// Words a headline is made of but that say nothing about the news.
pub const STOP: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "these", "those", "into", "onto", "over",
    "under", "about", "after", "before", "amid", "says", "said", "say", "tells", "told", "asks",
    "asked", "new", "news", "will", "would", "could", "should", "can", "may", "might", "must",
    "are", "was", "were", "has", "have", "had", "been", "being", "not", "but", "its", "his", "her",
    "hers", "him", "she", "they", "them", "their", "you", "your", "our", "ours", "who", "what",
    "when", "where", "why", "how", "which", "all", "any", "some", "more", "most", "than", "then",
    "out", "off", "just", "now", "here", "there", "still", "also", "per", "via", "gets", "get",
    "got", "set", "sets", "makes", "make", "made", "take", "takes", "took", "back", "year",
    "years", "week", "weeks", "day", "days", "today", "first", "last", "next", "live", "watch",
    "video", "photos", "update", "report",
];

/// Every word of the given sections' headlines that is in the list, 3 to 5 letters, not a
/// stop word and not part of the edition's place name or the section's title; the first time
/// each word appears.
/// `sections` are the news sections (and Your Reading), in paper order.
pub fn harvest(e: &Edition, sections: &[usize], words: &Words) -> Vec<ThemeWord> {
    let place: Vec<String> = tokens(&e.place).map(|t| t.to_lowercase()).collect();
    let mut out: Vec<ThemeWord> = Vec::new();
    for &i in sections {
        let Some(section) = e.sections.get(i) else {
            continue;
        };
        // "Rain" in a feed called "Rain Gauge": the clue names the section, so it would show
        let named: Vec<String> = tokens(&section.title).map(|t| t.to_lowercase()).collect();
        for a in &section.articles {
            for t in tokens(&a.title) {
                let w = t.to_lowercase();
                if (3..=5).contains(&w.len())
                    && words.contains(&w)
                    && !STOP.contains(&w.as_str())
                    && !place.contains(&w)
                    && !named.contains(&w)
                    && !out.iter().any(|x| x.word == w)
                {
                    out.push(ThemeWord {
                        word: w,
                        headline: a.title.clone(),
                        section: section.title.clone(),
                    });
                }
            }
        }
    }
    out
}

/// A headline's words: runs of letters ("Leaf's" is "Leaf" and "s").
fn tokens(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !c.is_alphabetic())
        .filter(|t| !t.is_empty())
}

/// Up to `max` theme words: longer first, one per section, ties in headline order.
pub fn pick(found: &[ThemeWord], max: usize) -> Vec<ThemeWord> {
    let mut by_len: Vec<&ThemeWord> = found.iter().collect();
    by_len.sort_by_key(|t| std::cmp::Reverse(t.word.len())); // stable: headline order
    let mut out: Vec<ThemeWord> = Vec::new();
    for t in by_len {
        if out.len() < max && !out.iter().any(|x| x.section == t.section) {
            out.push(t.clone());
        }
    }
    out
}

/// Headlines longer than this are cut around the blank.
const MAX_HEADLINE: usize = 90;
const BLANK: &str = "____";

/// "Leaf ____ peaks on the ridge this weekend (Local, yesterday)": the headline with every
/// whole-word use of the answer blanked, whatever its case.
pub fn clue(t: &ThemeWord) -> String {
    let mut blanked = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.eq_ignore_ascii_case(&t.word) {
            out.push_str(BLANK);
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in t.headline.chars() {
        if c.is_alphanumeric() {
            word.push(c);
        } else {
            flush(&mut word, &mut blanked);
            blanked.push(c);
        }
    }
    flush(&mut word, &mut blanked);
    format!("{} ({}, yesterday)", cut(&blanked), t.section)
}

/// At most [`MAX_HEADLINE`] characters at word boundaries, keeping the first blank, with "…"
/// where text was dropped.
fn cut(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= MAX_HEADLINE {
        return s.to_owned();
    }
    let blank = s.find(BLANK).map_or(0, |b| s[..b].chars().count());
    // the window starts at the headline's start when the blank fits there, else a little
    // before the blank; each dropped end costs one character for its "…"
    let mut start = 0;
    if blank + BLANK.len() > MAX_HEADLINE - 1 {
        start = blank.saturating_sub(MAX_HEADLINE / 3);
        while start > 0 && start < blank && chars[start - 1] != ' ' {
            start += 1;
        }
    }
    let lead = usize::from(start > 0);
    let mut end = (start + MAX_HEADLINE - lead).min(chars.len());
    if end < chars.len() {
        end -= 1; // room for the trailing "…"
        while end > blank + BLANK.len() && chars[end] != ' ' {
            end -= 1;
        }
    }
    let body: String = chars[start..end].iter().collect();
    let body = body.trim_end_matches([' ', ',', ';', ':', '-', '—']);
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        body,
        if end < chars.len() { "…" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Article, Section};
    use chrono::NaiveDate;

    fn words() -> Words {
        Words::from_tsv(
            "leaf\tA green organ\nridge\tA long hill\nrain\tWater from clouds\nstorm\tBad weather\n\
             vote\tA choice\nboard\tA plank\nthe\tDefinite article\nsays\tSpeaks\nnew\tFresh\n\
             pine\tA tree\nwin\tTo succeed\njackpot\tToo long\nodds\tChances\nlow\tNot high\n",
        )
    }

    fn section(title: &str, headlines: &[&str]) -> Section {
        Section {
            brief: Vec::new(),
            key: title.to_lowercase(),
            title: title.into(),
            articles: headlines
                .iter()
                .map(|h| Article {
                    title: (*h).into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn edition() -> (Edition, Vec<usize>) {
        let e = Edition {
            large_print: false,
            paper: "The Deckle Daily".into(),
            date: NaiveDate::from_ymd_opt(2026, 10, 21).unwrap(),
            place: "Pine Ridge, NC".into(),
            weather_line: None,
            masthead: Default::default(),
            sections: vec![
                section("Weather", &["Rain and storm, low of 40"]),
                section(
                    "Local",
                    &[
                        "Leaf season peaks on the ridge this weekend",
                        "School board says new vote set",
                    ],
                ),
                section("Lottery", &["Win the jackpot? Odds are low"]),
                section("Hacker News", &["A storm of new rain data"]),
            ],
            assets: Vec::new(),
        };
        (e, vec![1, 3])
    }

    #[test]
    fn harvest_takes_list_words_from_news_headlines() {
        let (e, news) = edition();
        let found = harvest(&e, &news, &words());
        let ws: Vec<(&str, &str)> = found
            .iter()
            .map(|t| (t.word.as_str(), t.section.as_str()))
            .collect();
        assert_eq!(
            ws,
            [
                ("leaf", "Local"),
                ("board", "Local"),
                ("vote", "Local"),
                ("storm", "Hacker News"),
                ("rain", "Hacker News"),
            ]
        );
        assert_eq!(
            found[0].headline,
            "Leaf season peaks on the ridge this weekend"
        );
        // stop words ("the", "says", "new") and the place ("Pine Ridge") are never answers,
        // and Weather and Lottery aren't news
        for w in ["the", "says", "new", "ridge", "pine", "win", "odds", "low"] {
            assert!(found.iter().all(|t| t.word != w), "{w}");
        }
    }

    #[test]
    fn pick_prefers_long_words_one_per_section() {
        let (e, news) = edition();
        let found = harvest(&e, &news, &words());
        let picked = pick(&found, 2);
        let picked: Vec<&str> = picked.iter().map(|t| t.word.as_str()).collect();
        // "board" and "storm" are both five letters: one each from Local and Hacker News
        assert_eq!(picked, ["board", "storm"]);
        assert_eq!(pick(&found, 1).len(), 1);
        assert!(pick(&[], 2).is_empty());
    }

    #[test]
    fn clue_blanks_every_form() {
        let t = ThemeWord {
            word: "rain".into(),
            headline: "Rain, rain and more RAIN for Rains County".into(),
            section: "Local".into(),
        };
        assert_eq!(
            clue(&t),
            "____, ____ and more ____ for Rains County (Local, yesterday)"
        );
    }

    #[test]
    fn clue_cuts_long_headlines_keeping_the_blank() {
        let headline = "County commissioners meet again on Tuesday night to weigh the long-delayed \
                        plan for the old mill site, with a vote on the rezoning and a public \
                        hearing to follow";
        assert!(headline.chars().count() > 140);
        let t = ThemeWord {
            word: "vote".into(),
            headline: headline.into(),
            section: "Local".into(),
        };
        let c = clue(&t);
        let (body, rest) = c.rsplit_once(" (").unwrap();
        assert_eq!(rest, "Local, yesterday)");
        assert!(body.chars().count() <= 90, "{body}");
        // the word is near the end: the start goes, marked with "…"
        assert!(body.starts_with('…'), "{body}");
        assert!(body.contains("____"), "{body}");
        assert!(!body.to_lowercase().contains("vote"), "{body}");
        // a word near the start keeps the start
        let t = ThemeWord {
            word: "meet".into(),
            ..t
        };
        let c = clue(&t);
        assert!(c.starts_with("County commissioners ____ again"), "{c}");
        assert!(c.split(" (").next().unwrap().ends_with('…'));
    }
}
