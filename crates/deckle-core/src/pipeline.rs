// SPDX-License-Identifier: GPL-3.0-or-later
//! The whole pipeline for one book: inspect → normalize → convert → repackage → deliver.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::deliver::{Target, deliver};
use crate::inspect::{BookInfo, inspect};
use crate::normalize::{NormalizeOptions, NormalizeReport, normalize};
use crate::previewer;
use deckle_kfx::convert::{CdeType, ConvertOptions, ConvertOutput, kpf_to_kfx};
use deckle_kfx::metadata::generate_asin;

#[derive(Clone, Debug)]
pub struct RunOptions {
    pub cde_type: CdeType,
    /// Explicit ASIN; `None` generates a recognizable one from the book's identifiers.
    pub asin: Option<String>,
    /// `None`: no page list; `Some(None)`: auto; `Some(Some(n))`: n pages.
    pub pages: Option<Option<u32>>,
    pub target: Target,
    pub previewer: Option<PathBuf>,
    pub keep_work_dir: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RunReport {
    pub input: PathBuf,
    pub info: BookInfo,
    pub normalize: NormalizeReport,
    pub kpf: Option<PathBuf>,
    pub kfx: Option<PathBuf>,
    pub delivered: Option<PathBuf>,
    pub pages: Option<u32>,
    pub asin: Option<String>,
    pub cde_type: Option<String>,
    pub work_dir: Option<PathBuf>,
    pub warnings: Vec<String>,
    /// `"<stage>: <message>"` of the stage that failed.
    pub error: Option<String>,
    pub stage_ms: BTreeMap<&'static str, u64>,
}

impl RunReport {
    pub fn push_size_warning(&mut self, len: usize) {
        if let Some(w) = ConvertOutput::size_warning(len) {
            self.warnings.push(w);
        }
    }
}

/// Author names for the KFX: `file-as` when present, else the display name (stamped in sort form).
pub fn kfx_authors(info: &BookInfo) -> Vec<String> {
    info.creators
        .iter()
        .map(|c| {
            c.file_as
                .clone()
                .filter(|f| !f.trim().is_empty())
                .unwrap_or_else(|| c.name.clone())
        })
        .collect()
}

/// Seed for the generated ASIN: the first identifier, else title + first author.
pub fn asin_seed(info: &BookInfo) -> Vec<u8> {
    match info.identifiers.first() {
        Some(id) => id.as_bytes().to_vec(),
        None => format!(
            "{}|{}",
            info.title,
            info.creators.first().map(|c| c.name.as_str()).unwrap_or("")
        )
        .into_bytes(),
    }
}

/// Runs every stage; never panics, failures land in `report.error`.
pub fn run(epub: &Path, opts: &RunOptions) -> RunReport {
    run_with_progress(epub, opts, &mut |_| {})
}

/// Like [`run`], calling `on_stage` with the stage name just before each stage starts.
pub fn run_with_progress(
    epub: &Path,
    opts: &RunOptions,
    on_stage: &mut dyn FnMut(&'static str),
) -> RunReport {
    let mut report = RunReport {
        input: epub.to_path_buf(),
        ..Default::default()
    };
    let previewer = match previewer::locate(opts.previewer.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            report.error = Some(format!("previewer: {e}"));
            return report;
        }
    };
    macro_rules! stage {
        ($name:literal, $expr:expr) => {{
            on_stage($name);
            let t = Instant::now();
            let r = $expr;
            report
                .stage_ms
                .insert($name, t.elapsed().as_millis() as u64);
            match r {
                Ok(v) => v,
                Err(e) => {
                    report.error = Some(format!("{}: {}", $name, e));
                    return report;
                }
            }
        }};
    }
    let info = stage!("inspect", inspect(epub));
    report.info = info.clone();
    report.warnings.extend(info.warnings.iter().cloned());

    let work = match tempfile::Builder::new().prefix("deckle-").tempdir() {
        Ok(d) => d,
        Err(e) => {
            report.error = Some(format!("workdir: {e}"));
            return report;
        }
    };
    let work_path = if opts.keep_work_dir {
        work.keep()
    } else {
        work.path().to_path_buf()
    };
    let work_keep = opts.keep_work_dir;
    report.work_dir = work_keep.then(|| work_path.clone());

    let normalized = work_path.join("normalized.epub");
    let nreport = stage!(
        "normalize",
        normalize(epub, &normalized, &NormalizeOptions::default())
    );
    report.warnings.extend(nreport.warnings.iter().cloned());
    report.normalize = nreport;

    let kpf = stage!(
        "convert",
        previewer::convert(&previewer, &normalized, &work_path.join("previewer"))
    );
    report.kpf = Some(kpf.kpf.clone());
    for issue in kpf.log_issues.iter().filter(|i| i.should_surface()) {
        report
            .warnings
            .push(format!("previewer: {}", issue.message));
    }

    let asin = opts
        .asin
        .clone()
        .unwrap_or_else(|| generate_asin(&asin_seed(&info)));
    let convert_opts = ConvertOptions {
        cde_type: opts.cde_type,
        asin: Some(asin.clone()),
        title: None,
        authors: if info.creators.is_empty() {
            None
        } else {
            Some(kfx_authors(&info))
        },
        language: Some(info.language.clone()),
        approximate_pages: opts.pages,
        container_id: None,
        creator: None,
    };
    let out = stage!("repackage", kpf_to_kfx(&kpf.kpf, &convert_opts));
    report.pages = out.pages;
    report.asin = Some(asin);
    report.cde_type = Some(opts.cde_type.as_str().to_owned());
    report.warnings.extend(
        out.warnings
            .iter()
            .filter(|w| !w.starts_with("KFX is "))
            .cloned(),
    );
    report.push_size_warning(out.bytes.len());
    let kfx_path = work_path.join("book.kfx");
    if work_keep && let Err(e) = std::fs::write(&kfx_path, &out.bytes) {
        report
            .warnings
            .push(format!("could not keep {}: {e}", kfx_path.display()));
    }
    report.kfx = work_keep.then_some(kfx_path);

    let delivered = stage!("deliver", deliver(&out.bytes, &info, &opts.target));
    report.delivered = Some(delivered);
    report
}
