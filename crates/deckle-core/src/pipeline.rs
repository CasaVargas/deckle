// SPDX-License-Identifier: GPL-3.0-or-later
//! The whole pipeline for one book: inspect → normalize → convert → repackage → deliver.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::cover_rule::{AsinChoice, Decision, DeviceClass, MatchResult, Override, decide};
use crate::deliver::{Conn, DeliverOptions, Target, deliver_on};
use crate::inspect::{BookInfo, inspect};
use crate::matcher::{MatchOutcome, match_book};
use crate::normalize::{NormalizeOptions, NormalizeReport, normalize};
use crate::previewer;
use crate::wordwise::{WordWiseOptions, WordWiseOutput, WordWiseSummary};
use crate::xray::{LlmConfig, XRayOptions, XRaySummary};
use deckle_kfx::convert::{CdeType, ConvertOptions, ConvertOutput, kpf_to_kfx};
use deckle_kfx::metadata::{fix_language, generate_asin};

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
    /// An ASIN the owner typed (design §3); replaces Match. `None`: Match, else generated.
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
    /// Word Wise sidecar on EBOK books; `None` = off.
    pub word_wise: Option<WordWiseOptions>,
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
    /// What went into the Word Wise sidecar, when one shipped.
    pub word_wise: Option<WordWiseSummary>,
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
    let manual = match opts.asin.as_deref().map(crate::cover_rule::manual_asin) {
        Some(Err(e)) => {
            report.error = Some(e);
            return report;
        }
        Some(Ok(a)) => Some(a),
        None => None,
    };
    // A typed ASIN replaces Match (design §3).
    let matched = if let Some(asin) = manual {
        MatchResult::Candidate {
            asin,
            verified: opts.verified_real_asin,
        }
    } else if needs_match {
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
    let asin = match &decision.asin {
        AsinChoice::Real(a) => a.clone(),
        AsinChoice::Generated => generate_asin(&asin_seed(&info)),
    };
    let cde_type = decision.cde;
    report.decision = Some(decision.clone());
    // WordDumb does the same: the Kindle hides Word Wise hints unless the language is `en`.
    let english_word_wise = cde_type == CdeType::Ebok
        && opts.word_wise.is_some()
        && fix_language(info.language.split(['-', '_']).next().unwrap_or("")) == "en";
    let convert_opts = ConvertOptions {
        cde_type,
        asin: Some(asin.clone()),
        title: None,
        authors: if info.creators.is_empty() {
            None
        } else {
            Some(kfx_authors(&info))
        },
        language: Some(if english_word_wise {
            "en".to_owned()
        } else {
            info.language.clone()
        }),
        exact_language: english_word_wise,
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
    // One device connection for the probe and the delivery: an MTP Kindle resets on close.
    let mut conn = Conn::new(&opts.target);
    let want_xray = cde_type == CdeType::Ebok && opts.xray == XRayMode::Auto;
    let word_wise = opts
        .word_wise
        .as_ref()
        .filter(|_| cde_type == CdeType::Ebok);
    if want_xray || word_wise.is_some() {
        on_stage("enrich");
        let t = Instant::now();
        match deckle_kfx::text::content_runs(&out.bytes) {
            Ok((runs, erl)) => {
                if want_xray {
                    let xray_opts = XRayOptions {
                        llm: opts.llm.clone(),
                        wikipedia: opts.wikipedia,
                    };
                    let built = crate::xray::build(
                        &asin,
                        &info.title,
                        &info.language,
                        &runs,
                        erl,
                        &xray_opts,
                    )
                    .map_err(|e| e.to_string());
                    if work_keep && let Ok(Some(x)) = &built {
                        keep_file(&mut report, &work_path, &x.file_name, &x.bytes);
                    }
                    attach_xray(&mut report, &mut deliver_opts, built);
                }
                if let Some(ww) = word_wise {
                    let built = crate::wordwise::enrich(
                        &runs,
                        &asin,
                        &out.asset_id,
                        ww,
                        &mut conn,
                        &mut crate::wordwise::http_get_bytes,
                    );
                    if work_keep && let Ok(Some(w)) = &built {
                        keep_file(&mut report, &work_path, &w.file_name, &w.bytes);
                    }
                    attach_word_wise(&mut report, &mut deliver_opts, built, ww.level);
                }
            }
            Err(e) => report.warnings.push(format!(
                "X-Ray and Word Wise skipped: cannot read the KFX text: {e}"
            )),
        }
        report
            .stage_ms
            .insert("enrich", t.elapsed().as_millis() as u64);
    }
    let delivered = stage!(
        "deliver",
        deliver_on(&out.bytes, &info, &mut conn, &deliver_opts)
    );
    report.delivered = Some(delivered);
    report
}

/// Puts a built sidecar on the delivery, or turns an X-Ray failure into a warning: the book
/// still ships (design §4.5: no X-Ray is an accepted outcome, a lost book is not).
pub fn attach_xray(
    report: &mut RunReport,
    deliver_opts: &mut DeliverOptions,
    built: Result<Option<crate::xray::XRayOutput>, String>,
) {
    match built {
        Ok(Some(x)) => {
            report.warnings.extend(x.warnings);
            report.xray = Some(x.summary);
            deliver_opts.sidecars.push((x.file_name, x.bytes));
        }
        Ok(None) => report
            .warnings
            .push("X-Ray: no recurring names found in this book; no sidecar written".into()),
        Err(e) => report.warnings.push(format!("X-Ray skipped: {e}")),
    }
}

fn keep_file(report: &mut RunReport, work: &Path, name: &str, bytes: &[u8]) {
    if let Err(e) = std::fs::write(work.join(name), bytes) {
        report.warnings.push(format!("could not keep {name}: {e}"));
    }
}

/// Puts a built Word Wise sidecar (and Deckle's dictionary, when the device needs it) on the
/// delivery, or turns a failure into a warning: the book still ships.
pub fn attach_word_wise(
    report: &mut RunReport,
    deliver_opts: &mut DeliverOptions,
    built: Result<Option<WordWiseOutput>, String>,
    level: u8,
) {
    match built {
        Ok(Some(w)) => {
            report.warnings.extend(w.warnings);
            if w.summary.dictionary == "deckle" {
                report.warnings.push(
                    "Word Wise uses Deckle's dictionary on this Kindle: set Word Wise → Language \
                     to 中文 (Chinese) once; the hints are in English"
                        .into(),
                );
            }
            report.word_wise = Some(w.summary);
            deliver_opts.sidecars.push((w.file_name, w.bytes));
            if let Some(f) = w.dictionary_file {
                deliver_opts.system_files.push(f);
            }
        }
        Ok(None) => report.warnings.push(format!(
            "Word Wise: no hint-worthy words at level {level}; no sidecar written"
        )),
        Err(e) => report.warnings.push(format!("Word Wise skipped: {e}")),
    }
}
