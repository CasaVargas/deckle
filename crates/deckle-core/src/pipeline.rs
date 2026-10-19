// SPDX-License-Identifier: GPL-3.0-or-later
//! The whole pipeline for one book: inspect → normalize → convert → repackage → deliver.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::cover_rule::{AsinChoice, Decision, DeviceClass, MatchResult, Override, decide};
use crate::deliver::{DeliverOptions, Target, deliver_with};
use crate::inspect::{BookInfo, inspect};
use crate::matcher::{MatchOutcome, match_book};
use crate::normalize::{NormalizeOptions, NormalizeReport, normalize};
use crate::previewer;
use crate::xray::{LlmConfig, XRayOptions, XRaySummary};
use deckle_kfx::convert::{CdeType, ConvertOptions, ConvertOutput, kpf_to_kfx};
use deckle_kfx::metadata::generate_asin;

#[derive(Clone, Debug)]
pub struct RunOptions {
    /// Legacy default; the cover rule decides unless `cde_override` is set.
    pub cde_type: CdeType,
    /// Owner override for this run (per book or per device).
    pub cde_override: Option<CdeType>,
    /// Look up an Amazon edition (Open Library) when the device needs a real ASIN.
    pub amazon_match: bool,
    /// Spike 2 passed on this device: real ASINs keep our sidecars.
    pub verified_real_asin: bool,
    /// Explicit ASIN; `None` generates a recognizable one from the book's identifiers.
    pub asin: Option<String>,
    /// `None`: no page list; `Some(None)`: auto; `Some(Some(n))`: n pages.
    pub pages: Option<Option<u32>>,
    pub target: Target,
    pub previewer: Option<PathBuf>,
    pub keep_work_dir: bool,
    /// X-Ray sidecar on EBOK books (design §4.5).
    pub xray: XRayMode,
    /// LLM descriptions for X-Ray when set (design §7).
    pub llm: Option<LlmConfig>,
    /// Wikipedia summaries for X-Ray terms (opt-in network).
    pub wikipedia: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum XRayMode {
    Off,
    /// Build one whenever the book ships as EBOK.
    Auto,
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
    pub decision: Option<Decision>,
    pub matched: Option<String>,
    /// What went into the X-Ray sidecar, when one shipped.
    pub xray: Option<XRaySummary>,
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

    // Cover rule (design §5): decide PDOC/EBOK, the ASIN, and whether we write the thumbnail.
    let device_class = match &opts.target {
        Target::Folder(_) => DeviceClass::Folder,
        Target::MassStorage(d) => DeviceClass::MassStorage {
            thumbnails_writable: d.thumbnails_writable,
        },
        Target::Mtp(_) => DeviceClass::Mtp,
    };
    let user = match opts.cde_override {
        Some(CdeType::Ebok) => Override::PreferFeatures,
        Some(CdeType::Pdoc) => Override::PreferCover,
        None => Override::None,
    };
    let needs_match = opts.amazon_match
        && matches!(
            device_class,
            DeviceClass::Mtp
                | DeviceClass::MassStorage {
                    thumbnails_writable: false
                }
        );
    let matched = if needs_match {
        on_stage("match");
        let t = Instant::now();
        let outcome = match_book(&info, &mut crate::matcher::http_fetch);
        report
            .stage_ms
            .insert("match", t.elapsed().as_millis() as u64);
        match outcome {
            MatchOutcome::Candidate { asin, .. } => {
                report.matched = Some(asin.clone());
                MatchResult::Candidate {
                    asin,
                    verified: opts.verified_real_asin,
                }
            }
            MatchOutcome::NotFound => MatchResult::None,
            MatchOutcome::Error(e) => {
                report.warnings.push(format!("edition lookup failed: {e}"));
                MatchResult::None
            }
        }
    } else {
        MatchResult::None
    };
    let decision = decide(&device_class, &matched, user);
    let asin = match (&opts.asin, &decision.asin) {
        (Some(a), _) => a.clone(),
        (None, AsinChoice::Real(a)) => a.clone(),
        (None, AsinChoice::Generated) => generate_asin(&asin_seed(&info)),
    };
    let cde_type = decision.cde;
    report.decision = Some(decision.clone());
    let convert_opts = ConvertOptions {
        cde_type,
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
    report.asin = Some(asin.clone());
    report.cde_type = Some(cde_type.as_str().to_owned());
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

    let mut deliver_opts = DeliverOptions::default();
    if decision.write_thumbnail {
        match crate::thumbnail::cover_jpeg_from_kfx(&out.bytes)
            .map(|c| crate::thumbnail::render(&c, crate::thumbnail::THUMBNAIL_HEIGHT))
        {
            Some(Ok(jpeg)) => {
                deliver_opts.thumbnail =
                    Some((crate::thumbnail::thumbnail_name(&asin, cde_type), jpeg))
            }
            Some(Err(e)) => report.warnings.push(format!("thumbnail not written: {e}")),
            None => report
                .warnings
                .push("thumbnail not written: the book has no cover".into()),
        }
    }
    if cde_type == CdeType::Ebok && opts.xray == XRayMode::Auto {
        let xray_opts = XRayOptions {
            llm: opts.llm.clone(),
            wikipedia: opts.wikipedia,
        };
        let built = stage!("enrich", enrich(&out.bytes, &asin, &info, &xray_opts));
        match built {
            Some(x) => {
                report.warnings.extend(x.warnings);
                if work_keep && let Err(e) = std::fs::write(work_path.join(&x.file_name), &x.bytes)
                {
                    report
                        .warnings
                        .push(format!("could not keep {}: {e}", x.file_name));
                }
                report.xray = Some(x.summary);
                deliver_opts.sidecars.push((x.file_name, x.bytes));
            }
            None => report
                .warnings
                .push("X-Ray: no recurring names found in this book; no sidecar written".into()),
        }
    }
    let delivered = stage!(
        "deliver",
        deliver_with(&out.bytes, &info, &opts.target, &deliver_opts)
    );
    report.delivered = Some(delivered);
    report
}

/// The X-Ray sidecar for an EBOK book, from the final KFX's text runs.
fn enrich(
    kfx: &[u8],
    asin: &str,
    info: &BookInfo,
    opts: &XRayOptions,
) -> Result<Option<crate::xray::XRayOutput>, String> {
    let (runs, erl) = deckle_kfx::text::content_runs(kfx).map_err(|e| e.to_string())?;
    crate::xray::build(asin, &info.title, &info.language, &runs, erl, opts)
        .map_err(|e| e.to_string())
}
