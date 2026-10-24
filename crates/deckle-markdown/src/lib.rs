// SPDX-License-Identifier: GPL-3.0-or-later
//! Markdown and flat notes-folder import. See the Markdown import design.
mod package;
mod render;

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub title: String,
    pub chapters: usize,
    pub contents: usize,
    pub warnings: Vec<String>,
}

const MAX_BYTES: u64 = 50 * 1024 * 1024;
const MAX_NOTES: usize = 1000;

pub fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
}

/// Directories are imported only when they contain top-level Markdown notes.
pub fn is_markdown(path: &Path) -> bool {
    if path.is_dir() {
        std::fs::read_dir(path).ok().is_some_and(|entries| {
            entries.flatten().any(|e| {
                e.file_type().is_ok_and(|t| t.is_file())
                    && !e.file_name().to_string_lossy().starts_with('.')
                    && is_markdown_file(&e.path())
            })
        })
    } else {
        is_markdown_file(path)
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Metadata {
    title: Option<String>,
    author: Option<String>,
    authors: Vec<String>,
    language: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_yaml_ng::Value>,
}

struct Note {
    path: PathBuf,
    title: String,
    body: String,
    metadata: Metadata,
    headings: Vec<render::Heading>,
}

fn frontmatter(text: &str, warnings: &mut Vec<String>) -> Result<(Metadata, String), Error> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok((Metadata::default(), text.into()));
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if matches!(line.trim_end(), "---" | "...") {
            let meta: Metadata = if rest[..offset].trim().is_empty() {
                Metadata::default()
            } else {
                serde_yaml_ng::from_str(&rest[..offset])
                    .map_err(|e| Error::Invalid(format!("invalid Markdown frontmatter: {e}")))?
            };
            if meta
                .title
                .iter()
                .chain(&meta.author)
                .chain(&meta.authors)
                .chain(&meta.language)
                .any(|s| s.chars().any(|c| !xml_char(c)))
            {
                return Err(Error::Invalid(
                    "frontmatter contains characters XML cannot represent".into(),
                ));
            }
            if !meta.extra.is_empty() {
                warnings.push(format!(
                    "ignored frontmatter fields: {}",
                    meta.extra.keys().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
            return Ok((meta, rest[offset + line.len()..].into()));
        }
        offset += line.len();
    }
    Err(Error::Invalid(
        "Markdown frontmatter has no closing --- line".into(),
    ))
}

fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn load(path: &Path, total: &mut u64, warnings: &mut Vec<String>) -> Result<Note, Error> {
    *total += std::fs::metadata(path)?.len();
    if *total > MAX_BYTES {
        return Err(Error::Invalid("Markdown input exceeds 50 MB".into()));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Invalid(format!("{}: expected UTF-8 Markdown: {e}", path.display())))?;
    if text.chars().any(|c| !xml_char(c)) {
        return Err(Error::Invalid(format!(
            "{}: contains binary or unsupported control characters",
            path.display()
        )));
    }
    let text = text
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let (metadata, body) = frontmatter(&text, warnings)?;
    if body.trim().is_empty() {
        return Err(Error::Invalid(format!(
            "{}: no Markdown content",
            path.display()
        )));
    }
    let headings = render::headings(&body);
    let title = metadata
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            headings
                .iter()
                .find(|h| h.level == 1)
                .map(|h| h.text.clone())
        })
        .unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    Ok(Note {
        path: path.into(),
        title,
        body,
        metadata,
        headings,
    })
}

/// Folder chapters follow filename order (case-folded, with exact spelling as a tie-breaker).
/// Local images must resolve inside the source folder; no network requests are made.
pub fn convert(source: &Path, output: &Path) -> Result<ImportReport, Error> {
    let folder = source.is_dir();
    let root = if folder {
        source
    } else {
        source
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    }
    .canonicalize()?;
    let mut warnings = Vec::new();
    let mut paths = if folder {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let ty = entry.file_type()?;
            if ty.is_symlink() && is_markdown_file(&entry.path()) {
                warnings.push(format!(
                    "skipped linked note {}",
                    entry.file_name().to_string_lossy()
                ));
            } else if ty.is_file() && is_markdown_file(&entry.path()) {
                paths.push(entry.path());
            } else if ty.is_dir() {
                warnings.push(format!(
                    "{}: nested notes are not imported (images may still be used)",
                    entry.file_name().to_string_lossy()
                ));
            }
        }
        paths
    } else {
        if !is_markdown_file(source) {
            return Err(Error::Invalid(
                "expected a .md/.markdown file or a flat notes folder".into(),
            ));
        }
        vec![source.into()]
    };
    paths.sort_by_key(|p| {
        (
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase(),
            p.clone(),
        )
    });
    if paths.is_empty() {
        return Err(Error::Invalid("no Markdown notes in this folder".into()));
    }
    if paths.len() > MAX_NOTES {
        return Err(Error::Invalid(
            "a notes book may contain at most 1000 notes".into(),
        ));
    }
    let mut total = 0;
    let notes = paths
        .iter()
        .map(|p| load(p, &mut total, &mut warnings))
        .collect::<Result<Vec<_>, _>>()?;
    let title = if folder {
        root.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        notes[0].title.clone()
    };
    let mut authors = Vec::new();
    for note in &notes {
        for author in note.metadata.author.iter().chain(&note.metadata.authors) {
            if !author.trim().is_empty() && !authors.contains(author) {
                authors.push(author.clone());
            }
        }
    }
    let language = notes
        .iter()
        .find_map(|n| n.metadata.language.as_deref())
        .unwrap_or("en");
    if language.is_empty()
        || !language
            .split('-')
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        return Err(Error::Invalid(
            "frontmatter language must be a language tag such as en or es-MX".into(),
        ));
    }
    let mut renderer = render::Renderer::new(&notes, &root, total, &mut warnings);
    let bodies = (0..notes.len())
        .map(|i| renderer.render(i))
        .collect::<Vec<_>>();
    let assets = renderer.assets;
    let contents = package::write(output, &title, &authors, language, &notes, bodies, assets)?;
    Ok(ImportReport {
        title,
        chapters: notes.len(),
        contents,
        warnings,
    })
}
