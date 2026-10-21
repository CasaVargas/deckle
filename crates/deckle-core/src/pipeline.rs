// SPDX-License-Identifier: GPL-3.0-or-later
//! The whole pipeline for one book: (import) → inspect → normalize → convert → repackage →
//! deliver.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::compat::{self, SendUse};
use crate::cover_rule::{AsinChoice, Decision, DeviceClass, MatchResult, Override, decide};
use crate::deliver::{Conn, DeliverError, DeliverOptions, Target, deliver_named_on, deliver_on};
use crate::devices::TransportKind;
use crate::inspect::{BookInfo, inspect};
use crate::matcher::{Fetcher, MatchOutcome, match_book};
use crate::metadata_fill::{FillOutcome, Metadata};
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
    /// Fill title, author, series, description and subjects the EPUB leaves empty from Open
    /// Library (opt-in network, issue #28).
    pub metadata_fill: bool,
    /// Curly quotes, dashes, ellipses and empty-paragraph cleanup in normalize (issue #30).
    pub typography: bool,
    /// Footnotes and endnotes to Kindle pop-up notes in normalize (issue #31).
    pub popup_notes: bool,
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
    /// Older copies of this book under other names in `documents/` (the library's send
    /// records); removed after delivery, on the same connection (Kindle-updates design §4).
    pub replaces: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum XRayMode {
    #[default]
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
    /// What the Open Library metadata fill did, when it was on.
    pub metadata_fill: Option<FillOutcome>,
    /// What went into the X-Ray sidecar, when one shipped.
    pub xray: Option<XRaySummary>,
    /// What went into the Word Wise sidecar, when one shipped.
    pub word_wise: Option<WordWiseSummary>,
    pub work_dir: Option<PathBuf>,
    /// The target Kindle's firmware, when known (`system/version.txt`, or read over MTP).
    pub firmware: Option<String>,
    pub warnings: Vec<String>,
    /// Older copies of the book removed after delivery (Kindle-updates design §4).
    pub replaced: Vec<String>,
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

/// What a prepared book asks for when it's finished: it travels with the KPF into the outbox
/// (`prepared.json`, device-aware build §4.1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wishes {
    /// The book's own CDE override; `None` lets the cover rule (and the device's override) decide.
    pub cde_override: Option<CdeType>,
    /// `None`: no page list; `Some(None)`: auto; `Some(Some(n))`: n pages. In JSON: absent,
    /// `null`, `n`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub pages: Option<Option<u32>>,
    pub xray: XRayMode,
    /// The Word Wise level wanted; `None` = off.
    pub word_wise: Option<u8>,
}

/// A field that is present (even as `null`) is `Some`.
fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}

impl Wishes {
    /// The travelling part of `opts`.
    pub fn of(opts: &RunOptions) -> Self {
        Wishes {
            cde_override: opts.cde_override,
            pages: opts.pages,
            xray: opts.xray,
            word_wise: opts.word_wise.as_ref().map(|w| w.level),
        }
    }
}

/// A book converted up to the KPF: everything before the device matters (inspect, normalize,
/// Previewer). [`finish`] does the rest for a given Kindle.
#[derive(Debug)]
pub struct Prepared {
    /// The report so far: input, book info, normalize, KPF, work dir, warnings, timings.
    pub report: RunReport,
    /// Previewer's KPF: in the work dir, or `book.kpf` in a queued entry.
    pub kpf: PathBuf,
    pub wishes: Wishes,
    /// The device file name; `None` uses `<Author sort> - <Title>.kfx`.
    pub name: Option<String>,
    /// Removes the work dir with the book unless it was kept.
    _work: Option<tempfile::TempDir>,
}

impl Prepared {
    /// A book prepared earlier (a queued entry): its KPF, info and wishes.
    pub fn from_parts(
        input: PathBuf,
        info: BookInfo,
        kpf: PathBuf,
        wishes: Wishes,
        name: Option<String>,
    ) -> Self {
        Prepared {
            report: RunReport {
                input,
                kpf: Some(kpf.clone()),
                info,
                ..Default::default()
            },
            kpf,
            wishes,
            name,
            _work: None,
        }
    }
}

/// Runs `$expr` as stage `$name`, timing it; on error records `"<stage>: <message>"` and
/// returns `$bail`.
macro_rules! stage {
    ($report:ident, $on_stage:ident, $name:literal, $expr:expr, $bail:expr) => {{
        $on_stage($name);
        let t = Instant::now();
        let r = $expr;
        $report
            .stage_ms
            .insert($name, t.elapsed().as_millis() as u64);
        match r {
            Ok(v) => v,
            Err(e) => {
                $report.error = Some(format!("{}: {}", $name, e));
                return $bail;
            }
        }
    }};
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
    match prepare(epub, opts, on_stage) {
        // One device connection for the probe and the delivery: an MTP Kindle resets on close.
        Ok(prepared) => finish(&prepared, &mut Conn::new(&opts.target), opts, on_stage),
        Err(report) => *report,
    }
}

/// The device-independent stages: (import) → inspect → (fill) → normalize → convert (Previewer
/// → KPF). `Err` carries the report with the failing stage in `error`.
pub fn prepare(
    epub: &Path,
    opts: &RunOptions,
    on_stage: &mut dyn FnMut(&'static str),
) -> Result<Prepared, Box<RunReport>> {
    prepare_with(epub, opts, on_stage, &mut crate::matcher::http_fetch)
}

/// [`prepare`] with the fetcher for the metadata fill (tests pass one that stays offline).
pub fn prepare_with(
    epub: &Path,
    opts: &RunOptions,
    on_stage: &mut dyn FnMut(&'static str),
    fetch: &mut Fetcher,
) -> Result<Prepared, Box<RunReport>> {
    let mut report = RunReport {
        input: epub.to_path_buf(),
        ..Default::default()
    };
    let previewer = match previewer::locate(opts.previewer.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            report.error = Some(format!("previewer: {e}"));
            return Err(Box::new(report));
        }
    };
    let work = match tempfile::Builder::new().prefix("deckle-").tempdir() {
        Ok(d) => d,
        Err(e) => {
            report.error = Some(format!("workdir: {e}"));
            return Err(Box::new(report));
        }
    };
    let (work_path, work) = if opts.keep_work_dir {
        (work.keep(), None)
    } else {
        (work.path().to_path_buf(), Some(work))
    };
    report.work_dir = opts.keep_work_dir.then(|| work_path.clone());

    // A Word document becomes an EPUB first; every later stage is the same (DOCX spec §3).
    let source = if deckle_docx::is_word(epub) {
        let imported = work_path.join("imported.epub");
        let ireport = stage!(
            report,
            on_stage,
            "import",
            deckle_docx::convert(epub, &imported),
            Err(Box::new(report))
        );
        report.warnings.extend(ireport.warnings);
        imported
    } else {
        epub.to_path_buf()
    };
    let mut info = stage!(
        report,
        on_stage,
        "inspect",
        inspect(&source),
        Err(Box::new(report))
    );
    // Fill-empty only: the book's own metadata always wins; a failed lookup is a warning.
    let fill = if opts.metadata_fill {
        on_stage("fill");
        let t = Instant::now();
        let (outcome, wrote) = crate::metadata_fill::fill(&mut info, fetch);
        report
            .stage_ms
            .insert("fill", t.elapsed().as_millis() as u64);
        if let FillOutcome::Error { message } = &outcome {
            report
                .warnings
                .push(format!("metadata lookup failed: {message}"));
        }
        report.metadata_fill = Some(outcome);
        wrote
    } else {
        Metadata::default()
    };
    report.info = info.clone();
    report.warnings.extend(info.warnings.iter().cloned());

    let normalized = work_path.join("normalized.epub");
    let normalize_opts = NormalizeOptions {
        fill,
        typography: opts.typography,
        popup_notes: opts.popup_notes,
        ..Default::default()
    };
    let nreport = stage!(
        report,
        on_stage,
        "normalize",
        normalize(&source, &normalized, &normalize_opts),
        Err(Box::new(report))
    );
    report.warnings.extend(nreport.warnings.iter().cloned());
    report.normalize = nreport;

    let kpf = stage!(
        report,
        on_stage,
        "convert",
        previewer::convert(&previewer, &normalized, &work_path.join("previewer")),
        Err(Box::new(report))
    );
    report.kpf = Some(kpf.kpf.clone());
    for issue in kpf.log_issues.iter().filter(|i| i.should_surface()) {
        report
            .warnings
            .push(format!("previewer: {}", issue.message));
    }
    Ok(Prepared {
        report,
        kpf: kpf.kpf,
        wishes: Wishes::of(opts),
        name: None,
        _work: work,
    })
}

/// The device-dependent stages for the Kindle on `conn`: cover rule → repackage → enrich →
/// deliver. `opts.target` is not used: the connection's target is. Never panics.
pub fn finish(
    prepared: &Prepared,
    conn: &mut Conn<'_>,
    opts: &RunOptions,
    on_stage: &mut dyn FnMut(&'static str),
) -> RunReport {
    finish_with(
        prepared,
        conn,
        opts,
        on_stage,
        &mut crate::wordwise::http_get_bytes,
    )
}

/// Removes Deckle's older copies of this book under other names, now that the new one is
/// written (Kindle-updates design §4): the files `named` (the library's records) and, on mass
/// storage and folders, other KFX with the same generated ASIN. Store books are never touched.
fn replace_older_copies(
    conn: &mut Conn<'_>,
    report: &mut RunReport,
    asin: &str,
    named: &[String],
    delivered: &Path,
) {
    let new_name = delivered
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut old: Vec<String> = named.iter().filter(|n| **n != new_name).cloned().collect();
    if crate::inventory::is_generated_asin(asin) && !matches!(conn.target, Target::Mtp(_)) {
        let cache = crate::wordwise::default_cache_dir().join("inventory");
        if let Ok(inv) = crate::inventory::list_on(conn, Some(&cache)) {
            for b in inv.books {
                if b.asin.as_deref() == Some(asin) && b.file != new_name && !old.contains(&b.file) {
                    old.push(b.file);
                }
            }
        }
    }
    for file in old {
        match crate::inventory::remove_on(conn, &file) {
            Ok(_) => report.replaced.push(file),
            // Already gone (removed by hand since that send).
            Err(DeliverError::Refused(_)) => {}
            Err(e) => report
                .warnings
                .push(format!("the older copy {file} is still on the Kindle: {e}")),
        }
    }
}

/// [`finish`] with the fetcher for the Word Wise database (a push passes one that refuses).
pub fn finish_with(
    prepared: &Prepared,
    conn: &mut Conn<'_>,
    opts: &RunOptions,
    on_stage: &mut dyn FnMut(&'static str),
    fetch: &mut crate::wordwise::FetchBytes<'_>,
) -> RunReport {
    let mut report = prepared.report.clone();
    let info = prepared.report.info.clone();
    let kpf = &prepared.kpf;
    let work_keep = report.work_dir.is_some();
    let work_path = report.work_dir.clone().unwrap_or_default();

    // Cover rule (design §5): decide PDOC/EBOK, the ASIN, and whether we write the thumbnail.
    let device_class = match conn.target {
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
    let out = stage!(
        report,
        on_stage,
        "repackage",
        kpf_to_kfx(kpf, &convert_opts),
        report
    );
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
                        &info
                            .creators
                            .iter()
                            .map(|c| c.name.as_str())
                            .collect::<Vec<_>>()
                            .join(" & "),
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
                    let built =
                        crate::wordwise::enrich(&runs, &asin, &out.asset_id, ww, conn, fetch);
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
        report,
        on_stage,
        "deliver",
        match &prepared.name {
            Some(name) => deliver_named_on(&out.bytes, name, conn, &deliver_opts),
            None => deliver_on(&out.bytes, &info, conn, &deliver_opts),
        },
        report
    );
    replace_older_copies(conn, &mut report, &asin, &opts.replaces, &delivered);
    report.delivered = Some(delivered);
    // Known issues for this Kindle (design §6): warnings only, the book is already on it.
    let used = SendUse {
        ebok_without_cover_source: cde_type == CdeType::Ebok
            && decision.asin == AsinChoice::Generated
            && deliver_opts.thumbnail.is_none(),
        xray: report.xray.is_some(),
        word_wise: report.word_wise.is_some(),
    };
    let (compat_warnings, firmware) = device_compat(conn, &used);
    for w in compat_warnings {
        if !report.warnings.contains(&w) {
            report.warnings.push(w);
        }
    }
    report.firmware = firmware;
    report
}

/// The build any Kindle can take: PDOC with its cover, no X-Ray or Word Wise, as a folder
/// export makes it. A queued book ships this when finishing it for the device fails
/// (device-aware build §4.2).
pub fn fallback_kfx(prepared: &Prepared) -> Result<Vec<u8>, String> {
    let dir = tempfile::tempdir().map_err(|e| format!("workdir: {e}"))?;
    let target = Target::Folder(dir.path().to_path_buf());
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        cde_override: Some(CdeType::Pdoc),
        amazon_match: false,
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        asin: None,
        pages: prepared.wishes.pages,
        target: target.clone(),
        previewer: None,
        keep_work_dir: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
    };
    let r = finish(prepared, &mut Conn::new(&target), &opts, &mut |_| {});
    match (r.error, r.delivered) {
        (None, Some(kfx)) => std::fs::read(&kfx).map_err(|e| e.to_string()),
        (e, _) => Err(e.unwrap_or_else(|| "not built".into())),
    }
}

/// The compatibility rows (`compat::RULES`) a finished send to `conn`'s Kindle triggers, and the
/// Kindle's firmware: `system/version.txt` on mass storage, PTP DeviceInfo on the already-open
/// MTP session. A folder has no device and gets nothing.
pub fn device_compat(conn: &mut Conn<'_>, used: &SendUse) -> (Vec<String>, Option<String>) {
    let (model, transport, firmware) = match conn.target {
        Target::Folder(_) => return (Vec::new(), None),
        Target::MassStorage(d) => (
            d.model_hint.clone(),
            TransportKind::MassStorage,
            d.firmware.clone(),
        ),
        Target::Mtp(m) => {
            let model = m.product.clone().unwrap_or_else(|| "Kindle (MTP)".into());
            let firmware = match conn.mtp() {
                Some(Ok(dev)) => dev
                    .device_info()
                    .ok()
                    .map(|i| i.device_version.trim().to_owned())
                    .filter(|v| !v.is_empty()),
                _ => None,
            };
            (model, TransportKind::Mtp, firmware)
        }
    };
    let warnings =
        compat::send_warnings(compat::RULES, &model, transport, firmware.as_deref(), used);
    (warnings, firmware)
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

#[cfg(test)]
mod tests {
    use deckle_transport::ptp::{Container, ContainerKind, DeviceInfo, op, rsp};
    use deckle_transport::session::{Mtp, Transport, TransportError};
    use deckle_transport::usb::MtpInfo;

    use super::*;

    /// Answers GetDeviceInfo with a Scribe on firmware 5.18.1.0.1 and everything else with OK.
    struct Scribe(Vec<Vec<u8>>);

    impl Transport for Scribe {
        fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
            let (c, _) = Container::decode(bytes).unwrap();
            let reply = |kind, code| Container {
                kind,
                code,
                tx_id: c.tx_id,
                params: Vec::new(),
            };
            if c.code == op::GET_DEVICE_INFO {
                let info = DeviceInfo {
                    manufacturer: "Amazon".into(),
                    model: "Kindle Scribe".into(),
                    device_version: "5.18.1.0.1".into(),
                    serial_number: "G0X1".into(),
                };
                self.0
                    .push(reply(ContainerKind::Data, c.code).encode(&info.encode()));
            }
            self.0
                .push(reply(ContainerKind::Response, rsp::OK).encode(&[]));
            Ok(())
        }
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
            if self.0.is_empty() {
                return Err(TransportError::Timeout);
            }
            let r = self.0.remove(0);
            buf[..r.len()].copy_from_slice(&r);
            Ok(r.len())
        }
    }

    fn open_scribe(_: &MtpInfo) -> Result<Mtp, TransportError> {
        Ok(Mtp::new(Box::new(Scribe(Vec::new()))))
    }

    fn scribe() -> Target {
        Target::Mtp(MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: Some("G0X1".into()),
            product: Some("Kindle Scribe".into()),
            bus: 1,
            address: 2,
        })
    }

    #[test]
    fn an_ebok_send_without_a_cover_source_to_an_mtp_kindle_is_warned_and_firmware_read() {
        let target = scribe();
        let mut conn = Conn::with_opener(&target, open_scribe);
        let used = SendUse {
            ebok_without_cover_source: true,
            xray: true,
            word_wise: true,
        };
        let (warnings, firmware) = device_compat(&mut conn, &used);
        assert_eq!(firmware.as_deref(), Some("5.18.1.0.1"));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("no cover"), "{warnings:?}");
    }

    #[test]
    fn a_pdoc_send_to_an_mtp_kindle_gets_no_compat_warning() {
        let target = scribe();
        let mut conn = Conn::with_opener(&target, open_scribe);
        let (warnings, firmware) = device_compat(&mut conn, &SendUse::default());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(firmware.as_deref(), Some("5.18.1.0.1"));
    }

    #[test]
    fn mass_storage_uses_the_version_txt_firmware_and_a_folder_has_none() {
        let target = Target::MassStorage(crate::deliver::Device {
            root: "/Volumes/Kindle".into(),
            model_hint: "Kindle 5.16.2.1.1 (4184940002)".into(),
            thumbnails_writable: false,
            firmware: Some("5.16.2.1.1".into()),
        });
        let used = SendUse {
            ebok_without_cover_source: true,
            ..Default::default()
        };
        let (warnings, firmware) = device_compat(&mut Conn::new(&target), &used);
        assert!(warnings.is_empty(), "no rows for mass storage yet");
        assert_eq!(firmware.as_deref(), Some("5.16.2.1.1"));
        let folder = Target::Folder("/tmp".into());
        assert_eq!(
            device_compat(&mut Conn::new(&folder), &used),
            (Vec::new(), None)
        );
    }
}
