// SPDX-License-Identifier: GPL-3.0-or-later
//! The `deckle` command line, as a library so the desktop app can run it too.

mod cli;
mod inbox;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use deckle_core::deliver::{Target, deliver, detect_mass_storage};
use deckle_core::inspect::{BookInfo, inspect};
use deckle_core::normalize::{NormalizeOptions, normalize};
use deckle_core::pipeline::{RunOptions, RunReport, run};
use deckle_core::previewer;
use deckle_kfx::convert::{ConvertOptions, kpf_to_kfx};

const EXIT_FAILURE: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Runs the `deckle` command line on `args` (program name first). The `deckle` binary and
/// `deckle-desktop digest|save …` both come here (Daily design §5c).
pub fn main_with<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = match cli::Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return ExitCode::from(if e.use_stderr() { EXIT_USAGE } else { 0 });
        }
    };
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
    deckle_core::inspect::inspect_kfx(kfx)
        .map(|k| k.info)
        .ok()
        .filter(|i| !i.title.is_empty())
        .unwrap_or_else(|| BookInfo {
            title: "book".into(),
            ..Default::default()
        })
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
                            "{}  {}  mass storage, thumbnails:{}  sync id \"{}\"",
                            d.root.display(),
                            d.model_hint,
                            if d.thumbnails_writable {
                                "writable"
                            } else {
                                "no"
                            },
                            deckle_core::sync::device_id(&Target::MassStorage(d.clone()))
                        )
                    })
                    .collect();
                lines.extend(mtp.iter().map(|m| {
                    format!(
                        "usb {}:{}  {}  MTP (serial {})  sync id \"{}\"",
                        m.bus,
                        m.address,
                        m.product.clone().unwrap_or_else(|| "Kindle".into()),
                        m.serial.clone().unwrap_or_default(),
                        deckle_core::sync::device_id(&Target::Mtp(m.clone()))
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
        cli::Command::Library { action } => library(json, action),
        cli::Command::Kindle { to, action } => kindle(json, &to, action),
        cli::Command::Save {
            targets,
            title,
            list,
            remove,
            bookmarklet,
        } => save(json, targets, title, list, remove, bookmarklet),
        cli::Command::Run {
            epubs,
            to,
            cde,
            pages,
            previewer: pv,
            amazon_match,
            fill_metadata,
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
                metadata_fill: fill_metadata,
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
                replaces: Vec::new(),
            };
            let reports: Vec<_> = epubs
                .iter()
                .map(|e| run_input(e, &opts, &deckle_digest::Http))
                .collect();
            let failed = reports.iter().filter(|r| r.error.is_some()).count();
            emit(json, &reports, || {
                reports
                    .iter()
                    .map(|r| match (&r.error, &r.delivered) {
                        (Some(e), _) => format!("FAIL  {}\n      {e}", r.input.display()),
                        (None, Some(d)) => format!(
                            "OK    {}\n      -> {}{}{}{}{}{}{}",
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
                            r.replaced
                                .iter()
                                .map(|f| format!("\n      replaced {f}"))
                                .collect::<String>(),
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

/// Where `run` and the desktop app write web articles' EPUBs (web-article design §3).
pub fn articles_dir() -> PathBuf {
    deckle_core::wordwise::default_cache_dir().join("articles")
}

/// A web address, link file or saved page built into an EPUB in `dir`, with the Daily's
/// photo settings when it has a config.
pub fn import_article(
    target: &str,
    net: &dyn deckle_digest::Net,
    dir: &Path,
) -> Result<deckle_digest::web_book::WebBook, String> {
    let cfg =
        deckle_digest::Config::load(&deckle_digest::config::default_path()).unwrap_or_default();
    let today = deckle_digest::chrono::Local::now().date_naive();
    deckle_digest::web_book::build(target, &cfg, net, dir, today)
}

/// One `run` argument: a web article is built into an EPUB first; the report names the
/// argument either way.
fn run_input(input: &Path, opts: &RunOptions, net: &dyn deckle_digest::Net) -> RunReport {
    let arg = input.to_string_lossy();
    let source = if !deckle_digest::web_book::is_article(&arg) {
        input.to_path_buf()
    } else {
        match import_article(&arg, net, &articles_dir()) {
            Ok(book) => book.path,
            Err(e) => {
                return RunReport {
                    input: input.to_path_buf(),
                    error: Some(format!("import: {e}")),
                    ..Default::default()
                };
            }
        }
    };
    // The copy the library last put on this Kindle under another name goes (§4 of the
    // Kindle-updates design).
    let config = deckle_library::config::default_path();
    let opts = RunOptions {
        replaces: deckle_library::previous_copies(&config, &source, &opts.target),
        ..opts.clone()
    };
    let report = RunReport {
        input: input.to_path_buf(),
        ..run(&source, &opts)
    };
    add_to_library(&source, report, &opts.target)
}

/// Every book a run sends goes into the library, when there is one (library design §3).
/// Failing to add is a warning: the send itself worked.
fn add_to_library(source: &Path, mut report: RunReport, target: &Target) -> RunReport {
    let config = deckle_library::config::default_path();
    if let Err(e) = deckle_library::auto_add(&config, source, &report, target) {
        report.warnings.push(deckle_library::auto_add_warning(&e));
    }
    report
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

fn kindle(json: bool, to: &str, action: cli::KindleCommand) -> Result<ExitCode, String> {
    use deckle_core::deliver::Conn;
    use deckle_core::inventory::{list_on, remove_on};
    let target = resolve_target(to)?;
    let device = deckle_library::device_ref(&target);
    let library = deckle_library::Library::configured(&deckle_library::config::default_path())
        .map(|(l, _)| l);
    let mut conn = Conn::new(&target);
    match action {
        cli::KindleCommand::Books => {
            let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
            let inv = list_on(&mut conn, Some(&cache)).map_err(|e| e.to_string())?;
            let current = deckle_library::freshness::Current::now(&Default::default());
            let shelf =
                deckle_library::shelf::shelve(inv.books, library.as_ref(), &device.key, &current);
            let rows: Vec<_> = shelf
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "book": s.book,
                        "library": s.library.as_ref().map(|b| &b.dir),
                        "updates": s.updates,
                        "duplicate": s.duplicate,
                    })
                })
                .collect();
            emit(
                json,
                &serde_json::json!({ "device": device.name, "space": inv.space, "books": rows }),
                || {
                    let mut lines = vec![match inv.space {
                        Some(s) => format!(
                            "{}: {} free of {}",
                            device.name,
                            megabytes(s.free),
                            megabytes(s.total)
                        ),
                        None => device.name.clone(),
                    }];
                    for s in &shelf {
                        let b = &s.book;
                        let title = match (&b.title, b.authors.first()) {
                            (Some(t), Some(a)) => format!("{t} — {a}"),
                            (Some(t), None) => t.clone(),
                            _ => b.file.clone(),
                        };
                        let flags = [
                            b.cde.clone().unwrap_or_else(|| b.format.to_uppercase()),
                            if b.xray {
                                "X-Ray".into()
                            } else {
                                String::new()
                            },
                            if b.word_wise {
                                "Word Wise".into()
                            } else {
                                String::new()
                            },
                            if s.library.is_some() {
                                "in library".into()
                            } else {
                                String::new()
                            },
                            if s.duplicate {
                                "DUPLICATE".into()
                            } else {
                                String::new()
                            },
                            if s.updates.is_empty() {
                                String::new()
                            } else {
                                let why: Vec<String> =
                                    s.updates.iter().map(|r| r.describe()).collect();
                                format!("can be updated: {}", why.join("; "))
                            },
                        ]
                        .into_iter()
                        .filter(|f| !f.is_empty())
                        .collect::<Vec<_>>()
                        .join(", ");
                        lines.push(format!(
                            "{title}\n      {flags} · {} · {}",
                            megabytes(b.size),
                            b.file
                        ));
                    }
                    if shelf.is_empty() {
                        lines.push("no books".into());
                    }
                    lines.join("\n")
                },
            )?;
            Ok(ExitCode::SUCCESS)
        }
        cli::KindleCommand::Remove { file, yes } => {
            if !yes {
                use std::io::{BufRead, IsTerminal, Write};
                if !std::io::stdin().is_terminal() {
                    return Err("pass --yes to remove without a terminal to confirm in".into());
                }
                print!(
                    "Remove {file} and its sidecars from {}? [y/N] ",
                    device.name
                );
                std::io::stdout().flush().ok();
                let mut answer = String::new();
                std::io::stdin()
                    .lock()
                    .read_line(&mut answer)
                    .map_err(|e| e.to_string())?;
                if !answer.trim().eq_ignore_ascii_case("y") {
                    println!("kept");
                    return Ok(ExitCode::SUCCESS);
                }
            }
            let removed = remove_on(&mut conn, &file).map_err(|e| e.to_string())?;
            if let Some(lib) = &library {
                lib.forget_send(&device.key, &file)?;
            }
            emit(json, &removed, || {
                removed
                    .iter()
                    .map(|r| format!("removed {r}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// One book `library reprocess` would update: the device file, the library source, the mode
/// it was sent in, and why.
struct Update {
    file: String,
    title: String,
    source: PathBuf,
    cde: Option<String>,
    reasons: Vec<String>,
}

fn reprocess(
    json: bool,
    lib: &deckle_library::Library,
    to: &str,
    dry_run: bool,
    yes: bool,
    pv: Option<PathBuf>,
) -> Result<ExitCode, String> {
    use deckle_core::deliver::Conn;
    let target = resolve_target(to)?;
    let device = deckle_library::device_ref(&target);
    let word_wise = deckle_core::wordwise::WordWiseOptions::default();
    let current = deckle_library::freshness::Current::now(&word_wise);
    let shelf = {
        // The listing's session ends before the first rebuild opens its own.
        let mut conn = Conn::new(&target);
        let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
        let inv =
            deckle_core::inventory::list_on(&mut conn, Some(&cache)).map_err(|e| e.to_string())?;
        deckle_library::shelf::shelve(inv.books, Some(lib), &device.key, &current)
    };
    let plan: Vec<Update> = shelf
        .into_iter()
        .filter(|s| !s.updates.is_empty())
        .filter_map(|s| {
            let book = s.library?;
            Some(Update {
                title: s.book.title.clone().unwrap_or_else(|| s.book.file.clone()),
                file: s.book.file,
                source: book.source_path(),
                cde: s.book.cde,
                reasons: s.updates.iter().map(|r| r.describe()).collect(),
            })
        })
        .collect();
    if plan.is_empty() {
        println!("every book from your library on {} is current", device.name);
        return Ok(ExitCode::SUCCESS);
    }
    for u in &plan {
        println!(
            "{}  ({}: {})",
            u.title,
            u.cde.as_deref().unwrap_or("as before"),
            u.reasons.join("; ")
        );
    }
    if dry_run {
        println!(
            "{} can be updated on {}; nothing changed",
            plan.len(),
            device.name
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !yes {
        use std::io::{BufRead, IsTerminal, Write};
        if !std::io::stdin().is_terminal() {
            return Err("pass --yes to update without a terminal to confirm in".into());
        }
        print!(
            "Rebuild and re-send {} on {}? [y/N] ",
            plan.len(),
            device.name
        );
        std::io::stdout().flush().ok();
        let mut answer = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("nothing changed");
            return Ok(ExitCode::SUCCESS);
        }
    }
    let located = previewer::locate(pv.as_deref()).map_err(|e| e.to_string())?;
    let mut reports = Vec::new();
    for u in &plan {
        let cde = match u.cde.as_deref() {
            Some("EBOK") => Some(deckle_kfx::convert::CdeType::Ebok),
            Some("PDOC") => Some(deckle_kfx::convert::CdeType::Pdoc),
            _ => None,
        };
        let opts = RunOptions {
            cde_type: cde.unwrap_or(deckle_kfx::convert::CdeType::Pdoc),
            cde_override: cde,
            amazon_match: false,
            metadata_fill: false,
            verified_real_asin: false,
            asin: None,
            pages: Some(None),
            target: target.clone(),
            previewer: Some(located.path.clone()),
            keep_work_dir: false,
            xray: deckle_core::pipeline::XRayMode::Auto,
            llm: None,
            wikipedia: false,
            word_wise: Some(word_wise.clone()),
            // This copy goes once the new one is written, whatever it's called now.
            replaces: vec![u.file.clone()],
        };
        let mut r = run(&u.source, &opts);
        if r.error.is_none()
            && let Err(e) = deckle_library::record_run(lib, &u.source, &r, &target)
        {
            r.warnings.push(deckle_library::auto_add_warning(&e));
        }
        reports.push(r);
    }
    let failed = reports.iter().filter(|r| r.error.is_some()).count();
    emit(json, &reports, || {
        plan.iter()
            .zip(&reports)
            .map(|(u, r)| match &r.error {
                Some(e) => format!("FAIL  {}\n      {e} (the old copy is still there)", u.title),
                None => format!(
                    "OK    {}{}{}",
                    u.title,
                    r.replaced
                        .iter()
                        .map(|f| format!("\n      replaced {f}"))
                        .collect::<String>(),
                    if r.warnings.is_empty() {
                        String::new()
                    } else {
                        format!("\n      warnings: {}", r.warnings.join("; "))
                    }
                ),
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

fn library(json: bool, action: cli::LibraryCommand) -> Result<ExitCode, String> {
    use deckle_library::{Library, LibraryConfig, config::default_path};
    let config = default_path();
    if let cli::LibraryCommand::Init { dir, no_auto_add } = action {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let root = dir.canonicalize().map_err(|e| e.to_string())?;
        LibraryConfig {
            root: Some(root.clone()),
            auto_add: !no_auto_add,
            ..LibraryConfig::load(&config)
        }
        .save(&config)?;
        println!("library: {}", root.display());
        if !no_auto_add {
            println!("every book `deckle run` sends is added");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let (lib, _) = Library::configured(&config)
        .ok_or("no library yet: run `deckle library init <folder>` first")?;
    match action {
        cli::LibraryCommand::Init { .. } => unreachable!("handled above"),
        cli::LibraryCommand::Add { books } => {
            let mut failed = false;
            for b in &books {
                match lib.add(b) {
                    Ok(a) => {
                        let what = match (a.new, a.copied) {
                            (true, _) => "added  ",
                            (false, true) => "updated",
                            (false, false) => "same   ",
                        };
                        println!("{what}  {}", a.book.dir.display());
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        failed = true;
                    }
                }
            }
            Ok(if failed {
                ExitCode::from(EXIT_FAILURE)
            } else {
                ExitCode::SUCCESS
            })
        }
        cli::LibraryCommand::List => {
            let books = lib.list();
            let rows: Vec<_> = books
                .iter()
                .map(|b| serde_json::json!({ "dir": b.dir, "book": b.sidecar }))
                .collect();
            emit(json, &rows, || {
                if books.is_empty() {
                    return format!("no books in {}", lib.root.display());
                }
                books
                    .iter()
                    .map(library_line)
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::LibraryCommand::Reprocess {
            to,
            dry_run,
            yes,
            previewer: pv,
        } => reprocess(json, &lib, &to, dry_run, yes, pv),
        cli::LibraryCommand::Scan => {
            let r = lib.scan();
            for p in &r.adopted {
                println!("adopted    {}", p.display());
            }
            for p in &r.refreshed {
                println!("refreshed  {}", p.display());
            }
            for p in &r.missing_source {
                println!("no source  {}", p.display());
            }
            for (p, why) in &r.problems {
                println!("problem    {}: {why}", p.display());
            }
            println!("{} books in {}", r.books, lib.root.display());
            Ok(if r.problems.is_empty() && r.missing_source.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(EXIT_FAILURE)
            })
        }
    }
}

/// `Author sort  Title  [Series #n]`, then one line per Kindle it was last sent to.
fn library_line(b: &deckle_library::Book) -> String {
    let m = &b.sidecar.metadata;
    let series = match &m.series {
        Some(s) => match s.index {
            Some(i) => format!("  [{} #{i}]", s.name),
            None => format!("  [{}]", s.name),
        },
        None => String::new(),
    };
    let sends: String = b
        .sidecar
        .sends
        .iter()
        .map(|s| {
            format!(
                "\n      {} on {}, {}",
                s.cde.as_deref().unwrap_or("sent"),
                s.device_name,
                s.at.get(..10).unwrap_or(&s.at)
            )
        })
        .collect();
    format!(
        "{}  {}{series}{sends}",
        m.author_sort.as_deref().unwrap_or("Unknown Author"),
        m.title
    )
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
    let explicit_config = config.clone();
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
            scan_inbox(&cfg);
            let report = deckle_digest::build(&cfg, date, now, &output, &deckle_digest::Http)?;
            emit(json, &report, || {
                let mut text = format!(
                    "OK    {}\n      -> {}\n{}",
                    report.title,
                    report.epub.display(),
                    section_lines(&report)
                );
                for p in &report.long_reads {
                    text.push_str(&format!("\n      long read -> {}", p.display()));
                }
                for n in &report.notes {
                    text.push_str(&format!("\n      note: {n}"));
                }
                if !report.reading_outcomes.is_empty() {
                    text.push_str(&format!(
                        "\n      preview: {} saved stories not marked (digest run marks them)",
                        report.reading_outcomes.len()
                    ));
                }
                text
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Run {
            to,
            date,
            previewer: pv,
            keep_work_dir,
        } => {
            let mut cfg = load_digest_config(&path)?;
            let date = parse_date(date.as_deref())?;
            let located = previewer::locate(pv.as_deref()).map_err(|e| e.to_string())?;
            // A folder target must exist (and is created) before any work; `device` may be
            // absent: the edition then waits in the outbox.
            let folder = (to != "device").then(|| resolve_target(&to)).transpose()?;
            let work = deckle_core::wordwise::default_cache_dir().join("digest");
            let converted = work.join("kfx");
            std::fs::create_dir_all(&converted).map_err(|e| e.to_string())?;
            let now = deckle_digest::chrono::Utc::now();
            scan_inbox(&cfg);
            let mut built = deckle_digest::build(&cfg, date, now, &work, &deckle_digest::Http)?;
            let opts = paper_options(Some(located.path.clone()), keep_work_dir, &converted);
            let (mut long_reads, failed) = prepare_long_reads(&built, &opts);
            let mut notes: Vec<String> = Vec::new();
            if !failed.is_empty() {
                for (id, why) in &failed {
                    notes.push(format!(
                        "long read {id} printed in the paper instead: {why}"
                    ));
                }
                cfg.reading.inline = failed.into_iter().map(|(id, _)| id).collect();
                built = deckle_digest::build(&cfg, date, now, &work, &deckle_digest::Http)?;
                long_reads = prepare_long_reads(&built, &opts).0;
            }
            // Email belongs to the morning run to the Kindle, not to `--to <folder>` (email
            // design §7). A paper that went out by email skips the outbox unless `usb_too`.
            let (email, to_usb) = match &folder {
                Some(_) => (None, true),
                None => email_paper(
                    &cfg.email,
                    date,
                    &built.epub,
                    &deckle_digest::secret::SystemKeyring,
                    &|k| std::env::var(k).ok(),
                    |pw| deckle_digest::mail::SmtpMailer::new(&cfg.email, pw),
                ),
            };
            let email = EmailRun {
                email: email.unwrap_or_default(),
            };
            // Converted only when it goes by USB.
            let report = to_usb.then(|| run(&built.epub, &opts));
            let kfx = report.as_ref().and_then(|r| {
                r.error
                    .is_none()
                    .then_some(r.delivered.as_deref())
                    .flatten()
            });
            let queue = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
            let mut outcome = DigestOutcome::default();
            let reading_outcomes = built.reading_outcomes.clone();
            if kfx.is_some() || !to_usb {
                let name = deckle_digest::edition_filename(date);
                match &folder {
                    Some(target) => {
                        let one = deckle_core::queue::Queue::new(work.join("to-folder"));
                        let _ = std::fs::remove_dir_all(&one.dir);
                        queue_edition(&one, kfx, &name, None, &long_reads)?;
                        outcome.flush = Some(
                            one.flush(target, Some(&digest_prune(&cfg)))
                                .map_err(|e| e.to_string())?,
                        );
                    }
                    None => {
                        queue_edition(
                            &queue,
                            kfx,
                            &name,
                            Some(deckle_digest::EDITION_PREFIX),
                            &long_reads,
                        )?;
                        outcome = push_queued(&queue, &cfg)?;
                    }
                }
                // Only a run that shipped (or queued) the edition uses up the saved articles.
                reading_queue(&cfg)
                    .commit(date, &reading_outcomes)
                    .map_err(|e| format!("reading queue: {e}"))?;
            }
            #[derive(serde::Serialize)]
            struct DigestRun<'a> {
                build: &'a deckle_digest::BuildReport,
                /// `null` when the paper went by email only.
                run: Option<&'a deckle_core::pipeline::RunReport>,
                outcome: &'a DigestOutcome,
                #[serde(flatten)]
                email: &'a EmailRun,
            }
            let all = DigestRun {
                build: &built,
                run: report.as_ref(),
                outcome: &outcome,
                email: &email,
            };
            let failed = report.as_ref().and_then(|r| r.error.as_ref());
            emit(json, &all, || {
                let head = match failed {
                    Some(e) => format!("FAIL  {}\n      {e}", built.title),
                    None => format!("OK    {}", built.title),
                };
                let mut text = format!("{head}\n{}", section_lines(&built));
                for part in [email.lines(), outcome.describe(&queue)] {
                    if !part.is_empty() {
                        text.push_str(&format!("\n{part}"));
                    }
                }
                for (n, _, _) in &long_reads {
                    text.push_str(&format!("\n      long read: {n}"));
                }
                for n in built.notes.iter().chain(&notes) {
                    text.push_str(&format!("\n      note: {n}"));
                }
                text
            })?;
            Ok(if failed.is_some() {
                ExitCode::from(EXIT_FAILURE)
            } else {
                ExitCode::SUCCESS
            })
        }
        cli::DigestCommand::Schedule { at, off } => {
            use deckle_digest::schedule;
            if off {
                let done = schedule::remove()?;
                emit(json, &done, || {
                    if done.is_empty() {
                        "nothing was scheduled".into()
                    } else {
                        done.join("\n")
                    }
                })?;
                return Ok(ExitCode::SUCCESS);
            }
            let Some(at) = at else {
                let st = schedule::status();
                emit(json, &st, || match (&st.at, st.watch) {
                    (None, false) => {
                        "not scheduled; `deckle digest schedule --at 05:30` sets it up".into()
                    }
                    (at, watch) => format!(
                        "edition: {}\nwatcher: {}{}",
                        at.as_deref()
                            .map(|t| format!("daily at {t}"))
                            .unwrap_or_else(|| "off".into()),
                        if watch { "on" } else { "off" },
                        st.exe
                            .as_deref()
                            .map(|e| format!("\nruns: {e}"))
                            .unwrap_or_default()
                    ),
                })?;
                return Ok(ExitCode::SUCCESS);
            };
            let (hour, minute) = schedule::parse_at(&at)?;
            // The jobs need the config that exists now, not one created later.
            load_digest_config(&path)?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let exe = exe.canonicalize().unwrap_or(exe);
            if let Some(w) = schedule::unstable_exe(&exe) {
                eprintln!("warning: {w}");
            }
            let s = schedule::Schedule {
                hour,
                minute,
                exe,
                config: explicit_config.map(|c| c.canonicalize().unwrap_or(c)),
            };
            let done = schedule::install(&s)?;
            emit(json, &done, || {
                format!(
                    "{}\nThe paper is built daily at {at} and goes to your Kindle when it's plugged in.",
                    done.join("\n")
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Email { action } => {
            let cfg = load_digest_config(&path)?.email;
            digest_email(json, &cfg, action)
        }
        cli::DigestCommand::Push => {
            let cfg = load_digest_config(&path)?;
            let queue = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
            let outcome = push_queued(&queue, &cfg)?;
            emit(json, &outcome, || outcome.describe(&queue))?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Watch { interval } => {
            let cfg = load_digest_config(&path)?;
            let queue = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
            eprintln!(
                "watching for a Kindle every {interval}s; outbox {}",
                queue.dir.display()
            );
            // The bookmarklet's listener (save-to-paper §4). A busy port only costs the
            // bookmarklet; watching goes on.
            let reading = reading_queue(&cfg);
            match reading
                .path
                .parent()
                .map(deckle_digest::bookmarklet::token)
                .transpose()
            {
                Ok(Some(token)) => {
                    std::thread::spawn(move || {
                        if let Err(e) = deckle_digest::bookmarklet::serve(reading, token) {
                            eprintln!(
                                "bookmarklet: can't listen on 127.0.0.1:{}: {e}",
                                deckle_digest::bookmarklet::PORT
                            );
                        }
                    });
                    eprintln!(
                        "bookmarklet: listening on 127.0.0.1:{}",
                        deckle_digest::bookmarklet::PORT
                    );
                }
                Ok(None) => {}
                Err(e) => eprintln!("bookmarklet: no token: {e}"),
            }
            if cfg.sync.any_clippings() {
                eprintln!("sync: clippings isn't built yet (#60); ignoring `clippings = true`");
            }
            // Each device's [sync] actions run once per connection: a failed delivery waits
            // for the Kindle to be reconnected (or for a new edition) instead of retrying
            // every tick.
            let mut watcher = deckle_core::sync::Watcher::default();
            loop {
                scan_inbox(&cfg);
                let pending = queue.pending();
                let kindle = detect_kindle();
                let id = kindle.as_ref().map(deckle_core::sync::device_id);
                let actions = id
                    .as_deref()
                    .map(|id| sync_actions(&cfg, id))
                    .unwrap_or_default();
                let plan = watcher.tick(id.as_deref(), &pending, actions);
                if let (Some(target), false) = (&kindle, plan.is_empty()) {
                    match sync_to(&queue, &cfg, target, plan) {
                        Ok(o) => emit(json, &o, || o.describe(&queue))?,
                        Err(e) => eprintln!("error: {e}"),
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(interval.max(1)));
            }
        }
    }
}

/// What happened to the converted edition: delivered (with pruning), or left queued.
#[derive(Debug, Default, serde::Serialize)]
struct DigestOutcome {
    flush: Option<deckle_core::queue::FlushReport>,
    /// No Kindle connected: the edition waits in the outbox.
    queued: bool,
}

impl DigestOutcome {
    fn describe(&self, queue: &deckle_core::queue::Queue) -> String {
        match &self.flush {
            Some(f) if f.delivered.is_empty() && f.pruned.is_empty() => {
                "      nothing queued".to_owned()
            }
            Some(f) => {
                let mut lines: Vec<String> = f
                    .delivered
                    .iter()
                    .map(|d| format!("      -> {}", d.display()))
                    .collect();
                if !f.pruned.is_empty() {
                    lines.push(format!("      removed {} older edition(s)", f.pruned.len()));
                }
                lines.extend(f.warnings.iter().map(|w| format!("      warning: {w}")));
                lines.join("\n")
            }
            None if self.queued => format!(
                "      queued in {}: plug in a Kindle and run `deckle digest push` (or keep \
                 `deckle digest watch` running)",
                queue.dir.display()
            ),
            None => String::new(),
        }
    }
}

/// `digest email password|test|forget` (email design §8), on the system keychain.
fn digest_email(
    json: bool,
    cfg: &deckle_digest::config::EmailConfig,
    action: cli::EmailCommand,
) -> Result<ExitCode, String> {
    use deckle_digest::secret::{self, SystemKeyring};
    use deckle_digest::zeroize::Zeroizing;
    let account = secret::account(cfg);
    match action {
        cli::EmailCommand::Password { stdin } => {
            // Nothing to file it under: say so before asking for it.
            if cfg.username.trim().is_empty() || cfg.smtp_host.trim().is_empty() {
                return Err(secret::SecretError::NoAccount.to_string());
            }
            let password = Zeroizing::new(if stdin {
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|e| format!("stdin: {e}"))?;
                let pw = line.trim_end_matches(['\r', '\n']).to_owned();
                deckle_digest::zeroize::Zeroize::zeroize(&mut line);
                pw
            } else {
                rpassword::prompt_password(format!("SMTP password for {account}: "))
                    .map_err(|e| format!("couldn't read the password: {e}"))?
            });
            secret::set(cfg, &SystemKeyring, &password).map_err(|e| e.to_string())?;
            emit(json, &account, || {
                format!(
                    "saved in the system keychain ({}, {account})",
                    secret::SERVICE
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::EmailCommand::Forget => {
            let had = secret::forget(cfg, &SystemKeyring).map_err(|e| e.to_string())?;
            emit(json, &had, || {
                if had {
                    format!("removed the keychain entry for {account}")
                } else {
                    format!("no keychain entry for {account}")
                }
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::EmailCommand::Test => {
            let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let src = deckle_digest::mail::send_test(
                cfg,
                dir.path(),
                deckle_digest::chrono::Local::now().date_naive(),
                &SystemKeyring,
                &|k| std::env::var(k).ok(),
                |pw| deckle_digest::mail::SmtpMailer::new(cfg, pw),
            )
            .map_err(|e| e.to_string())?;
            let to = cfg.to.trim();
            emit(json, &to, || {
                format!(
                    "sent a test to {to}{}. If it doesn't reach the Kindle in a few minutes, \
                     check that {} is on Amazon's Approved Personal Document E-mail List.",
                    match src {
                        secret::Source::Env => format!(" (password from {})", secret::ENV),
                        secret::Source::Keychain => String::new(),
                    },
                    cfg.from.trim()
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// What the run says about the email: nothing when `[email]` is off.
#[derive(Debug, Default, serde::Serialize)]
struct EmailRun {
    #[serde(flatten)]
    email: deckle_digest::mail::EmailReport,
}

impl EmailRun {
    fn lines(&self) -> String {
        let e = &self.email;
        match (&e.emailed, &e.email_error) {
            (Some(to), _) => format!(
                "      emailed to {to}{}",
                match e.password_from {
                    Some(deckle_digest::secret::Source::Env) => {
                        format!(" (password from {})", deckle_digest::secret::ENV)
                    }
                    _ => String::new(),
                }
            ),
            (None, Some(why)) => format!("      email failed: {why}; queued for USB instead"),
            (None, None) => String::new(),
        }
    }
}

/// Emails the paper when `[email]` is on (email design §7), and says whether it still goes to
/// USB: after a failure always, after a success only with `usb_too`.
fn email_paper<M: deckle_digest::mail::Mailer>(
    cfg: &deckle_digest::config::EmailConfig,
    date: deckle_digest::chrono::NaiveDate,
    epub: &Path,
    keys: &dyn deckle_digest::secret::Keyring,
    env: &dyn Fn(&str) -> Option<String>,
    connect: impl FnOnce(deckle_digest::zeroize::Zeroizing<String>) -> M,
) -> (Option<deckle_digest::mail::EmailReport>, bool) {
    let report = deckle_digest::mail::email_edition(cfg, date, epub, keys, env, connect);
    let to_usb = report.as_ref().is_none_or(|r| r.to_usb(cfg));
    (report, to_usb)
}

/// Puts the paper's KFX (`None`: it went by email) and the long reads in `queue`.
fn queue_edition(
    queue: &deckle_core::queue::Queue,
    paper: Option<&Path>,
    name: &str,
    replaces: Option<&str>,
    long_reads: &[LongRead],
) -> Result<(), String> {
    if let Some(kfx) = paper {
        queue.add(kfx, name, replaces).map_err(|e| e.to_string())?;
    }
    for (n, p, fallback) in long_reads {
        queue
            .add_prepared_with(p, fallback, n, None)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The paper's build: into `converted`, always PDOC, because the cover is the front page and
/// must always show (design §5).
fn paper_options(previewer: Option<PathBuf>, keep_work_dir: bool, converted: &Path) -> RunOptions {
    RunOptions {
        cde_type: deckle_kfx::convert::CdeType::Pdoc,
        cde_override: Some(deckle_kfx::convert::CdeType::Pdoc),
        amazon_match: false,
        metadata_fill: false,
        verified_real_asin: false,
        asin: None,
        pages: None,
        target: Target::Folder(converted.to_path_buf()),
        previewer,
        keep_work_dir,
        xray: deckle_core::pipeline::XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
    }
}

/// A long read to queue: its name in the outbox, the book up to the KPF, its fallback build.
type LongRead = (String, deckle_core::pipeline::Prepared, Vec<u8>);

/// The paper's long reads, prepared for the Kindle that takes them: at push they get the cover
/// rule's choice, X-Ray and Word Wise (device-aware build §4.5). One that won't convert, or
/// can't even build its fallback, comes back as `(id, why)` to print inside the paper, so the
/// paper never points to a book that isn't coming.
fn prepare_long_reads(
    built: &deckle_digest::BuildReport,
    paper: &RunOptions,
) -> (Vec<LongRead>, Vec<(String, String)>) {
    let opts = RunOptions {
        cde_override: None,
        xray: deckle_core::pipeline::XRayMode::Auto,
        word_wise: Some(Default::default()),
        ..paper.clone()
    };
    let book_ids = built
        .reading_outcomes
        .iter()
        .filter(|(_, o)| *o == deckle_digest::reading::Outcome::Book)
        .map(|(id, _)| id.clone());
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for (id, epub) in book_ids.zip(&built.long_reads) {
        let prepared = deckle_core::pipeline::prepare(epub, &opts, &mut |_| {})
            .map_err(|r| r.error.unwrap_or_else(|| "not converted".into()))
            .and_then(|p| deckle_core::pipeline::fallback_kfx(&p).map(|f| (p, f)));
        match prepared {
            Ok((p, fallback)) => {
                let stem = epub.file_stem().unwrap_or_default().to_string_lossy();
                ok.push((format!("{stem}.kfx"), p, fallback));
            }
            Err(why) => failed.push((id, why)),
        }
    }
    (ok, failed)
}

fn digest_prune(cfg: &deckle_digest::Config) -> deckle_core::queue::Prune {
    deckle_core::queue::Prune {
        prefix: deckle_digest::EDITION_PREFIX.into(),
        keep: cfg.keep.max(1),
    }
}

/// The first connected Kindle, mass storage first; `None` when there is none.
fn detect_kindle() -> Option<Target> {
    resolve_target("device").ok()
}

/// Flushes the outbox to a connected Kindle, or reports it queued.
fn push_queued(
    queue: &deckle_core::queue::Queue,
    cfg: &deckle_digest::Config,
) -> Result<DigestOutcome, String> {
    if queue.pending().is_empty() {
        return Ok(DigestOutcome {
            flush: Some(Default::default()),
            queued: false,
        });
    }
    let Some(target) = detect_kindle() else {
        return Ok(DigestOutcome {
            flush: None,
            queued: true,
        });
    };
    // An explicit push always pushes; the device's `prune` still counts.
    let plan = deckle_core::sync::Plan {
        push: true,
        prune: sync_actions(cfg, &deckle_core::sync::device_id(&target)).prune,
    };
    sync_to(queue, cfg, &target, plan)
}

/// The device's `[sync]` actions (auto-sync design §3).
fn sync_actions(cfg: &deckle_digest::Config, id: &str) -> deckle_core::sync::Actions {
    let a = cfg.sync.for_device(id);
    deckle_core::sync::Actions {
        push: a.push,
        prune: a.prune,
    }
}

/// Runs `plan` on `target` over one connection.
fn sync_to(
    queue: &deckle_core::queue::Queue,
    cfg: &deckle_digest::Config,
    target: &Target,
    plan: deckle_core::sync::Plan,
) -> Result<DigestOutcome, String> {
    // Prepared books are finished for this Kindle: its class from the connection, the owner's
    // choices for it from the app's devices.json (device-aware build §4.3).
    let registry =
        deckle_core::devices::DeviceRegistry::load(&deckle_core::devices::default_registry_file());
    let finishing = deckle_core::queue::Finishing {
        profile: deckle_core::devices::profile_for(target, &registry),
        ..Default::default()
    };
    let mut conn = deckle_core::deliver::Conn::new(target);
    let flush = deckle_core::sync::run_with(
        queue,
        &mut conn,
        plan,
        &digest_prune(cfg),
        &finishing,
        &mut |i, n, name| eprintln!("push: {i}/{n} {name}"),
    )
    .map_err(|e| e.to_string())?;
    Ok(DigestOutcome {
        flush: Some(flush),
        queued: false,
    })
}

fn reading_queue(cfg: &deckle_digest::Config) -> deckle_digest::reading::ReadingQueue {
    deckle_digest::reading::ReadingQueue::new(
        cfg.reading
            .queue_path
            .clone()
            .unwrap_or_else(deckle_digest::reading::ReadingQueue::default_path),
    )
}

/// Queues what was dropped in the Inbox folder: articles for the paper, books for the
/// outbox (design §10). Says so when it found anything.
fn scan_inbox(cfg: &deckle_digest::Config) {
    let dir = deckle_digest::inbox::expand_home(&cfg.reading.inbox);
    let outbox = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
    let r = inbox::scan(
        &dir,
        &reading_queue(cfg),
        &outbox,
        std::time::SystemTime::now(),
        &mut inbox::pipeline(),
    );
    if r.queued > 0 {
        eprintln!(
            "inbox: saved {} article(s) from {}",
            r.queued,
            dir.display()
        );
    }
    if !r.sent.is_empty() {
        eprintln!(
            "inbox: {} book(s) in the outbox, sent when a Kindle connects",
            r.sent.len()
        );
    }
    for (file, why) in &r.unreadable {
        eprintln!("inbox: couldn't read {}: {why}", file.display());
    }
    for (file, why) in &r.retry {
        eprintln!("inbox: {} waits for the next scan: {why}", file.display());
    }
}

fn save(
    json: bool,
    targets: Vec<String>,
    title: Option<String>,
    list: bool,
    remove: Option<String>,
    bookmarklet: bool,
) -> Result<ExitCode, String> {
    use deckle_digest::reading::{Added, ReadingQueue, State};
    let queue = ReadingQueue::new(ReadingQueue::default_path());
    if bookmarklet {
        let dir = queue.path.parent().unwrap_or(Path::new("."));
        let token = deckle_digest::bookmarklet::token(dir).map_err(|e| e.to_string())?;
        println!(
            "Drag this to your bookmarks bar (or make a bookmark with it as the address).\n\
             It works while `deckle digest watch` is running.\n\n{}",
            deckle_digest::bookmarklet::javascript(&token)
        );
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(r) = remove {
        return if queue.remove(&r)? {
            println!("removed {r}");
            Ok(ExitCode::SUCCESS)
        } else {
            Err(format!("nothing saved as {r}"))
        };
    }
    if list || targets.is_empty() {
        let week_ago = deckle_digest::chrono::Utc::now() - deckle_digest::chrono::Duration::days(7);
        let mut entries: Vec<_> = queue
            .load()
            .into_iter()
            .filter(|e| e.state == State::Pending || e.saved_at > week_ago)
            .collect();
        entries.sort_by_key(|e| (e.state != State::Pending, std::cmp::Reverse(e.saved_at)));
        emit(json, &entries, || {
            if entries.is_empty() {
                return "nothing saved".into();
            }
            entries
                .iter()
                .map(|e| {
                    let what = e
                        .url
                        .clone()
                        .or(e.file.as_ref().map(|f| f.display().to_string()))
                        .unwrap_or_default();
                    let state = format!("{:?}", e.state).to_lowercase();
                    let extra = match (&e.printed_in, &e.note) {
                        (Some(d), _) if e.state != State::Pending => format!("  ({d})"),
                        (_, Some(n)) => format!("  (tried {}x: {n})", e.tries),
                        _ => String::new(),
                    };
                    format!("{}  {state:<8} {what}{extra}", e.id)
                })
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        return Ok(ExitCode::SUCCESS);
    }
    let now = deckle_digest::chrono::Utc::now();
    let mut failed = false;
    for t in &targets {
        let p = Path::new(t);
        let added = if p.is_file() {
            queue.add_page(p, "cli", now)
        } else if t.starts_with("http://") || t.starts_with("https://") {
            queue.add_url(t, title.as_deref(), "cli", now)
        } else {
            Err("not a web address or a saved .html page".into())
        };
        match added {
            Ok(Added::New(id)) => println!("saved {id}  {t}"),
            Ok(Added::Duplicate(id)) => println!("already saved {id}  {t}"),
            Err(e) => {
                eprintln!("error: {t}: {e}");
                failed = true;
            }
        }
    }
    Ok(if failed {
        ExitCode::from(EXIT_FAILURE)
    } else {
        ExitCode::SUCCESS
    })
}

// Unix only: the fake Previewer is a shell script.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use deckle_core::pipeline::XRayMode;
    use deckle_digest::reading::Outcome;

    pub(crate) fn fake_previewer() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../deckle-core/tests/fake_previewer/ok.sh")
    }

    /// Long reads are queued prepared, asking for X-Ray and Word Wise (device-aware build
    /// §4.5); one that won't convert comes back to be printed in the paper.
    #[test]
    fn long_reads_are_prepared_with_xray_and_word_wise_and_a_failure_prints_inline() {
        let tmp = tempfile::tempdir().unwrap();
        let mini = tmp.path().join("deckle-reads-2026-10-21-mini.epub");
        std::fs::copy(deckle_kfx::fixtures::path("deckle-mini.epub"), &mini).unwrap();
        let broken = tmp.path().join("deckle-reads-2026-10-21-broken.epub");
        std::fs::write(&broken, b"not an epub").unwrap();
        let built = deckle_digest::BuildReport {
            epub: tmp.path().join("paper.epub"),
            title: "Paper".into(),
            sections: Vec::new(),
            long_reads: vec![mini, broken],
            reading_outcomes: vec![
                ("a1".into(), Outcome::Printed),
                ("b2".into(), Outcome::Book),
                ("c3".into(), Outcome::Book),
            ],
            notes: Vec::new(),
        };
        let opts = paper_options(Some(fake_previewer()), false, tmp.path());
        let (ok, failed) = prepare_long_reads(&built, &opts);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, "c3");
        assert!(failed[0].1.starts_with("inspect:"), "{failed:?}");
        let [(name, p, fallback)] = ok.as_slice() else {
            panic!("one long read: {}", ok.len());
        };
        assert_eq!(name, "deckle-reads-2026-10-21-mini.kfx");
        assert_eq!(p.wishes.cde_override, None, "the cover rule decides");
        assert_eq!(p.wishes.xray, XRayMode::Auto);
        assert_eq!(p.wishes.word_wise, Some(5));
        assert!(fallback.starts_with(b"CONT"));

        let q = deckle_core::queue::Queue::new(tmp.path().join("outbox"));
        let entry = q.add_prepared_with(p, fallback, name, None).unwrap();
        assert_eq!(
            std::fs::read(entry.join("fallback.kfx")).unwrap(),
            *fallback
        );
    }
}

/// `digest run` with Send-to-Kindle email (email design §7), on fakes: no keychain, no SMTP.
#[cfg(test)]
mod email_tests {
    use super::*;
    use deckle_digest::config::EmailConfig;
    use deckle_digest::mail::{MAX_BYTES, MailError, Mailer, Outgoing};
    use deckle_digest::secret::{Keyring, MemoryKeyring};
    use std::cell::RefCell;

    const PW: &str = "hunter2-Secret!";

    #[derive(Default)]
    struct Recording {
        sent: RefCell<Vec<Outgoing>>,
        fail: Option<MailError>,
    }

    impl Mailer for &Recording {
        fn send(&self, msg: &Outgoing) -> Result<(), MailError> {
            self.sent.borrow_mut().push(msg.clone());
            self.fail.clone().map_or(Ok(()), Err)
        }
    }

    fn cfg(usb_too: bool) -> EmailConfig {
        EmailConfig {
            enabled: true,
            to: "me_1@kindle.com".into(),
            from: "me@example.com".into(),
            smtp_host: "smtp.example.com".into(),
            smtp_port: 465,
            username: "me@example.com".into(),
            usb_too,
        }
    }

    fn keys() -> MemoryKeyring {
        let k = MemoryKeyring::default();
        k.set(&deckle_digest::secret::account(&cfg(false)), PW)
            .unwrap();
        k
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn date() -> deckle_digest::chrono::NaiveDate {
        deckle_digest::chrono::NaiveDate::from_ymd_opt(2026, 10, 21).unwrap()
    }

    /// A built paper, its KFX and an empty outbox.
    struct Run {
        _tmp: tempfile::TempDir,
        epub: PathBuf,
        kfx: PathBuf,
        queue: deckle_core::queue::Queue,
    }

    fn built() -> Run {
        let tmp = tempfile::tempdir().unwrap();
        let epub = tmp.path().join("deckle-daily-2026-10-21.epub");
        std::fs::write(&epub, b"PK paper").unwrap();
        let kfx = tmp.path().join("paper.kfx");
        std::fs::write(&kfx, b"CONT paper").unwrap();
        let queue = deckle_core::queue::Queue::new(tmp.path().join("outbox"));
        Run {
            _tmp: tmp,
            epub,
            kfx,
            queue,
        }
    }

    /// The run's delivery step: email, then the outbox for what still goes by USB.
    fn deliver(run: &Run, cfg: &EmailConfig, keys: &dyn Keyring, rec: &Recording) -> EmailRun {
        let (email, to_usb) = email_paper(cfg, date(), &run.epub, keys, &no_env, |_| rec);
        queue_edition(
            &run.queue,
            to_usb.then_some(run.kfx.as_path()),
            &deckle_digest::edition_filename(date()),
            Some(deckle_digest::EDITION_PREFIX),
            &[],
        )
        .unwrap();
        EmailRun {
            email: email.unwrap_or_default(),
        }
    }

    #[test]
    fn an_emailed_paper_skips_the_outbox_unless_usb_too() {
        for usb_too in [false, true] {
            let run = built();
            let rec = Recording::default();
            let r = deliver(&run, &cfg(usb_too), &keys(), &rec);
            assert_eq!(r.email.emailed.as_deref(), Some("me_1@kindle.com"));
            assert_eq!(r.email.email_error, None);
            let sent = rec.sent.borrow();
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].attachment.0, "deckle-daily-2026-10-21.epub");
            assert_eq!(sent[0].attachment.1, b"PK paper");
            assert_eq!(
                sent[0].subject,
                "Deckle Daily — Wednesday, October 21, 2026"
            );
            assert_eq!(
                run.queue.pending().len(),
                usize::from(usb_too),
                "usb_too = {usb_too}"
            );
        }
    }

    #[test]
    fn every_failed_email_queues_the_edition_and_says_why() {
        let with_pw = keys();
        let without = MemoryKeyring::default();
        let cases: [(&str, Option<MailError>, bool, &dyn Keyring); 4] = [
            (
                "refused the login",
                Some(MailError::Auth("535 5.7.8 not accepted".into())),
                false,
                &with_pw,
            ),
            (
                "couldn't reach",
                Some(MailError::Network("connection refused".into())),
                false,
                &with_pw,
            ),
            ("45 MB", None, true, &with_pw),
            ("deckle digest email password", None, false, &without),
        ];
        for (why, fail, too_big, keys) in cases {
            let run = built();
            if too_big {
                std::fs::File::options()
                    .write(true)
                    .open(&run.epub)
                    .unwrap()
                    .set_len(MAX_BYTES + 1)
                    .unwrap();
            }
            let rec = Recording {
                fail,
                ..Default::default()
            };
            let r = deliver(&run, &cfg(false), keys, &rec);
            assert_eq!(r.email.emailed, None, "{why}");
            let e = r.email.email_error.as_deref().unwrap_or_default();
            assert!(e.contains(why), "{why:?} not in {e:?}");
            assert_eq!(run.queue.pending().len(), 1, "{why}: queued for USB");
            let lines = r.lines();
            assert!(lines.contains("queued for USB"), "{lines}");
        }
    }

    #[test]
    fn email_off_queues_as_before() {
        let run = built();
        let rec = Recording::default();
        let off = EmailConfig {
            enabled: false,
            ..cfg(false)
        };
        let r = deliver(&run, &off, &keys(), &rec);
        assert_eq!(r.email, Default::default());
        assert!(rec.sent.borrow().is_empty());
        assert_eq!(run.queue.pending().len(), 1);
        assert_eq!(r.lines(), "");
        assert_eq!(serde_json::to_string(&r).unwrap(), "{}");
    }

    #[test]
    fn the_run_report_never_shows_the_password() {
        let mut shown = Vec::new();
        for fail in [None, Some(MailError::Auth("535 no".into()))] {
            let run = built();
            let rec = Recording {
                fail,
                ..Default::default()
            };
            let r = deliver(&run, &cfg(false), &keys(), &rec);
            shown.push(format!(
                "{}\n{}\n{:?}",
                r.lines(),
                serde_json::to_string(&r).unwrap(),
                r.email
            ));
        }
        assert!(
            shown[0].contains("emailed to me_1@kindle.com"),
            "{}",
            shown[0]
        );
        for s in shown {
            assert!(!s.contains(PW), "{s}");
        }
    }
}
