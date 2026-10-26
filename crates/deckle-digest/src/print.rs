// SPDX-License-Identifier: GPL-3.0-or-later
//! The paper's print language (print layout design): the book each section is in, the skybox,
//! the lead's jump and the pull quotes. Computed once per edition, so every output agrees.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use crate::front::FrontPlan;
use crate::sources::nudges::is_nudge;
use crate::{Gathered, Section};

/// Words of the lead the front page prints before it jumps (spec §2.2).
pub const OPENING_WORDS: usize = 110;
/// Teasers above the masthead, at most.
const SKYBOX: usize = 3;
/// A teaser's headline is shortened at a word to this many characters.
const TEASER_CHARS: usize = 48;
/// A story needs this many paragraphs before it gets a pull quote.
const PULL_MIN_PARAS: usize = 6;
/// How long a quoted passage must be to stand as a pull quote, in words.
const PULL_WORDS: RangeInclusive<usize> = 8..=30;

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "the", "of", "to", "in", "on", "for", "at", "by", "with", "is", "are", "was",
    "will", "be", "as", "from", "after", "how", "why", "what", "new",
];

/// What the outputs print beyond the stories themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrintPlan {
    /// The book (A, B, C…) each section is in, by section index; nudges have none.
    pub book: Vec<Option<char>>,
    /// Up to three teasers above the masthead; none when fewer than two qualify.
    pub skybox: Vec<Teaser>,
    /// The front-page lead's jump, when its story continues inside.
    pub jump: Option<Jump>,
    /// The lead, when it's short enough to print whole on the front page.
    pub whole: Option<(usize, usize)>,
    /// A pull quote per story, keyed by (section, article).
    pub pulls: BTreeMap<(usize, usize), Pull>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Teaser {
    pub section: usize,
    pub article: usize,
    /// The section's title, in capitals.
    pub title: String,
    /// The article's title, shortened at a word.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Jump {
    pub section: usize,
    pub article: usize,
    /// "MILL": the word the jump line and the jump head share.
    pub slug: String,
    /// The opening paragraphs the front page prints (XHTML, as in the body).
    pub opening: String,
    /// The rest of the body, which the section prints under the jump head.
    pub rest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pull {
    /// The quoted passage with its marks, as plain text.
    pub text: String,
    /// The index of the top-level paragraph it follows, in the body the story prints.
    pub after: usize,
}

/// The edition's print plan, from its front plan.
pub fn print_plan(g: &Gathered, front: &FrontPlan) -> PrintPlan {
    let e = &g.edition;
    let mut jump = None;
    let mut whole = None;
    if let Some((s, a)) = front.lead {
        let art = &e.sections[s].articles[a];
        if let Some((opening, rest)) = split_opening(&art.body, OPENING_WORDS) {
            if has_paragraph_text(&rest) {
                jump = Some(Jump {
                    section: s,
                    article: a,
                    slug: slug(&art.title),
                    opening,
                    rest,
                });
            } else {
                whole = Some((s, a));
            }
        }
    }
    let mut pulls = BTreeMap::new();
    for (i, s) in e.sections.iter().enumerate() {
        if !(is_news(&s.key) || s.key == "reading") {
            continue;
        }
        for (j, art) in s.articles.iter().enumerate() {
            if whole == Some((i, j)) {
                continue;
            }
            let body = match &jump {
                Some(x) if (x.section, x.article) == (i, j) => x.rest.as_str(),
                _ => art.body.as_str(),
            };
            if let Some(p) = pull_quote(body) {
                pulls.insert((i, j), p);
            }
        }
    }
    PrintPlan {
        book: books(&e.sections),
        skybox: skybox(g, front),
        jump,
        whole,
        pulls,
    }
}

/// Whether a section's stories are news (Local, feeds, interests, subscribed papers) rather
/// than the back of the book.
pub fn is_news(key: &str) -> bool {
    key == "local"
        || ["local:", "feeds:", "interests:", "subscribed:"]
            .iter()
            .any(|p| key.starts_with(p))
}

/// Books are runs in page-plan order (spec §2.1): the front page opens A, the first section
/// joins it, and a new letter starts whenever the kind changes between news and the rest.
pub fn books(sections: &[Section]) -> Vec<Option<char>> {
    let mut out = Vec::with_capacity(sections.len());
    let mut letter = b'A';
    let mut kind: Option<bool> = None;
    for s in sections {
        if is_nudge(&s.key) {
            out.push(None);
            continue;
        }
        let news = is_news(&s.key);
        if kind.is_some_and(|k| k != news) {
            letter = (letter + 1).min(b'Z');
        }
        kind = Some(news);
        out.push(Some(letter as char));
    }
    out
}

/// The EPUB's labels (spec §3): a section is a page, the front page is A1, and each section
/// takes the next number in its book.
pub fn epub_labels(book: &[Option<char>]) -> Vec<Option<String>> {
    let mut next: BTreeMap<char, u32> = BTreeMap::from([('A', 2)]);
    book.iter()
        .map(|b| {
            b.map(|c| {
                let n = next.entry(c).or_insert(1);
                *n += 1;
                format!("{c}{}", *n - 1)
            })
        })
        .collect()
}

/// The front page's own label.
pub const FRONT_LABEL: &str = "A1";

fn skybox(g: &Gathered, front: &FrontPlan) -> Vec<Teaser> {
    let mut out = Vec::new();
    for (i, s) in g.edition.sections.iter().enumerate() {
        if out.len() == SKYBOX {
            break;
        }
        if s.key == "weather" || is_nudge(&s.key) || s.articles.is_empty() || front.contains(i, 0) {
            continue;
        }
        out.push(Teaser {
            section: i,
            article: 0,
            title: s.title.to_uppercase(),
            text: shorten(&s.articles[0].title, TEASER_CHARS),
        });
    }
    if out.len() < 2 {
        out.clear();
    }
    out
}

/// `s` cut at a word to at most `max` characters, with an ellipsis when it was cut.
fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let cut: String = s.chars().take(max).collect();
    let kept = match cut.rfind(char::is_whitespace) {
        Some(k) if k > 0 => &cut[..k],
        _ => cut.as_str(),
    };
    format!(
        "{}…",
        kept.trim_end_matches(|c: char| c.is_whitespace() || ",;:-–—".contains(c))
    )
}

/// The jump's slug (spec §2.2): the headline's first word that isn't a stopword, in capitals,
/// letters only.
pub fn slug(title: &str) -> String {
    let letters = |w: &str| -> String {
        w.chars()
            .skip_while(|c| !c.is_alphabetic())
            .take_while(|c| c.is_alphabetic())
            .collect()
    };
    let words: Vec<String> = title
        .split_whitespace()
        .map(letters)
        .filter(|w| !w.is_empty())
        .collect();
    words
        .iter()
        .find(|w| !STOPWORDS.contains(&w.to_lowercase().as_str()))
        .or(words.first())
        .map(|w| w.to_uppercase())
        .unwrap_or_else(|| "STORY".into())
}

/// A top-level element of an XHTML fragment, as a byte range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Element {
    start: usize,
    end: usize,
    para: bool,
}

/// The top-level elements of a well-formed XHTML fragment. Text between them isn't an element.
fn top_level(body: &str) -> Vec<Element> {
    let mut out = Vec::new();
    let (mut depth, mut start, mut para) = (0usize, 0usize, false);
    let mut i = 0;
    while let Some(k) = body[i..].find('<') {
        let at = i + k;
        if body[at..].starts_with("<!--") {
            i = body[at..].find("-->").map_or(body.len(), |e| at + e + 3);
            continue;
        }
        let Some(close) = body[at..].find('>') else {
            break;
        };
        let tag = &body[at + 1..at + close];
        let end = at + close + 1;
        if tag.starts_with('/') {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                out.push(Element { start, end, para });
            }
        } else if tag.starts_with('?') || tag.starts_with('!') {
        } else if tag.ends_with('/') {
            if depth == 0 {
                out.push(Element {
                    start: at,
                    end,
                    para: false,
                });
            }
        } else {
            if depth == 0 {
                start = at;
                para = tag.split(char::is_whitespace).next() == Some("p");
            }
            depth += 1;
        }
        i = end;
    }
    out
}

/// The text of an XHTML fragment: tags dropped (a `<br>` is a space), entities decoded,
/// whitespace collapsed.
pub fn plain(xhtml: &str) -> String {
    let mut text = String::with_capacity(xhtml.len());
    let mut rest = xhtml;
    while let Some(k) = rest.find('<') {
        text.push_str(&rest[..k]);
        let Some(close) = rest[k..].find('>') else {
            rest = "";
            break;
        };
        if rest[k + 1..].starts_with("br") {
            text.push(' ');
        }
        rest = &rest[k + close + 1..];
    }
    text.push_str(rest);
    decode(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(k) = rest.find('&') {
        out.push_str(&rest[..k]);
        let tail = &rest[k..];
        let ch = tail.find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let c = match &tail[1..semi] {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16)
                    .ok()
                    .and_then(char::from_u32),
                n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            c.map(|c| (c, semi))
        });
        match ch {
            Some((c, semi)) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn words(xhtml: &str) -> usize {
    plain(xhtml).split_whitespace().count()
}

/// `(opening, rest)`: the body's top-level paragraphs from the first, until they reach `words`
/// words (at least one), and everything else. Whatever comes before the first paragraph (a
/// lead figure) stays at the head of `rest`; anything but a paragraph ends the opening. `None`
/// when the body has no top-level paragraph.
pub fn split_opening(body: &str, words_wanted: usize) -> Option<(String, String)> {
    let els = top_level(body);
    let first = els.iter().position(|e| e.para)?;
    let mut count = 0;
    let mut last = first;
    for (k, e) in els.iter().enumerate().skip(first) {
        if !e.para {
            break;
        }
        count += words(&body[e.start..e.end]);
        last = k;
        if count >= words_wanted {
            break;
        }
    }
    let (a, b) = (els[first].start, els[last].end);
    let rest = format!("{}{}", &body[..a], body[b..].trim_start());
    Some((body[a..b].to_owned(), rest))
}

fn has_paragraph_text(body: &str) -> bool {
    top_level(body)
        .iter()
        .any(|e| e.para && !plain(&body[e.start..e.end]).is_empty())
}

/// The body's top-level paragraphs, as byte ranges.
pub fn paragraph_ends(body: &str) -> Vec<usize> {
    top_level(body)
        .iter()
        .filter(|e| e.para)
        .map(|e| e.end)
        .collect()
}

/// A pull quote (spec §2.4): the first quoted passage of 8–30 words from the third paragraph
/// on, set after the paragraph two after its own. Nothing is reworded.
pub fn pull_quote(body: &str) -> Option<Pull> {
    let paras: Vec<&str> = top_level(body)
        .iter()
        .filter(|e| e.para)
        .map(|e| &body[e.start..e.end])
        .collect();
    if paras.len() < PULL_MIN_PARAS {
        return None;
    }
    (2..paras.len().saturating_sub(2))
        .find_map(|i| quoted(&plain(paras[i])).map(|text| Pull { text, after: i + 2 }))
}

/// The first passage in quotation marks (curly or straight) of [`PULL_WORDS`] words, with its
/// marks.
fn quoted(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some((k, open)) = rest.char_indices().find(|(_, c)| matches!(c, '“' | '"')) {
        let close = if open == '“' { '”' } else { '"' };
        let after = &rest[k + open.len_utf8()..];
        let end = after.find(close)?;
        let inner = after[..end].trim();
        if PULL_WORDS.contains(&inner.split_whitespace().count()) {
            return Some(format!("{open}{inner}{close}"));
        }
        rest = &after[end + close.len_utf8()..];
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Article, Edition};
    use chrono::NaiveDate;

    fn p(words: usize, word: &str) -> String {
        format!("<p>{}</p>", vec![word; words].join(" "))
    }

    fn section(key: &str, bodies: &[&str]) -> Section {
        Section {
            key: key.into(),
            title: key.into(),
            articles: bodies
                .iter()
                .enumerate()
                .map(|(j, b)| Article {
                    title: format!("{key} story {j}"),
                    body: (*b).into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn gathered(sections: Vec<Section>) -> Gathered {
        Gathered {
            news: (0..sections.len())
                .filter(|&i| is_news(&sections[i].key))
                .collect(),
            reading: None,
            long_reads: Vec::new(),
            reading_outcomes: Vec::new(),
            notes: Vec::new(),
            names: Vec::new(),
            edition: Edition {
                large_print: false,
                paper: "The Deckle Daily".into(),
                date: NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
                place: String::new(),
                weather_line: None,
                masthead: Default::default(),
                sections,
                assets: Vec::new(),
            },
        }
    }

    fn keys(keys: &[&str]) -> Vec<Section> {
        keys.iter().map(|k| section(k, &[])).collect()
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Mill Yard Will Be a Market Again"), "MILL");
        assert_eq!(
            slug("Council Weighs a Night Bus to the Hospital"),
            "COUNCIL"
        );
        assert_eq!(slug("The Mill's Wheel"), "MILL");
        assert_eq!(slug("“Herons” Stay Later"), "HERONS");
        assert_eq!(slug("The And Of"), "THE");
        assert_eq!(slug("2026"), "STORY");
        assert_eq!(slug(""), "STORY");
    }

    #[test]
    fn plain_text_drops_tags_and_decodes() {
        assert_eq!(
            plain("<p>A &amp; <em>B</em>&#8217;s &quot;c&quot;<br/>d &#x41;</p>"),
            "A & B’s \"c\" d A"
        );
        assert_eq!(plain("<p>  a\n  b </p>"), "a b");
        assert_eq!(plain("AT&T & co"), "AT&T & co");
    }

    #[test]
    fn top_level_finds_paragraphs_and_figures() {
        let body = "<div class=\"figure lead\"><img src=\"x\" alt=\"\"/></div>\n<p>a <b>b</b></p>\n<!-- c --><h3 class=\"subhead\">s</h3><img src=\"y\"/>";
        let els = top_level(body);
        assert_eq!(els.len(), 4);
        assert!(!els[0].para && els[1].para && !els[2].para && !els[3].para);
        assert_eq!(&body[els[1].start..els[1].end], "<p>a <b>b</b></p>");
    }

    #[test]
    fn the_opening_stops_at_the_first_boundary_past_the_count() {
        let body = format!(
            "{}\n{}\n{}\n{}",
            p(60, "a"),
            p(60, "b"),
            p(60, "c"),
            p(5, "d")
        );
        let (opening, rest) = split_opening(&body, 110).unwrap();
        assert_eq!(opening, format!("{}\n{}", p(60, "a"), p(60, "b")));
        assert_eq!(rest, format!("{}\n{}", p(60, "c"), p(5, "d")));
        // at least one paragraph, however long
        let (opening, _) = split_opening(&p(300, "x"), 110).unwrap();
        assert_eq!(opening, p(300, "x"));
    }

    #[test]
    fn a_lead_figure_stays_in_the_rest_and_a_subhead_ends_the_opening() {
        let fig = "<div class=\"figure lead\"><img src=\"t\" alt=\"\"/></div>\n";
        let body = format!("{fig}{}<h3>Sub</h3>{}", p(20, "a"), p(200, "b"));
        let (opening, rest) = split_opening(&body, 110).unwrap();
        assert_eq!(opening, p(20, "a"));
        assert_eq!(rest, format!("{fig}<h3>Sub</h3>{}", p(200, "b")));
    }

    #[test]
    fn no_paragraph_no_opening() {
        assert_eq!(split_opening("", 110), None);
        assert_eq!(
            split_opening("<table><tr><td>1</td></tr></table>", 110),
            None
        );
    }

    #[test]
    fn the_default_plan_makes_three_books() {
        let s = keys(&[
            "reading",
            "weather",
            "local",
            "interests:Rust",
            "feeds:World",
            "shelf",
            "lottery",
            "horoscope",
            "puzzle",
            "onthisday",
        ]);
        let b: String = books(&s).into_iter().map(Option::unwrap).collect();
        assert_eq!(b, "AABBBCCCCC");
        assert_eq!(
            epub_labels(&books(&s)),
            ["A2", "A3", "B1", "B2", "B3", "C1", "C2", "C3", "C4", "C5"]
                .map(|l| Some(l.to_owned()))
        );
    }

    #[test]
    fn books_change_with_the_kind_and_skip_nudges() {
        let s = keys(&[
            "local",
            "subscribed:thepilot.com:news",
            "weather",
            "local:Raleigh",
            "puzzle",
            "nudge:interests:Rust",
            "nudge-ok:interests:Rust",
        ]);
        assert_eq!(
            books(&s),
            vec![
                Some('A'),
                Some('A'),
                Some('B'),
                Some('C'),
                Some('D'),
                None,
                None
            ]
        );
        let l = epub_labels(&books(&s));
        assert_eq!(l[0].as_deref(), Some("A2"));
        assert_eq!(l[1].as_deref(), Some("A3"));
        assert_eq!(l[2].as_deref(), Some("B1"));
        assert_eq!(l[5], None);
    }

    #[test]
    fn an_empty_section_keeps_its_letter() {
        let mut s = keys(&["local", "feeds:A", "puzzle"]);
        s[1].unavailable = Some("Unavailable today.".into());
        assert_eq!(books(&s), vec![Some('A'), Some('A'), Some('B')]);
    }

    #[test]
    fn the_skybox_skips_weather_nudges_empties_and_the_front() {
        let g = gathered(vec![
            section("local", &["", ""]),
            section("weather", &[""]),
            section("reading", &[""]),
            section("feeds:Empty", &[]),
            section("puzzle", &[""]),
            section("horoscope", &[""]),
            section("onthisday", &[""]),
            section("nudge:interests:Rust", &[""]),
        ]);
        let front = crate::front::front_plan(&g);
        assert_eq!(front.lead, Some((0, 0)));
        let sky = skybox(&g, &front);
        let at: Vec<usize> = sky.iter().map(|t| t.section).collect();
        assert_eq!(at, vec![2, 4, 5]);
        assert_eq!(sky[0].title, "READING");
        assert_eq!(sky[0].text, "reading story 0");
    }

    #[test]
    fn one_candidate_is_no_skybox() {
        let g = gathered(vec![
            section("local", &[""]),
            section("weather", &[""]),
            section("puzzle", &[""]),
        ]);
        assert!(skybox(&g, &crate::front::front_plan(&g)).is_empty());
    }

    #[test]
    fn teasers_are_shortened_at_a_word() {
        assert_eq!(shorten("Short one", 48), "Short one");
        assert_eq!(
            shorten(
                "Council Weighs a Night Bus to the Hospital, and a Second One Too",
                48
            ),
            "Council Weighs a Night Bus to the Hospital, and…"
        );
        assert_eq!(shorten(&"x".repeat(60), 48), format!("{}…", "x".repeat(48)));
    }

    fn quote(words: usize) -> String {
        format!("“{}”", vec!["q"; words].join(" "))
    }

    fn paras_with(at: usize, text: &str, n: usize) -> String {
        (0..n)
            .map(|i| {
                if i == at {
                    format!("<p>He said {text} then left.</p>")
                } else {
                    p(10, "w")
                }
            })
            .collect()
    }

    #[test]
    fn pull_quotes_come_from_the_third_paragraph_on() {
        assert_eq!(
            pull_quote(&paras_with(2, &quote(8), 6)),
            Some(Pull {
                text: quote(8),
                after: 4
            })
        );
        // too early, too short, too long, too near the end
        assert_eq!(pull_quote(&paras_with(1, &quote(10), 8)), None);
        assert_eq!(pull_quote(&paras_with(3, &quote(7), 8)), None);
        assert_eq!(pull_quote(&paras_with(3, &quote(31), 8)), None);
        assert_eq!(pull_quote(&paras_with(6, &quote(10), 8)), None);
        // fewer than six paragraphs
        assert_eq!(pull_quote(&paras_with(2, &quote(10), 5)), None);
    }

    #[test]
    fn straight_and_escaped_marks_work_too() {
        let straight = format!("\"{}\"", vec!["s"; 9].join(" "));
        assert_eq!(
            pull_quote(&paras_with(3, &straight, 7)).map(|p| p.text),
            Some(straight.clone())
        );
        let escaped = format!("&quot;{}&quot;", vec!["e"; 9].join(" "));
        assert_eq!(
            pull_quote(&paras_with(3, &escaped, 7)).map(|p| p.text),
            Some(format!("\"{}\"", vec!["e"; 9].join(" ")))
        );
        // a short quote first, then one that fits
        let both = format!("{} and {}", quote(3), quote(12));
        assert_eq!(
            pull_quote(&paras_with(2, &both, 6)).map(|p| p.text),
            Some(quote(12))
        );
    }

    #[test]
    fn a_long_lead_jumps_and_its_pull_comes_from_the_rest() {
        let long = format!(
            "{}{}{}",
            p(120, "open"),
            paras_with(3, &quote(12), 8),
            p(5, "end")
        );
        let g = gathered(vec![section("local", &[&long]), section("feeds:A", &[""])]);
        let plan = print_plan(&g, &crate::front::front_plan(&g));
        let jump = plan.jump.as_ref().expect("a jump");
        assert_eq!((jump.section, jump.article), (0, 0));
        assert_eq!(jump.slug, "LOCAL");
        assert_eq!(jump.opening, p(120, "open"));
        assert!(jump.rest.starts_with("<p>w"));
        assert_eq!(plan.whole, None);
        // paragraph 3 of the rest, set after paragraph 5 of the rest
        assert_eq!(plan.pulls[&(0, 0)].after, 5);
    }

    #[test]
    fn a_short_lead_prints_whole_and_a_bare_one_neither() {
        let g = gathered(vec![section("local", &[&p(30, "a")])]);
        let plan = print_plan(&g, &crate::front::front_plan(&g));
        assert_eq!((plan.jump, plan.whole), (None, Some((0, 0))));
        let g = gathered(vec![section("local", &[""])]);
        let plan = print_plan(&g, &crate::front::front_plan(&g));
        assert_eq!((plan.jump, plan.whole), (None, None));
    }

    #[test]
    fn the_back_of_the_book_gets_no_pull_quotes() {
        let body = paras_with(2, &quote(10), 6);
        let g = gathered(vec![
            section("local", &[""]),
            section("horoscope", &[&body]),
            section("reading", &[&body]),
        ]);
        let plan = print_plan(&g, &crate::front::front_plan(&g));
        assert_eq!(plan.pulls.keys().copied().collect::<Vec<_>>(), vec![(2, 0)]);
    }
}
