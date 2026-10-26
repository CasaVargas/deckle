// SPDX-License-Identifier: GPL-3.0-or-later
//! Nudges (interests design §7, as amended by §15): what Deckle noticed, printed as the last
//! pages of the paper, one section per suggestion so the Kindle's page history can tell which
//! one was tapped. A tap on the page's one link is an accept at the next connection.

use crate::{Article, Nudge, Section, esc};

/// The line under every nudge's heading.
pub const KICKER: &str = "Deckle noticed";
/// Nudges printed per edition at most.
pub const MAX: usize = 3;

/// The section key of a nudge for candidate `key`.
pub fn key(candidate: &str) -> String {
    format!("nudge:{candidate}")
}

/// Whether a section key is a nudge's.
pub fn is_nudge(section_key: &str) -> bool {
    section_key.starts_with("nudge:")
}

/// The candidate key a nudge section is for.
pub fn candidate(section_key: &str) -> Option<&str> {
    section_key.strip_prefix("nudge:")
}

/// One nudge as its own section: the reason, one link to tap, and how to undo it.
pub fn section(n: &Nudge) -> Section {
    let name = esc(n.name.trim());
    Section {
        key: key(&n.key),
        title: format!("Deckle noticed: {}", n.name.trim()),
        kicker: Some(KICKER.into()),
        articles: vec![Article {
            title: n.name.trim().to_owned(),
            body: format!(
                "<p>{}</p>\n<p><a href=\"front.xhtml\">Tap here to make {name} a section.</a></p>\n\
                 <p class=\"meta\">Change your mind on the Daily tab.</p>",
                esc(n.why.trim())
            ),
            ..Default::default()
        }],
        ..Default::default()
    }
}
