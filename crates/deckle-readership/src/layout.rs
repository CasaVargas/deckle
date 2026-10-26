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
    /// Stories or articles the section held, from the build report; `None` for the front page
    /// and layouts recorded before it was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<u32>,
    /// Where each story starts, in order (learning plan, Task 2); empty for the front page and
    /// for layouts recorded before stories were kept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stories: Vec<StoryStart>,
}

/// One story's heading: its title and the KFX position of the heading's first character.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoryStart {
    pub title: String,
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
        items: None,
        stories: Vec::new(),
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
            items: None,
            stories: Vec::new(),
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

/// Each section's stories from the runs inside its range: for each title, the **last** run equal
/// to it (whitespace collapsed) inside the section, because a section with two or more stories
/// opens with an "In this section" list that prints every title once before the headings
/// (render.rs); a title with no such run is left out. `titles` is `(section key, titles)`.
pub fn stories_from_runs(
    runs: &[deckle_kfx::text::Run],
    layout: &Layout,
    titles: &[(String, Vec<String>)],
) -> Vec<(String, Vec<StoryStart>)> {
    let mut out = Vec::new();
    for (key, names) in titles {
        let Some(i) = layout.sections.iter().position(|s| &s.key == key) else {
            continue;
        };
        let from = layout.sections[i].start;
        let to = layout.sections.get(i + 1).map_or(layout.end, |s| s.start);
        let mut stories = Vec::new();
        for name in names {
            let want = words(name);
            let found = runs
                .iter()
                .rev()
                .find(|r| r.pid >= from && r.pid < to && words(&r.text) == want);
            if let Some(r) = found {
                stories.push(StoryStart {
                    title: name.clone(),
                    start: r.pid,
                });
            }
        }
        stories.sort_by_key(|s| s.start);
        out.push((key.clone(), stories));
    }
    out
}

/// Fills each section's stories from the finished KFX.
pub fn add_stories(
    layout: &mut Layout,
    kfx: &[u8],
    titles: &[(String, Vec<String>)],
) -> Result<(), String> {
    let (runs, _) = deckle_kfx::text::content_runs(kfx).map_err(|e| e.to_string())?;
    for (key, stories) in stories_from_runs(&runs, layout, titles) {
        if let Some(s) = layout.sections.iter_mut().find(|s| s.key == key) {
            s.stories = stories;
        }
    }
    Ok(())
}

/// The section and story a position falls in: the section as [`section_at`], the story the
/// last one starting at or before the position. `None` before a section's first story, in a
/// section without stories, or at the end of the book.
pub fn story_at(layout: &Layout, position: i64) -> Option<(&SectionStart, &StoryStart)> {
    if position >= layout.end {
        return None;
    }
    let section = section_at(layout, position)?;
    let story = section.stories.iter().rev().find(|s| s.start <= position)?;
    Some((section, story))
}
