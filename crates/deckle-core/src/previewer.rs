// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 4a: drive Kindle Previewer's command line to turn an EPUB into a KPF.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

pub const PREVIEWER_DOWNLOAD_URL: &str = "https://kdp.amazon.com/en_US/help/topic/G202131170";

#[derive(Clone, Debug, Serialize)]
pub struct Previewer {
    pub path: PathBuf,
    /// e.g. `4.0.3`, parsed from the `--help` banner.
    pub version: String,
    pub major: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum IssueKind {
    Error,
    Warning,
    Notice,
}

/// One row of Previewer's per-book log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LogIssue {
    pub kind: IssueKind,
    /// "Description (Source File:Line)".
    pub message: String,
}

impl LogIssue {
    /// Errors and warnings always matter; notices only when they mean content went missing.
    pub fn should_surface(&self) -> bool {
        match self.kind {
            IssueKind::Error | IssueKind::Warning => true,
            IssueKind::Notice => {
                let m = self.message.to_lowercase();
                m.contains("not found") || m.contains("missing") || m.contains("could not be")
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct KpfResult {
    /// Previewer's book: a KPF, or for a fixed-layout book a joint KF7+KF8 MOBI (with
    /// `et_supported` false; fixed-layout spec §4).
    pub kpf: PathBuf,
    pub et_supported: bool,
    /// `Error` rows of the per-book log.
    pub log_errors: Vec<String>,
    /// Every row of the per-book log.
    pub log_issues: Vec<LogIssue>,
    pub duration_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("Kindle Previewer is not installed; download it from {download_url}")]
    PreviewerMissing { download_url: &'static str },
    #[error("Kindle Previewer at {path} could not be started: {reason}")]
    PreviewerUnlaunchable { path: PathBuf, reason: String },
    #[error("Kindle Previewer refused Enhanced Typesetting: {first_error}")]
    EnhancedTypesettingUnsupported { first_error: String },
    #[error("Kindle Previewer could not convert the book: {message}")]
    ConversionFailed { message: String },
    #[error("Kindle Previewer failed: {stderr}")]
    PreviewerFailed { stderr: String },
    #[error("Kindle Previewer produced no output")]
    NoOutput,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Candidate install locations, most preferred first.
pub fn default_candidates() -> Vec<PathBuf> {
    #[allow(unused_mut)]
    let mut v: Vec<PathBuf> = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    {
        for base in [
            Some(PathBuf::from("/Applications")),
            home.as_ref().map(|h| h.join("Applications")),
        ]
        .into_iter()
        .flatten()
        {
            v.push(base.join("Kindle Previewer 4.app/Contents/MacOS/KindlePreviewer4CLI"));
            v.push(base.join("Kindle Previewer 3.app/Contents/lib/fc/bin/kindlepreviewer"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            // The 4.x installer drops App Execution Aliases into WindowsApps (issue #101).
            v.extend(windows_apps_candidates(
                &local.join("Microsoft/WindowsApps"),
            ));
            v.push(local.join("Amazon/Kindle Previewer 4/KindlePreviewer4CLI.exe"));
            v.push(local.join("Amazon/Kindle Previewer 4/kindlepreviewer4.exe"));
            v.push(local.join("Amazon/Kindle Previewer 3/lib/fc/bin/kindlepreviewer.exe"));
        }
    }
    let _ = home;
    v
}

/// Kindle Previewer executables in a WindowsApps-style folder, CLI aliases first. The alias
/// names aren't documented, so match any `*kindle*previewer*.exe`.
pub fn windows_apps_candidates(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(bool, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let hit =
                name.ends_with(".exe") && name.contains("kindle") && name.contains("previewer");
            hit.then(|| (!name.contains("cli"), e.path()))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// Whether `p` looks like something we can run. App Execution Aliases are reparse points that
/// `metadata` can't follow (OS error 1920), so fall back to the link's own metadata.
fn is_executable_candidate(p: &Path) -> bool {
    p.is_file() || (cfg!(windows) && std::fs::symlink_metadata(p).is_ok_and(|m| !m.is_dir()))
}

/// Finds Kindle Previewer: an explicit path, else `DECKLE_PREVIEWER`, else the default install
/// locations.
pub fn locate(override_path: Option<&Path>) -> Result<Previewer, ConvertError> {
    let env = std::env::var_os("DECKLE_PREVIEWER").map(PathBuf::from);
    locate_in(
        &default_candidates(),
        override_path.map(Path::to_path_buf).or(env).as_deref(),
    )
}

/// `locate` over an explicit candidate list (for tests).
pub fn locate_in(
    candidates: &[PathBuf],
    override_path: Option<&Path>,
) -> Result<Previewer, ConvertError> {
    let path = match override_path {
        Some(p) => {
            if !is_executable_candidate(p) {
                return Err(ConvertError::PreviewerMissing {
                    download_url: PREVIEWER_DOWNLOAD_URL,
                });
            }
            p.to_path_buf()
        }
        None => match candidates.iter().find(|c| is_executable_candidate(c)) {
            Some(c) => c.clone(),
            None => {
                return Err(ConvertError::PreviewerMissing {
                    download_url: PREVIEWER_DOWNLOAD_URL,
                });
            }
        },
    };
    let version = match probe_version(&path) {
        Ok(v) => v,
        // A path the user chose should say why it won't run, not claim Previewer is absent.
        Err(e) if override_path.is_some() => {
            return Err(ConvertError::PreviewerUnlaunchable {
                path,
                reason: e.to_string(),
            });
        }
        Err(_) => String::from("unknown"),
    };
    let major = version
        .split('.')
        .next()
        .and_then(|m| m.parse().ok())
        .unwrap_or_else(|| {
            if path.to_string_lossy().contains("Previewer 3") {
                3
            } else {
                4
            }
        });
    Ok(Previewer {
        path,
        version,
        major,
    })
}

fn probe_version(path: &Path) -> std::io::Result<String> {
    let o = crate::quiet::command(path).arg("--help").output()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    // "Kindle Previewer 4.0.3 Copyright (c) Amazon.com"
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("Kindle Previewer ") {
            let ver: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !ver.is_empty() {
                return Ok(ver);
            }
        }
    }
    Ok(String::from("unknown"))
}

/// Waits for any other Previewer run, in this process or another, to finish. Previewer works in
/// `$TMPDIR/kpr_cli_<input stem>_epub` whatever folder the input is in, and every caller passes a
/// `normalized.epub`, so overlapping runs delete each other's files (#235). The lock sits in the
/// same temp folder, so it covers exactly the runs that would share that scratch folder. Held
/// until the returned file is dropped.
fn previewer_lock() -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::env::temp_dir().join("deckle-kindle-previewer.lock"))?;
    file.lock()?;
    Ok(file)
}

/// Runs the conversion, one at a time. `out_dir` is created if needed (the CLI refuses a missing
/// folder).
pub fn convert(p: &Previewer, epub: &Path, out_dir: &Path) -> Result<KpfResult, ConvertError> {
    convert_as(p, epub, out_dir, false)
}

/// [`convert`], where a `fixed_layout` book may come back as the MOBI Previewer makes when it
/// can't use Enhanced Typesetting (fixed-layout spec §4). A reflowable book refused Enhanced
/// Typesetting is still an error: its MOBI would be a worse book than the KFX it should get.
pub fn convert_as(
    p: &Previewer,
    epub: &Path,
    out_dir: &Path,
    fixed_layout: bool,
) -> Result<KpfResult, ConvertError> {
    std::fs::create_dir_all(out_dir)?;
    let epub = std::path::absolute(epub)?;
    let out_dir = std::path::absolute(out_dir)?;
    let _lock = previewer_lock()?;
    let started = Instant::now();
    let mut cmd = crate::quiet::command(&p.path);
    cmd.arg(&epub);
    if p.major >= 4 {
        cmd.args(["--convert", "--locale", "en", "--output"])
            .arg(&out_dir);
    } else {
        cmd.args(["-convert", "-locale", "en", "-output"])
            .arg(&out_dir);
    }
    let output = cmd.output()?;
    let console = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let console_tail = || {
        console
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    if !output.status.success() {
        return Err(ConvertError::PreviewerFailed {
            stderr: console.trim().to_owned(),
        });
    }
    let summary_path = out_dir.join("Summary_Log.csv");
    let summary = std::fs::read(&summary_path).map_err(|_| ConvertError::ConversionFailed {
        message: format!("no Summary_Log.csv was written ({})", console_tail()),
    })?;
    let summary = strip_bom(&summary);
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(summary.as_slice());
    let headers = rdr.headers().map_err(|_| ConvertError::NoOutput)?.clone();
    let col = |name: &str| headers.iter().position(|h| h == name);
    let (et_col, status_col, out_col, log_col) = (
        col("Enhanced Typesetting Status"),
        col("Conversion Status"),
        col("Output File Path"),
        col("Log File Path"),
    );
    let Some(rec) = rdr.records().flatten().next() else {
        return Err(ConvertError::ConversionFailed {
            message: format!("empty Summary_Log.csv ({})", console_tail()),
        });
    };
    let et_supported = et_col
        .and_then(|c| rec.get(c))
        .is_some_and(|s| s.eq_ignore_ascii_case("Supported"));
    let status = status_col.and_then(|c| rec.get(c)).unwrap_or("").to_owned();
    let kpf = out_col
        .and_then(|c| rec.get(c))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let log_path = log_col
        .and_then(|c| rec.get(c))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let log_issues = log_path.as_deref().map(read_log_issues).unwrap_or_default();
    let log_errors: Vec<String> = log_issues
        .iter()
        .filter(|i| i.kind == IssueKind::Error)
        .map(|i| i.message.clone())
        .collect();
    if !status.eq_ignore_ascii_case("Success") {
        let message = log_errors
            .first()
            .cloned()
            .unwrap_or_else(|| format!("status {status:?} ({})", console_tail()));
        return Err(ConvertError::ConversionFailed { message });
    }
    let is_mobi = |k: &Path| {
        k.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("mobi"))
    };
    if !et_supported
        && fixed_layout
        && let Some(mobi) = kpf.as_deref().filter(|k| is_mobi(k))
    {
        let mobi = if mobi.is_absolute() {
            mobi.to_path_buf()
        } else {
            out_dir.join(mobi)
        };
        if !mobi.exists() {
            return Err(ConvertError::NoOutput);
        }
        return Ok(KpfResult {
            kpf: mobi,
            et_supported,
            log_errors,
            log_issues,
            duration_ms: started.elapsed().as_millis() as u64,
        });
    }
    if !et_supported {
        let first_error = log_errors
            .first()
            .cloned()
            .unwrap_or_else(|| "book does not support Enhanced Typesetting".into());
        return Err(ConvertError::EnhancedTypesettingUnsupported { first_error });
    }
    let kpf = kpf
        .filter(|k| k.extension().is_some_and(|e| e.eq_ignore_ascii_case("kpf")))
        .ok_or(ConvertError::NoOutput)?;
    let kpf = if kpf.is_absolute() {
        kpf
    } else {
        out_dir.join(kpf)
    };
    if !kpf.exists() {
        return Err(ConvertError::NoOutput);
    }
    Ok(KpfResult {
        kpf,
        et_supported,
        log_errors,
        log_issues,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn strip_bom(b: &[u8]) -> Vec<u8> {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b).to_vec()
}

/// Every row of a Previewer per-book log, as "Description (Source File:Line)".
pub fn read_log_issues(path: &Path) -> Vec<LogIssue> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&strip_bom(&bytes)).into_owned();
    // The file starts with free-text legend lines; the table begins at the "Type" header.
    let Some(start) = text.find("\"Type\"") else {
        return Vec::new();
    };
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(&text.as_bytes()[start..]);
    let mut out = Vec::new();
    for rec in rdr.records().flatten() {
        let kind = match rec.get(0).map(str::trim) {
            Some(t) if t.eq_ignore_ascii_case("Error") => IssueKind::Error,
            Some(t) if t.eq_ignore_ascii_case("Warning") => IssueKind::Warning,
            Some(t) if t.eq_ignore_ascii_case("Notice") || t.eq_ignore_ascii_case("Info") => {
                IssueKind::Notice
            }
            _ => continue,
        };
        let desc = rec.get(1).unwrap_or("").trim();
        let file = rec.get(2).unwrap_or("").trim();
        let line = rec.get(3).unwrap_or("").trim();
        let loc = match (file.is_empty(), line.is_empty()) {
            (true, _) => String::new(),
            (false, true) => format!(" ({file})"),
            (false, false) => format!(" ({file}:{line})"),
        };
        out.push(LogIssue {
            kind,
            message: format!("{desc}{loc}"),
        });
    }
    out
}

/// `Error` rows only (kept for callers that just want blockers).
pub fn read_log_errors(path: &Path) -> Vec<String> {
    read_log_issues(path)
        .into_iter()
        .filter(|i| i.kind == IssueKind::Error)
        .map(|i| i.message)
        .collect()
}
