// SPDX-License-Identifier: GPL-3.0-or-later
//! Which story leads the front page, for the press and the preview alike (tomorrow's paper
//! design §5.2).

use crate::Gathered;

/// Where a story comes from, for the lead rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Local,
    Reading,
    Feed,
}

/// A story that could lead: its section and article, and whether it has an opening paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub tier: Tier,
    pub section: usize,
    pub article: usize,
    pub teaser: bool,
}

/// The lead, as an index into `c` (stories in paper order):
/// 1. Local leads when it has a story;
/// 2. else the first section in paper order with one, Your Reading and the feeds alike;
///
/// within the section, its first story with an opening paragraph, else its first.
pub fn lead(c: &[Candidate]) -> Option<usize> {
    let section = c
        .iter()
        .find(|x| x.tier == Tier::Local)
        .or_else(|| c.first())?
        .section;
    let mine = || {
        c.iter()
            .enumerate()
            .filter(move |(_, x)| x.section == section)
    };
    mine()
        .find(|(_, x)| x.teaser)
        .or_else(|| mine().next())
        .map(|(i, _)| i)
}

/// Every story of the news sections and Your Reading, in paper order.
pub fn candidates(g: &Gathered) -> Vec<Candidate> {
    let mut sections: Vec<usize> = g.news.iter().copied().chain(g.reading).collect();
    sections.sort_unstable();
    sections.dedup();
    let mut out = Vec::new();
    for s in sections {
        let section = &g.edition.sections[s];
        let tier = if Some(s) == g.reading {
            Tier::Reading
        } else if section.key == "local" {
            Tier::Local
        } else {
            Tier::Feed
        };
        for (a, art) in section.articles.iter().enumerate() {
            out.push(Candidate {
                tier,
                section: s,
                article: a,
                teaser: art.teaser.is_some(),
            });
        }
    }
    out
}

/// What the front page prints: the lead, and the headline tiles under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontPlan {
    pub lead: Option<(usize, usize)>,
    pub tiles: Vec<(usize, usize)>,
}

impl FrontPlan {
    /// Whether the front page already prints this story (as the lead or a tile).
    pub fn contains(&self, s: usize, a: usize) -> bool {
        self.lead == Some((s, a)) || self.tiles.contains(&(s, a))
    }
}

/// Tiles under the lead.
const TILES: usize = 10;

/// The lead by [`lead`], then the round-robin headlines of the news sections, minus the lead.
pub fn front_plan(g: &Gathered) -> FrontPlan {
    let lead = crate::cover::lead_of(g);
    let tiles = g
        .edition
        .headlines(&g.news, TILES + 1)
        .into_iter()
        .filter(|&t| Some(t) != lead)
        .take(TILES)
        .collect();
    FrontPlan { lead, tiles }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Article, Edition, Section};

    fn c(tier: Tier, section: usize, article: usize, teaser: bool) -> Candidate {
        Candidate {
            tier,
            section,
            article,
            teaser,
        }
    }

    #[test]
    fn local_beats_saved() {
        let cs = [
            c(Tier::Reading, 0, 0, true),
            c(Tier::Local, 1, 0, false),
            c(Tier::Local, 1, 1, true),
        ];
        assert_eq!(lead(&cs), Some(2));
    }

    #[test]
    fn local_without_teaser_still_leads() {
        let cs = [c(Tier::Reading, 0, 0, true), c(Tier::Local, 1, 0, false)];
        assert_eq!(lead(&cs), Some(1));
    }

    #[test]
    fn no_local_plan_order_decides() {
        assert_eq!(
            lead(&[c(Tier::Reading, 0, 0, true), c(Tier::Feed, 1, 0, true)]),
            Some(0)
        );
        assert_eq!(
            lead(&[c(Tier::Feed, 0, 0, true), c(Tier::Reading, 1, 0, true)]),
            Some(0)
        );
    }

    #[test]
    fn teaser_preferred_within_section() {
        let cs = [
            c(Tier::Feed, 1, 0, false),
            c(Tier::Feed, 1, 1, true),
            c(Tier::Feed, 2, 0, true),
        ];
        assert_eq!(lead(&cs), Some(1));
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(lead(&[]), None);
    }

    fn story(title: &str, teaser: bool) -> Article {
        Article {
            title: title.into(),
            teaser: teaser.then(|| "Opening.".into()),
            ..Default::default()
        }
    }

    fn gathered(local: Vec<Article>) -> Gathered {
        let section = |key: &str, title: &str, articles| Section {
            brief: Vec::new(),
            key: key.into(),
            title: title.into(),
            articles,
            ..Default::default()
        };
        Gathered {
            edition: Edition {
                paper: "The Deckle Daily".into(),
                date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
                place: String::new(),
                weather_line: None,
                masthead: Default::default(),
                sections: vec![
                    section("reading", "Your Reading", vec![story("Saved", true)]),
                    section("local", "Local", local),
                    section("feeds:Science", "Science", vec![story("Comet", true)]),
                ],
                assets: Vec::new(),
            },
            news: vec![1, 2],
            reading: Some(0),
            long_reads: Vec::new(),
            reading_outcomes: Vec::new(),
            notes: Vec::new(),
        }
    }

    #[test]
    fn candidates_in_paper_order() {
        let g = gathered(vec![story("Fire", false)]);
        let tiers: Vec<(Tier, usize)> =
            candidates(&g).iter().map(|c| (c.tier, c.section)).collect();
        assert_eq!(
            tiers,
            [(Tier::Reading, 0), (Tier::Local, 1), (Tier::Feed, 2)]
        );
    }

    #[test]
    fn cover_lead_matches_rule() {
        // nothing local: the saved article, first in the paper, leads (it never used to)
        assert_eq!(crate::cover::lead_of(&gathered(Vec::new())), Some((0, 0)));
        // one local story leads, opening paragraph or not
        assert_eq!(
            crate::cover::lead_of(&gathered(vec![story("Fire", false)])),
            Some((1, 0))
        );
    }

    fn many(key: &str, n: usize) -> Section {
        Section {
            brief: Vec::new(),
            key: key.into(),
            title: key.into(),
            articles: (0..n).map(|i| story(&format!("{key}{i}"), false)).collect(),
            ..Default::default()
        }
    }

    fn big(sections: Vec<Section>) -> Gathered {
        let mut g = gathered(Vec::new());
        g.news = (0..sections.len()).collect();
        g.reading = None;
        g.edition.sections = sections;
        g
    }

    #[test]
    fn front_plan_excludes_lead_from_tiles() {
        let g = big(vec![many("local", 3), many("feeds:A", 3)]);
        let p = front_plan(&g);
        assert_eq!(p.lead, Some((0, 0)));
        assert!(!p.tiles.contains(&(0, 0)));
        assert_eq!(p.tiles.len(), 5);
        assert!(p.contains(0, 0) && p.contains(1, 2) && !p.contains(5, 5));
    }

    #[test]
    fn front_plan_caps_tiles_at_ten() {
        let g = big(vec![many("local", 20), many("feeds:A", 20)]);
        assert_eq!(front_plan(&g).tiles.len(), 10);
    }

    #[test]
    fn front_plan_round_robins_sections() {
        let g = big(vec![
            many("local", 3),
            many("feeds:A", 3),
            many("feeds:B", 3),
        ]);
        let p = front_plan(&g);
        assert_eq!(&p.tiles[..2], &[(1, 0), (2, 0)]);
    }

    #[test]
    fn front_plan_with_no_news_is_empty() {
        let g = big(vec![many("local", 0), many("feeds:A", 0)]);
        let p = front_plan(&g);
        assert_eq!(p.lead, None);
        assert!(p.tiles.is_empty());
    }
}
