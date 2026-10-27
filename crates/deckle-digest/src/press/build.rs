// SPDX-License-Identifier: GPL-3.0-or-later
//! Job-local full-edition generation for compatible folder clients.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use sha2::{Digest, Sha256};

use super::{FreshIdentity, PressError, PressErrorCode};
use crate::reading::{Entry, ReadingQueue};
use crate::{Config, GatherOptions, Net};

#[derive(Clone, Debug)]
pub struct FreshBuildContext {
    pub identity: FreshIdentity,
    pub started_at: DateTime<Utc>,
    /// A nonexistent private attempt directory under a caller-controlled parent.
    /// Existing paths, including symlinks, are rejected without changing their contents.
    pub work_dir: PathBuf,
    /// Whether the selected folder includes saved long-read EPUBs.
    pub books: bool,
}

#[derive(Clone, Debug)]
pub struct FreshArtifact {
    /// Complete local bundle; publication, archive retention and delivery belong to the caller.
    pub bundle: PathBuf,
    /// SHA-256 of the exact edition.json bytes returned above, in lowercase hexadecimal.
    pub manifest_sha256: String,
}

fn storage(_: impl std::fmt::Display) -> PressError {
    PressError::new(PressErrorCode::StorageUnavailable)
}

fn read_optional(path: &Path) -> Result<Vec<u8>, PressError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(storage(e)),
    }
}

fn snapshot(source: &Path, dest: &Path) -> Result<(), PressError> {
    std::fs::write(dest, read_optional(source)?).map_err(storage)
}

/// Snapshot queue bytes first, then copy only this edition's selected saved pages. Missing
/// pages remain missing locally so normal per-source failure handling still applies.
fn snapshot_reading(
    source: &Path,
    dest: &Path,
    date: NaiveDate,
    max: usize,
) -> Result<(), PressError> {
    let bytes = read_optional(source)?;
    std::fs::write(dest, &bytes).map_err(storage)?;
    let selected: HashSet<String> = ReadingQueue::new(dest.into())
        .for_edition(date, max)
        .into_iter()
        .map(|e| e.id)
        .collect();
    let pages = dest.with_file_name("reading-pages");
    std::fs::create_dir(&pages).map_err(storage)?;
    let mut lines = Vec::new();
    for (index, line) in bytes.split(|b| *b == b'\n').enumerate() {
        if let Ok(mut entry) = serde_json::from_slice::<Entry>(line)
            && selected.contains(&entry.id)
            && let Some(file) = &entry.file
        {
            let local = snapshot_page(file, &pages.join(index.to_string()))?;
            entry.file = Some(local);
            lines.extend(serde_json::to_vec(&entry).map_err(storage)?);
        } else {
            lines.extend(line);
        }
        lines.push(b'\n');
    }
    std::fs::write(dest, lines).map_err(storage)
}

/// Keep a saved page's local photo paths valid while isolating the bytes gather will read.
/// The extractor already restricts file photos to the saved page's own folder.
fn snapshot_page(source: &Path, dir: &Path) -> Result<PathBuf, PressError> {
    std::fs::create_dir(dir).map_err(storage)?;
    let local = dir.join(
        source
            .file_name()
            .ok_or_else(|| storage("invalid saved page"))?,
    );
    let body = match std::fs::read(source) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(local),
        Err(e) => return Err(storage(e)),
    };
    let original =
        crate::url::Url::from_file_path(source).map_err(|_| storage("invalid saved page"))?;
    let parent = source
        .parent()
        .ok_or_else(|| storage("invalid saved page"))?;
    let text = String::from_utf8_lossy(&body);
    if let Some(extracted) = crate::article::extract_with(&text, original.as_str(), None) {
        for photo in extracted.photos {
            let Ok(url) = crate::url::Url::parse(&photo) else {
                continue;
            };
            let Ok(path) = url.to_file_path() else {
                continue;
            };
            let Ok(relative) = path.strip_prefix(parent) else {
                continue;
            };
            let dest = dir.join(relative);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent).map_err(storage)?;
                    }
                    std::fs::write(dest, bytes).map_err(storage)?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(storage(e)),
            }
        }
    }
    // Relative references keep the same tree; absolute file references must follow it too.
    let original_dir =
        crate::url::Url::from_directory_path(parent).map_err(|_| storage("invalid saved page"))?;
    let local_dir =
        crate::url::Url::from_directory_path(dir).map_err(|_| storage("invalid saved page"))?;
    std::fs::write(
        &local,
        text.replace(original_dir.as_str(), local_dir.as_str()),
    )
    .map_err(storage)?;
    Ok(local)
}

/// Fetches current sources for one full edition. No scheduled build, export, queue commit,
/// Inbox scan, conversion, Morgue or preview pipeline runs. Source authorization is carried
/// by `net`, and the configured editor uses the normal gather entry point.
pub fn build_fresh(
    cfg: &Config,
    context: &FreshBuildContext,
    net: &dyn Net,
    opts: &GatherOptions,
) -> Result<FreshArtifact, PressError> {
    // A new directory fences every output from both persistent inputs and prior attempts.
    // Never recursively create a caller-supplied path or follow an existing output symlink.
    std::fs::create_dir(&context.work_dir).map_err(storage)?;
    let inputs = context.work_dir.join("inputs");
    std::fs::create_dir(&inputs).map_err(storage)?;
    let date = context
        .started_at
        .with_timezone(&crate::tz::local_tz())
        .date_naive();

    // Gather's complete write audit: puzzles saves/prunes/headline harvest, shelf's shown
    // markers/pruning, and the frost cache. Reading is read-only until a scheduled run
    // commits outcomes. Snapshot before any gather work, preserving main/extra semantics.
    let mut isolated = cfg.clone();
    let puzzles = inputs.join("puzzles.json");
    let shelf = inputs.join("shelf.json");
    let almanac = inputs.join("almanac.json");
    snapshot(&cfg.puzzle_store(), &puzzles)?;
    snapshot(&cfg.shelf_store(), &shelf)?;
    snapshot(&cfg.almanac_cache(), &almanac)?;
    isolated.puzzle_path = Some(puzzles);
    isolated.shelf_path = Some(shelf);
    isolated.almanac_path = Some(almanac);
    let queue = inputs.join("reading.jsonl");
    if cfg.paper.is_none() {
        let original = cfg
            .reading
            .queue_path
            .clone()
            .unwrap_or_else(ReadingQueue::default_path);
        snapshot_reading(&original, &queue, date, cfg.reading.max_items)?;
    } else {
        std::fs::write(&queue, []).map_err(storage)?;
    }
    isolated.reading.queue_path = Some(queue);
    isolated.reading.inbox = inputs.join("Inbox").display().to_string();
    isolated.press_path = Some(inputs.join("press.json"));
    if let Some(paper) = &mut isolated.paper {
        paper.state_dir = inputs.join("paper");
    }
    // Rendering is explicit below; other destination preferences must not request formats.
    isolated.export.clear();
    isolated.remarkable.enabled = false;
    isolated.email.enabled = false;
    // Bundles retain original-color assets; compatible clients choose their display mode.
    isolated.force_color = true;
    // This folder is the only delivery: without separate books, keep every saved story
    // inside the fresh paper instead of pointing to an EPUB that will not be delivered.
    if !context.books {
        isolated.reading.long_read_words = usize::MAX;
    }
    let gathered =
        crate::gather_with(&isolated, date, context.started_at, net, opts).without_nudges();
    let epub = context
        .work_dir
        .join(format!("{}.epub", crate::edition_stem(cfg, date)));
    crate::write_edition(&gathered, &epub)
        .map_err(|_| PressError::new(PressErrorCode::BuildFailed))?;
    let mut built = crate::report(&gathered.edition, epub);
    if context.books {
        for lr in &gathered.long_reads {
            let path = context.work_dir.join(format!("{}.epub", lr.file_stem));
            crate::longread::write_long_read(lr, date, &path)
                .map_err(|_| PressError::new(PressErrorCode::BuildFailed))?;
            built.long_reads.push(path);
        }
    }
    let bundle = crate::bundle::write_fresh(
        &gathered,
        &built,
        &context.work_dir,
        context.books,
        context.started_at,
        &context.identity,
    )
    .map_err(|_| PressError::new(PressErrorCode::BuildFailed))?;
    let bytes = std::fs::read(bundle.join("edition.json")).map_err(storage)?;
    let manifest_sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(FreshArtifact {
        bundle,
        manifest_sha256,
    })
}
