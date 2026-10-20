// SPDX-License-Identifier: GPL-3.0-or-later
mod cli;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use deckle_core::deliver::{Target, deliver, detect_mass_storage};
use deckle_core::inspect::{BookInfo, inspect};
use deckle_core::normalize::{NormalizeOptions, normalize};
use deckle_core::pipeline::{RunOptions, run};
use deckle_core::previewer;
use deckle_kfx::convert::{ConvertOptions, kpf_to_kfx};

const EXIT_FAILURE: u8 = 1;
const EXIT_USAGE: u8 = 2;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match dispatch(cli) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

fn emit<T: serde::Serialize>(
    json: bool,
    value: &T,
    text: impl FnOnce() -> String,
) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
        );
    } else {
        println!("{}", text());
    }
    Ok(())
}

fn resolve_target(to: &str) -> Result<Target, String> {
    if to == "device" {
        let mut devices = detect_mass_storage();
        if !devices.is_empty() {
            return Ok(Target::MassStorage(devices.remove(0)));
        }
        let mut mtp = deckle_transport::usb::detect();
        if !mtp.is_empty() {
            return Ok(Target::Mtp(mtp.remove(0)));
        }
        return Err(
            "no Kindle connected (mass storage or MTP); pass --to <folder> to export instead"
                .into(),
        );
    }
    let dir = PathBuf::from(to);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(Target::Folder(dir))
}

/// Title and first author of a KFX, for naming the file when sending it.
fn info_from_kfx(kfx: &[u8]) -> BookInfo {
    use deckle_kfx::ion::{Sym, Value};
    let mut info = BookInfo {
        title: "book".into(),
        ..Default::default()
    };
    let Ok(doc) = deckle_kfx::container::read_kfx(kfx) else {
        return info;
    };
    let Some(md) = doc.fragments.get_single(&Sym::new("$490")) else {
        return info;
    };
    let Some(cats) = md
        .value
        .as_struct()
        .and_then(|s| s.get("$491"))
        .and_then(Value::as_list)
    else {
        return info;
    };
    for cat in cats {
        let Some(c) = cat.as_struct() else { continue };
        if c.get("$495").and_then(Value::as_str) != Some("kindle_title_metadata") {
            continue;
        }
        for kv in c.get("$258").and_then(Value::as_list).into_iter().flatten() {
            let Some(s) = kv.as_struct() else { continue };
            match (
                s.get("$492").and_then(Value::as_str),
                s.get("$307").and_then(Value::as_str),
            ) {
                (Some("title"), Some(t)) => info.title = t.to_owned(),
                (Some("author"), Some(a)) => info.creators.push(deckle_core::inspect::Creator {
                    name: a.to_owned(),
                    file_as: Some(a.to_owned()),
                }),
                _ => {}
            }
        }
    }
    info
}

fn dispatch(cli: cli::Cli) -> Result<ExitCode, String> {
    let json = cli.json;
    match cli.command {
        cli::Command::Inspect { epub } => {
            let info = inspect(&epub).map_err(|e| e.to_string())?;
            emit(json, &info, || {
                let authors: Vec<&str> = info.creators.iter().map(|c| c.name.as_str()).collect();
                let series = info
                    .series
                    .as_ref()
                    .map(|s| {
                        format!(
                            "\nseries:   {} #{}",
                            s.name,
                            s.index.map(|i| i.to_string()).unwrap_or_default()
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "title:    {}\nauthors:  {}\nlanguage: {}{}\nspine:    {} items\ntoc:      {} entries\ncover:    {}\nwarnings: {}",
                    info.title,
                    authors.join(", "),
                    info.language,
                    series,
                    info.spine.len(),
                    info.toc.len(),
                    info.cover_href.clone().unwrap_or_else(|| "none".into()),
                    if info.warnings.is_empty() {
                        "none".to_owned()
                    } else {
                        info.warnings.join("; ")
                    }
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::Command::Normalize { epub, output } => {
            let r = normalize(&epub, &output, &NormalizeOptions::default())
                .map_err(|e| e.to_string())?;
            emit(json, &r, || format!("{r:#?}"))?;
            Ok(ExitCode::SUCCESS)
        }
        cli::Command::Convert {
            input,
            output,
            cde,
            asin,
            pages,
            no_authors,
            container_id,
            creator,
            previewer: pv,
        } => {
            let pages = cli::parse_pages(pages.as_deref())?;
            let is_kpf = input
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("kpf"));
            let (kpf_path, authors, language, _work) = if is_kpf {
                (
                    input.clone(),
                    if no_authors { Some(Vec::new()) } else { None },
                    None,
                    None,
                )
            } else {
                let p = previewer::locate(pv.as_deref()).map_err(|e| e.to_string())?;
                let info = inspect(&input).map_err(|e| e.to_string())?;
                let work = tempfile::tempdir().map_err(|e| e.to_string())?;
                let normalized = work.path().join("normalized.epub");
                normalize(&input, &normalized, &NormalizeOptions::default())
                    .map_err(|e| e.to_string())?;
                let r = previewer::convert(&p, &normalized, &work.path().join("previewer"))
                    .map_err(|e| e.to_string())?;
                let authors = if no_authors || info.creators.is_empty() {
                    Some(Vec::new())
                } else {
                    Some(deckle_core::pipeline::kfx_authors(&info))
                };
                (r.kpf, authors, Some(info.language), Some(work))
            };
            let opts = ConvertOptions {
                cde_type: cde.into(),
                asin,
                title: None,
                authors,
                language,
                approximate_pages: pages,
                container_id,
                creator,
                exact_language: false,
            };
            let out = kpf_to_kfx(&kpf_path, &opts).map_err(|e| e.to_string())?;
            std::fs::write(&output, &out.bytes)
                .map_err(|e| format!("cannot write {}: {e}", output.display()))?;
            #[derive(serde::Serialize)]
            struct Out<'a> {
                output: &'a Path,
                bytes: usize,
                pages: Option<u32>,
                warnings: &'a [String],
            }
            let o = Out {
                output: &output,
                bytes: out.bytes.len(),
                pages: out.pages,
                warnings: &out.warnings,
            };
            emit(json, &o, || {
                format!(
                    "wrote {} ({} bytes){}{}",
                    output.display(),
                    out.bytes.len(),
                    out.pages
                        .map(|p| format!(", {p} pages"))
                        .unwrap_or_default(),
                    if out.warnings.is_empty() {
                        String::new()
                    } else {
                        format!("\nwarnings: {}", out.warnings.join("; "))
                    }
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::Command::Send { kfx, to } => {
            let bytes =
                std::fs::read(&kfx).map_err(|e| format!("cannot read {}: {e}", kfx.display()))?;
            let target = resolve_target(&to)?;
            let info = info_from_kfx(&bytes);
            let path = deliver(&bytes, &info, &target).map_err(|e| e.to_string())?;
            emit(json, &path, || format!("sent to {}", path.display()))?;
            Ok(ExitCode::SUCCESS)
        }
        cli::Command::Devices => {
            let devices = detect_mass_storage();
            let mtp = deckle_transport::usb::detect();
            #[derive(serde::Serialize)]
            struct All<'a> {
                mass_storage: &'a [deckle_core::deliver::Device],
                mtp: &'a [deckle_transport::usb::MtpInfo],
            }
            let all = All {
                mass_storage: &devices,
                mtp: &mtp,
            };
            emit(json, &all, || {
                let mut lines: Vec<String> = devices
                    .iter()
                    .map(|d| {
                        format!(
                            "{}  {}  mass storage, thumbnails:{}",
                            d.root.display(),
                            d.model_hint,
                            if d.thumbnails_writable {
                                "writable"
                            } else {
                                "no"
                            }
                        )
                    })
                    .collect();
                lines.extend(mtp.iter().map(|m| {
                    format!(
                        "usb {}:{}  {}  MTP (serial {})",
                        m.bus,
                        m.address,
                        m.product.clone().unwrap_or_else(|| "Kindle".into()),
                        m.serial.clone().unwrap_or_default()
                    )
                }));
                if lines.is_empty() {
                    "no Kindle connected".to_owned()
                } else {
                    lines.join("\n")
                }
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::Command::Digest { config, action } => digest(json, config, action),
        cli::Command::Run {
            epubs,
            to,
            cde,
            pages,
            previewer: pv,
            amazon_match,
            verified_asin,
            asin,
            xray,
            llm,
            llm_model,
            llm_endpoint,
            wikipedia,
            word_wise,
            word_wise_level,
            wordwise_db,
            keep_work_dir,
        } => {
            if epubs.is_empty() {
                return Ok(ExitCode::from(EXIT_USAGE));
            }
            if asin.is_some() && epubs.len() > 1 {
                return Err("--asin names one book's edition: pass a single EPUB".into());
            }
            let asin = asin
                .as_deref()
                .map(deckle_core::cover_rule::manual_asin)
                .transpose()?;
            let pages = cli::parse_pages(pages.as_deref())?;
            // Fail before any work (and before touching the target) when Previewer is missing.
            let located = previewer::locate(pv.as_deref()).map_err(|e| e.to_string())?;
            let target = resolve_target(&to)?;
            let opts = RunOptions {
                cde_type: cde.into(),
                cde_override: cde.to_override(),
                amazon_match,
                verified_real_asin: verified_asin,
                asin,
                pages,
                target,
                previewer: Some(located.path),
                keep_work_dir,
                xray: xray.into(),
                llm: llm.config(llm_model, llm_endpoint),
                wikipedia,
                word_wise: (word_wise == cli::XRayArg::Auto).then(|| {
                    deckle_core::wordwise::WordWiseOptions {
                        level: word_wise_level,
                        database: wordwise_db,
                        ..Default::default()
                    }
                }),
            };
            let reports: Vec<_> = epubs.iter().map(|e| run(e, &opts)).collect();
            let failed = reports.iter().filter(|r| r.error.is_some()).count();
            emit(json, &reports, || {
                reports
                    .iter()
                    .map(|r| match (&r.error, &r.delivered) {
                        (Some(e), _) => format!("FAIL  {}\n      {e}", r.input.display()),
                        (None, Some(d)) => format!(
                            "OK    {}\n      -> {}{}{}{}{}{}",
                            r.input.display(),
                            d.display(),
                            r.pages
                                .map(|p| format!("  ({p} pages)"))
                                .unwrap_or_default(),
                            r.decision
                                .as_ref()
                                .map(|x| format!("\n      {}: {}", x.cde.as_str(), x.reason))
                                .unwrap_or_default(),
                            r.xray
                                .as_ref()
                                .map(|x| format!(
                                    "\n      X-Ray: {} people, {} terms ({})",
                                    x.people, x.terms, x.source
                                ))
                                .unwrap_or_default(),
                            r.word_wise
                                .as_ref()
                                .map(|w| format!(
                                    "\n      Word Wise: {} hints (level {}, {} dictionary)",
                                    w.hints, w.level, w.dictionary
                                ))
                                .unwrap_or_default(),
                            if r.warnings.is_empty() {
                                String::new()
                            } else {
                                format!("\n      warnings: {}", r.warnings.join("; "))
                            }
                        ),
                        (None, None) => format!("??    {}", r.input.display()),
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
            Ok(if failed == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(EXIT_FAILURE)
            })
        }
    }
}

fn parse_date(arg: Option<&str>) -> Result<deckle_digest::chrono::NaiveDate, String> {
    match arg {
        None => Ok(deckle_digest::chrono::Local::now().date_naive()),
        Some(d) => deckle_digest::chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|_| format!("--date expects YYYY-MM-DD, got {d:?}")),
    }
}

fn load_digest_config(path: &Path) -> Result<deckle_digest::Config, String> {
    if !path.exists() {
        return Err(format!(
            "no digest config at {}; run `deckle digest init` first",
            path.display()
        ));
    }
    deckle_digest::Config::load(path)
}

fn section_lines(r: &deckle_digest::BuildReport) -> String {
    r.sections
        .iter()
        .map(|s| match (&s.unavailable, s.items) {
            (Some(u), 0) => format!("      {}: {u}", s.title),
            (Some(u), n) => format!("      {}: {n} ({u})", s.title),
            (None, n) => format!("      {}: {n}", s.title),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn digest(
    json: bool,
    config: Option<PathBuf>,
    action: cli::DigestCommand,
) -> Result<ExitCode, String> {
    let path = config.unwrap_or_else(deckle_digest::config::default_path);
    match action {
        cli::DigestCommand::Init { force } => {
            if path.exists() && !force {
                return Err(format!(
                    "{} exists; pass --force to replace it",
                    path.display()
                ));
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, deckle_digest::config::STARTER).map_err(|e| e.to_string())?;
            emit(json, &path, || format!("wrote {}", path.display()))?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Build { output, date } => {
            let cfg = load_digest_config(&path)?;
            let date = parse_date(date.as_deref())?;
            std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
            let now = deckle_digest::chrono::Utc::now();
            let report =
                deckle_digest::build(&cfg, date, now, &output, &mut deckle_digest::http_get)?;
            emit(json, &report, || {
                format!(
                    "OK    {}\n      -> {}\n{}",
                    report.title,
                    report.epub.display(),
                    section_lines(&report)
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Run {
            to,
            date,
            previewer: pv,
            keep_work_dir,
        } => {
            let cfg = load_digest_config(&path)?;
            let date = parse_date(date.as_deref())?;
            let located = previewer::locate(pv.as_deref()).map_err(|e| e.to_string())?;
            let target = resolve_target(&to)?;
            let work = deckle_core::wordwise::default_cache_dir().join("digest");
            std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
            let now = deckle_digest::chrono::Utc::now();
            let built = deckle_digest::build(&cfg, date, now, &work, &mut deckle_digest::http_get)?;
            let opts = RunOptions {
                cde_type: deckle_kfx::convert::CdeType::Pdoc,
                // The cover is the front page: it must always show (design §5).
                cde_override: Some(deckle_kfx::convert::CdeType::Pdoc),
                amazon_match: false,
                verified_real_asin: false,
                asin: None,
                pages: None,
                target: target.clone(),
                previewer: Some(located.path),
                keep_work_dir,
                xray: deckle_core::pipeline::XRayMode::Off,
                llm: None,
                wikipedia: false,
                word_wise: None,
            };
            let report = run(&built.epub, &opts);
            let pruned = if report.delivered.is_some() {
                let prefix = deckle_core::deliver::device_filename(&BookInfo {
                    title: cfg.title.clone(),
                    creators: vec![deckle_core::inspect::Creator {
                        name: deckle_digest::AUTHOR.into(),
                        file_as: Some(deckle_digest::AUTHOR.into()),
                    }],
                    ..Default::default()
                });
                let prefix = prefix.trim_end_matches(".kfx");
                deckle_core::deliver::prune_editions(&target, prefix, cfg.keep.max(1))
                    .map_err(|e| e.to_string())?
            } else {
                Some(Vec::new())
            };
            #[derive(serde::Serialize)]
            struct DigestRun<'a> {
                build: &'a deckle_digest::BuildReport,
                run: &'a deckle_core::pipeline::RunReport,
                pruned: &'a Option<Vec<PathBuf>>,
            }
            let all = DigestRun {
                build: &built,
                run: &report,
                pruned: &pruned,
            };
            emit(json, &all, || {
                let head = match (&report.error, &report.delivered) {
                    (Some(e), _) => format!("FAIL  {}\n      {e}", built.title),
                    (None, Some(d)) => format!("OK    {}\n      -> {}", built.title, d.display()),
                    (None, None) => format!("OK    {}", built.title),
                };
                let prune = match &pruned {
                    None => "\n      older editions: not pruned over MTP yet".to_owned(),
                    Some(p) if p.is_empty() => String::new(),
                    Some(p) => format!("\n      removed {} older edition(s)", p.len()),
                };
                format!("{head}\n{}{prune}", section_lines(&built))
            })?;
            Ok(if report.error.is_some() {
                ExitCode::from(EXIT_FAILURE)
            } else {
                ExitCode::SUCCESS
            })
        }
    }
}
