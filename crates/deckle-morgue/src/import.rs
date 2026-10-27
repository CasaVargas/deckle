// SPDX-License-Identifier: GPL-3.0-or-later
//! Bringing the editions in the cache folder into the Morgue (design §3, goal 3). An EPUB keeps
//! no story teasers, so an imported sidecar has them unset; the weather line and the lead are
//! read back from the front page, and the rest from the section pages.

use std::io::{self, Read};
use std::path::Path;

use chrono::NaiveDate;

use crate::folder::Morgue;
use crate::sidecar::{Files, Lead, SectionText, Sidecar, Story, TEXT_CAP, plain_text};
use crate::state::MorgueState;

fn has_class(n: roxmltree::Node, class: &str) -> bool {
    n.attribute("class")
        .is_some_and(|c| c.split_whitespace().any(|t| t == class))
}

fn text_of(n: roxmltree::Node) -> String {
    n.descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Classes whose elements aren't story text: the byline, links, rules, the section's own list,
/// the "unavailable" note.
const NOT_STORY: [&str; 9] = [
    "meta",
    "link",
    "rule",
    "back",
    "in-section",
    "toc",
    "unavailable",
    // print layout §5: a folio line, and a pull quote that repeats the story's own words
    "folio",
    "pull",
];

/// `s3.xhtml#a2` → section index 2 (zero-based).
fn section_of_href(href: &str) -> Option<usize> {
    let n = href.strip_prefix('s')?.split_once(".xhtml")?.0;
    n.parse::<usize>().ok()?.checked_sub(1)
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut s = String::new();
    zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
    Some(s)
}

fn slug(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn bad(msg: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// The sidecar of an edition EPUB: its masthead and each section page's stories.
pub fn sidecar_from_epub(epub: &Path, date: NaiveDate) -> io::Result<Sidecar> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(epub)?).map_err(bad)?;
    let (mut paper, mut place, mut number) = (
        String::new(),
        String::new(),
        date.format("%j").to_string().parse().unwrap_or(1),
    );
    let mut weather = None;
    // The lead's title, the section page it links to and (when the page prints one) its teaser.
    let mut lead: Option<(String, Option<usize>, Option<String>)> = None;
    // What the front page prints of the lead (print layout §2.2): its opening when it jumps, or
    // all of it when it's short. A section page's jump head or refer picks it up.
    let mut lead_text: Vec<String> = Vec::new();
    if let Some(front) = read_entry(&mut zip, "OEBPS/front.xhtml")
        && let Ok(doc) = roxmltree::Document::parse(&front)
    {
        for n in doc.descendants().filter(|n| n.is_element()) {
            if has_class(n, "weather") && n.has_tag_name("p") {
                weather = Some(text_of(n).trim().to_owned()).filter(|w| !w.is_empty());
            } else if lead.is_none() && n.has_tag_name("h2") && has_class(n, "lead") {
                let link = n.descendants().find(|d| d.has_tag_name("a"));
                let title = text_of(n).trim().to_owned();
                // The byline paragraph, then the teaser paragraph, follow the headline.
                let teaser = n
                    .next_siblings()
                    .filter(|s| s.is_element())
                    .nth(2)
                    .filter(|p| p.has_tag_name("p"))
                    .map(|p| text_of(p).trim().to_owned())
                    .filter(|t| !t.is_empty());
                let idx = link
                    .and_then(|a| a.attribute("href"))
                    .and_then(section_of_href);
                if !title.is_empty() {
                    lead = Some((title, idx, teaser));
                }
                // after the headline (`next_siblings` starts at it) and its byline: the story's
                // paragraphs, up to the jump line, the tiles or the index
                lead_text = n
                    .next_siblings()
                    .filter(|s| s.is_element())
                    .skip(2)
                    .take_while(|s| {
                        !(s.has_tag_name("table")
                            || s.has_tag_name("h2")
                            || s.has_tag_name("ul")
                            || has_class(*s, "jump"))
                    })
                    .filter(|s| !NOT_STORY.iter().any(|c| has_class(*s, c)))
                    .map(text_of)
                    .collect();
            } else if has_class(n, "paper") {
                paper = text_of(n).trim().to_owned();
            } else if has_class(n, "ears") {
                let ears = text_of(n);
                let (no, rest) = ears.split_once(" · ").unwrap_or((ears.as_str(), ""));
                if let Some(num) = no
                    .trim()
                    .strip_prefix("No. ")
                    .and_then(|v| v.trim().parse().ok())
                {
                    number = num;
                }
                place = rest.trim().to_owned();
            }
        }
    }
    let mut sections = Vec::new();
    for i in 1.. {
        let Some(page) = read_entry(&mut zip, &format!("OEBPS/s{i}.xhtml")) else {
            break;
        };
        // A page that won't parse costs that page, not the edition.
        let Ok(doc) = roxmltree::Document::parse(&page) else {
            continue;
        };
        let Some(body) = doc.descendants().find(|n| n.has_tag_name("body")) else {
            continue;
        };
        let mut section = SectionText {
            key: String::new(),
            title: String::new(),
            stories: Vec::new(),
        };
        let mut current: Option<(String, Vec<String>)> = None;
        let flush = |current: &mut Option<(String, Vec<String>)>, section: &mut SectionText| {
            if let Some((title, parts)) = current.take() {
                section.stories.push(Story {
                    title,
                    teaser: None,
                    text: plain_text(&parts.join(" "), TEXT_CAP),
                });
            }
        };
        for n in body.children().filter(|n| n.is_element()) {
            if n.has_tag_name("h1") && has_class(n, "section-head") {
                section.title = text_of(n).trim().to_owned();
                section.key = slug(&section.title);
            } else if n.has_tag_name("h2") && has_class(n, "headline") {
                flush(&mut current, &mut section);
                current = Some((text_of(n).trim().to_owned(), Vec::new()));
            } else if has_class(n, "jumphead") {
                // the lead continued: its opening is on the front page, its title comes next
                flush(&mut current, &mut section);
                current = Some((String::new(), lead_text.clone()));
            } else if has_class(n, "jumptitle") {
                if let Some((title, _)) = current.as_mut()
                    && title.is_empty()
                {
                    *title = text_of(n).trim().to_owned();
                }
            } else if has_class(n, "refer") {
                // the lead printed whole on the front page
                flush(&mut current, &mut section);
                let title = n
                    .descendants()
                    .find(|d| d.has_tag_name("a"))
                    .map(|a| text_of(a).trim().to_owned())
                    .unwrap_or_default();
                current = Some((title, lead_text.clone()));
            } else if let Some((_, parts)) = current.as_mut()
                && !NOT_STORY.iter().any(|c| has_class(n, c))
            {
                parts.push(text_of(n));
            }
        }
        flush(&mut current, &mut section);
        sections.push(section);
    }
    // Older editions have no `h2.lead`: their first story link on the front page is the lead.
    if lead.is_none()
        && let Some(front) = read_entry(&mut zip, "OEBPS/front.xhtml")
        && let Ok(doc) = roxmltree::Document::parse(&front)
    {
        lead = doc
            .descendants()
            .filter(|n| n.has_tag_name("a"))
            .find_map(|a| {
                let idx = section_of_href(a.attribute("href")?)?;
                let title = text_of(a).trim().to_owned();
                (!title.is_empty() && a.attribute("href")?.contains("#a")).then_some((
                    title,
                    Some(idx),
                    None,
                ))
            });
    }
    let lead = lead.map(|(title, idx, teaser)| Lead {
        section: idx
            .and_then(|i| sections.get(i))
            .map(|s: &SectionText| s.title.clone())
            .unwrap_or_default(),
        title,
        teaser,
    });
    Ok(Sidecar {
        date,
        number,
        paper,
        place,
        weather,
        lead,
        sections,
        files: Files {
            epub: format!("{date}.epub"),
            kfx: None,
        },
        stem: deckle_digest::edition_filename(date)
            .trim_end_matches(".kfx")
            .to_owned(),
    })
}

/// `deckle-daily-YYYY-MM-DD.epub` as a date, only when that is the whole name.
fn edition_date(name: &str) -> Option<NaiveDate> {
    let rest = name
        .strip_prefix(deckle_digest::EDITION_PREFIX)?
        .strip_suffix(".epub")?;
    (rest.len() == 10)
        .then(|| NaiveDate::parse_from_str(rest, "%Y-%m-%d").ok())
        .flatten()
}

/// What an import did: the dates brought in, and how many editions couldn't be.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Imported {
    /// Newest first.
    pub done: Vec<NaiveDate>,
    pub failed: usize,
}

/// Imports every edition EPUB in `cache_dir` the Morgue doesn't have, with the matching KFX
/// from `cache_dir/kfx` (named "… -- <long date>.kfx"). An EPUB that can't be read or kept is
/// counted in `failed`.
pub fn import_cache_report(morgue: &Morgue, cache_dir: &Path) -> Imported {
    let have = morgue.editions();
    let mut found: Vec<(NaiveDate, std::path::PathBuf)> = std::fs::read_dir(cache_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let date = edition_date(e.file_name().to_str()?)?;
            Some((date, e.path()))
        })
        .filter(|(d, _)| !have.contains(d))
        .collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    let mut out = Imported::default();
    for (date, path) in found {
        let kept = sidecar_from_epub(&path, date)
            .and_then(|sidecar| morgue.archive_edition(&path, &sidecar));
        if kept.is_err() {
            out.failed += 1;
            continue;
        }
        let suffix = format!(" -- {}.kfx", deckle_digest::long_date(date));
        let kfx = std::fs::read_dir(cache_dir.join("kfx"))
            .into_iter()
            .flatten()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().ends_with(&suffix));
        if let Some(k) = kfx {
            let _ = morgue.archive_kfx(date, &k.path());
        }
        out.done.push(date);
    }
    out
}

/// [`import_cache_report`]'s dates.
pub fn import_cache(morgue: &Morgue, cache_dir: &Path) -> Vec<NaiveDate> {
    import_cache_report(morgue, cache_dir).done
}

/// [`import_cache`], once: it runs only while `state.imported` is false. It sets the flag only
/// when no edition failed, so one that couldn't be kept (the folder offline, say) is tried
/// again at the next build instead of being forgotten.
pub fn import_once(morgue: &Morgue, cache_dir: &Path, state: &mut MorgueState) -> Vec<NaiveDate> {
    if state.imported {
        return Vec::new();
    }
    let report = import_cache_report(morgue, cache_dir);
    state.imported = report.failed == 0;
    report.done
}
