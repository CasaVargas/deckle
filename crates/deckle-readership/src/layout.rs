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
    /// The EPUB's spine, for KOReader's positions (KOReader hub design §C); `None` for layouts
    /// recorded before it was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epub: Option<EpubLayout>,
}

impl Layout {
    /// The positions are KFX positions, not [`EpubLayout`] ones.
    pub fn is_kfx(&self) -> bool {
        !self.epub.as_ref().is_some_and(|e| e.positions)
    }
}

/// Positions per spine file in an edition with no KFX: a section starts at its file's spine
/// index times this.
pub const SPINE_SPAN: i64 = 1_000_000;

/// One edition's EPUB spine and the file each section starts in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EpubLayout {
    /// The spine's files in order, as the OPF names them: KOReader's `DocFragment[N]` is
    /// `files[N - 1]`.
    pub files: Vec<String>,
    /// In paper order, the front page first.
    pub sections: Vec<EpubSection>,
    /// No KFX was built: the layout's positions are this EPUB's ([`SPINE_SPAN`] per file).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub positions: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpubSection {
    /// As [`SectionStart::key`].
    pub key: String,
    pub name: String,
    /// Its file's index in [`EpubLayout::files`].
    pub spine: usize,
}

impl EpubLayout {
    /// The section a spine file belongs to: the last one starting at or before it.
    pub fn section_at(&self, spine: usize) -> Option<&EpubSection> {
        self.sections.iter().rev().find(|s| s.spine <= spine)
    }
}

/// The spine of the paper's EPUB: `front.xhtml`, then `s<N>.xhtml` for the `N`th of `sections`
/// (`(key, printed title)` in paper order, without the front page; render.rs).
pub fn epub_layout(epub: &[u8], sections: &[(String, String)]) -> Result<EpubLayout, String> {
    let files = spine(epub)?;
    let mut out = EpubLayout {
        files,
        ..EpubLayout::default()
    };
    for (i, f) in out.files.iter().enumerate() {
        let name = f.rsplit('/').next().unwrap_or(f);
        let (key, title) = if name == "front.xhtml" {
            ("front".to_owned(), "Front page".to_owned())
        } else {
            let Some(n) = name
                .strip_prefix('s')
                .and_then(|n| n.strip_suffix(".xhtml"))
                .and_then(|n| n.parse::<usize>().ok())
            else {
                continue;
            };
            n.checked_sub(1)
                .and_then(|n| sections.get(n))
                .ok_or_else(|| format!("{name} has no section in the build"))?
                .clone()
        };
        out.sections.push(EpubSection {
            key,
            name: title,
            spine: i,
        });
    }
    if out.sections.first().is_none_or(|s| s.key != "front") {
        return Err("the edition has no front page".into());
    }
    if out.sections.len() != sections.len() + 1 {
        return Err(format!(
            "the edition has {} sections, the build {}",
            out.sections.len() - 1,
            sections.len()
        ));
    }
    Ok(out)
}

/// Every spine file's href, as the OPF writes it, linear or not: KOReader counts them all.
fn spine(epub: &[u8]) -> Result<Vec<String>, String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(epub)).map_err(|e| e.to_string())?;
    let mut text = |name: &str| -> Result<String, String> {
        let mut s = String::new();
        zip.by_name(name)
            .map_err(|e| format!("{name}: {e}"))?
            .read_to_string(&mut s)
            .map_err(|e| format!("{name}: {e}"))?;
        Ok(s)
    };
    let container = text("META-INF/container.xml")?;
    let doc = roxmltree::Document::parse(&container).map_err(|e| e.to_string())?;
    let opf_path = doc
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or("container.xml names no OPF")?
        .to_owned();
    let opf = text(&opf_path)?;
    let doc = roxmltree::Document::parse(&opf).map_err(|e| format!("{opf_path}: {e}"))?;
    let href = |id: &str| {
        doc.descendants()
            .find(|n| n.has_tag_name("item") && n.attribute("id") == Some(id))
            .and_then(|n| n.attribute("href"))
    };
    let files: Vec<String> = doc
        .descendants()
        .filter(|n| n.has_tag_name("itemref"))
        .filter_map(|n| href(n.attribute("idref")?).map(str::to_owned))
        .collect();
    if files.is_empty() {
        return Err(format!("{opf_path}: the spine is empty"));
    }
    Ok(files)
}

/// The layout of an edition with no KFX, in the EPUB's own positions: each section starts at
/// its file's spine index times [`SPINE_SPAN`], and the edition ends after its last section.
pub fn layout_from_epub(mut epub: EpubLayout, date: NaiveDate, title: &str, file: &str) -> Layout {
    epub.positions = true;
    let at = |spine: usize| spine as i64 * SPINE_SPAN;
    let sections = epub
        .sections
        .iter()
        .map(|s| SectionStart {
            key: s.key.clone(),
            name: s.name.clone(),
            start: at(s.spine),
            items: None,
            stories: Vec::new(),
        })
        .collect();
    Layout {
        date,
        title: title.to_owned(),
        file: file.to_owned(),
        end: epub.sections.last().map_or(0, |s| at(s.spine + 1)),
        sections,
        epub: Some(epub),
    }
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
        epub: None,
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
