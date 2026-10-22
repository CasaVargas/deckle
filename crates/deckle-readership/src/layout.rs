// SPDX-License-Identifier: GPL-3.0-or-later
//! Where each section of an edition starts (readership design §4), taken from the finished KFX
//! when the edition is built, so it outlives the edition on the Kindle.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// One edition's sections as KFX positions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub date: NaiveDate,
    pub title: String,
    /// The stem it ships under: the `.sdr` folder name on the Kindle.
    pub file: String,
    /// The end position (`erl`).
    pub end: i64,
    /// In paper order, the front page first.
    pub sections: Vec<SectionStart>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SectionStart {
    /// `front`, or `deckle_digest::config::section_key`.
    pub key: String,
    /// What the paper prints.
    pub name: String,
    pub start: i64,
}

/// The layout of an edition from its KFX. `sections` is `(key, printed title)` in paper order,
/// without the front page: the KFX's first section with text is the front page, and each
/// section after it is matched, in order, to the next section whose first run is its heading.
pub fn layout_from_kfx(
    kfx: &[u8],
    date: NaiveDate,
    title: &str,
    file: &str,
    sections: &[(String, String)],
) -> Result<Layout, String> {
    let firsts = deckle_kfx::text::section_first_runs(kfx).map_err(|e| e.to_string())?;
    let (_, end) = deckle_kfx::text::content_runs(kfx).map_err(|e| e.to_string())?;
    let front = firsts.first().ok_or("the edition has no text")?;
    let mut out = vec![SectionStart {
        key: "front".into(),
        name: "Front page".into(),
        start: front.pid,
    }];
    let mut rest = firsts[1..].iter();
    for (key, name) in sections {
        let run = rest
            .find(|r| words(&r.text) == words(name))
            .ok_or_else(|| format!("section \"{name}\" not found in the edition"))?;
        out.push(SectionStart {
            key: key.clone(),
            name: name.clone(),
            start: run.pid,
        });
    }
    Ok(Layout {
        date,
        title: title.to_owned(),
        file: file.to_owned(),
        end,
        sections: out,
    })
}

/// `s` with runs of whitespace as single spaces: the converter may collapse or keep them.
fn words(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The section a position falls in: the last one starting at or before it.
pub fn section_at(layout: &Layout, position: i64) -> Option<&SectionStart> {
    layout.sections.iter().rev().find(|s| s.start <= position)
}
