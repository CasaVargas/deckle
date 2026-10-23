// SPDX-License-Identifier: GPL-3.0-or-later
//! From Your Shelf (docs/superpowers/specs/2026-10-22-from-your-shelf-design.md): a page drawn
//! from your own Kindle. `store` keeps what each Kindle shared, `select` chooses what prints,
//! and [`section`] lays it out.

pub mod select;
pub mod store;

use chrono::{DateTime, NaiveDate};
use chrono_tz::Tz;

use crate::config::{Config, SectionKind, section_key};
use crate::{Article, Section, esc};
use select::{Line, Nightstand, Pick};
use store::{KindleSnapshot, Shelf, WordSnap};

/// Wraps each `*…*` pair of already-escaped text in italics.
fn italicize(s: &str) -> String {
    let mut out = String::new();
    for (i, part) in s.split('*').enumerate() {
        match i % 2 {
            1 => out.push_str(&format!("<i>{part}</i>")),
            _ => out.push_str(part),
        }
    }
    out
}

/// The day a snapshot was taken, in the paper's time zone.
fn seen_day(k: &KindleSnapshot, tz: Tz) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(&k.seen)
        .ok()
        .map(|t| t.with_timezone(&tz).date_naive())
        .or_else(|| store::seen_on(k))
}

fn kicker(k: &KindleSnapshot, date: NaiveDate, tz: Tz) -> String {
    let when = match seen_day(k, tz).map(|d| (date - d).num_days()) {
        Some(days) if (0..=6).contains(&days) => seen_day(k, tz)
            .map(|d| d.format("%A").to_string())
            .unwrap_or_default(),
        Some(days) => format!("{days} days ago"),
        None => "earlier".to_owned(),
    };
    format!(
        "Taken from your Kindle ({}) when it was last plugged in, {when}",
        k.name
    )
}

fn nightstand_body(n: &Nightstand) -> String {
    let mut body = String::new();
    for Line {
        title,
        percent,
        about,
        state,
    } in &n.books
    {
        body.push_str(&format!(
            "<p class=\"shelf-book\"><i>{}</i></p>\n",
            esc(title)
        ));
        let meta = match percent {
            Some(p) => {
                body.push_str(&format!(
                    "<div class=\"bar\"><div style=\"width:{p}%\"></div></div>\n"
                ));
                format!("{}{p}% · {state}", if *about { "about " } else { "" })
            }
            None => state.clone(),
        };
        body.push_str(&format!("<p class=\"meta\">{}</p>\n", esc(&meta)));
    }
    if let Some(nudge) = &n.nudge {
        body.push_str(&format!(
            "<p class=\"nudge\">{}</p>\n",
            italicize(&esc(nudge))
        ));
    }
    body
}

fn highlight_body(p: &Pick) -> String {
    let h = &p.highlight;
    let mut body = String::from("<blockquote>");
    for para in h.text.split("\n\n").filter(|t| !t.trim().is_empty()) {
        body.push_str(&format!("<p>{}</p>", esc(para.trim())));
    }
    body.push_str("</blockquote>\n");
    let mut source = h.title.clone();
    if !h.author.is_empty() {
        source.push_str(&format!(", {}", h.author));
    }
    let mut parts = vec![source];
    parts.extend(h.page.as_ref().map(|p| format!("page {p}")));
    parts.extend(h.added.map(|d| d.format("%B %-d, %Y").to_string()));
    body.push_str(&format!(
        "<p class=\"source\">{}</p>\n",
        esc(&parts.join(" · "))
    ));
    for note in &h.notes {
        body.push_str(&format!("<p class=\"note\">Your note: {}</p>\n", esc(note)));
    }
    body
}

fn words_body(words: &[(WordSnap, String)]) -> String {
    let mut body = String::new();
    for (w, from) in words {
        let def = w
            .definition
            .as_ref()
            .map(|d| format!(" — {}", esc(d)))
            .unwrap_or_default();
        body.push_str(&format!(
            "<p class=\"shelf-word\"><b>{}</b>{def}</p>\n",
            esc(&w.word)
        ));
        if !w.usage.is_empty() {
            body.push_str(&format!(
                "<p class=\"small\">\u{201C}{}\u{201D}</p>\n",
                esc(&w.usage)
            ));
        }
        body.push_str(&format!("<p class=\"meta\">{}</p>\n", esc(from)));
    }
    body
}

/// " (from your Kindle Paperwhite)" when the part came from a Kindle other than the one the
/// page is dated by.
fn from(name: &str, primary: &str) -> String {
    if name == primary {
        String::new()
    } else {
        format!(" (from your {name})")
    }
}

/// The page, from the fresh snapshots; `None` when no part has anything to print. The
/// highlight it picks is recorded in `shelf.shown` under `date`.
pub fn section(
    shelf: &mut Shelf,
    cfg: &Config,
    date: NaiveDate,
    tz: Tz,
    fraction_ok: bool,
) -> Option<Section> {
    let primary = shelf.fresh(date).first().map(|(_, k)| (*k).clone())?;
    let mut articles = Vec::new();

    if let Some(n) = select::nightstand(shelf, &cfg.shelf, date, tz, fraction_ok) {
        articles.push(Article {
            title: format!("On your nightstand{}", from(&n.from, &primary.name)),
            body: nightstand_body(&n),
            ..Default::default()
        });
    }
    if cfg.shelf.highlights
        && let Some(p) = select::highlight(shelf, date)
    {
        articles.push(Article {
            title: format!(
                "You highlighted this {} ago{}",
                p.age,
                from(&p.from, &primary.name)
            ),
            body: highlight_body(&p),
            ..Default::default()
        });
        shelf.shown.insert(p.highlight.id.clone(), date);
    }
    if cfg.shelf.words {
        let words = select::words(shelf, date, tz);
        if let Some((first, _)) = words.first() {
            let name = shelf
                .fresh(date)
                .into_iter()
                .find(|(_, k)| k.words_shared && k.words.contains(first))
                .map(|(_, k)| k.name.clone())
                .unwrap_or_default();
            articles.push(Article {
                title: format!(
                    "Words you looked up this week{}",
                    from(&name, &primary.name)
                ),
                body: words_body(&words),
                ..Default::default()
            });
        }
    }
    if articles.is_empty() {
        return None;
    }
    Some(Section {
        key: section_key(SectionKind::Shelf, None),
        title: "From Your Shelf".into(),
        kicker: Some(kicker(&primary, date, tz)),
        articles,
        unavailable: None,
    })
}
