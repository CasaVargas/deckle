// SPDX-License-Identifier: GPL-3.0-or-later
//! Your Reading (save-to-paper design §5): saved articles, whole, with their photos. A piece
//! longer than `reading.long_read_words` becomes its own book; the paper points to it.

use chrono::Local;

use crate::config::Config;
use crate::fetch::Net;
use crate::publisher::Access;
use crate::reading::{Entry, MAX_TRIES, Outcome};
use crate::sources::feeds::{by_line, host, par_map, story_body};
use crate::{Article, Section, article, esc};

pub const TITLE: &str = "Your Reading";
/// Long reads land on the Kindle as `deckle-reads-YYYY-MM-DD-<slug>`: outside the paper's
/// replace-and-prune, never pruned.
pub const READS_PREFIX: &str = "deckle-reads-";
const WORDS_PER_MINUTE: usize = 230;

/// A saved article that became its own book.
#[derive(Clone, Debug)]
pub struct LongRead {
    pub id: String,
    pub title: String,
    pub byline: Option<String>,
    pub outlet: String,
    pub words: usize,
    /// The whole article, photos placed as in the paper.
    pub article: Article,
    pub file_stem: String,
}

pub fn minutes(words: usize) -> usize {
    (words / WORDS_PER_MINUTE).max(1)
}

/// Lowercase ASCII words joined by hyphens, at most 40 characters.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for word in deunicode::deunicode(title)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let next = if out.is_empty() {
            word.to_ascii_lowercase()
        } else {
            format!("{out}-{}", word.to_ascii_lowercase())
        };
        if next.len() > 40 {
            break;
        }
        out = next;
    }
    if out.is_empty() {
        "article".into()
    } else {
        out
    }
}

enum Got {
    Story(Article),
    Long(Box<LongRead>, Article),
    /// Gave up after the last try: a headline with the reason.
    GaveUp(Article, String),
    Retry(String),
    /// A subscriber page the site withheld: not a failed try, so it waits without one.
    Withheld(String),
}

fn fetch_html(e: &Entry, net: &dyn Net) -> Result<(String, String, Access), String> {
    if let Some(file) = &e.file {
        let bytes = std::fs::read(file).map_err(|err| format!("{}: {err}", file.display()))?;
        let base = url::Url::from_file_path(file)
            .map_err(|_| format!("{}: not an absolute path", file.display()))?;
        return Ok((
            String::from_utf8_lossy(&bytes).into_owned(),
            base.to_string(),
            Access::Open,
        ));
    }
    let url = e.url.as_deref().ok_or("nothing to fetch")?;
    let url = if article::is_google_news(url) {
        article::resolve_google_news(url, net).ok_or("couldn't follow the Google News link")?
    } else {
        url.to_owned()
    };
    // A walled page is still extracted: it's what a stranger sees, and a free story reads.
    let (bytes, access) = net.get_page_as(&url)?;
    Ok((String::from_utf8_lossy(&bytes).into_owned(), url, access))
}

fn one(e: &Entry, cfg: &Config, net: &dyn Net, date: chrono::NaiveDate) -> Got {
    let outlet = e
        .url
        .as_deref()
        .map(|u| host(u).to_owned())
        .unwrap_or_else(|| "a saved page".into());
    let saved = e
        .saved_at
        .with_timezone(&Local)
        .format("Saved %a %-I:%M %p")
        .to_string();
    let inline = cfg.reading.inline.contains(&e.id);
    // Inline: printed in the paper, so the paper's 6,000-word cap applies.
    let cap = if inline {
        Some(article::MAX_WORDS)
    } else {
        None
    };
    let extracted = match fetch_html(e, net) {
        Ok((html, base, access)) => match article::extract_with(&html, &base, cap) {
            Some(x) => Ok((x, access)),
            None => match &access {
                // The site withheld it from a stranger: wait, without spending a try.
                Access::Walled { site, why } => {
                    return Got::Withheld(format!("no readable article: {}", why.note(site)));
                }
                _ => Err("no readable article (a paywall, a sign-in page, or a video)".to_owned()),
            },
        },
        Err(why) => Err(why),
    };
    let (x, access) = match extracted {
        Ok(got) => got,
        Err(why) => {
            // The try this edition makes; a same-day rebuild doesn't count again.
            let tries = e.tries + u32::from(e.tried_on != Some(date));
            if tries < MAX_TRIES {
                return Got::Retry(why);
            }
            let title = e.title.clone().or(e.url.clone()).unwrap_or_default();
            let body = format!(
                "<p class=\"note\">Deckle couldn't get this one after {MAX_TRIES} tries: {}.</p>",
                esc(&why)
            );
            return Got::GaveUp(
                Article {
                    title,
                    meta: Some(format!("{saved} · {outlet}")),
                    body,
                    ..Default::default()
                },
                why,
            );
        }
    };
    let title = e
        .title
        .clone()
        .or(x.title.clone())
        .unwrap_or_else(|| outlet.clone());
    let meta = format!("{saved} · {outlet} · about {} min", minutes(x.words));
    if x.words > cfg.reading.long_read_words && !inline {
        let (body, photos, mut extra) = story_body(&x, &outlet, cfg, net);
        extra.access = access;
        let lr = LongRead {
            id: e.id.clone(),
            title: title.clone(),
            byline: x.byline.clone(),
            outlet: outlet.clone(),
            words: x.words,
            file_stem: format!("{READS_PREFIX}{date}-{}", slug(&title)),
            article: Article {
                title: title.clone(),
                meta: x.byline.as_deref().map(by_line),
                body,
                teaser: x.teaser.clone(),
                photos,
                url: e.url.clone(),
                outlet: Some(outlet.clone()),
                extra,
            },
        };
        let pointer = Article {
            title: title.clone(),
            meta: Some(meta),
            body: format!(
                "<p>In your library: <em>{}</em>, about {} min. It came as its own book with \
                 today's paper.</p>",
                esc(&title),
                minutes(x.words)
            ),
            teaser: x.teaser.clone(),
            photos: Vec::new(),
            url: e.url.clone(),
            outlet: Some(outlet.clone()),
            extra: Default::default(),
        };
        return Got::Long(Box::new(lr), pointer);
    }
    let (body, photos, mut extra) = story_body(&x, &outlet, cfg, net);
    extra.access = access;
    Got::Story(Article {
        title,
        meta: Some(match x.byline.as_deref() {
            Some(b) => format!("{meta} · {}", by_line(b)),
            None => meta,
        }),
        body,
        teaser: x.teaser,
        photos,
        url: e.url.clone(),
        outlet: Some(outlet),
        extra,
    })
}

/// The section, the long reads it points to, and what happened to each entry. An entry that
/// failed and still has tries left is not in the section (it waits for tomorrow).
pub fn section(
    entries: &[Entry],
    cfg: &Config,
    net: &dyn Net,
    date: chrono::NaiveDate,
) -> (Section, Vec<LongRead>, Vec<(String, Outcome)>) {
    let got = par_map(entries, |e| one(e, cfg, net, date));
    let mut articles = Vec::new();
    let mut long_reads = Vec::new();
    let mut outcomes = Vec::new();
    for (e, g) in entries.iter().zip(got) {
        match g {
            Got::Story(a) => {
                articles.push(a);
                outcomes.push((e.id.clone(), Outcome::Printed));
            }
            Got::Long(lr, pointer) => {
                articles.push(pointer);
                long_reads.push(*lr);
                outcomes.push((e.id.clone(), Outcome::Book));
            }
            Got::GaveUp(a, why) => {
                articles.push(a);
                outcomes.push((e.id.clone(), Outcome::Failed(why)));
            }
            Got::Retry(why) => outcomes.push((e.id.clone(), Outcome::Failed(why))),
            Got::Withheld(why) => outcomes.push((e.id.clone(), Outcome::Waiting(why))),
        }
    }
    let kicker = match articles.len() {
        1 => "1 article you saved".to_owned(),
        n => format!("{n} articles you saved"),
    };
    let section = Section {
        weather: None,
        puzzle: None,
        brief: Vec::new(),
        key: String::new(),
        title: TITLE.into(),
        kicker: Some(kicker),
        articles,
        unavailable: None,
    };
    (section, long_reads, outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::publisher::Walled;
    use crate::reading::{Entry, State};

    fn entry(tries: u32) -> Entry {
        Entry {
            id: "r-1".into(),
            url: Some("https://www.gazette.example/news/a.html".into()),
            file: None,
            title: None,
            saved_at: "2026-10-20T14:03:11Z".parse().unwrap(),
            source: "cli".into(),
            state: State::Pending,
            tries,
            printed_in: None,
            note: None,
            tried_on: None,
            waits: 0,
        }
    }

    /// A subscribed host serving `page` as a stranger would see it.
    struct WalledNet(String);
    impl Net for WalledNet {
        fn get(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(self.0.clone().into_bytes())
        }
        fn get_page_as(&self, u: &str) -> Result<(Vec<u8>, Access), String> {
            self.get(u).map(|b| (b, walled()))
        }
    }

    fn walled() -> Access {
        Access::Walled {
            site: "gazette.example".into(),
            why: Walled::NoAdapter,
        }
    }

    fn date() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 21).unwrap()
    }

    #[test]
    fn a_walled_page_that_reads_prints_as_a_stranger_would_see_it() {
        let words = "word ".repeat(300);
        let net = WalledNet(format!(
            "<html><body><article><h1>T</h1><p>{words}</p><p>{words}</p></article></body></html>"
        ));
        let (section, long_reads, outcomes) =
            section(&[entry(0)], &Config::default(), &net, date());
        assert!(long_reads.is_empty());
        assert_eq!(outcomes, [("r-1".to_owned(), Outcome::Printed)]);
        assert_eq!(section.articles.len(), 1);
        let a = &section.articles[0];
        assert!(a.body.contains("word word"), "{}", a.body);
        assert!(!a.body.contains("Subscriber story"), "{}", a.body);
        assert_eq!(a.extra.access, walled());
    }

    #[test]
    fn a_walled_page_that_doesnt_read_waits_without_spending_a_try() {
        let net = WalledNet("<html><body><p>Log in to keep reading.</p></body></html>".into());
        // Even on what would be its last try, it waits rather than giving up.
        let (section, _, outcomes) =
            section(&[entry(MAX_TRIES - 1)], &Config::default(), &net, date());
        assert!(section.articles.is_empty());
        assert_eq!(
            outcomes,
            [(
                "r-1".to_owned(),
                Outcome::Waiting(
                    "no readable article: Subscriber story (Deckle doesn't know how \
                     gazette.example signs in yet)."
                        .into()
                )
            )]
        );
    }

    #[test]
    fn slugs_are_short_ascii_words() {
        assert_eq!(slug("The Long One"), "the-long-one");
        assert_eq!(slug("Café — déjà vu?"), "cafe-deja-vu");
        assert_eq!(
            slug("An extremely long headline that keeps going past forty characters"),
            "an-extremely-long-headline-that-keeps"
        );
        assert_eq!(slug("!!!"), "article");
    }

    #[test]
    fn reading_time_rounds_down_with_a_minimum() {
        assert_eq!(minutes(100), 1);
        assert_eq!(minutes(4600), 20);
    }
}
