// SPDX-License-Identifier: GPL-3.0-or-later
//! Nudges (interests design §7, as amended by §15 and §17): what Deckle noticed, printed as the
//! last pages of the paper. Each suggestion is two sections of its own, so the Kindle's page
//! history can tell them apart: the nudge page with one link, and the confirmation page it
//! leads to with one link back to the front. A tap is both jumps in one read; leaving the nudge
//! page any other way, or paging into the confirmation page, is not.

use crate::{Article, Nudge, Section, esc};

/// The line under every nudge's heading.
pub const KICKER: &str = "Deckle noticed";
/// Nudges printed per edition at most.
pub const MAX: usize = 3;

/// The section key of the nudge page for candidate `key`.
pub fn key(candidate: &str) -> String {
    format!("nudge:{candidate}")
}

/// The section key of the confirmation page for candidate `key`.
pub fn ok_key(candidate: &str) -> String {
    format!("nudge-ok:{candidate}")
}

/// Whether a section key is a nudge's page or its confirmation page.
pub fn is_nudge(section_key: &str) -> bool {
    section_key.starts_with("nudge:") || section_key.starts_with("nudge-ok:")
}

/// The candidate key a nudge page is for.
pub fn candidate(section_key: &str) -> Option<&str> {
    section_key.strip_prefix("nudge:")
}

/// The candidate key a confirmation page is for.
pub fn ok_candidate(section_key: &str) -> Option<&str> {
    section_key.strip_prefix("nudge-ok:")
}

/// The nudge page: the reason, one link (to `ok_href`, its confirmation page), how to undo it.
pub fn section(n: &Nudge, ok_href: &str) -> Section {
    let name = esc(n.name.trim());
    Section {
        key: key(&n.key),
        title: format!("Deckle noticed: {}", n.name.trim()),
        kicker: Some(KICKER.into()),
        articles: vec![Article {
            title: n.name.trim().to_owned(),
            body: format!(
                "<p>{}</p>\n<p><a href=\"{}\">Tap here to make {name} a section.</a></p>\n\
                 <p class=\"meta\">Change your mind on the Daily tab.</p>",
                esc(n.why.trim()),
                esc(ok_href)
            ),
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// The confirmation page: what happens next, and one link back to the front.
pub fn confirmation(n: &Nudge) -> Section {
    let name = esc(n.name.trim());
    Section {
        key: ok_key(&n.key),
        title: "Noted".into(),
        kicker: Some(KICKER.into()),
        articles: vec![Article {
            title: format!("{} becomes a section", n.name.trim()),
            body: format!(
                "<p>{name} becomes a section the next time your Kindle connects.</p>\n\
                 <p><a href=\"front.xhtml\">Done: back to the front page.</a></p>\n\
                 <p class=\"meta\">Change your mind on the Daily tab.</p>"
            ),
            ..Default::default()
        }],
        ..Default::default()
    }
}
