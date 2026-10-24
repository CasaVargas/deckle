// SPDX-License-Identifier: GPL-3.0-or-later
//! Before you send (Books and Kindle design §8): what Deckle will do to a book, worked out by
//! running the stages before Kindle Previewer (import, inspect, the cover, normalize) into a
//! scratch folder, and the first page of chapter one before and after them.

use std::path::{Path, PathBuf};

use base64::Engine;
use regex::Regex;
use serde::Serialize;

use crate::epub::{EpubReader, join, percent_decode};
use crate::inspect::{BookInfo, inspect};
use crate::normalize::{NormalizeOptions, NormalizeReport, normalize};
use crate::pipeline::{CoverPick, CoverTemplate, cover_for};

/// The settings that change what normalize does.
#[derive(Clone, Debug, Default)]
pub struct PreflightOptions {
    pub preset: Option<crate::preset::Preset>,
    pub typography: bool,
    pub popup_notes: bool,
    /// As [`crate::pipeline::RunOptions::generated_cover`].
    pub generated_cover: Option<Option<CoverTemplate>>,
    pub cover: CoverPick,
}

/// Where the cover a book goes out with comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverSource {
    /// The book's own.
    Own,
    /// Deckle's typographic cover.
    Generated,
    /// An image picked for it.
    Picked,
    /// None at all.
    None,
}

/// The first page of chapter one, ready for a sandboxed frame: its body's markup with images
/// inlined as `data:` URLs, and its stylesheets with fonts and imports taken out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Chapter {
    /// The spine item, under the OPF's folder.
    pub href: String,
    /// The contents entry it was found by, when it was.
    pub title: Option<String>,
    pub html: String,
    pub css: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Preflight {
    pub info: BookInfo,
    pub normalize: NormalizeReport,
    pub warnings: Vec<String>,
    pub cover: CoverSource,
    /// The cover as it goes out, JPEG.
    #[serde(skip)]
    pub cover_jpeg: Option<Vec<u8>>,
    pub before: Option<Chapter>,
    pub after: Option<Chapter>,
    /// The normalized EPUB, in the scratch folder.
    #[serde(skip)]
    pub normalized: PathBuf,
}

/// Runs the stages before Previewer on `src` (an EPUB, or anything `import` takes) inside
/// `work`. Nothing leaves `work`; nothing touches the network.
pub fn preflight(src: &Path, opts: &PreflightOptions, work: &Path) -> Result<Preflight, String> {
    std::fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
    let mut warnings = Vec::new();
    let source = if crate::import::needs_import(src) {
        let imported =
            crate::import::import(src, work, "imported.epub").expect("an importable source")?;
        warnings.extend(imported.warnings);
        imported.epub
    } else {
        src.to_path_buf()
    };
    let info = inspect(&source).map_err(|e| e.to_string())?;
    warnings.extend(info.warnings.iter().cloned());
    let (generated, replace) = cover_for(&info, &opts.cover, opts.generated_cover, &mut warnings);
    let normalized = work.join("normalized.epub");
    let report = normalize(
        &source,
        &normalized,
        &NormalizeOptions {
            preset: opts.preset,
            typography: opts.typography,
            popup_notes: opts.popup_notes,
            generated_cover: generated,
            replace_cover: replace,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    warnings.extend(report.warnings.iter().cloned());
    let cover = match (&report.cover_generated, &info.cover_href) {
        (Some(style), _) if style == "picked" => CoverSource::Picked,
        (Some(_), _) => CoverSource::Generated,
        (None, Some(_)) => CoverSource::Own,
        (None, None) => CoverSource::None,
    };
    let cover_jpeg = cover_of(&normalized);
    Ok(Preflight {
        before: first_chapter(&source).ok().flatten(),
        after: first_chapter(&normalized).ok().flatten(),
        info,
        normalize: report,
        warnings,
        cover,
        cover_jpeg,
        normalized,
    })
}

/// The cover image of an EPUB, as its file holds it.
pub fn cover_of(epub: &Path) -> Option<Vec<u8>> {
    let info = inspect(epub).ok()?;
    let href = info.cover_href?;
    let mut r = EpubReader::open(epub).ok()?;
    let name = join(&r.opf_dir, &percent_decode(&href));
    r.read_bytes(&name).ok()
}

/// A spine item that's never chapter one, by its file name.
fn front_matter(href: &str) -> bool {
    let name = href.rsplit('/').next().unwrap_or(href).to_ascii_lowercase();
    [
        "cover",
        "toc",
        "nav",
        "title",
        "copyright",
        "contents",
        "dedication",
        "halftitle",
        "frontmatter",
        "colophon",
    ]
    .iter()
    .any(|w| name.contains(w))
}

fn text_len(html: &str) -> usize {
    let tags = Regex::new(r"(?s)<[^>]*>").unwrap();
    tags.replace_all(html, " ")
        .split_whitespace()
        .map(str::len)
        .sum()
}

/// A contents entry that names chapter one.
fn names_chapter_one(title: &str) -> bool {
    let re = Regex::new(r"(?i)^\s*(chapter|ch\.|book|part)?\s*(1|one|i)\b(\s|[.:—–-]|$)").unwrap();
    re.is_match(title) || title.trim().eq_ignore_ascii_case("prologue")
}

/// Chapter one of an EPUB: the contents entry that names it, else the first spine item past
/// the front matter with a page of text.
pub fn first_chapter(epub: &Path) -> Result<Option<Chapter>, String> {
    let info = inspect(epub).map_err(|e| e.to_string())?;
    let mut r = EpubReader::open(epub).map_err(|e| e.to_string())?;
    let opf_dir = r.opf_dir.clone();
    let by_toc = info
        .toc
        .iter()
        .find(|t| names_chapter_one(&t.title))
        .map(|t| {
            let (file, frag) = t.href.split_once('#').unwrap_or((&t.href, ""));
            (
                file.to_owned(),
                Some(frag.to_owned()).filter(|f| !f.is_empty()),
                Some(t.title.clone()),
            )
        });
    let candidates: Vec<(String, Option<String>, Option<String>)> = by_toc
        .into_iter()
        .chain(
            info.spine
                .iter()
                .filter(|h| !front_matter(h))
                .map(|h| (h.clone(), None, None)),
        )
        .collect();
    for (href, frag, title) in candidates {
        let name = join(&opf_dir, &percent_decode(&href));
        let Ok(doc) = r.read_string(&name) else {
            continue;
        };
        let Some(body) = body_of(&doc, frag.as_deref()) else {
            continue;
        };
        // A title page or a part divider isn't a page to judge typesetting by.
        if title.is_none() && text_len(&body) < 1200 {
            continue;
        }
        let dir = name.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
        let html = inline_images(&body, &dir, &mut r);
        let css = stylesheets(&doc, &dir, &mut r);
        return Ok(Some(Chapter {
            href,
            title,
            html,
            css,
        }));
    }
    Ok(None)
}

/// The inside of `<body>`, from the element with id `frag` on when there is one.
fn body_of(doc: &str, frag: Option<&str>) -> Option<String> {
    let open = Regex::new(r"(?is)<body\b[^>]*>").unwrap().find(doc)?;
    let start = open.end();
    let end = doc[start..]
        .to_ascii_lowercase()
        .rfind("</body")
        .map_or(doc.len(), |i| start + i);
    let body = &doc[start..end];
    let from = frag
        .and_then(|f| {
            let re =
                Regex::new(&format!(r#"<[^>]*\bid\s*=\s*["']{}["']"#, regex::escape(f))).ok()?;
            re.find(body).map(|m| m.start())
        })
        .unwrap_or(0);
    Some(body[from..].to_owned())
}

fn mime_of(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// Above this an image isn't inlined (the frame shows its alt text instead).
const MAX_INLINE: usize = 3_000_000;

/// `src` and `xlink:href` image references as `data:` URLs.
fn inline_images(html: &str, dir: &str, r: &mut EpubReader) -> String {
    let re = Regex::new(r#"(?i)\b(src|xlink:href)\s*=\s*(["'])([^"']+)["']"#).unwrap();
    re.replace_all(html, |c: &regex::Captures| {
        let href = &c[3];
        if href.starts_with("data:") || href.contains("://") {
            return c[0].to_owned();
        }
        let name = join(dir, &percent_decode(href.split('#').next().unwrap_or(href)));
        match r.read_bytes(&name) {
            Ok(b) if b.len() <= MAX_INLINE => format!(
                "{}={q}data:{};base64,{}{q}",
                &c[1],
                mime_of(&name),
                base64::engine::general_purpose::STANDARD.encode(&b),
                q = &c[2]
            ),
            _ => format!("{}={q}{q}", &c[1], q = &c[2]),
        }
    })
    .into_owned()
}

/// The document's linked and inline stylesheets, without `@import`, `@font-face` or `url()`:
/// the frame loads nothing.
fn stylesheets(doc: &str, dir: &str, r: &mut EpubReader) -> String {
    let head = Regex::new(r"(?is)<head\b.*?</head>")
        .unwrap()
        .find(doc)
        .map_or("", |m| m.as_str());
    let link = Regex::new(r"(?is)<link\b[^>]*>").unwrap();
    let href = Regex::new(r#"(?i)\bhref\s*=\s*["']([^"']+)["']"#).unwrap();
    let mut css = String::new();
    for l in link.find_iter(head) {
        let tag = l.as_str();
        if !tag.to_ascii_lowercase().contains("stylesheet") {
            continue;
        }
        if let Some(h) = href.captures(tag) {
            let name = join(dir, &percent_decode(&h[1]));
            if let Ok(text) = r.read_string(&name) {
                css.push_str(&text);
                css.push('\n');
            }
        }
    }
    for s in Regex::new(r"(?is)<style\b[^>]*>(.*?)</style>")
        .unwrap()
        .captures_iter(head)
    {
        css.push_str(&s[1]);
        css.push('\n');
    }
    let css = Regex::new(r"(?is)@font-face\s*\{[^}]*\}")
        .unwrap()
        .replace_all(&css, "");
    let css = Regex::new(r"(?i)@import[^;]*;")
        .unwrap()
        .replace_all(&css, "");
    Regex::new(r"(?i)url\([^)]*\)")
        .unwrap()
        .replace_all(&css, "none")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_one_is_named_by_the_contents() {
        assert!(names_chapter_one("Chapter 1"));
        assert!(names_chapter_one("CHAPTER ONE"));
        assert!(names_chapter_one("I. Loomings"));
        assert!(names_chapter_one("1"));
        assert!(names_chapter_one("Prologue"));
        assert!(!names_chapter_one("Chapter 12"));
        assert!(!names_chapter_one("Introduction"));
        assert!(!names_chapter_one("Ice"));
    }

    #[test]
    fn a_body_starts_at_its_fragment() {
        let doc =
            "<html><body><h1 id=\"a\">A</h1><p>x</p><h1 id=\"b\">B</h1><p>y</p></body></html>";
        assert_eq!(
            body_of(doc, Some("b")).unwrap(),
            "<h1 id=\"b\">B</h1><p>y</p>"
        );
        assert_eq!(body_of(doc, None).unwrap().len(), 50);
    }
}
