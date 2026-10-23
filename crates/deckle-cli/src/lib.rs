// SPDX-License-Identifier: GPL-3.0-or-later
//! The `deckle` command line, as a library so the desktop app can run it too.

mod cli;
mod highlights;
mod inbox;
mod series;

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

/// `Notes: 84 pop-ups (12 already marked, 72 detected), 3 one-way links left` (#31 design §4).
fn notes_line(n: &deckle_core::notes::NotesReport) -> String {
    let left = match n.skipped_one_way {
        0 => String::new(),
        1 => ", 1 one-way link left".to_owned(),
        k => format!(", {k} one-way links left"),
    };
    format!(
        "\n      Notes: {} pop-ups ({} already marked, {} detected){left}",
        n.semantic + n.heuristic,
        n.semantic,
        n.heuristic
    )
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
            // A Word document, text file or PDF reports the EPUB it imports as.
            let info = if deckle_library::is_source(&epub)
                && !epub
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
            {
                deckle_library::book_info(&epub)?
            } else {
                inspect(&epub).map_err(|e| e.to_string())?
            };
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
        cli::Command::Highlights { from, action } => highlights_cmd(json, &from, action),
        cli::Command::Words { from, action } => words_cmd(json, &from, action),
        cli::Command::Bind {
            inputs,
            title,
            author,
            out,
        } => bind_cmd(json, &inputs, title, author, out, &deckle_digest::Http),
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
            typography,
            no_generated_cover,
            cover_style,
            popup_notes,
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
                generated_cover: (!no_generated_cover)
                    .then(|| cover_style.as_deref().map(str::parse).transpose())
                    .transpose()?,
                metadata_fill: fill_metadata,
                typography,
                popup_notes,
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
                            "OK    {}\n      -> {}{}{}{}{}{}{}{}{}{}",
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
                            r.normalize
                                .cover_generated
                                .as_ref()
                                .map(|c| format!("\n      Cover: generated ({c})"))
                                .unwrap_or_default(),
                            r.normalize
                                .typography
                                .as_ref()
                                .map(|t| format!(
                                    "\n      Typography: {} quotes, {} dashes, {} ellipses, {} empty paragraphs",
                                    t.quotes, t.dashes, t.ellipses, t.empty_paragraphs
                                ))
                                .unwrap_or_default(),
                            r.normalize
                                .notes
                                .as_ref()
                                .map(notes_line)
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

/// `deckle highlights list|export` (highlights export design §6).
/// `deckle bind`: each input made an EPUB (imported, or fetched for a web article), then bound
/// in order (batch-bind design §4).
fn bind_cmd(
    json: bool,
    inputs: &[PathBuf],
    title: String,
    author: Option<String>,
    out: Option<PathBuf>,
    net: &dyn deckle_digest::Net,
) -> Result<ExitCode, String> {
    use deckle_core::bind::{BindOptions, Part, bind};
    if inputs.len() < 2 {
        return Err("bind takes at least two files".into());
    }
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut parts = Vec::new();
    let mut warnings = Vec::new();
    for (n, input) in inputs.iter().enumerate() {
        let arg = input.to_string_lossy();
        let shown = input.display().to_string();
        if deckle_digest::web_book::is_article(&arg) {
            let book =
                import_article(&arg, net, &articles_dir()).map_err(|e| format!("{shown}: {e}"))?;
            parts.push(Part {
                epub: book.path,
                title: None,
            });
            continue;
        }
        if input
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("kfx"))
        {
            return Err(format!(
                "{shown}: a finished Kindle book can't be bound; use its EPUB or source"
            ));
        }
        if !input.is_file() {
            return Err(format!("{shown}: no such file"));
        }
        let epub = match deckle_core::import::import(input, work.path(), &format!("part-{n}.epub"))
        {
            Some(Ok(imported)) => {
                for w in imported.warnings {
                    if !warnings.contains(&w) {
                        warnings.push(w);
                    }
                }
                imported.epub
            }
            Some(Err(e)) => return Err(format!("{shown}: {e}")),
            None => input.clone(),
        };
        parts.push(Part { epub, title: None });
    }
    let out = out.unwrap_or_else(|| {
        PathBuf::from(format!("{}.epub", deckle_core::deliver::sanitize(&title)))
    });
    let mut report =
        bind(&parts, &BindOptions { title, author }, &out).map_err(|e| e.to_string())?;
    warnings.append(&mut report.warnings);
    report.warnings = warnings;
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    emit(
        json,
        &serde_json::json!({
            "file": out,
            "title": report.title,
            "author": report.author,
            "parts": report.parts,
            "warnings": report.warnings,
        }),
        || {
            let mut lines: Vec<String> = report
                .parts
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{:>3}. {p}", i + 1))
                .collect();
            let shown = out.display().to_string();
            let arg = if shown.contains(char::is_whitespace) {
                format!("\"{shown}\"")
            } else {
                shown.clone()
            };
            lines.push(format!("-> {shown}  (send it with `deckle run {arg}`)"));
            lines.join("\n")
        },
    )?;
    Ok(ExitCode::SUCCESS)
}

/// `deckle words`: reads `vocab.db` over one connection (one MTP session), never writing to the
/// Kindle, and defines words only from a Word Wise database already on this machine.
fn words_cmd(json: bool, from: &str, action: cli::WordsCommand) -> Result<ExitCode, String> {
    let target = resolve_target(from)?;
    let read = match &target {
        // A copy of a Kindle's drive (a backup, say) has the same layout.
        Target::Folder(dir) => {
            let at = dir.join("system/vocabulary");
            Ok(std::fs::read(at.join("vocab.db"))
                .ok()
                .map(|db| deckle_core::vocab::VocabFiles {
                    db,
                    wal: std::fs::read(at.join("vocab.db-wal")).ok(),
                }))
        }
        _ => deckle_core::vocab::read_vocab_on(&mut deckle_core::deliver::Conn::new(&target)),
    };
    let files = match read {
        Ok(Some(f)) => f,
        Ok(None) => {
            eprintln!("error: this Kindle has no Vocabulary Builder (system/vocabulary/vocab.db)");
            return Ok(ExitCode::from(1));
        }
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(ExitCode::from(1));
        }
    };
    match action {
        cli::WordsCommand::Anki { to } => {
            let define = deckle_shelf::words::wordwise_definer();
            let n = deckle_shelf::anki::export(&files, &to, &*define)?;
            emit(json, &serde_json::json!({ "words": n, "file": to }), || {
                format!("{n} words  -> {}", to.display())
            })?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn highlights_cmd(
    json: bool,
    from: &str,
    action: cli::HighlightsCommand,
) -> Result<ExitCode, String> {
    let target = resolve_target(from)?;
    let read = deckle_core::clippings::read_on(&mut deckle_core::deliver::Conn::new(&target));
    let bytes = match read {
        Ok(Some(b)) => b,
        Ok(None) => {
            emit(
                json,
                &serde_json::json!({ "written": [], "failed": [] }),
                || "no highlights on this Kindle yet".into(),
            )?;
            return Ok(ExitCode::SUCCESS);
        }
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(ExitCode::from(1));
        }
    };
    let state_path = highlights::state_path();
    match action {
        cli::HighlightsCommand::List => {
            let parsed = deckle_highlights::parse(&bytes);
            if let Some(e) = &parsed.error {
                eprintln!("warning: My Clippings.txt line {}: {}", e.line, e.message);
            }
            let state = deckle_highlights::State::load(&state_path);
            let books = deckle_highlights::group(parsed.entries);
            let rows: Vec<_> = deckle_highlights::pending(&books, state.folder.as_deref(), &state)
                .into_iter()
                .map(|p| {
                    serde_json::json!({
                        "title": p.title, "author": p.author, "file": p.file,
                        "total": p.total, "new": p.new,
                    })
                })
                .collect();
            emit(json, &rows, || {
                rows.iter()
                    .map(|r| {
                        format!(
                            "{:>4} ({} new)  {}  -> {}",
                            r["total"],
                            r["new"],
                            r["title"].as_str().unwrap_or_default(),
                            r["file"].as_str().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::HighlightsCommand::Export { to } => {
            match highlights::export(&bytes, &state_path, to.as_deref()) {
                Ok(o) => {
                    for (_, e) in &o.report.failed {
                        eprintln!("error: {e}");
                    }
                    if let Some(e) = &o.parse_error {
                        eprintln!("warning: My Clippings.txt line {}: {}", e.line, e.message);
                    }
                    emit(json, &o, || {
                        o.report
                            .written
                            .iter()
                            .map(|w| format!("{} new  {}", w.new, w.file))
                            .chain(std::iter::once(
                                o.describe().lines().next().unwrap_or_default().to_owned(),
                            ))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })?;
                    Ok(ExitCode::SUCCESS)
                }
                Err(e) => {
                    eprintln!("{e}");
                    Ok(ExitCode::from(2))
                }
            }
        }
    }
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
            generated_cover: Some(None),
            metadata_fill: false,
            typography: false,
            popup_notes: false,
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
        cli::LibraryCommand::List { series: true } => series::list(json, &lib),
        cli::LibraryCommand::Series { name, missing } => {
            series::series(json, &lib, name.as_deref(), missing)
        }
        cli::LibraryCommand::List { series: false } => {
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

/// `next.json`: the headlines fetched for tomorrow's front page (disposable).
fn next_cache() -> PathBuf {
    deckle_core::wordwise::default_cache_dir()
        .join("digest")
        .join("next.json")
}

/// Tomorrow's paper as of `now`, from local files.
fn next_paper(
    cfg: &deckle_digest::Config,
    now: deckle_digest::chrono::DateTime<deckle_digest::chrono_tz::Tz>,
) -> deckle_digest::next::plan::NextPaper {
    deckle_readership::sizes::next_paper(
        cfg,
        now,
        schedule_at(),
        &deckle_readership::state_path(),
        &next_cache(),
        &deckle_core::wordwise::default_cache_dir().join("digest"),
    )
}

fn next_text(
    p: &deckle_digest::next::plan::NextPaper,
    now: deckle_digest::chrono::DateTime<deckle_digest::chrono::Utc>,
) -> String {
    use deckle_digest::next::WeekState;
    let mut lines = vec![
        deckle_digest::press::bar_line(&p.press, now),
        format!(
            "{} · No. {} · {}",
            p.paper,
            p.number,
            deckle_digest::long_date(p.date)
        ),
    ];
    if p.weather {
        lines.push("Weather · fills in at press time".into());
    }
    if let Some(l) = &p.lead {
        lines.push(format!(
            "Lead: {} — {}",
            l.kicker,
            l.title.as_deref().unwrap_or_default()
        ));
        if let Some(r) = &l.reason {
            lines.push(format!("      {r}"));
        }
    }
    for t in &p.tiles {
        lines.push(match &t.title {
            Some(title) => format!("  {} — {title}", t.kicker),
            None => format!("  {}", t.kicker),
        });
    }
    lines.push(match p.checked {
        Some(c) => format!(
            "Headlines checked {}",
            c.with_timezone(&deckle_digest::tz::local_tz())
                .format("%H:%M")
        ),
        None => "Headlines not checked yet".into(),
    });
    lines.push(String::new());
    lines.push(format!("Page plan (about {} Kindle pages):", p.pages));
    for (n, r) in p.plan.iter().enumerate() {
        lines.push(format!(
            "  {}. {} — {} [{}]",
            n + 1,
            r.name,
            r.detail,
            r.budget
        ));
    }
    if !p.off.is_empty() {
        let off: Vec<&str> = p.off.iter().map(|r| r.name.as_str()).collect();
        lines.push(format!("  Off: {}", off.join(", ")));
    }
    let week: Vec<String> = p
        .week
        .iter()
        .map(|d| {
            let s = match d.state {
                WeekState::Read(n) => format!("read {n}%"),
                WeekState::NotOpened => "not opened".into(),
                WeekState::Sent => "sent".into(),
                WeekState::NoPaper => "no paper".into(),
            };
            format!("{} {s}", d.date.format("%a"))
        })
        .collect();
    lines.push(String::new());
    lines.push(format!("This week: {}", week.join(" · ")));
    lines.join("\n")
}

/// The installed edition job's time, when there is one.
fn schedule_at() -> Option<(u8, u8)> {
    deckle_digest::schedule::status()
        .at
        .and_then(|a| deckle_digest::schedule::parse_at(&a).ok())
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
            let (mut report, gathered) = deckle_digest::build_gathered(
                &cfg,
                date,
                now,
                &output,
                &deckle_digest::Http,
                &gather_options(),
            )?;
            report
                .notes
                .extend(archive_to_morgue(&cfg, date, &report.epub, &gathered));
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
            force,
        } => {
            let mut cfg = load_digest_config(&path)?;
            let date = parse_date(date.as_deref())?;
            // Before anything is located or touched: a stopped morning builds nothing.
            let today = deckle_digest::chrono::Local::now().date_naive();
            let stop = deckle_digest::press::read_stop(&cfg.press_state(), today);
            if !force && stop == Some(date) {
                let out = serde_json::json!({ "stopped": date });
                emit(json, &out, || {
                    format!(
                        "Presses stopped for {}: no paper today (deckle digest stop --undo \
                         restarts them)",
                        date.format("%A, %B %-d")
                    )
                })?;
                return Ok(ExitCode::SUCCESS);
            }
            let located = previewer::locate(pv.as_deref()).map_err(|e| e.to_string())?;
            // A folder target must exist (and is created) before any work; `device` may be
            // absent: the edition then waits in the outbox.
            let folder = (to != "device").then(|| resolve_target(&to)).transpose()?;
            let work = deckle_core::wordwise::default_cache_dir().join("digest");
            let converted = work.join("kfx");
            std::fs::create_dir_all(&converted).map_err(|e| e.to_string())?;
            let now = deckle_digest::chrono::Utc::now();
            scan_inbox(&cfg);
            let (mut built, mut gathered) = deckle_digest::build_gathered(
                &cfg,
                date,
                now,
                &work,
                &deckle_digest::Http,
                &gather_options(),
            )?;
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
                (built, gathered) = deckle_digest::build_gathered(
                    &cfg,
                    date,
                    now,
                    &work,
                    &deckle_digest::Http,
                    &gather_options(),
                )?;
                long_reads = prepare_long_reads(&built, &opts).0;
            }
            // Keep the paper in the Morgue whatever happens to its conversion.
            let note = archive_to_morgue(&cfg, date, &built.epub, &gathered);
            built.notes.extend(note);
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
            if let Some(k) = kfx {
                record_layout(k, date, &built);
                if is_today(date) {
                    let note = keep_kfx_in_morgue(&cfg, date, k);
                    built.notes.extend(note);
                }
            }
            if kfx.is_some() || !to_usb {
                let name = deckle_digest::edition_filename(date);
                match &folder {
                    Some(target) => {
                        let one = deckle_core::queue::Queue::new(work.join("to-folder"));
                        let _ = std::fs::remove_dir_all(&one.dir);
                        queue_edition(&one, kfx, &name, None, &long_reads, &[])?;
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
                            // an edition sent again from the Morgue outlives the morning's paper
                            &morgue_exempt(),
                        )?;
                        outcome = push_queued(&queue, &cfg, detect_kindle())?;
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
        cli::DigestCommand::Readership { action } => digest_readership(json, &path, action),
        cli::DigestCommand::Next { fetch } => {
            let cfg = load_digest_config(&path)?;
            let now =
                deckle_digest::chrono::Utc::now().with_timezone(&deckle_digest::tz::local_tz());
            if fetch {
                let path = next_cache();
                let mut cache = deckle_digest::next::live::LiveCache::load(&path);
                deckle_digest::next::live::refresh(
                    &cfg,
                    &mut cache,
                    now.to_utc(),
                    &deckle_digest::Http,
                );
                cache.save(&path)?;
            }
            let paper = next_paper(&cfg, now);
            emit(json, &paper, || next_text(&paper, now.to_utc()))?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Stop { undo, date } => {
            let cfg = load_digest_config(&path)?;
            let stop = if undo {
                None
            } else {
                let date = match date {
                    Some(d) => parse_date(Some(&d))?,
                    None => {
                        let now = deckle_digest::chrono::Utc::now()
                            .with_timezone(&deckle_digest::tz::local_tz());
                        deckle_digest::press::press(schedule_at(), None, now)
                            .next
                            .map(|n| n.date_naive())
                            .ok_or("nothing is scheduled; there's no press to stop")?
                    }
                };
                Some(date)
            };
            deckle_digest::press::write_stop(&cfg.press_state(), stop)?;
            let out = serde_json::json!({ "stopped": stop });
            emit(json, &out, || match stop {
                Some(d) => format!("Presses stopped for {}.", deckle_digest::long_date(d)),
                None => "Presses restarted.".to_owned(),
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Puzzle => digest_puzzle(json, &path),
        cli::DigestCommand::Shelf { forget } => digest_shelf(json, &path, forget),
        cli::DigestCommand::Morgue { action } => digest_morgue(json, &path, action),
        cli::DigestCommand::Email { action } => {
            let cfg = load_digest_config(&path)?.email;
            digest_email(json, &cfg, action)
        }
        cli::DigestCommand::Push { to } => {
            let cfg = load_digest_config(&path)?;
            let queue = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
            let target = match to.as_str() {
                "device" => detect_kindle(),
                folder => Some(resolve_target(folder)?),
            };
            let outcome = push_queued(&queue, &cfg, target)?;
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
    /// Highlights exported in the same session (`[sync] clippings`).
    #[serde(skip_serializing_if = "Option::is_none")]
    highlights: Option<highlights::Outcome>,
    /// Reading positions recorded in the same session (`[sync] readership`).
    #[serde(skip_serializing_if = "Option::is_none")]
    readership: Option<deckle_readership::Recorded>,
    /// The Kindle's shelf recorded in the same session (`[sync] shelf`).
    #[serde(skip)]
    shelf: Option<deckle_shelf::Summary>,
}

impl DigestOutcome {
    fn describe(&self, queue: &deckle_core::queue::Queue) -> String {
        match &self.flush {
            Some(f)
                if f.delivered.is_empty()
                    && f.pruned.is_empty()
                    && self.highlights.is_none()
                    && self.readership.is_none()
                    && self.shelf.is_none()
                    && f.shelf_error.is_none()
                    && f.positions_error.is_none() =>
            {
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
                if let Some(h) = &self.highlights {
                    lines.extend(h.describe().lines().map(|l| format!("      {l}")));
                }
                if let Some(e) = &f.clippings_error {
                    lines.push(format!("      highlights: error: {e}"));
                }
                if let Some(r) = &self.readership {
                    let s = if r.opened == 1 { "" } else { "s" };
                    lines.push(format!("      readership: {} edition{s} read", r.opened));
                    lines.extend(
                        r.errors
                            .iter()
                            .map(|(stem, e)| format!("      readership: {stem}: {e}")),
                    );
                }
                if let Some(e) = &f.positions_error {
                    lines.push(format!("      readership: {e}"));
                }
                if let Some(s) = &self.shelf {
                    lines.push(format!("      {}", s.describe()));
                }
                if let Some(e) = &f.shelf_error {
                    lines.push(format!("      shelf: {e}"));
                }
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
    keep: &[String],
) -> Result<(), String> {
    if let Some(kfx) = paper {
        queue
            .add_keeping(kfx, name, replaces, keep)
            .map_err(|e| e.to_string())?;
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
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
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
        exempt: morgue_exempt(),
    }
}

/// How long an edition sent again from the Morgue stays on the Kindle past `keep`.
const RESENT_DAYS: i64 = 7;

/// The editions prune must leave alone: sent again and still waiting, or delivered within the
/// last week.
fn morgue_exempt() -> Vec<String> {
    use deckle_digest::chrono::{DateTime, Utc};
    let state = deckle_morgue::MorgueState::load(&deckle_morgue::state::state_path());
    let now = Utc::now();
    let outbox = deckle_core::queue::Queue::default_dir();
    state
        .resent
        .iter()
        .filter(|(date, r)| match &r.delivered {
            // Waiting to go: exempt while it is still in the outbox. One that was superseded or
            // deleted there is not waiting for anything, and must not stay exempt for ever.
            None => outbox
                .join(deckle_digest::edition_filename(**date))
                .exists(),
            Some(t) => DateTime::parse_from_rfc3339(t)
                .is_ok_and(|t| (now - t.with_timezone(&Utc)).num_days() < RESENT_DAYS),
        })
        .map(|(date, _)| deckle_digest::edition_filename(*date))
        .collect()
}

/// What a connection taught the Morgue: sent-again editions that arrived, passages highlighted
/// per edition (from `My Clippings.txt`, when it was read) and which editions are on the
/// Kindle (from the readership read). Problems are logged; they never fail the sync.
fn record_morgue(flush: &deckle_core::queue::FlushReport) {
    use deckle_core::yjf::OnDevice;
    let path = deckle_morgue::state::state_path();
    let mut state = deckle_morgue::MorgueState::load(&path);
    let before = state.clone();
    let now = deckle_digest::chrono::Utc::now().to_rfc3339();
    for delivered in &flush.delivered {
        let Some(name) = delivered.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        for (date, resent) in state.resent.iter_mut() {
            if resent.delivered.is_none() && name == deckle_digest::edition_filename(*date) {
                resent.delivered = Some(now.clone());
            }
        }
    }
    if let Some(bytes) = &flush.clippings {
        let mut counts = std::collections::BTreeMap::new();
        for book in deckle_highlights::group(deckle_highlights::parse(bytes).entries) {
            if !book.editions {
                continue;
            }
            for item in &book.items {
                let deckle_highlights::Item::Highlight { entry, .. } = item else {
                    continue;
                };
                if let Some(date) = deckle_highlights::edition_date(entry).and_then(|d| {
                    deckle_digest::chrono::NaiveDate::parse_from_str(d, "%A, %B %d, %Y").ok()
                }) {
                    *counts.entry(date).or_insert(0usize) += 1;
                }
            }
        }
        state.highlights = counts;
    }
    for (stem, on) in &flush.positions {
        state
            .on_kindle
            .insert(stem.clone(), !matches!(on, OnDevice::Absent));
    }
    // The positions were read before this session pushed and pruned: what it removed is gone,
    // and what it delivered is there.
    let stem_of = |name: &str| name.strip_suffix(".kfx").unwrap_or(name).to_owned();
    for name in &flush.pruned {
        state.on_kindle.insert(stem_of(name), false);
    }
    for delivered in &flush.delivered {
        if let Some(name) = delivered.file_name().and_then(|n| n.to_str())
            && name.starts_with(deckle_digest::EDITION_PREFIX)
        {
            state.on_kindle.insert(stem_of(name), true);
        }
    }
    if state != before
        && let Err(e) = state.save(&path)
    {
        eprintln!("morgue: {}: {e}", path.display());
    }
}

/// The first connected Kindle, mass storage first; `None` when there is none.
fn detect_kindle() -> Option<Target> {
    resolve_target("device").ok()
}

/// Flushes the outbox to `target` (a connected Kindle, or a folder), or reports it queued.
fn push_queued(
    queue: &deckle_core::queue::Queue,
    cfg: &deckle_digest::Config,
    target: Option<Target>,
) -> Result<DigestOutcome, String> {
    let empty = queue.pending().is_empty();
    let Some(target) = target else {
        return Ok(DigestOutcome {
            flush: empty.then(Default::default),
            queued: !empty,
            ..Default::default()
        });
    };
    // An explicit push always pushes; the device's `prune`, `clippings` and `readership` count.
    let actions = sync_actions(cfg, &deckle_core::sync::device_id(&target));
    let Some(plan) = push_plan(actions, empty) else {
        return Ok(DigestOutcome {
            flush: Some(Default::default()),
            ..Default::default()
        });
    };
    sync_to(queue, cfg, &target, plan)
}

/// What `digest push` runs: the outbox with the device's actions, or (empty outbox) just the
/// reads (positions, clippings) that are on; `None` leaves the Kindle alone.
fn push_plan(
    actions: deckle_core::sync::Actions,
    outbox_empty: bool,
) -> Option<deckle_core::sync::Plan> {
    if !outbox_empty {
        return Some(deckle_core::sync::Plan {
            push: true,
            prune: actions.prune,
            clippings: actions.clippings,
            readership: actions.readership,
            shelf: actions.shelf,
        });
    }
    (actions.readership || actions.clippings || actions.shelf).then_some(deckle_core::sync::Plan {
        push: false,
        prune: false,
        clippings: actions.clippings,
        readership: actions.readership,
        shelf: actions.shelf,
    })
}

/// The device's `[sync]` actions (auto-sync design §3). The clippings are read whenever the
/// paper has a puzzle (its answers are notes, puzzle design §5) or a shelf (its highlights).
fn sync_actions(cfg: &deckle_digest::Config, id: &str) -> deckle_core::sync::Actions {
    let a = cfg.sync.for_device(id);
    let shelf = shelf_on(cfg, id);
    deckle_core::sync::Actions {
        push: a.push,
        prune: a.prune,
        clippings: a.clippings || puzzle_on(cfg) || shelf,
        readership: a.readership,
        shelf,
    }
}

/// The paper has a shelf page and this Kindle isn't left out of it.
fn shelf_on(cfg: &deckle_digest::Config, id: &str) -> bool {
    cfg.sections.contains(&deckle_digest::SectionKind::Shelf) && cfg.sync.for_device(id).shelf
}

/// What `gather` can't know: whether `deckle-shelf` trusts the Kindle's own progress fraction.
fn gather_options() -> deckle_digest::GatherOptions {
    deckle_digest::GatherOptions {
        shelf_fraction_ok: deckle_shelf::KINDLE_FRACTION_OK,
    }
}

/// The paper has a puzzle page.
fn puzzle_on(cfg: &deckle_digest::Config) -> bool {
    cfg.sections.contains(&deckle_digest::SectionKind::Puzzle)
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
    let readership_path = deckle_readership::state_path();
    let mut readership = deckle_readership::State::load(&readership_path);
    let stems = match plan.readership {
        true => readership.stems(),
        false => Vec::new(),
    };
    let mut conn = deckle_core::deliver::Conn::new(target);
    let flush = deckle_core::sync::run_with(
        queue,
        &mut conn,
        plan,
        &digest_prune(cfg),
        &finishing,
        &mut |i, n, name| eprintln!("push: {i}/{n} {name}"),
        &stems,
        deckle_digest::EDITION_PREFIX,
    )
    .map_err(|e| e.to_string())?;
    // Read in the same session; written to the folder after it's closed. The puzzle may have
    // read them alone: highlights are exported only when they're asked for.
    let export = cfg
        .sync
        .for_device(&deckle_core::sync::device_id(target))
        .clippings;
    if plan.clippings && puzzle_on(cfg) && flush.clippings_error.is_none() {
        let path = cfg.puzzle_store();
        let mut store = deckle_digest::puzzle::store::Store::load(&path);
        let today = deckle_digest::chrono::Local::now().date_naive();
        use deckle_digest::puzzle::check::Notes;
        let notes = match flush.clippings.as_deref() {
            Some(b) => Notes::Read(b),
            None if deckle_core::clippings::notes_unreachable(target) => Notes::Unreachable,
            None => Notes::NoFile,
        };
        deckle_digest::puzzle::check::record(
            &mut store,
            notes,
            today,
            deckle_digest::puzzle::words::Words::embedded(),
        );
        if let Err(e) = store.save(&path) {
            eprintln!("puzzle: {}: {e}", path.display());
        }
    }
    let highlights = match flush.clippings.as_deref().filter(|_| export) {
        Some(bytes) => match highlights::export(bytes, &highlights::state_path(), None) {
            Ok(o) => Some(o),
            Err(e) => {
                eprintln!("{e}");
                None
            }
        },
        None => None,
    };
    let recorded = (plan.readership && !flush.positions.is_empty()).then(|| {
        let found: Vec<_> = flush
            .positions
            .iter()
            .map(|(stem, on)| (stem.clone(), on_device(on)))
            .collect();
        let now = deckle_digest::chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let r = readership.record_reads(&deckle_core::sync::device_id(target), &found, &now);
        if let Err(e) = readership.save(&readership_path) {
            eprintln!("readership: {e}");
        }
        r
    });
    record_morgue(&flush);
    let shelf = record_shelf(cfg, target, &flush);
    Ok(DigestOutcome {
        flush: Some(flush),
        highlights,
        readership: recorded,
        shelf,
        ..Default::default()
    })
}

/// Turns what the session read into this Kindle's snapshot in `shelf.json`. A problem with one
/// book is reported and the rest are kept; a problem with the file is reported and nothing is
/// lost but this connection's snapshot.
fn record_shelf(
    cfg: &deckle_digest::Config,
    target: &Target,
    flush: &deckle_core::queue::FlushReport,
) -> Option<deckle_shelf::Summary> {
    let read = flush.shelf.as_ref()?;
    let device = deckle_library::device_ref(target);
    let id = deckle_core::sync::device_id(target);
    let path = cfg.shelf_store();
    // A read that failed (not "no file") must not blank what the last plug-in found.
    let previous = deckle_digest::shelf::store::Shelf::load(&path)
        .kindles
        .get(&id)
        .cloned();
    let library = deckle_library::Library::configured(&deckle_library::config::default_path())
        .map(|(lib, _)| lib);
    let (snap, errors) = deckle_shelf::snapshot(
        deckle_shelf::Read {
            books: read.books.clone(),
            skipped: read.skipped,
            clippings: flush.clippings.clone(),
            vocab: read.vocab.clone(),
            clippings_failed: flush.clippings_error.is_some(),
            vocab_failed: flush.shelf_error.is_some() && read.vocab.is_none(),
            previous,
        },
        &cfg.sync.device_name(&id).unwrap_or(device.name),
        &device.key,
        library.as_ref(),
        deckle_digest::chrono::Utc::now(),
    );
    for e in errors {
        eprintln!("shelf: {e}");
    }
    match deckle_shelf::record(&path, &id, snap) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("shelf: {e}");
            None
        }
    }
}

fn on_device(on: &deckle_core::yjf::OnDevice) -> deckle_readership::OnDevice {
    match on {
        deckle_core::yjf::OnDevice::Absent => deckle_readership::OnDevice::Absent,
        deckle_core::yjf::OnDevice::Unopened => deckle_readership::OnDevice::Unopened,
        deckle_core::yjf::OnDevice::Opened(b) => deckle_readership::OnDevice::Opened(b.clone()),
    }
}

/// Where each section of the edition just built starts, for the readership report. Never fails
/// the run.
fn record_layout(
    kfx: &Path,
    date: deckle_digest::chrono::NaiveDate,
    built: &deckle_digest::BuildReport,
) {
    let sections: Vec<(String, String)> = built
        .sections
        .iter()
        .map(|s| (s.key.clone(), s.title.clone()))
        .collect();
    let name = deckle_digest::edition_filename(date);
    let stem = name.trim_end_matches(".kfx");
    let layout = std::fs::read(kfx)
        .map_err(|e| format!("{}: {e}", kfx.display()))
        .and_then(|bytes| {
            deckle_readership::layout::layout_from_kfx(&bytes, date, &built.title, stem, &sections)
        })
        .map(|l| with_items(l, built));
    let path = deckle_readership::state_path();
    let saved = layout.and_then(|l| {
        let mut state = deckle_readership::State::load(&path);
        state.record_layout(l);
        state.save(&path)
    });
    if let Err(e) = saved {
        eprintln!("readership: {e}");
    }
}

/// Each section's item count from the build report, matched by key, so tomorrow's paper can
/// estimate a story's length (tomorrow's paper design §6.3). The front page has none.
fn with_items(
    mut layout: deckle_readership::layout::Layout,
    built: &deckle_digest::BuildReport,
) -> deckle_readership::layout::Layout {
    for s in &mut layout.sections {
        s.items = built
            .sections
            .iter()
            .find(|b| b.key == s.key)
            .map(|b| b.items as u32);
    }
    layout
}

/// What `digest puzzle --json` prints: today's puzzle, and yesterday's day when there is one.
#[derive(serde::Serialize)]
struct PuzzleOut {
    date: deckle_digest::chrono::NaiveDate,
    #[serde(flatten)]
    puzzle: deckle_digest::puzzle::store::Puzzle,
    yesterday: Option<deckle_digest::puzzle::store::Day>,
}

/// `digest puzzle` (puzzle design goal 8): the same puzzle the paper prints today.
fn digest_puzzle(json: bool, path: &Path) -> Result<ExitCode, String> {
    use deckle_digest::puzzle;
    let cfg = load_digest_config(path)?;
    let date = deckle_digest::chrono::Local::now().date_naive();
    let store_path = cfg.puzzle_store();
    let mut store = puzzle::store::Store::load(&store_path);
    let fresh = !store.days.contains_key(&date);
    let p = puzzle::today(
        date,
        &cfg.puzzle,
        puzzle::words::Words::embedded(),
        &mut store,
    )
    .puzzle
    .clone();
    if fresh {
        store.prune(date);
        store
            .save(&store_path)
            .map_err(|e| format!("{}: {e}", store_path.display()))?;
    }
    let out = PuzzleOut {
        date,
        puzzle: p,
        yesterday: store
            .days
            .get(&(date - deckle_digest::chrono::Days::new(1)))
            .cloned(),
    };
    emit(json, &out, || puzzle_text(&out))?;
    Ok(ExitCode::SUCCESS)
}

fn puzzle_text(out: &PuzzleOut) -> String {
    use deckle_digest::puzzle::grid::Dir;
    let p = &out.puzzle;
    let mut lines = Vec::new();
    match &p.rows {
        Some(rows) => {
            lines.push(format!("The Mini · No. {}", p.number));
            lines.extend(
                rows.iter()
                    .map(|r| format!("  {}", r.replace(|c: char| c != '#', "."))),
            );
            for (dir, name) in [(Dir::Across, "Across"), (Dir::Down, "Down")] {
                lines.push(name.to_owned());
                lines.extend(
                    p.clues
                        .iter()
                        .filter(|c| c.dir == dir)
                        .map(|c| format!("  {:>2}  {}", c.number, c.clue)),
                );
            }
        }
        None => lines.push("No mini today.".to_owned()),
    }
    if let Some(l) = &p.ladder {
        lines.push(format!(
            "Word ladder: {} to {} in {} steps",
            l.from.to_uppercase(),
            l.to.to_uppercase(),
            l.path.len() - 1
        ));
    }
    if let Some(y) = &out.yesterday {
        lines.push(String::new());
        lines.push("Yesterday:".to_owned());
        if let Some(rows) = &y.puzzle.rows {
            lines.extend(rows.iter().map(|r| format!("  {r}")));
        }
        if let Some(l) = &y.puzzle.ladder {
            let path: Vec<String> = l.path.iter().map(|w| w.to_uppercase()).collect();
            lines.push(format!("  Ladder: {}", path.join(", ")));
        }
        if let Some(s) = &y.solve {
            lines.push(format!("  You solved {} of {}", s.solved, s.of));
        }
    }
    lines.join("\n")
}

/// Keeps a built edition in the Morgue (importing the cached ones first, once). A problem is
/// returned as a run note: it never fails the build.
fn archive_to_morgue(
    cfg: &deckle_digest::Config,
    date: deckle_digest::chrono::NaiveDate,
    epub: &Path,
    gathered: &deckle_digest::Gathered,
) -> Option<String> {
    // A build for another date gathers today's news under that date's name: keeping it would
    // replace the day's real paper (or invent one for a day that never had it).
    if !is_today(date) {
        return Some(format!(
            "Morgue: not kept (a build for {date}, not for today, would replace that day's paper)"
        ));
    }
    let morgue = deckle_morgue::Morgue::from_config(cfg);
    let state_path = deckle_morgue::state::state_path();
    let mut state = deckle_morgue::MorgueState::load(&state_path);
    let cache = deckle_core::wordwise::default_cache_dir().join("digest");
    let was_imported = state.imported;
    if !deckle_morgue::import::import_once(&morgue, &cache, &mut state).is_empty() {
        eprintln!("morgue: imported the editions in {}", cache.display());
    }
    // Only a first import changes the state; saving on every build widens the window in which
    // another writer's update is lost.
    if state.imported != was_imported
        && let Err(e) = state.save(&state_path)
    {
        eprintln!("morgue: {}: {e}", state_path.display());
    }
    morgue
        .archive_edition(epub, &deckle_morgue::from_gathered(gathered))
        .err()
        .map(|e| format!("Morgue: couldn't save the edition ({e})"))
}

fn is_today(date: deckle_digest::chrono::NaiveDate) -> bool {
    date == deckle_digest::chrono::Local::now().date_naive()
}

/// Keeps the converted KFX beside the edition in the Morgue; a problem is a run note.
fn keep_kfx_in_morgue(
    cfg: &deckle_digest::Config,
    date: deckle_digest::chrono::NaiveDate,
    kfx: &Path,
) -> Option<String> {
    deckle_morgue::Morgue::from_config(cfg)
        .archive_kfx(date, kfx)
        .err()
        .map(|e| format!("Morgue: couldn't keep the converted paper ({e})"))
}

/// Queues the kept edition for the Kindle again, as `deckle-daily-YYYY-MM-DD.kfx`: its kept
/// KFX, or one converted from the kept EPUB when that is gone. Returns the queued name.
fn morgue_send(
    cfg: &deckle_digest::Config,
    date: deckle_digest::chrono::NaiveDate,
) -> Result<String, String> {
    let morgue = deckle_morgue::Morgue::from_config(cfg);
    if morgue.sidecar(date).is_none() {
        return Err(format!("no edition kept for {date}"));
    }
    let (epub, _, kept) = morgue.paths(date);
    let kfx = if kept.is_file() {
        kept
    } else {
        let located = previewer::locate(None).map_err(|e| e.to_string())?;
        let converted = deckle_core::wordwise::default_cache_dir()
            .join("digest")
            .join("kfx");
        std::fs::create_dir_all(&converted).map_err(|e| e.to_string())?;
        let report = run(&epub, &paper_options(Some(located.path), false, &converted));
        if let Some(e) = &report.error {
            return Err(format!("converting {}: {e}", epub.display()));
        }
        let made = report
            .delivered
            .ok_or_else(|| "the conversion made no file".to_owned())?;
        if let Err(e) = morgue.archive_kfx(date, &made) {
            eprintln!("morgue: couldn't keep the converted paper ({e})");
        }
        made
    };
    let name = deckle_digest::edition_filename(date);
    let queue = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
    // Recorded first: a plug-in between the two steps must already know to leave it alone (it
    // is only exempt while it is in the outbox, so this costs nothing if the add fails).
    let path = deckle_morgue::state::state_path();
    let mut state = deckle_morgue::MorgueState::load(&path);
    let before = state.resent.get(&date).cloned();
    state.resent.insert(
        date,
        deckle_morgue::state::Resent {
            queued: deckle_digest::chrono::Utc::now().to_rfc3339(),
            delivered: None,
        },
    );
    state
        .save(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if let Err(e) = queue.add(&kfx, &name, None) {
        match before {
            Some(r) => state.resent.insert(date, r),
            None => state.resent.remove(&date),
        };
        let _ = state.save(&path);
        return Err(e.to_string());
    }
    Ok(name)
}

/// "Morgue: 214 editions, 590 MB".
fn morgue_size_line(editions: usize, bytes: u64) -> String {
    let mb = (bytes as f64 / (1024.0 * 1024.0)).round() as u64;
    format!(
        "Morgue: {editions} edition{}, {mb} MB",
        if editions == 1 { "" } else { "s" }
    )
}

/// `digest morgue list|search|show|send` (Morgue design §5).
fn digest_morgue(json: bool, path: &Path, action: cli::MorgueCommand) -> Result<ExitCode, String> {
    use deckle_digest::chrono::{Datelike, Local, NaiveDate};
    let cfg = load_digest_config(path)?;
    let morgue = deckle_morgue::Morgue::from_config(&cfg);
    let day = |s: &str| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|_| format!("expected a date as YYYY-MM-DD, got {s:?}"))
    };
    match action {
        cli::MorgueCommand::List { month } => {
            let (year, month) = match month {
                Some(m) => {
                    let first = NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d")
                        .map_err(|_| format!("--month expects YYYY-MM, got {m:?}"))?;
                    (first.year(), first.month())
                }
                None => {
                    let today = Local::now().date_naive();
                    (today.year(), today.month())
                }
            };
            let days = deckle_morgue::search::month(&morgue, year, month);
            let (editions, bytes) = morgue.usage();
            #[derive(serde::Serialize)]
            struct ListOut<'a> {
                days: &'a [deckle_morgue::search::DaySummary],
                editions: usize,
                bytes: u64,
                folder: String,
            }
            let out = ListOut {
                days: &days,
                editions,
                bytes,
                folder: morgue.root.display().to_string(),
            };
            emit(json, &out, || {
                let mut lines: Vec<String> = days
                    .iter()
                    .map(|d| match &d.lead {
                        Some(l) => format!("{}  {} · {l}", d.date, d.paper),
                        None => format!("{}  {}", d.date, d.paper),
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push(format!("No editions kept for {year}-{month:02}."));
                }
                lines.push(morgue_size_line(editions, bytes));
                lines.join("\n")
            })?;
        }
        cli::MorgueCommand::Search { words } => {
            let hits = deckle_morgue::search::search(&morgue, &words.join(" "));
            emit(json, &hits, || {
                if hits.is_empty() {
                    return "Nothing found.".to_owned();
                }
                hits.iter()
                    .flat_map(|h| {
                        h.stories.iter().map(move |(section, title)| {
                            format!("{} · {section} · {title}", h.date)
                        })
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
        }
        cli::MorgueCommand::Show { date } => {
            let date = day(&date)?;
            let reads = deckle_readership::State::load(&deckle_readership::state_path());
            let state = deckle_morgue::MorgueState::load(&deckle_morgue::state::state_path());
            let detail = deckle_morgue::detail::detail(&morgue, date, &reads, &state)
                .ok_or_else(|| format!("no edition kept for {date}"))?;
            emit(json, &detail, || {
                let mut lines = vec![detail.heading.clone()];
                lines.extend(detail.weather.clone());
                if let Some(l) = &detail.lead {
                    lines.push(format!("Lead: {} ({})", l.title, l.section));
                }
                lines.extend(detail.read.clone());
                lines.extend(detail.highlights.clone());
                lines.extend(detail.on_kindle.clone());
                lines.push(detail.epub.display().to_string());
                lines.join("\n")
            })?;
        }
        cli::MorgueCommand::Send { date } => {
            let date = day(&date)?;
            let name = morgue_send(&cfg, date)?;
            let out = serde_json::json!({ "queued": name });
            emit(json, &out, || {
                format!(
                    "queued {name}: it goes over the next time a Kindle connects \
                     (`deckle digest push`, or `watch`)"
                )
            })?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// What `digest shelf --json` prints: each Kindle's snapshot in brief, and the page as of today.
#[derive(serde::Serialize)]
struct ShelfOut {
    kindles: Vec<ShelfKindle>,
    page: ShelfPage,
}

#[derive(serde::Serialize)]
struct ShelfKindle {
    id: String,
    name: String,
    seen: String,
    books: usize,
    highlights_shared: bool,
    highlights: usize,
    words_shared: bool,
    words: usize,
}

#[derive(serde::Serialize)]
struct ShelfPage {
    nightstand: Option<deckle_digest::shelf::select::Nightstand>,
    highlight: Option<deckle_digest::shelf::select::Pick>,
    /// Each word with its "<Book>, <Weekday>" line.
    words: Vec<(deckle_digest::shelf::store::WordSnap, String)>,
}

/// `digest shelf [--forget]`: the page From Your Shelf would print today, from the snapshots.
fn digest_shelf(json: bool, path: &Path, forget: bool) -> Result<ExitCode, String> {
    use deckle_digest::shelf::{select, store::Shelf};
    let cfg = load_digest_config(path)?;
    let store = cfg.shelf_store();
    if forget {
        Shelf::forget(&store)?;
        println!("Forgot the shelf snapshot.");
        return Ok(ExitCode::SUCCESS);
    }
    let shelf = Shelf::load(&store);
    let date = deckle_digest::chrono::Local::now().date_naive();
    let tz = deckle_digest::tz::local_tz();
    let out = ShelfOut {
        kindles: shelf
            .kindles
            .iter()
            .map(|(id, k)| ShelfKindle {
                id: id.clone(),
                name: k.name.clone(),
                seen: k.seen.clone(),
                books: k.books.len(),
                highlights_shared: k.highlights_shared,
                highlights: k.highlights.len(),
                words_shared: k.words_shared,
                words: k.words.len(),
            })
            .collect(),
        page: ShelfPage {
            nightstand: select::nightstand(
                &shelf,
                &cfg.shelf,
                date,
                tz,
                deckle_shelf::KINDLE_FRACTION_OK,
            ),
            highlight: cfg
                .shelf
                .highlights
                .then(|| select::highlight(&shelf, date, tz))
                .flatten(),
            words: if cfg.shelf.words {
                select::words(&shelf, date, tz)
            } else {
                Vec::new()
            },
        },
    };
    emit(json, &out, || shelf_text(&out))?;
    Ok(ExitCode::SUCCESS)
}

fn shelf_text(out: &ShelfOut) -> String {
    if out.kindles.is_empty() {
        return "Nothing on the shelf yet: add \"shelf\" to sections in digest.toml and plug \
                your Kindle in (or run `deckle digest push`)."
            .to_owned();
    }
    let mut lines = Vec::new();
    for k in &out.kindles {
        let shares = |shared: bool, n: usize, what: &str| match shared {
            true => format!("{n} {what}"),
            false => format!("no {what} shared"),
        };
        lines.push(format!(
            "{} · seen {} · {} books · {} · {}",
            k.name,
            k.seen,
            k.books,
            shares(k.highlights_shared, k.highlights, "highlights"),
            shares(k.words_shared, k.words, "words"),
        ));
    }
    let page = &out.page;
    if let Some(n) = &page.nightstand {
        lines.push(String::new());
        lines.push("On your nightstand".to_owned());
        for b in &n.books {
            let pct = b
                .percent
                .map(|p| format!("{}{p}% · ", if b.about { "about " } else { "" }))
                .unwrap_or_default();
            lines.push(format!("  {} — {pct}{}", b.title, b.state));
        }
        if let Some(nudge) = &n.nudge {
            lines.push(format!("  {}", nudge.replace('*', "")));
        }
    }
    if let Some(p) = &page.highlight {
        lines.push(String::new());
        lines.push(format!("You highlighted this {} ago", p.age));
        lines.push(format!("  \u{201C}{}\u{201D}", p.highlight.text));
        lines.push(format!("  {}, {}", p.highlight.title, p.highlight.author));
        lines.extend(
            p.highlight
                .notes
                .iter()
                .map(|n| format!("  Your note: {n}")),
        );
    }
    if !page.words.is_empty() {
        lines.push(String::new());
        lines.push("Words you looked up this week".to_owned());
        for (w, from) in &page.words {
            let def = w
                .definition
                .as_deref()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default();
            lines.push(format!("  {}{def} ({from})", w.word));
        }
    }
    lines.join("\n")
}

/// `digest readership [apply|dismiss N]` (readership design §7).
fn digest_readership(
    json: bool,
    path: &Path,
    action: Option<cli::ReadershipCommand>,
) -> Result<ExitCode, String> {
    let mut cfg = load_digest_config(path)?;
    let state_path = deckle_readership::state_path();
    let mut state = deckle_readership::State::load(&state_path);
    let report = deckle_readership::report(&state, &cfg);
    let Some(action) = action else {
        emit(json, &report, || {
            if state.editions.is_empty() {
                "No editions recorded yet. Build one with `deckle digest run --to device`.".into()
            } else {
                readership_text(&report)
            }
        })?;
        return Ok(ExitCode::SUCCESS);
    };
    let (cli::ReadershipCommand::Apply { n } | cli::ReadershipCommand::Dismiss { n }) = action;
    let Some(s) = n.checked_sub(1).and_then(|i| report.suggestions.get(i)) else {
        eprintln!(
            "no suggestion {n}: the report has {}",
            report.suggestions.len()
        );
        return Ok(ExitCode::from(2));
    };
    match action {
        cli::ReadershipCommand::Apply { .. } => {
            deckle_readership::apply(s, &mut cfg)?;
            deckle_digest::config::save(path, &cfg)?;
            // Set aside, or a Move up would come straight back from the same editions.
            deckle_readership::dismiss(&mut state, s);
            state.save(&state_path)?;
            emit(json, s, || format!("applied: {}", s.title))?;
        }
        cli::ReadershipCommand::Dismiss { .. } => {
            deckle_readership::dismiss(&mut state, s);
            state.save(&state_path)?;
            emit(json, s, || format!("dismissed: {}", s.title))?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn readership_text(r: &deckle_readership::Report) -> String {
    let mut lines = vec![match r.median_percent {
        Some(p) => format!(
            "Opened {} of {} editions · usually about {p}% in",
            r.opened, r.window
        ),
        None => format!("Opened {} of {} editions", r.opened, r.window),
    }];
    let width = r
        .sections
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(0);
    for s in &r.sections {
        let share = s
            .share
            .map_or("not enough editions yet".to_owned(), |p| format!("{p:>3}%"));
        lines.push(format!("  {:<width$}  {share}", s.name));
    }
    if !r.suggestions.is_empty() {
        lines.push("Suggested edits:".into());
        for (i, s) in r.suggestions.iter().enumerate() {
            lines.push(format!("  {}. {}: {}", i + 1, s.title, s.why));
        }
    }
    lines.join("\n")
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
            &[],
        )
        .unwrap();
        EmailRun {
            email: email.unwrap_or_default(),
        }
    }

    #[test]
    fn an_edition_sent_again_survives_the_days_paper_in_the_outbox() {
        let run = built();
        let old = "deckle-daily-2026-10-01.kfx";
        run.queue.add(&run.kfx, old, None).unwrap();
        run.queue
            .add(&run.kfx, "deckle-daily-2026-10-19.kfx", None)
            .unwrap();
        queue_edition(
            &run.queue,
            Some(run.kfx.as_path()),
            &deckle_digest::edition_filename(date()),
            Some(deckle_digest::EDITION_PREFIX),
            &[],
            &[old.to_owned()],
        )
        .unwrap();
        let names: Vec<String> = run
            .queue
            .pending()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(
            names.contains(&old.to_owned()),
            "the re-send waits: {names:?}"
        );
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

#[cfg(test)]
mod push_tests {
    use super::*;
    use deckle_core::sync::Actions;

    #[test]
    fn push_with_an_empty_outbox_still_reads_positions() {
        let on = Actions {
            push: true,
            prune: true,
            clippings: false,
            readership: true,
            shelf: false,
        };
        assert_eq!(
            push_plan(on, true).map(|p| (p.push, p.readership)),
            Some((false, true))
        );
        assert_eq!(
            push_plan(
                Actions {
                    readership: false,
                    ..on
                },
                true
            ),
            None
        );
        // the puzzle's notes alone are reason enough to read the Kindle
        assert_eq!(
            push_plan(
                Actions {
                    readership: false,
                    clippings: true,
                    ..on
                },
                true
            )
            .map(|p| (p.push, p.clippings, p.readership)),
            Some((false, true, false))
        );
        // the shelf alone is too
        assert_eq!(
            push_plan(
                Actions {
                    readership: false,
                    shelf: true,
                    ..on
                },
                true
            )
            .map(|p| (p.push, p.shelf)),
            Some((false, true))
        );
        assert_eq!(push_plan(on, false).map(|p| p.push), Some(true));
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use deckle_readership::layout::{Layout, SectionStart};

    #[test]
    fn layout_records_items() {
        let sec = |key: &str| SectionStart {
            key: key.into(),
            name: key.into(),
            start: 0,
            items: None,
        };
        let layout = Layout {
            date: "2026-10-22".parse().unwrap(),
            title: "T".into(),
            file: "f".into(),
            end: 10,
            sections: vec![sec("front"), sec("local"), sec("feeds:Science")],
        };
        let summary = |key: &str, items| deckle_digest::SectionSummary {
            key: key.into(),
            title: key.into(),
            items,
            unavailable: None,
        };
        let built = deckle_digest::BuildReport {
            epub: PathBuf::new(),
            title: "T".into(),
            sections: vec![summary("feeds:Science", 7), summary("local", 3)],
            long_reads: Vec::new(),
            reading_outcomes: Vec::new(),
            notes: Vec::new(),
        };
        let items: Vec<Option<u32>> = with_items(layout, &built)
            .sections
            .iter()
            .map(|s| s.items)
            .collect();
        // matched by key; the front page has none
        assert_eq!(items, [None, Some(3), Some(7)]);
    }
}
