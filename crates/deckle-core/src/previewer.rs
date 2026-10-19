// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 4a: drive Kindle Previewer's command line to turn an EPUB into a KPF.

use std::path::{Path, PathBuf};
use std::process::Command;
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

#[derive(Clone, Debug, Serialize)]
pub struct KpfResult {
    pub kpf: PathBuf,
    pub et_supported: bool,
    /// `Error` rows of the per-book log.
    pub log_errors: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("Kindle Previewer is not installed; download it from {download_url}")]
    PreviewerMissing { download_url: &'static str },
    #[error("Kindle Previewer refused Enhanced Typesetting: {first_error}")]
    EnhancedTypesettingUnsupported { first_error: String },
    #[error("Kindle Previewer failed: {stderr}")]
    PreviewerFailed { stderr: String },
    #[error("Kindle Previewer produced no output")]
    NoOutput,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Candidate install locations, most preferred first.
pub fn default_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
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
            // Verify on a Windows machine; mirrors the 3.x layout plus the 4.x CLI name.
            v.push(local.join("Amazon/Kindle Previewer 4/KindlePreviewer4CLI.exe"));
            v.push(local.join("Amazon/Kindle Previewer 4/kindlepreviewer4.exe"));
            v.push(local.join("Amazon/Kindle Previewer 3/lib/fc/bin/kindlepreviewer.exe"));
        }
    }
    let _ = home;
    v
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
            if !p.is_file() {
                return Err(ConvertError::PreviewerMissing {
                    download_url: PREVIEWER_DOWNLOAD_URL,
                });
            }
            p.to_path_buf()
        }
        None => match candidates.iter().find(|c| c.is_file()) {
            Some(c) => c.clone(),
            None => {
                return Err(ConvertError::PreviewerMissing {
                    download_url: PREVIEWER_DOWNLOAD_URL,
                });
            }
        },
    };
    let version = probe_version(&path);
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

fn probe_version(path: &Path) -> String {
    let out = Command::new(path).arg("--help").output();
    let text = match out {
        Ok(o) => format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(_) => String::new(),
    };
    // "Kindle Previewer 4.0.3 Copyright (c) Amazon.com"
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("Kindle Previewer ") {
            let ver: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !ver.is_empty() {
                return ver;
            }
        }
    }
    String::from("unknown")
}

/// Runs the conversion. `out_dir` is created if needed (the CLI refuses a missing folder).
pub fn convert(p: &Previewer, epub: &Path, out_dir: &Path) -> Result<KpfResult, ConvertError> {
    std::fs::create_dir_all(out_dir)?;
    let epub = std::path::absolute(epub)?;
    let out_dir = std::path::absolute(out_dir)?;
    let started = Instant::now();
    let mut cmd = Command::new(&p.path);
    cmd.arg(&epub);
    if p.major >= 4 {
        cmd.args(["--convert", "--locale", "en", "--output"])
            .arg(&out_dir);
    } else {
        cmd.args(["-convert", "-locale", "en", "-output"])
            .arg(&out_dir);
    }
    let output = cmd.output()?;
    if !output.status.success() {
        return Err(ConvertError::PreviewerFailed {
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let summary_path = out_dir.join("Summary_Log.csv");
    let summary = std::fs::read(&summary_path).map_err(|_| ConvertError::NoOutput)?;
    let summary = strip_bom(&summary);
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(summary.as_slice());
    let headers = rdr.headers().map_err(|_| ConvertError::NoOutput)?.clone();
    let col = |name: &str| headers.iter().position(|h| h == name);
    let (et_col, out_col, log_col) = (
        col("Enhanced Typesetting Status"),
        col("Output File Path"),
        col("Log File Path"),
    );
    let mut et_supported = false;
    let mut kpf: Option<PathBuf> = None;
    let mut log_path: Option<PathBuf> = None;
    if let Some(rec) = rdr.records().flatten().next() {
        et_supported = et_col
            .and_then(|c| rec.get(c))
            .is_some_and(|s| s.eq_ignore_ascii_case("Supported"));
        kpf = out_col
            .and_then(|c| rec.get(c))
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        log_path = log_col
            .and_then(|c| rec.get(c))
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
    }
    let log_errors = log_path.as_deref().map(read_log_errors).unwrap_or_default();
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
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn strip_bom(b: &[u8]) -> Vec<u8> {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b).to_vec()
}

/// `Error` rows of a Previewer per-book log, as "Description (Source File:Line)".
pub fn read_log_errors(path: &Path) -> Vec<String> {
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
        if rec.get(0).is_some_and(|t| t.eq_ignore_ascii_case("Error")) {
            let desc = rec.get(1).unwrap_or("").trim();
            let file = rec.get(2).unwrap_or("").trim();
            let line = rec.get(3).unwrap_or("").trim();
            let loc = match (file.is_empty(), line.is_empty()) {
                (true, _) => String::new(),
                (false, true) => format!(" ({file})"),
                (false, false) => format!(" ({file}:{line})"),
            };
            out.push(format!("{desc}{loc}"));
        }
    }
    out
}
