// SPDX-License-Identifier: GPL-3.0-or-later
//! Export folders (docs/superpowers/specs/2026-10-23-other-readers-design.md §7): after every
//! build, each `[[export]]` folder gets the edition as an EPUB, in color or gray, and keeps the
//! newest `keep`. A folder is always reachable, so there is no outbox: an iCloud Drive folder
//! reaches an iPad's Files app, Dropbox a Boox.
//!
//! A folder that can't be written is a line in the report, never a failed run (as an email
//! failure, email design §13).

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::Serialize;

use crate::config::{ExportConfig, ExportFormat, Photos};
use crate::{BuildReport, EDITION_PREFIX};

/// What one export folder got.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ExportReport {
    pub name: String,
    pub dir: PathBuf,
    /// Files written, the paper first.
    pub written: Vec<PathBuf>,
    /// Older editions removed to keep `keep`.
    pub pruned: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The edition's name in an export folder: `deckle-daily-YYYY-MM-DD.epub`.
pub fn export_filename(date: NaiveDate) -> String {
    format!("{EDITION_PREFIX}{date}.epub")
}

/// The extension an export's editions have.
fn extension(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Epub => "epub",
        ExportFormat::Pdf => "pdf",
        ExportFormat::Bundle => "deckle",
    }
}

/// Every export in turn.
pub fn export_all(
    exports: &[ExportConfig],
    date: NaiveDate,
    built: &BuildReport,
) -> Vec<ExportReport> {
    exports.iter().map(|e| export(e, date, built)).collect()
}

/// The edition (and with `books`, the long reads) into one export folder, then the prune.
pub fn export(e: &ExportConfig, date: NaiveDate, built: &BuildReport) -> ExportReport {
    let dir = crate::inbox::expand_home(e.dir.trim());
    let mut report = ExportReport {
        name: e.name.clone(),
        dir: dir.clone(),
        ..Default::default()
    };
    if let Err(why) = ready(&dir) {
        report.error = Some(why);
        return report;
    }
    if e.format == ExportFormat::Bundle {
        return export_bundle(e, date, built, report);
    }
    let color = e.photos == Photos::Color;
    let paper = match (e.format, &built.color_epub, color) {
        (ExportFormat::Pdf, ..) => match built.pdf(e.screen, e.photos) {
            Some(p) => p.to_path_buf(),
            None => {
                report.error = Some("the PDF edition wasn't built".into());
                return report;
            }
        },
        (_, Some(c), true) => c.clone(),
        _ => built.epub.clone(),
    };
    let name = format!("{EDITION_PREFIX}{date}.{}", extension(e.format));
    let mut files = vec![(paper, name)];
    if e.books {
        let reads = match color && built.color_long_reads.len() == built.long_reads.len() {
            true => &built.color_long_reads,
            false => &built.long_reads,
        };
        for p in reads {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            files.push((p.clone(), name));
        }
    }
    for (from, name) in files {
        match copy_into(&from, &dir, &name) {
            Ok(p) => report.written.push(p),
            Err(why) => {
                report.error = Some(why);
                return report;
            }
        }
    }
    match prune_ext(&dir, e.keep, extension(e.format)) {
        Ok(gone) => report.pruned = gone,
        Err(why) => report.error = Some(why),
    }
    report
}

/// A bundle export: the build's bundle copied in whole, through a hidden folder renamed into
/// place, then the prune (edition bundle design §4, §5).
fn export_bundle(
    e: &ExportConfig,
    date: NaiveDate,
    built: &BuildReport,
    mut report: ExportReport,
) -> ExportReport {
    let dir = report.dir.clone();
    let Some(src) = built.bundle(e.books) else {
        report.error = Some("the edition bundle wasn't built".into());
        return report;
    };
    let name = crate::bundle::bundle_name(date);
    let tmp = dir.join(format!(".{name}.tmp"));
    let to = dir.join(&name);
    let copied = (|| -> std::io::Result<()> {
        let _ = std::fs::remove_dir_all(&tmp);
        copy_dir(src, &tmp)?;
        if to.exists() {
            std::fs::remove_dir_all(&to)?;
        }
        std::fs::rename(&tmp, &to)
    })();
    if let Err(err) = copied {
        let _ = std::fs::remove_dir_all(&tmp);
        report.error = Some(format!("{}: {err}", to.display()));
        return report;
    }
    report.written.push(to);
    match prune_dirs(&dir, e.keep) {
        Ok(gone) => report.pruned = gone,
        Err(why) => report.error = Some(why),
    }
    report
}

/// Copies the folder `from` to `to`, files and subfolders.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

/// Removes all but the newest `keep` bundles (`deckle-daily-YYYY-MM-DD.deckle` folders) in
/// `dir`; 0 keeps all. A bundle still being written has a hidden name and never counts.
fn prune_dirs(dir: &Path, keep: usize) -> Result<Vec<PathBuf>, String> {
    if keep == 0 {
        return Ok(Vec::new());
    }
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut editions: Vec<(NaiveDate, PathBuf)> = rd
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let date = name.strip_prefix(EDITION_PREFIX)?.strip_suffix(".deckle")?;
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            Some((date, entry.path()))
        })
        .collect();
    editions.sort_unstable_by_key(|e| std::cmp::Reverse(e.0));
    let mut gone = Vec::new();
    for (_, p) in editions.into_iter().skip(keep) {
        std::fs::remove_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        gone.push(p);
    }
    Ok(gone)
}

/// The folder exists, or is made when its parent does: a missing iCloud Drive or Dropbox is
/// reported, not created.
fn ready(dir: &Path) -> Result<(), String> {
    if dir.is_dir() {
        return Ok(());
    }
    match dir.parent() {
        Some(parent) if parent.is_dir() => {
            std::fs::create_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))
        }
        _ => Err(format!("{}: no such folder", dir.display())),
    }
}

/// Copies `from` to `dir/name` through a hidden temporary file, so a sync client never uploads
/// half a book.
fn copy_into(from: &Path, dir: &Path, name: &str) -> Result<PathBuf, String> {
    let to = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::copy(from, &tmp).map_err(|e| format!("{}: {e}", from.display()))?;
    std::fs::rename(&tmp, &to).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", to.display())
    })?;
    Ok(to)
}

/// Removes all but the newest `keep` editions in `dir`; 0 keeps all. Only names Deckle writes
/// (`deckle-daily-YYYY-MM-DD.epub`) count, so long reads and the owner's own files stay.
pub fn prune(dir: &Path, keep: usize) -> Result<Vec<PathBuf>, String> {
    prune_ext(dir, keep, "epub")
}

/// [`prune`] for editions with extension `ext` (`deckle-daily-YYYY-MM-DD.<ext>`).
pub fn prune_ext(dir: &Path, keep: usize, ext: &str) -> Result<Vec<PathBuf>, String> {
    if keep == 0 {
        return Ok(Vec::new());
    }
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut editions: Vec<(NaiveDate, PathBuf)> = rd
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let date = name
                .strip_prefix(EDITION_PREFIX)?
                .strip_suffix(ext)?
                .strip_suffix('.')?;
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            Some((date, entry.path()))
        })
        .collect();
    editions.sort_unstable_by_key(|e| std::cmp::Reverse(e.0));
    let mut gone = Vec::new();
    for (_, p) in editions.into_iter().skip(keep) {
        std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        gone.push(p);
    }
    Ok(gone)
}

/// One line per export for the run's text report.
pub fn lines(reports: &[ExportReport]) -> String {
    reports
        .iter()
        .map(|r| match &r.error {
            Some(why) => format!("export {}: not written: {why}", r.name),
            None => {
                let mut line = format!("export {} -> {}", r.name, r.dir.display());
                if !r.pruned.is_empty() {
                    line.push_str(&format!(" (removed {} older)", r.pruned.len()));
                }
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built(dir: &Path, color: bool, reads: &[&str]) -> BuildReport {
        let epub = dir.join("deckle-daily-2026-10-23.epub");
        std::fs::write(&epub, b"gray paper").unwrap();
        let color_dir = dir.join("color");
        std::fs::create_dir_all(&color_dir).unwrap();
        let mut long_reads = Vec::new();
        let mut color_long_reads = Vec::new();
        for r in reads {
            let p = dir.join(format!("{r}.epub"));
            std::fs::write(&p, b"gray read").unwrap();
            long_reads.push(p);
            if color {
                let c = color_dir.join(format!("{r}.epub"));
                std::fs::write(&c, b"color read").unwrap();
                color_long_reads.push(c);
            }
        }
        let color_epub = color.then(|| {
            let c = color_dir.join("deckle-daily-2026-10-23.epub");
            std::fs::write(&c, b"color paper").unwrap();
            c
        });
        BuildReport {
            epub,
            title: "The Deckle Daily".into(),
            sections: Vec::new(),
            long_reads,
            color_epub,
            color_long_reads,
            pdfs: Vec::new(),
            bundles: Vec::new(),
            reading_outcomes: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn cfg(dir: &Path) -> ExportConfig {
        ExportConfig {
            name: "iPad".into(),
            dir: dir.display().to_string(),
            ..Default::default()
        }
    }

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
    }

    #[test]
    fn a_color_export_gets_the_color_paper_and_a_gray_one_the_gray() {
        let tmp = tempfile::tempdir().unwrap();
        let b = built(tmp.path(), true, &[]);
        let out = tmp.path().join("ipad");
        let r = export(&cfg(&out), day(), &b);
        assert_eq!(r.error, None);
        assert_eq!(r.written, vec![out.join("deckle-daily-2026-10-23.epub")]);
        assert_eq!(std::fs::read(&r.written[0]).unwrap(), b"color paper");

        let gray = ExportConfig {
            photos: Photos::Gray,
            ..cfg(&tmp.path().join("boox"))
        };
        let r = export(&gray, day(), &b);
        assert_eq!(std::fs::read(&r.written[0]).unwrap(), b"gray paper");
    }

    #[test]
    fn color_without_a_color_build_falls_back_to_the_paper() {
        let tmp = tempfile::tempdir().unwrap();
        let b = built(tmp.path(), false, &[]);
        let r = export(&cfg(&tmp.path().join("ipad")), day(), &b);
        assert_eq!(std::fs::read(&r.written[0]).unwrap(), b"gray paper");
    }

    #[test]
    fn books_brings_the_long_reads_in_the_same_photos() {
        let tmp = tempfile::tempdir().unwrap();
        let b = built(tmp.path(), true, &["deckle-read-a"]);
        let out = tmp.path().join("ipad");
        let without = export(&cfg(&out), day(), &b);
        assert_eq!(without.written.len(), 1);
        let with = ExportConfig {
            books: true,
            ..cfg(&out)
        };
        let r = export(&with, day(), &b);
        assert_eq!(r.written[1], out.join("deckle-read-a.epub"));
        assert_eq!(std::fs::read(&r.written[1]).unwrap(), b"color read");
    }

    #[test]
    fn keep_removes_only_older_editions_deckle_wrote() {
        let tmp = tempfile::tempdir().unwrap();
        let b = built(tmp.path(), false, &[]);
        let out = tmp.path().join("ipad");
        std::fs::create_dir(&out).unwrap();
        for d in ["2026-10-19", "2026-10-20", "2026-10-21", "2026-10-22"] {
            std::fs::write(out.join(format!("deckle-daily-{d}.epub")), b"old").unwrap();
        }
        for other in [
            "deckle-read-a.epub",
            "notes.epub",
            "deckle-daily-draft.epub",
        ] {
            std::fs::write(out.join(other), b"mine").unwrap();
        }
        let e = ExportConfig {
            keep: 2,
            ..cfg(&out)
        };
        let r = export(&e, day(), &b);
        let mut pruned: Vec<_> = r.pruned.iter().map(|p| p.file_name().unwrap()).collect();
        pruned.sort();
        assert_eq!(
            pruned,
            [
                "deckle-daily-2026-10-19.epub",
                "deckle-daily-2026-10-20.epub",
                "deckle-daily-2026-10-21.epub"
            ]
        );
        let mut left: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "deckle-daily-2026-10-22.epub",
                "deckle-daily-2026-10-23.epub",
                "deckle-daily-draft.epub",
                "deckle-read-a.epub",
                "notes.epub"
            ]
        );
    }

    #[test]
    fn a_pdf_export_gets_the_pdf_for_its_screen_and_prunes_pdfs() {
        use crate::pdf::Screen;
        let tmp = tempfile::tempdir().unwrap();
        let mut b = built(tmp.path(), false, &[]);
        let pdf = tmp.path().join("p.pdf");
        std::fs::write(&pdf, b"%PDF-paper").unwrap();
        b.pdfs.push(crate::PdfBuild {
            screen: Screen::Remarkable2,
            photos: Photos::Gray,
            path: pdf,
        });
        let out = tmp.path().join("boox");
        std::fs::create_dir(&out).unwrap();
        for d in ["2026-10-20", "2026-10-21"] {
            std::fs::write(out.join(format!("deckle-daily-{d}.pdf")), b"old").unwrap();
            std::fs::write(out.join(format!("deckle-daily-{d}.epub")), b"old").unwrap();
        }
        let e = ExportConfig {
            format: ExportFormat::Pdf,
            photos: Photos::Gray,
            keep: 2,
            ..cfg(&out)
        };
        let r = export(&e, day(), &b);
        assert_eq!(r.error, None);
        assert_eq!(r.written, [out.join("deckle-daily-2026-10-23.pdf")]);
        assert_eq!(std::fs::read(&r.written[0]).unwrap(), b"%PDF-paper");
        assert_eq!(r.pruned, [out.join("deckle-daily-2026-10-20.pdf")]);
        assert!(
            out.join("deckle-daily-2026-10-20.epub").is_file(),
            "EPUBs aren't its"
        );
        // Another screen's PDF wasn't built: said so, nothing written.
        let other = ExportConfig {
            screen: Screen::Ipad,
            ..e
        };
        assert!(export(&other, day(), &b).error.is_some());
    }

    #[test]
    fn keep_zero_keeps_everything() {
        let tmp = tempfile::tempdir().unwrap();
        for d in ["2026-10-01", "2026-10-02"] {
            std::fs::write(tmp.path().join(format!("deckle-daily-{d}.epub")), b"old").unwrap();
        }
        assert!(prune(tmp.path(), 0).unwrap().is_empty());
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 2);
    }

    #[test]
    fn a_missing_folder_is_made_only_when_its_parent_is_there() {
        let tmp = tempfile::tempdir().unwrap();
        let b = built(tmp.path(), false, &[]);
        let r = export(&cfg(&tmp.path().join("new")), day(), &b);
        assert_eq!(r.error, None);
        let r = export(&cfg(&tmp.path().join("no-icloud/Deckle Daily")), day(), &b);
        assert!(r.error.as_deref().unwrap().contains("no such folder"));
        assert!(r.written.is_empty());
        assert!(!tmp.path().join("no-icloud").exists());
        assert!(lines(&[r]).starts_with("export iPad: not written:"));
    }

    #[test]
    fn no_temporary_file_is_left_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let b = built(tmp.path(), false, &[]);
        let out = tmp.path().join("ipad");
        export(&cfg(&out), day(), &b);
        let names: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["deckle-daily-2026-10-23.epub"]);
    }
}
