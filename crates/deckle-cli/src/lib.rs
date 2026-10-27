// SPDX-License-Identifier: GPL-3.0-or-later
//! The `deckle` command line, as a library so the desktop app can run it too.

mod cli;
mod cookbook;
pub mod cookbook_api;
mod cookbook_delivery;
mod highlights;
mod inbox;
pub mod press_worker;
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

/// Every connected Kindle: mass storage first, then MTP.
fn connected_kindles() -> Vec<Target> {
    let mut out: Vec<Target> = detect_mass_storage()
        .into_iter()
        .map(Target::MassStorage)
        .collect();
    out.extend(deckle_transport::usb::detect().into_iter().map(Target::Mtp));
    out
}

/// A Kindle picked with `--kindle`: its sync id, what to call it, and its connection when it's
/// plugged in.
struct Chosen {
    id: String,
    label: String,
    target: Option<Target>,
}

/// The Kindle `arg` names: a connected one by its name or id, else a named one from the
/// config (Kindle profiles design §2).
fn choose_kindle(
    arg: &str,
    sync: &deckle_digest::config::SyncConfig,
    connected: Vec<Target>,
) -> Result<Chosen, String> {
    let want = arg.trim();
    for t in connected {
        let id = deckle_core::sync::device_id(&t);
        let name = sync.device_name(&id);
        let by_name = name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case(want));
        if by_name || deckle_core::sync::same_kindle(&id, want) {
            return Ok(Chosen {
                label: name.unwrap_or_else(|| id.clone()),
                id,
                target: Some(t),
            });
        }
    }
    if let Some(id) = sync.find_device(want) {
        return Ok(Chosen {
            label: sync.device_name(&id).unwrap_or_else(|| id.clone()),
            id,
            target: None,
        });
    }
    let mut names: Vec<&str> = sync.named().map(|(_, n)| n).collect();
    names.sort_unstable();
    names.dedup();
    Err(if names.is_empty() {
        format!("no Kindle is called \"{want}\"; plug it in and name it with `deckle devices name`")
    } else {
        format!(
            "no Kindle is called \"{want}\"; known: {}",
            names.join(", ")
        )
    })
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
        cli::Command::Devices {
            action: Some(cli::DevicesCommand::Name { name, kindle }),
        } => name_kindle(json, &name, kindle.as_deref()),
        cli::Command::Devices { action: None } => {
            let names = deckle_digest::Config::load(&deckle_digest::config::default_path())
                .unwrap_or_default()
                .sync;
            let named = |t: &Target| {
                names
                    .device_name(&deckle_core::sync::device_id(t))
                    .map(|n| format!("{n}  "))
                    .unwrap_or_default()
            };
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
                            "{}{}  {}  mass storage, thumbnails:{}  sync id \"{}\"",
                            named(&Target::MassStorage(d.clone())),
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
                        "{}usb {}:{}  {}  MTP (serial {})  sync id \"{}\"",
                        named(&Target::Mtp(m.clone())),
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
        cli::Command::Digest {
            config,
            paper,
            action,
        } => digest(json, config, paper, action),
        cli::Command::Cookbook { action } => cookbook::run(json, action),
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
            preset,
            to,
            kindle,
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
            let pages_explicit = pages.is_some();
            let pages = cli::parse_pages(pages.as_deref())?;
            if kindle.is_some() && to != "device" {
                return Err("--kindle sends to a Kindle; leave out --to".into());
            }
            // No Kindle but a Kobo or a PocketBook: neither needs Previewer (other readers §8).
            if to == "device"
                && kindle.is_none()
                && detect_kindle().is_none()
                && let Some(reader) = detect_reader()
            {
                if preset.is_some() {
                    return Err(
                        "style presets currently require a Kindle or --to <folder> KFX export"
                            .into(),
                    );
                }
                return send_to_reader(&epubs, &reader, json);
            }
            // Fail before any work (and before touching the target) when Previewer is missing.
            let located = previewer::locate(pv.as_deref()).map_err(|e| e.to_string())?;
            // `--kindle`: that Kindle when it's connected; otherwise the books wait for it.
            let chosen = match &kindle {
                Some(k) => {
                    let cfg = deckle_digest::Config::load(&deckle_digest::config::default_path())
                        .unwrap_or_default();
                    Some(choose_kindle(k, &cfg.sync, connected_kindles())?)
                }
                None => None,
            };
            let target = match &chosen {
                Some(Chosen {
                    target: Some(t), ..
                }) => t.clone(),
                // `prepare` delivers nothing: the Kindle is known at push.
                Some(_) => Target::Folder(std::env::temp_dir()),
                None => {
                    let connected = connected_kindles();
                    if to == "device" && connected.len() > 1 {
                        eprintln!(
                            "{} Kindles are connected; sending to the first. Choose with --kindle <name>.",
                            connected.len()
                        );
                    }
                    resolve_target(&to)?
                }
            };
            let mut opts = RunOptions {
                preset: None,
                cover: Default::default(),
                cde_type: cde.into(),
                cde_override: cde.to_override(),
                amazon_match,
                generated_cover: (!no_generated_cover)
                    .then(|| cover_style.as_deref().map(str::parse).transpose())
                    .transpose()?,
                metadata_fill: fill_metadata,
                typography: typography.unwrap_or(false),
                popup_notes: popup_notes.unwrap_or(false),
                verified_real_asin: verified_asin,
                asin,
                pages,
                target,
                previewer: Some(located.path),
                keep_work_dir,
                xray: xray.unwrap_or(cli::XRayArg::Auto).into(),
                llm: llm.config(llm_model, llm_endpoint),
                wikipedia,
                word_wise: (word_wise.unwrap_or(cli::XRayArg::Auto) == cli::XRayArg::Auto).then(
                    || deckle_core::wordwise::WordWiseOptions {
                        level: word_wise_level,
                        database: wordwise_db,
                        ..Default::default()
                    },
                ),
                replaces: Vec::new(),
                originals: None,
            };
            if let Some(preset) = preset {
                let word_wise_options = opts.word_wise.take();
                preset.apply(&mut opts);
                if let Some(on) = typography {
                    opts.typography = on;
                }
                if let Some(on) = popup_notes {
                    opts.popup_notes = on;
                }
                if pages_explicit {
                    opts.pages = pages;
                }
                if let Some(xray) = xray {
                    opts.xray = xray.into();
                }
                if word_wise.is_some() {
                    opts.word_wise = word_wise_options;
                }
            }
            if let Some(Chosen {
                id,
                label,
                target: None,
            }) = &chosen
            {
                return queue_for_kindle(json, &epubs, &opts, id, label);
            }
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

/// `devices name`: names the connected Kindle (the one `kindle` picks when several are).
fn name_kindle(json: bool, name: &str, kindle: Option<&str>) -> Result<ExitCode, String> {
    let path = deckle_digest::config::default_path();
    let mut cfg = deckle_digest::Config::load(&path).unwrap_or_default();
    let connected = connected_kindles();
    let target = match kindle {
        Some(k) => choose_kindle(k, &cfg.sync, connected)?
            .target
            .ok_or_else(|| format!("{k} isn't connected; plug it in to name it"))?,
        None => match connected.len() {
            0 => return Err("no Kindle connected; plug in the one to name".into()),
            1 => connected.into_iter().next().expect("one"),
            n => {
                return Err(format!(
                    "{n} Kindles are connected; choose one with --kindle <name or sync id> (`deckle devices` lists them)"
                ));
            }
        },
    };
    let id = deckle_core::sync::device_id(&target);
    cfg.sync.set_device_name(&id, name)?;
    deckle_digest::config::save(&path, &cfg)?;
    let name = name.trim();
    #[derive(serde::Serialize)]
    struct Named<'a> {
        id: &'a str,
        name: &'a str,
    }
    emit(json, &Named { id: &id, name }, || match name {
        "" => format!("{id}: name cleared"),
        n => format!("{id} is now \"{n}\""),
    })?;
    Ok(ExitCode::SUCCESS)
}

/// `run --kindle` for a Kindle that isn't connected: each book is prepared and queued for it,
/// delivered by the next `push` or `watch` that sees it (Kindle profiles design §2).
fn queue_for_kindle(
    json: bool,
    inputs: &[PathBuf],
    opts: &RunOptions,
    id: &str,
    label: &str,
) -> Result<ExitCode, String> {
    #[derive(serde::Serialize)]
    struct Queued {
        input: PathBuf,
        kindle: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        queued: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    }
    let outbox = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
    let results: Vec<Queued> = inputs
        .iter()
        .map(|input| {
            let queued = queue_one(input, opts, &outbox, id);
            Queued {
                input: input.clone(),
                kindle: label.to_owned(),
                error: queued.as_ref().err().cloned(),
                queued: queued.ok(),
            }
        })
        .collect();
    let failed = results.iter().filter(|r| r.error.is_some()).count();
    emit(json, &results, || {
        let mut lines: Vec<String> = results
            .iter()
            .map(|r| match (&r.queued, &r.error) {
                (_, Some(e)) => format!("FAIL  {}\n      {e}", r.input.display()),
                (Some(name), None) => {
                    format!("QUEUED {}\n      for {label}: {name}", r.input.display())
                }
                _ => format!("??    {}", r.input.display()),
            })
            .collect();
        if failed < results.len() {
            lines.push(format!(
                "{label} isn't connected: `deckle digest push` (or `watch`) delivers when it is"
            ));
        }
        lines.join("\n")
    })?;
    Ok(if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_FAILURE)
    })
}

/// One `run --kindle` input up to the KPF, into the outbox for Kindle `id`; the queue name.
fn queue_one(
    input: &Path,
    opts: &RunOptions,
    outbox: &deckle_core::queue::Queue,
    id: &str,
) -> Result<String, String> {
    let arg = input.to_string_lossy();
    let source = if deckle_digest::web_book::is_article(&arg) {
        import_article(&arg, &deckle_digest::Http, &articles_dir())
            .map_err(|e| format!("import: {e}"))?
            .path
    } else {
        input.to_path_buf()
    };
    let prepared = deckle_core::pipeline::prepare(&source, opts, &mut |_| {})
        .map_err(|r| r.error.unwrap_or_else(|| "not converted".into()))?;
    let name = deckle_core::deliver::device_filename(&prepared.report.info);
    outbox
        .add_prepared_owned(&prepared, &name, None, Some(id))
        .map_err(|e| format!("outbox: {e}"))?;
    Ok(name)
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
        cover: Default::default(),
        replaces: deckle_library::previous_copies(&config, &source, &opts.target),
        originals: None,
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
/// Binds `inputs`, in order, into one EPUB at `out`: each made an EPUB first (imported, or
/// fetched for a web article; an EPUB as it is). The importers' warnings come first in the
/// report, each once. Shared by `deckle bind` and the app (batch-bind design §4, §6).
pub fn bind_files(
    inputs: &[PathBuf],
    title: String,
    author: Option<String>,
    out: &Path,
    net: &dyn deckle_digest::Net,
) -> Result<deckle_core::bind::BindReport, String> {
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
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut report =
        bind(&parts, &BindOptions { title, author }, out).map_err(|e| e.to_string())?;
    warnings.append(&mut report.warnings);
    report.warnings = warnings;
    Ok(report)
}

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
    let out = out.unwrap_or_else(|| {
        PathBuf::from(format!("{}.epub", deckle_core::deliver::sanitize(&title)))
    });
    let report = bind_files(inputs, title, author, &out, net)?;
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
    // No Kindle but a Kobo: its highlights, written as a Kindle's clippings (other readers §8.2).
    let kobo = (from == "device" && detect_kindle().is_none())
        .then(detect_reader)
        .flatten()
        .and_then(|r| match r {
            deckle_core::reader::Reader::Kobo(k) => Some(k),
            _ => None,
        });
    let read = match &kobo {
        Some(k) => deckle_core::kobo::read(k)
            .map(|r| r.clippings)
            .map_err(|e| e.to_string()),
        None => {
            let target = resolve_target(from)?;
            deckle_core::clippings::read_on(&mut deckle_core::deliver::Conn::new(&target))
                .map_err(|e| e.to_string())
        }
    };
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

/// `digest lint`: the story as the paper would keep it, and what it drops.
fn digest_lint(
    json: bool,
    page: &str,
    net: &dyn deckle_digest::Net,
    subs: &[deckle_digest::config::SubscriptionConfig],
) -> Result<ExitCode, String> {
    let mut access_line: Option<String> = None;
    let mut walled = false;
    let (html, base) = if page.starts_with("http://") || page.starts_with("https://") {
        let (bytes, access) = net.get_page_as(page)?;
        access_line = match &access {
            deckle_digest::Access::Open => None,
            deckle_digest::Access::Subscription { site, .. } => Some(
                match subs
                    .iter()
                    .find(|s| s.site.trim().eq_ignore_ascii_case(site))
                {
                    Some(s) => format!("signed in as {} at {site}", s.username.trim()),
                    None => format!("signed in at {site}"),
                },
            ),
            deckle_digest::Access::Walled { site, why } => {
                walled = true;
                Some(why.note(site))
            }
        };
        (
            String::from_utf8_lossy(&bytes).into_owned(),
            page.to_owned(),
        )
    } else {
        let path = Path::new(page)
            .canonicalize()
            .map_err(|e| format!("{page}: {e}"))?;
        let html =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let base = deckle_digest::url::Url::from_file_path(&path)
            .map_err(|_| format!("{}: not an absolute path", path.display()))?
            .to_string();
        (html, base)
    };
    let (x, dropped) =
        deckle_digest::article::extract_linted(&html, &base, None).ok_or_else(|| {
            match (&access_line, &walled) {
                (Some(note), true) => format!("no readable story here: {note}"),
                _ => "no readable story here (a paywall, a sign-in page, or a video)".to_owned(),
            }
        })?;
    let photos: Vec<_> = x
        .photos
        .iter()
        .zip(&x.photo_meta)
        .map(|(url, m)| serde_json::json!({ "url": url, "caption": m.caption, "credit": m.credit, "alt": m.alt }))
        .collect();
    emit(
        json,
        &serde_json::json!({
            "title": x.title, "byline": x.byline, "published": x.published, "words": x.words,
            "dropped": dropped, "photos": photos, "access": access_line,
        }),
        || {
            let mut lines: Vec<String> = access_line.iter().cloned().collect();
            lines.push(format!(
                "{} · {} words{}",
                x.title.as_deref().unwrap_or("(no title)"),
                x.words,
                x.byline
                    .as_deref()
                    .map(|b| format!(" · {b}"))
                    .unwrap_or_default()
            ));
            if dropped.is_empty() {
                lines.push("nothing dropped".into());
            }
            lines.extend(dropped.iter().map(|d| format!("dropped: {d}")));
            for (url, m) in x.photos.iter().zip(&x.photo_meta) {
                lines.push(format!(
                    "photo: {url}{}{}",
                    m.caption
                        .as_deref()
                        .map(|c| format!("\n      caption: {c}"))
                        .unwrap_or_default(),
                    m.credit
                        .as_deref()
                        .map(|c| format!("\n      credit: {c}"))
                        .unwrap_or_default()
                ));
            }
            lines.join("\n")
        },
    )?;
    Ok(ExitCode::SUCCESS)
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
            let sources = originals();
            let sorted = deckle_library::upgrade::sort(shelf, &sources, |_| None);
            let rows: Vec<_> = sorted
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "book": s.shelved.book,
                        "library": s.shelved.library.as_ref().map(|b| &b.dir),
                        "updates": s.shelved.updates,
                        "duplicate": s.shelved.duplicate,
                        "class": s.class,
                        "source": s.source.as_ref().map(|x| &x.path),
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
                    for sorted in &sorted {
                        let s = &sorted.shelved;
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
                            if s.library.is_none() && !b.from_deckle {
                                sorted.class.label().to_owned()
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
                    if sorted.is_empty() {
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
        cli::KindleCommand::Upgrade {
            files,
            dry_run,
            yes,
            include_reading,
            previewer: pv,
        } => {
            drop(conn);
            kindle_upgrade(json, &target, &files, dry_run, yes, include_reading, pv)
        }
        cli::KindleCommand::PutBack { file } => {
            let config = deckle_library::config::default_path();
            let done = deckle_library::upgrade::put_back(&mut conn, &config, &file)?;
            emit(json, &done, || {
                let mut lines = vec![format!("put back {}", done.restored)];
                lines.extend(done.removed.iter().map(|r| format!("removed {r}")));
                lines.join("\n")
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::KindleCommand::Backup { into } => {
            let root = into.unwrap_or_else(|| {
                deckle_library::care::backups_root(&deckle_library::config::default_path())
            });
            let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
            // Every book's details: an AZW whose record 0 says DRM is left out over MTP too.
            let (inv, _) = deckle_core::inventory::list_detailed(
                &mut conn,
                Some(&cache),
                deckle_digest::EDITION_PREFIX,
            )
            .map_err(|e| e.to_string())?;
            let current = deckle_library::freshness::Current::now(&Default::default());
            let shelf =
                deckle_library::shelf::shelve(inv.books, library.as_ref(), &device.key, &current);
            let at = deckle_digest::chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            let done = deckle_library::care::backup_on(
                &mut conn,
                &shelf,
                &root,
                &deckle_shelf::friendly_name(&device.name),
                deckle_digest::EDITION_PREFIX,
                &at,
            )?;
            emit(json, &done, || {
                let mut lines: Vec<String> =
                    done.copied.iter().map(|f| format!("copied {f}")).collect();
                lines.extend(done.failed.iter().map(|(f, e)| format!("failed {f}: {e}")));
                lines.push(format!(
                    "{} backed up to {}; {} left out (Amazon's own, the paper)",
                    done.copied.len(),
                    done.dir.display(),
                    done.skipped.len()
                ));
                lines.join("\n")
            })?;
            Ok(if done.failed.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        cli::KindleCommand::Restore { dir } => {
            let present: Vec<String> = list_on(&mut conn, None)
                .map_err(|e| e.to_string())?
                .books
                .into_iter()
                .map(|b| b.file)
                .collect();
            let done = deckle_library::care::restore_on(&mut conn, &dir, &present)?;
            emit(json, &done, || {
                let mut lines: Vec<String> = done
                    .restored
                    .iter()
                    .map(|f| format!("restored {f}"))
                    .collect();
                lines.extend(done.already.iter().map(|f| format!("already there: {f}")));
                lines.extend(done.failed.iter().map(|(f, e)| format!("failed {f}: {e}")));
                lines.push(format!(
                    "{} restored to {}",
                    done.restored.len(),
                    device.name
                ));
                lines.join("\n")
            })?;
            Ok(if done.failed.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
    }
}

/// The originals on this computer (Books and Kindle design §4); a calibre library that won't
/// open is a warning.
fn originals() -> deckle_library::sources::Sources {
    let cfg = deckle_library::LibraryConfig::load(&deckle_library::config::default_path());
    let found =
        deckle_library::sources::Sources::gather(&cfg, deckle_library::sources::home().as_deref());
    for w in &found.warnings {
        eprintln!("warning: {w}");
    }
    found
}

/// `kindle upgrade`: books with an original on this computer, rebuilt and put in place of the
/// Kindle's copy, which is kept (Books and Kindle design §6).
fn kindle_upgrade(
    json: bool,
    target: &Target,
    files: &[String],
    dry_run: bool,
    yes: bool,
    include_reading: bool,
    pv: Option<PathBuf>,
) -> Result<ExitCode, String> {
    use deckle_core::deliver::Conn;
    use deckle_library::upgrade::{Class, Sorted};
    let config = deckle_library::config::default_path();
    let device = deckle_library::device_ref(target);
    let library = deckle_library::Library::configured(&config).map(|(l, _)| l);
    let sources = originals();
    let word_wise = deckle_core::wordwise::WordWiseOptions::default();
    let current = deckle_library::freshness::Current::now(&word_wise);
    let sorted = {
        // The listing's session ends before the first rebuild opens its own.
        let mut conn = Conn::new(target);
        let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
        let (inv, _) = deckle_core::inventory::list_detailed(
            &mut conn,
            Some(&cache),
            deckle_digest::EDITION_PREFIX,
        )
        .map_err(|e| e.to_string())?;
        let shelf =
            deckle_library::shelf::shelve(inv.books, library.as_ref(), &device.key, &current);
        deckle_library::upgrade::sort(shelf, &sources, |b| {
            deckle_readership::parse_yjf(b)
                .ok()
                .and_then(|p| p.kindle_fraction)
        })
    };
    let title = |s: &Sorted| {
        s.shelved
            .book
            .title
            .clone()
            .unwrap_or_else(|| s.shelved.book.file.clone())
    };
    let named = |s: &Sorted| files.contains(&s.shelved.book.file);
    // An original on this computer, or the Kindle's own DRM-free KF8 copy (design §5).
    let from_here = |s: &Sorted| matches!(s.class, Class::HasSource | Class::KindleCopy);
    let candidates: Vec<&Sorted> = sorted
        .iter()
        .filter(|s| from_here(s))
        .filter(|s| files.is_empty() || named(s))
        .collect();
    let (plan, reading): (Vec<&Sorted>, Vec<&Sorted>) = candidates
        .into_iter()
        .partition(|s| include_reading || named(s) || !s.partway());
    for f in files {
        if !sorted
            .iter()
            .any(|s| &s.shelved.book.file == f && from_here(s))
        {
            eprintln!(
                "warning: {f}: not a book Deckle can upgrade (no original, DRM, or not a book)"
            );
        }
    }
    let count = |c: Class| sorted.iter().filter(|s| s.class == c).count();
    let summary = [
        (
            Class::Older,
            "Deckle's older builds: `deckle library reprocess`",
        ),
        (Class::NoSource, "KFX with no original here"),
        (Class::AmazonOwn, "Amazon's own, left alone"),
        (Class::Locked, "locked by DRM, left alone"),
        (Class::FixedLayout, "fixed layout, left alone"),
    ]
    .into_iter()
    .filter(|(c, _)| count(*c) > 0)
    .map(|(c, what)| format!("{} {what}", count(c)))
    .collect::<Vec<_>>();
    if !json {
        for s in &plan {
            let from = match &s.source {
                Some(src) => src.path.display().to_string(),
                None => "rebuilt from the Kindle's own copy".to_owned(),
            };
            println!("{}\n      {} ← {from}", title(s), s.shelved.book.file);
        }
        for s in &reading {
            println!(
                "{} (skipped: {:.0}% read; name its file or pass --include-reading)",
                title(s),
                s.fraction.unwrap_or(0.0) * 100.0
            );
        }
        for line in &summary {
            println!("{line}");
        }
    }
    if plan.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("nothing to upgrade on {}", device.name);
        }
        return Ok(ExitCode::SUCCESS);
    }
    if dry_run {
        if !json {
            println!(
                "{} can be upgraded on {}; nothing changed",
                plan.len(),
                device.name
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    if !yes {
        use std::io::{BufRead, IsTerminal, Write};
        if !std::io::stdin().is_terminal() {
            return Err("pass --yes to upgrade without a terminal to confirm in".into());
        }
        print!(
            "Upgrade {} on {}? The old copies are kept. [y/N] ",
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
    // Highlights first, when there's a folder for them (design §6). The old .sdr is kept too.
    let state = highlights::state_path();
    if deckle_highlights::State::load(&state).folder.is_some() {
        let read =
            deckle_core::clippings::read_on(&mut Conn::new(target)).map_err(|e| e.to_string());
        match read.map(|b| b.map(|b| deckle_highlights::export_clippings(&b, &state, None))) {
            Ok(Some(Err(e))) | Err(e) => eprintln!("warning: highlights weren't saved first: {e}"),
            _ => {}
        }
    }
    let located = previewer::locate(pv.as_deref()).map_err(|e| e.to_string())?;
    let mut reports = Vec::new();
    for s in &plan {
        let file = &s.shelved.book.file;
        let (source, origin) = match &s.source {
            Some(src) => (src.path.clone(), src.origin),
            // No original here: the Kindle's copy, rebuilt as an EPUB first.
            None => match deckle_library::upgrade::rebuild_from_kindle(
                &mut Conn::new(target),
                &config,
                file,
            ) {
                Ok(epub) => (epub, deckle_library::sources::Origin::KindleCopy),
                Err(e) => {
                    reports.push(RunReport {
                        error: Some(e),
                        ..Default::default()
                    });
                    continue;
                }
            },
        };
        let at = deckle_core::devices::now_iso();
        let kept = deckle_library::upgrade::originals_dir(&config, &device.key, file, &at);
        let opts = RunOptions {
            preset: None,
            cover: Default::default(),
            cde_type: deckle_kfx::convert::CdeType::Pdoc,
            // The cover rule picks the mode, as for any book.
            cde_override: None,
            amazon_match: false,
            generated_cover: Some(None),
            metadata_fill: false,
            typography: true,
            popup_notes: true,
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
            replaces: vec![file.clone()],
            originals: Some(kept.clone()),
        };
        let mut r = run(&source, &opts);
        if let Err(e) = deckle_library::auto_add(&config, &source, &r, target) {
            r.warnings.push(deckle_library::auto_add_warning(&e));
        }
        if let Err(e) = deckle_library::upgrade::record(
            &config,
            &device.key,
            file,
            &title(s),
            &source,
            Some(origin),
            &kept,
            &r,
            &at,
        ) {
            r.warnings.push(format!(
                "the upgrade wasn't recorded, so it can't be put back from Deckle: {e}"
            ));
        }
        reports.push(r);
    }
    let failed = reports.iter().filter(|r| r.error.is_some()).count();
    emit(json, &reports, || {
        plan.iter()
            .zip(&reports)
            .map(|(s, r)| match &r.error {
                Some(e) => format!(
                    "FAIL  {}\n      {e} (the old copy is still there)",
                    title(s)
                ),
                None => format!(
                    "OK    {}{}{}",
                    title(s),
                    r.replaced
                        .iter()
                        .map(|f| format!(
                            "\n      replaced {f} (kept; `deckle kindle put-back {f}` undoes it)"
                        ))
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
            preset: None,
            cover: Default::default(),
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
            originals: None,
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
    if let cli::LibraryCommand::Search { query } = &action {
        let mut targets: Vec<_> = deckle_core::deliver::detect_mass_storage()
            .into_iter()
            .map(Target::MassStorage)
            .collect();
        targets.extend(deckle_transport::usb::detect().into_iter().map(Target::Mtp));
        let report = deckle_library::search::collect(&config, &targets, &query.join(" "));
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
        } else {
            for hit in &report.results {
                println!("{} — {}", hit.title, hit.authors.join(", "));
                for loc in &hit.locations {
                    let status = match loc.kind {
                        deckle_library::search::LocationKind::Library => "local",
                        deckle_library::search::LocationKind::Connected => "connected",
                        deckle_library::search::LocationKind::RecordedSend => {
                            "previously sent; presence unverified"
                        }
                    };
                    println!("  {} ({status}): {}", loc.name, loc.path);
                }
            }
            if report.results.is_empty() {
                println!("No books match.");
            }
            for warning in &report.warnings {
                eprintln!("warning: {warning}");
            }
        }
        return Ok(ExitCode::SUCCESS);
    }
    let (lib, _) = Library::configured(&config)
        .ok_or("no library yet: run `deckle library init <folder>` first")?;
    match action {
        cli::LibraryCommand::Init { .. } | cli::LibraryCommand::Search { .. } => {
            unreachable!("handled above")
        }
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

/// The paper `--paper` names (many papers §3): `None` or `main` is digest.toml's.
fn paper_config(path: &Path, paper: Option<&str>) -> Result<deckle_digest::Config, String> {
    match paper {
        None => load_digest_config(path),
        Some(slug) => {
            load_digest_config(path)?;
            deckle_digest::papers::load_one(path, slug)
        }
    }
}

/// Every paper, main first. What was skipped is said on stderr.
fn all_papers(path: &Path) -> Result<Vec<deckle_digest::Config>, String> {
    load_digest_config(path)?;
    let loaded = deckle_digest::papers::load_all(path)?;
    for note in &loaded.notes {
        eprintln!("papers: {note}");
    }
    Ok(loaded.papers)
}

/// A command that works on the main paper only, for now.
fn main_paper_only(paper: Option<&str>, what: &str) -> Result<(), String> {
    match paper {
        None => Ok(()),
        Some(p) if p == deckle_digest::papers::MAIN => Ok(()),
        Some(_) => Err(format!(
            "{what} works on the main paper (digest.toml) for now; leave out --paper"
        )),
    }
}

/// The sync id of the Kindle a paper is bound to (`kindle =`, a name or a sync id), `None` for
/// any Kindle. A name no Kindle has is an error, so an edition never waits for nobody.
fn paper_owner(cfg: &deckle_digest::Config) -> Result<Option<String>, String> {
    let want = cfg.kindle.trim();
    if want.is_empty() {
        return Ok(None);
    }
    cfg.sync.find_device(want).map(Some).ok_or_else(|| {
        let file = match &cfg.paper {
            None => "digest.toml".to_owned(),
            Some(p) => format!("papers/{}.toml", p.slug),
        };
        format!(
            "{file}: kindle = \"{want}\", but no Kindle is called that; plug it in and name it \
             with `deckle devices name`"
        )
    })
}

/// A paper's `readership.json`: today's file for the main paper, `papers/<slug>/` for another.
fn readership_path(cfg: &deckle_digest::Config) -> PathBuf {
    cfg.state_file("readership.json")
        .unwrap_or_else(deckle_readership::state_path)
}

/// Waits for, then holds, the run lock (`<cache>/digest/run.lock`): two papers due at the same
/// minute convert one after the other, since two Kindle Previewer runs at once are unsafe.
fn run_lock() -> Result<std::fs::File, String> {
    let dir = deckle_core::wordwise::default_cache_dir().join("digest");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("run.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.lock()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(file)
}

/// `digest interests`: the paper's `[[interests]]`, listed, added to or removed from, on the
/// paper's own file (interests design §5.5).
fn digest_interests(
    json: bool,
    path: &Path,
    paper: Option<&str>,
    action: Option<cli::InterestsCommand>,
) -> Result<ExitCode, String> {
    use deckle_digest::config::{InterestConfig, SectionKind};
    use deckle_readership::learn::{self, Learning, Noticed};
    let mut cfg = paper_config(path, paper)?;
    let learning_path = Learning::path_for(&cfg);
    let today = deckle_digest::chrono::Local::now().date_naive();
    // "Bryce Young" or "bryce young": the candidate's key, or its spelling
    let find_key = |learning: &Learning, phrase: &str| -> Option<String> {
        let want = phrase.trim().to_lowercase();
        learning
            .candidates
            .iter()
            .find(|(k, c)| {
                **k == want
                    || c.display.trim().to_lowercase() == want
                    || c.named
                        .as_ref()
                        .is_some_and(|n| n.name.trim().to_lowercase() == want)
            })
            .map(|(k, _)| k.clone())
    };
    match action {
        None => {
            let learning = Learning::load(&learning_path);
            let noticed = learn::suggest(&learning, &cfg, today);
            #[derive(serde::Serialize)]
            struct Out<'a> {
                paper: &'a str,
                interests: &'a [InterestConfig],
                noticed: &'a [Noticed],
            }
            let out = Out {
                paper: cfg.slug(),
                interests: &cfg.interests,
                noticed: &noticed,
            };
            emit(json, &out, || {
                let mut lines: Vec<String> = if cfg.interests.is_empty() {
                    vec!["No interests yet: `deckle digest interests add \"Rust\"`.".to_owned()]
                } else {
                    cfg.interests
                        .iter()
                        .map(|i| {
                            let mut line = format!(
                                "{} — {} · {} stories",
                                i.name,
                                i.search_query(),
                                i.max_items
                            );
                            if !i.also.is_empty() {
                                line.push_str(&format!(" · also {}", i.also.join(", ")));
                            }
                            if !i.not.is_empty() {
                                line.push_str(&format!(" · not {}", i.not.join(", ")));
                            }
                            line
                        })
                        .collect()
                };
                for n in &noticed {
                    lines.push(format!(
                        "noticed: {} — {} (accept with: deckle digest interests accept \"{}\")",
                        n.name, n.why, n.name
                    ));
                }
                lines.join("\n")
            })?;
            Ok(ExitCode::SUCCESS)
        }
        Some(cli::InterestsCommand::Accept { phrase }) => {
            let mut learning = Learning::load(&learning_path);
            let key = find_key(&learning, &phrase)
                .ok_or_else(|| format!("no suggestion \"{phrase}\""))?;
            let entry = learn::accept(&mut learning, &mut cfg, &key, today)?;
            deckle_digest::papers::save(path, &cfg)?;
            learning.save(&learning_path)?;
            emit(json, &entry, || format!("added {}", entry.name))?;
            Ok(ExitCode::SUCCESS)
        }
        Some(cli::InterestsCommand::Dismiss { phrase }) => {
            let mut learning = Learning::load(&learning_path);
            let key = find_key(&learning, &phrase)
                .ok_or_else(|| format!("no suggestion \"{phrase}\""))?;
            learn::dismiss(&mut learning, &key, today)?;
            let name = learning.candidates[&key].display.clone();
            learning.save(&learning_path)?;
            let out = serde_json::json!({ "dismissed": key });
            emit(json, &out, || format!("dismissed {name}"))?;
            Ok(ExitCode::SUCCESS)
        }
        Some(cli::InterestsCommand::Why { name }) => {
            let learning = Learning::load(&learning_path);
            let key = find_key(&learning, &name)
                .ok_or_else(|| format!("nothing noticed about \"{name}\""))?;
            let c = &learning.candidates[&key];
            emit(json, c, || {
                let mut lines = vec![format!("{} — {} hits", c.display, c.hits.len())];
                for h in &c.hits {
                    lines.push(format!(
                        "  {} · {} · {} ({:?})",
                        h.date, h.section, h.title, h.signal
                    ));
                }
                lines.join("\n")
            })?;
            Ok(ExitCode::SUCCESS)
        }
        Some(cli::InterestsCommand::Add {
            name,
            query,
            also,
            not,
            max_items,
        }) => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err("give the interest a name".into());
            }
            if cfg
                .interests
                .iter()
                .any(|i| i.name.trim().eq_ignore_ascii_case(&name))
            {
                return Err(format!("an interest named \"{name}\" already exists"));
            }
            cfg.interests.push(InterestConfig {
                name: name.clone(),
                query: query.unwrap_or_default(),
                also,
                not,
                max_items: max_items.unwrap_or(0),
                ..Default::default()
            });
            let added = cfg.ensure_section(SectionKind::Interests, SectionKind::Feeds);
            deckle_digest::papers::save(path, &cfg)?;
            let out = serde_json::json!({ "added": name, "section_added": added });
            emit(json, &out, || match added {
                true => format!("added {name} (section \"interests\" added to sections)"),
                false => format!("added {name}"),
            })?;
            Ok(ExitCode::SUCCESS)
        }
        Some(cli::InterestsCommand::Remove { name }) => {
            let before = cfg.interests.len();
            cfg.interests
                .retain(|i| !i.name.trim().eq_ignore_ascii_case(name.trim()));
            if cfg.interests.len() == before {
                return Err(format!("no interest named \"{name}\""));
            }
            deckle_digest::papers::save(path, &cfg)?;
            let out = serde_json::json!({ "removed": name });
            emit(json, &out, || format!("removed {name}"))?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `digest papers` (list) and `digest papers new`.
fn digest_papers(
    json: bool,
    path: &Path,
    action: Option<cli::PapersCommand>,
) -> Result<ExitCode, String> {
    load_digest_config(path)?;
    if let Some(cli::PapersCommand::New {
        title,
        slug,
        reader,
    }) = action
    {
        let slug = slug.unwrap_or_else(|| deckle_digest::papers::slug_from_title(&title));
        let file =
            deckle_digest::papers::create(path, &slug, &title, reader.as_deref().unwrap_or(""))?;
        emit(json, &file, || format!("wrote {}", file.display()))?;
        return Ok(ExitCode::SUCCESS);
    }
    let loaded = deckle_digest::papers::load_all(path)?;
    #[derive(serde::Serialize)]
    struct Row {
        slug: String,
        title: String,
        reader: String,
        kindle: String,
        delivery: String,
        large_print: bool,
    }
    let rows: Vec<Row> = loaded
        .papers
        .iter()
        .map(|p| {
            let at = p.schedule.at.as_deref();
            let days: Vec<&str> = p.schedule.days.iter().map(|d| d.key()).collect();
            let when = match (at, days.is_empty()) {
                (Some(at), true) => format!(" at {at}"),
                (Some(at), false) => format!(" ({}) at {at}", days.join(", ")),
                (None, _) => String::new(),
            };
            let how = match (p.email.enabled, p.email.usb_too) {
                (true, false) => "email",
                (true, true) => "email and USB",
                _ => "USB",
            };
            Row {
                slug: p.slug().to_owned(),
                title: p.title.clone(),
                reader: p.reader.clone(),
                kindle: p.kindle.clone(),
                delivery: format!("{how}{when}"),
                large_print: p.large_print,
            }
        })
        .collect();
    #[derive(serde::Serialize)]
    struct Out<'a> {
        papers: &'a [Row],
        notes: &'a [String],
    }
    emit(
        json,
        &Out {
            papers: &rows,
            notes: &loaded.notes,
        },
        || {
            let mut lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let reader = match r.reader.is_empty() {
                        true => String::new(),
                        false => format!(" · for {}", r.reader),
                    };
                    let kindle = match r.kindle.is_empty() {
                        true => "any Kindle".to_owned(),
                        false => r.kindle.clone(),
                    };
                    let kind = if r.large_print {
                        "Large print"
                    } else {
                        "Regular"
                    };
                    format!(
                        "{} · {}{reader} · {kindle} · {} · {kind}",
                        r.slug, r.title, r.delivery
                    )
                })
                .collect();
            lines.extend(loaded.notes.iter().map(|n| format!("note: {n}")));
            lines.join("\n")
        },
    )?;
    Ok(ExitCode::SUCCESS)
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
    paper: Option<String>,
    action: cli::DigestCommand,
) -> Result<ExitCode, String> {
    let explicit_config = config.clone();
    let path = config.unwrap_or_else(deckle_digest::config::default_path);
    let paper = paper.as_deref();
    match action {
        cli::DigestCommand::PressWorker { job } => {
            press_worker::run_job(&job)?;
            Ok(ExitCode::SUCCESS)
        }
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
        cli::DigestCommand::Build {
            output,
            date,
            bundle,
        } => {
            let mut cfg = paper_config(&path, paper)?;
            cfg.force_color = bundle;
            let date = parse_date(date.as_deref())?;
            std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
            let now = deckle_digest::chrono::Utc::now();
            scan_inbox(&cfg, false);
            let net = household_net(&path, &cfg);
            let built = deckle_digest::build_gathered(
                &cfg,
                date,
                now,
                &output,
                &net,
                &gather_options_for(&cfg, date),
            );
            save_jars_noted(&net);
            let (mut report, gathered) = built?;
            record_naming(&cfg, &gathered);
            report.notes.extend(archive_to_morgue(
                &cfg,
                date,
                &report.epub,
                &gathered.without_nudges(),
            ));
            if bundle {
                let path = deckle_digest::bundle::write(
                    &gathered.without_nudges(),
                    &report,
                    &output,
                    true,
                    now,
                )?;
                report
                    .bundles
                    .push(deckle_digest::BundleBuild { books: true, path });
            }
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
                for b in &report.bundles {
                    text.push_str(&format!("\n      bundle -> {}", b.path.display()));
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
            let mut cfg = paper_config(&path, paper)?;
            let date = parse_date(date.as_deref())?;
            // A paper bound to a Kindle names one that exists, before any work.
            let bound = paper_owner(&cfg)?;
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
            // Two papers due at the same minute convert one after the other (many papers §5).
            let _lock = run_lock()?;
            let mut no_kindle = None;
            // Export folders need no Previewer (other readers design §6): without one, the
            // paper still goes to them, and nothing is converted or queued for a Kindle.
            let located = match previewer::locate(pv.as_deref()) {
                Ok(l) => Some(l),
                Err(e)
                    if to == "device"
                        && (!cfg.export.is_empty() || reader_seen() || cfg.remarkable.enabled) =>
                {
                    let to: Vec<&str> = [
                        (!cfg.export.is_empty()).then_some("the export folders"),
                        reader_seen().then_some("the e-reader outbox"),
                        cfg.remarkable.enabled.then_some("the reMarkable"),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    no_kindle = Some(format!(
                        "no Kindle copy: {e}; the paper went to {} only",
                        to.join(" and ")
                    ));
                    // A long read with no book to become prints in the paper, unless an
                    // e-reader or an export folder takes it as a book.
                    if !reader_seen()
                        && !cfg.export.iter().any(|x| x.books)
                        && !(cfg.remarkable.enabled && cfg.remarkable.books)
                    {
                        cfg.reading.long_read_words = usize::MAX;
                    }
                    None
                }
                Err(e) => return Err(e.to_string()),
            };
            // A folder target must exist (and is created) before any work; `device` may be
            // absent: the edition then waits in the outbox.
            let folder = (to != "device").then(|| resolve_target(&to)).transpose()?;
            let work = cfg.work_dir(&deckle_core::wordwise::default_cache_dir());
            let converted = work.join("kfx");
            std::fs::create_dir_all(&converted).map_err(|e| e.to_string())?;
            let now = deckle_digest::chrono::Utc::now();
            scan_inbox(&cfg, false);
            let net = household_net(&path, &cfg);
            let first = deckle_digest::build_gathered(
                &cfg,
                date,
                now,
                &work,
                &net,
                &gather_options_for(&cfg, date),
            );
            save_jars_noted(&net);
            let (mut built, mut gathered) = first?;
            record_naming(&cfg, &gathered);
            let opts = paper_options(
                located.as_ref().map(|l| l.path.clone()),
                keep_work_dir,
                &converted,
            );
            let (mut long_reads, failed) = match located {
                Some(_) => prepare_long_reads(&built, &opts),
                None => Default::default(),
            };
            let mut notes: Vec<String> = Vec::new();
            if !failed.is_empty() {
                for (id, why) in &failed {
                    notes.push(format!(
                        "long read {id} printed in the paper instead: {why}"
                    ));
                }
                cfg.reading.inline = failed.into_iter().map(|(id, _)| id).collect();
                let again = deckle_digest::build_gathered(
                    &cfg,
                    date,
                    now,
                    &work,
                    &net,
                    &gather_options_for(&cfg, date),
                );
                save_jars_noted(&net);
                (built, gathered) = again?;
                record_naming(&cfg, &gathered);
                long_reads = prepare_long_reads(&built, &opts).0;
            }
            // Keep the paper in the Morgue whatever happens to its conversion.
            let note = archive_to_morgue(&cfg, date, &built.epub, &gathered.without_nudges());
            built.notes.extend(note);
            built.notes.extend(no_kindle);
            // Export folders, like email, belong to the morning run, not to `--to <folder>`.
            // A paper bound to one Kindle goes only there (many papers §4).
            let exports = match (&folder, &bound) {
                (None, None) => deckle_digest::export::export_all(&cfg.export, date, &built),
                _ => Vec::new(),
            };
            let exported = exports.iter().any(|x| x.error.is_none());
            // The EPUB outbox, for a Kobo or a PocketBook: when one has been seen, or there is
            // no Previewer.
            let kobo_queue = epub_queue();
            let epub_queued = folder.is_none()
                && bound.is_none()
                && epub_wanted(located.is_some())
                && match queue_epubs(&kobo_queue, &built, date) {
                    Ok(()) => true,
                    Err(e) => {
                        built.notes.push(format!("not queued for an e-reader: {e}"));
                        false
                    }
                };
            // Email belongs to the morning run to the Kindle, not to `--to <folder>` (email
            // design §7). A paper that went out by email skips the outbox unless `usb_too`.
            let (email, to_usb) = match &folder {
                Some(_) => (None, true),
                None => email_paper(
                    &cfg.email,
                    cfg.paper.as_ref().map(|_| cfg.title.as_str()),
                    &deckle_digest::edition_stem(&cfg, date),
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
            // Converted only when it goes by USB, to a Kindle there is a Previewer for.
            let report = (to_usb && located.is_some()).then(|| run(&built.epub, &opts));
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
                record_layout(k, date, &built, &cfg);
                if is_today(date) {
                    let note = keep_kfx_in_morgue(&cfg, date, k);
                    built.notes.extend(note);
                }
            }
            // The reMarkable's PDF, from its own outbox.
            let rm_queue = remarkable_queue();
            let rm_queued = folder.is_none()
                && bound.is_none()
                && cfg.remarkable.enabled
                && match queue_remarkable(&rm_queue, &cfg, &built, date) {
                    Ok(()) => true,
                    Err(e) => {
                        built
                            .notes
                            .push(format!("not queued for the reMarkable: {e}"));
                        false
                    }
                };
            if located.is_none() {
                // Nothing for a Kindle: the export folders, the e-reader or reMarkable outbox or
                // the email were the delivery.
                if exported || epub_queued || rm_queued || !to_usb {
                    reading_queue(&cfg)
                        .commit(date, &reading_outcomes)
                        .map_err(|e| format!("reading queue: {e}"))?;
                }
            } else if kfx.is_some() || !to_usb {
                let name = deckle_digest::edition_file(&cfg, date);
                match &folder {
                    Some(target) => {
                        let one = deckle_core::queue::Queue::new(work.join("to-folder"));
                        let _ = std::fs::remove_dir_all(&one.dir);
                        queue_edition(&one, kfx, &name, None, &long_reads, &[], None)?;
                        outcome.flush = Some(
                            one.flush(target, Some(&digest_prune(&cfg)))
                                .map_err(|e| e.to_string())?,
                        );
                    }
                    None => {
                        let exempt = match cfg.paper {
                            // an edition sent again from the Morgue outlives the morning's paper
                            None => morgue_exempt(),
                            Some(_) => Vec::new(),
                        };
                        queue_edition(
                            &queue,
                            kfx,
                            &name,
                            Some(&deckle_digest::edition_prefix(&cfg)),
                            &long_reads,
                            &exempt,
                            bound.as_deref(),
                        )?;
                        outcome = push_queued(&path, &queue, &all_papers(&path)?, detect_kindle())?;
                    }
                }
                // Only a run that shipped (or queued) the edition uses up the saved articles.
                reading_queue(&cfg)
                    .commit(date, &reading_outcomes)
                    .map_err(|e| format!("reading queue: {e}"))?;
            }
            // A reMarkable plugged in now takes the PDF at once.
            let remarkable = rm_queued.then(|| push_remarkable(&cfg)).flatten();
            // A Kobo or a PocketBook plugged in now takes its copy at once.
            let device = detect_reader().filter(|_| folder.is_none() && epub_queued);
            let reader = match push_reader(&kobo_queue, &cfg, device.as_ref()) {
                Ok(o) => o,
                Err(e) => {
                    built.notes.push(format!("e-reader: {e}"));
                    None
                }
            };
            #[derive(serde::Serialize)]
            struct DigestRun<'a> {
                build: &'a deckle_digest::BuildReport,
                /// `null` when the paper went by email only.
                run: Option<&'a deckle_core::pipeline::RunReport>,
                outcome: &'a DigestOutcome,
                #[serde(flatten)]
                email: &'a EmailRun,
                #[serde(skip_serializing_if = "<[_]>::is_empty")]
                exports: &'a [deckle_digest::export::ExportReport],
                /// The paper queued for a Kobo or a PocketBook (`outbox-epub`).
                #[serde(skip_serializing_if = "std::ops::Not::not")]
                epub_queued: bool,
                /// A Kobo or a PocketBook was connected and took it.
                #[serde(skip_serializing_if = "Option::is_none")]
                reader: Option<&'a DigestOutcome>,
                /// The paper's PDF queued for a reMarkable (`outbox-remarkable`).
                #[serde(skip_serializing_if = "std::ops::Not::not")]
                remarkable_queued: bool,
                /// A reMarkable answered and took it.
                #[serde(skip_serializing_if = "Option::is_none")]
                remarkable: Option<&'a deckle_core::sync::RemarkablePush>,
            }
            let all = DigestRun {
                build: &built,
                run: report.as_ref(),
                outcome: &outcome,
                email: &email,
                exports: &exports,
                epub_queued,
                reader: reader.as_ref(),
                remarkable_queued: rm_queued,
                remarkable: remarkable.as_ref(),
            };
            let failed = report.as_ref().and_then(|r| r.error.as_ref());
            emit(json, &all, || {
                let head = match failed {
                    Some(e) => format!("FAIL  {}\n      {e}", built.title),
                    None => format!("OK    {}", built.title),
                };
                let mut text = format!("{head}\n{}", section_lines(&built));
                let exported = deckle_digest::export::lines(&exports);
                let usb = match located {
                    Some(_) => outcome.describe(&queue),
                    None => String::new(),
                };
                let reader_lines = match (&reader, &device) {
                    (Some(o), Some(d)) => {
                        format!("      {}:\n{}", d.name(), o.describe(&kobo_queue))
                    }
                    _ if epub_queued => "      queued for a Kobo or PocketBook: it goes over when \
                                         one is plugged in"
                        .to_owned(),
                    _ => String::new(),
                };
                let rm_lines = match &remarkable {
                    Some(p) => remarkable_lines(p),
                    None if rm_queued => "      queued for the reMarkable: it goes over when it's \
                                          plugged in with its USB web interface on"
                        .to_owned(),
                    None => String::new(),
                };
                for part in [email.lines(), exported, usb, reader_lines, rm_lines] {
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
        cli::DigestCommand::Schedule { at, days, off } => {
            use deckle_digest::schedule;
            let extra = paper.filter(|p| *p != deckle_digest::papers::MAIN);
            if off {
                let done = match extra {
                    // One paper: its job goes, and `at` leaves its file.
                    Some(slug) => {
                        let mut cfg = paper_config(&path, Some(slug))?;
                        cfg.schedule = Default::default();
                        deckle_digest::papers::save(&path, &cfg)?;
                        schedule::remove_paper(slug)?
                    }
                    None => schedule::remove()?,
                };
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
                emit(json, &st, || {
                    if st.papers.is_empty() && !st.watch {
                        return "not scheduled; `deckle digest schedule --at 05:30` sets it up"
                            .into();
                    }
                    let mut lines: Vec<String> = st
                        .papers
                        .iter()
                        .map(|p| {
                            let days: Vec<&str> = p.days.iter().map(|d| d.key()).collect();
                            let when = match (&p.at, days.is_empty()) {
                                (Some(at), true) => format!("daily at {at}"),
                                (Some(at), false) => format!("{} at {at}", days.join(", ")),
                                (None, _) => "installed".into(),
                            };
                            format!("edition {}: {when}", p.slug)
                        })
                        .collect();
                    lines.push(format!("watcher: {}", if st.watch { "on" } else { "off" }));
                    if let Some(e) = &st.exe {
                        lines.push(format!("runs: {e}"));
                    }
                    lines.join("\n")
                })?;
                return Ok(ExitCode::SUCCESS);
            };
            schedule::parse_at(&at)?;
            let days: Vec<deckle_digest::config::Day> = match &days {
                None => Vec::new(),
                Some(list) => list
                    .split(',')
                    .filter(|d| !d.trim().is_empty())
                    .map(|d| {
                        deckle_digest::config::Day::parse(d)
                            .ok_or_else(|| format!("--days: {d:?} isn't a day (mon … sun)"))
                    })
                    .collect::<Result<_, _>>()?,
            };
            // Saved in the paper's own file, then every paper's job installed from the files.
            let mut cfg = paper_config(&path, paper)?;
            cfg.schedule.at = Some(at.clone());
            cfg.schedule.days = days.clone();
            deckle_digest::papers::save(&path, &cfg)?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let exe = exe.canonicalize().unwrap_or(exe);
            if let Some(w) = schedule::unstable_exe(&exe) {
                eprintln!("warning: {w}");
            }
            let config = explicit_config.map(|c| c.canonicalize().unwrap_or(c));
            let jobs = schedule::jobs_for(&all_papers(&path)?, &exe, config.as_deref());
            let done = schedule::install_all(&jobs, &exe, config.as_deref())?;
            let when = match days.is_empty() {
                true => "daily".to_owned(),
                false => days.iter().map(|d| d.key()).collect::<Vec<_>>().join(", "),
            };
            emit(json, &done, || {
                format!(
                    "{}\n{} is built {when} at {at} and goes to its Kindle when it's plugged in.",
                    done.join("\n"),
                    cfg.title
                )
            })?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Readership { action } => digest_readership(json, &path, paper, action),
        cli::DigestCommand::Papers { action } => digest_papers(json, &path, action),
        cli::DigestCommand::Interests { action } => digest_interests(json, &path, paper, action),
        cli::DigestCommand::Lint { page } => {
            // The household's sessions when there's a config, so a subscriber page reads.
            if path.exists() {
                let cfg = load_digest_config(&path)?;
                let net = household_net(&path, &cfg);
                let r = digest_lint(json, &page, &net, &cfg.subscriptions);
                save_jars_noted(&net);
                r
            } else {
                digest_lint(json, &page, &deckle_digest::Http, &[])
            }
        }
        cli::DigestCommand::Subscribe {
            site,
            username,
            stdin,
            test,
            forget,
        } => digest_subscribe(json, &path, site, username, stdin, test, forget),
        cli::DigestCommand::Next { fetch } => {
            let cfg = load_digest_config(&path)?;
            let now =
                deckle_digest::chrono::Utc::now().with_timezone(&deckle_digest::tz::local_tz());
            if fetch {
                let cache_path = next_cache();
                let mut cache = deckle_digest::next::live::LiveCache::load(&cache_path);
                let net = household_net(&path, &cfg);
                deckle_digest::next::live::refresh(&cfg, &mut cache, now.to_utc(), &net);
                save_jars_noted(&net);
                cache.save(&cache_path)?;
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
        cli::DigestCommand::Puzzle => digest_puzzle(json, &paper_config(&path, paper)?),
        cli::DigestCommand::Shelf { forget } => digest_shelf(json, &path, forget),
        cli::DigestCommand::Morgue { action } => {
            main_paper_only(paper, "The Morgue commands")?;
            digest_morgue(json, &path, action)
        }
        cli::DigestCommand::Email { action } => {
            let cfg = load_digest_config(&path)?.email;
            digest_email(json, &cfg, action)
        }
        cli::DigestCommand::Push { to } => {
            let papers = all_papers(&path)?;
            let cfg = papers[0].clone();
            let queue = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
            let target = match to.as_str() {
                "device" => detect_kindle(),
                folder => Some(resolve_target(folder)?),
            };
            // A Kobo or a PocketBook plugged in takes the EPUB outbox (other readers design §8).
            let kobo_queue = epub_queue();
            let reader = match to.as_str() {
                "device" => detect_reader(),
                _ => None,
            };
            // A reMarkable that answers takes its outbox too (other readers design §9).
            let remarkable = match to.as_str() {
                "device" => push_remarkable(&cfg),
                _ => None,
            };
            if target.is_none()
                && let Some(reader) = &reader
            {
                let mut outcome = push_reader(&kobo_queue, &cfg, Some(reader))?.unwrap_or_default();
                outcome.remarkable = remarkable;
                emit(json, &outcome, || match &outcome.flush {
                    Some(_) => outcome.describe(&kobo_queue),
                    None => {
                        let none = format!("      nothing queued for the {}", reader.name());
                        match &outcome.remarkable {
                            Some(p) => format!("{none}\n{}", remarkable_lines(p)),
                            None => none,
                        }
                    }
                })?;
                return Ok(ExitCode::SUCCESS);
            }
            let mut outcome = push_queued(&path, &queue, &papers, target)?;
            outcome.remarkable = remarkable;
            emit(json, &outcome, || outcome.describe(&queue))?;
            Ok(ExitCode::SUCCESS)
        }
        cli::DigestCommand::Watch { interval } => {
            let mut papers = all_papers(&path)?;
            let mut cfg = papers[0].clone();
            let mut papers_error: Option<String> = None;
            let mut press_supervisor = press_worker::PressSupervisor::new(path.clone())?;
            let mut press_error = None;
            let queue = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
            eprintln!(
                "watching for a Kindle, a Kobo, a PocketBook or a reMarkable every {interval}s; \
                 outbox {}",
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
                    // Save recipe drops its link file in the Inbox's Recipes folder, where
                    // the scan below adds it to the cookbook.
                    let recipes = deckle_digest::inbox::expand_home(&cfg.reading.inbox)
                        .join(deckle_digest::inbox::RECIPES_FOLDER);
                    std::thread::spawn(move || {
                        if let Err(e) = deckle_digest::bookmarklet::serve(reading, token, recipes) {
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
            // A Kobo or a PocketBook has its own outbox and connection (other readers design §8).
            let kobo_queue = epub_queue();
            let mut reader_watcher = deckle_core::sync::Watcher::default();
            // A reMarkable answers on its USB link; one push per connection, as for the others.
            let rm_queue = remarkable_queue();
            let mut rm_watcher = deckle_core::sync::Watcher::default();
            // The Kitchen Book, rebuilt when it changes and offered to each Kindle that plugs in
            // (cookbook design §9). Each error is said once, not every tick.
            let cookbook = deckle_digest::cookbook::store::Store::new(
                deckle_digest::cookbook::store::Store::default_dir(),
            );
            let (mut build_error, mut offer_error) = (None, None);
            loop {
                // A new or edited paper applies without a restart; a file that fails keeps the
                // last good set, and its error is said once (many papers plan).
                match deckle_digest::papers::load_all(&path) {
                    Ok(l) => {
                        papers = l.papers;
                        cfg = papers[0].clone();
                        papers_error = None;
                    }
                    Err(e) if papers_error.as_ref() != Some(&e) => {
                        eprintln!("papers: {e}");
                        papers_error = Some(e);
                    }
                    Err(_) => {}
                }
                say_once(
                    &mut press_error,
                    press_supervisor.tick(&papers, deckle_digest::chrono::Utc::now()),
                );
                scan_inbox(&cfg, true);
                say_once(
                    &mut build_error,
                    cookbook_delivery::rebuild_if_dirty(&cookbook, None).map(|built| {
                        if let Some(b) = built {
                            eprintln!(
                                "cookbook: rebuilt with {} {}, sent when each Kindle connects",
                                b.recipes,
                                if b.recipes == 1 { "recipe" } else { "recipes" }
                            );
                        }
                    }),
                );
                let kindle = detect_kindle();
                let id = kindle.as_ref().map(deckle_core::sync::device_id);
                let actions = id
                    .as_deref()
                    .map(|id| sync_actions(&cfg, id))
                    .unwrap_or_default();
                // Queued before the outbox is read, so it rides this connection's push.
                let cookbook_kindle = kindle.as_ref().and_then(cookbook_delivery::kindle_id);
                if let Some(id) = &cookbook_kindle {
                    let on = actions.push && cfg.sync.for_device(id).cookbook;
                    say_once(
                        &mut offer_error,
                        cookbook_delivery::offer(&cookbook, &queue, id, on).map(|_| ()),
                    );
                }
                let pending = queue.pending();
                let plan = watcher.tick(id.as_deref(), &pending, actions);
                if let (Some(target), false) = (&kindle, plan.is_empty()) {
                    // The build that goes now, read before the push takes the entry.
                    let pushed = cookbook_kindle
                        .as_deref()
                        .and_then(|k| cookbook_delivery::queued_build(&queue, k));
                    match sync_to(&path, &queue, &papers, target, plan) {
                        Ok(o) => {
                            if let (Some(flush), Some(k)) = (&o.flush, &cookbook_kindle)
                                && let Err(e) = cookbook_delivery::record(
                                    &cookbook,
                                    k,
                                    flush,
                                    pushed.as_deref(),
                                )
                            {
                                eprintln!("cookbook: {e}");
                            }
                            emit(json, &o, || o.describe(&queue))?
                        }
                        Err(e) => eprintln!("error: {e}"),
                    }
                }
                let reader = detect_reader();
                let reader_id = reader.as_ref().map(deckle_core::reader::Reader::id);
                let reader_actions = reader_id
                    .as_deref()
                    .map(|id| deckle_core::sync::Actions {
                        readership: false,
                        ..sync_actions(&cfg, id)
                    })
                    .unwrap_or_default();
                let plan = reader_watcher.tick(
                    reader_id.as_deref(),
                    &kobo_queue.pending(),
                    reader_actions,
                );
                if let (Some(k), false) = (&reader, plan.is_empty()) {
                    match reader_sync(&kobo_queue, &cfg, k, plan) {
                        Ok(o) => emit(json, &o, || o.describe(&kobo_queue))?,
                        Err(e) => eprintln!("error: {e}"),
                    }
                }
                if cfg.remarkable.enabled {
                    let pending = rm_queue.pending();
                    // Asked only while something waits: an absent tablet costs a second a tick.
                    let tablet = (!pending.is_empty())
                        .then(deckle_core::remarkable::detect)
                        .flatten();
                    let id = tablet.as_ref().map(deckle_core::remarkable::Remarkable::id);
                    let push_only = deckle_core::sync::Actions {
                        push: true,
                        prune: false,
                        clippings: false,
                        readership: false,
                        shelf: false,
                    };
                    let plan = rm_watcher.tick(id.as_deref(), &pending, push_only);
                    if let (Some(r), true) = (&tablet, plan.push) {
                        let p = deckle_core::sync::run_remarkable(&rm_queue, r);
                        emit(json, &p, || remarkable_lines(&p))?;
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(interval.max(1)));
            }
        }
    }
}

#[cfg(test)]
mod prune_tests {
    use super::*;

    /// The Daily's prefix is `deckle-daily-` and the date after it, which the Kitchen Book's
    /// name never matches; it's exempt by name all the same (cookbook design §9).
    #[test]
    fn the_dailys_prune_exempts_the_kitchen_book() {
        let device = deckle_digest::cookbook::book::DEVICE_NAME;
        let paper = deckle_digest::Config {
            paper: Some(deckle_digest::config::PaperRef {
                slug: "kid-courier".into(),
                state_dir: PathBuf::from("/nonexistent"),
            }),
            ..Default::default()
        };
        let prune = digest_prune(&paper);
        assert_eq!(prune.exempt, [device]);
        let names = [
            device.to_owned(),
            "deckle-daily-kid-courier-2026-10-01.kfx".to_owned(),
            "deckle-daily-kid-courier-2026-10-02.kfx".to_owned(),
        ];
        assert_eq!(
            deckle_core::deliver::prunable_except(names, &prune.prefix, 1, &prune.exempt),
            ["deckle-daily-kid-courier-2026-10-01.kfx"]
        );
        // The main paper's own exemptions (the Morgue's) keep it too.
        assert!(
            digest_prune(&deckle_digest::Config::default())
                .exempt
                .iter()
                .any(|n| n == device)
        );
    }
}

/// Says a cookbook error once: again only when it changes, or after a success.
fn say_once(said: &mut Option<String>, r: Result<(), String>) {
    match r {
        Ok(()) => *said = None,
        Err(e) if said.as_ref() != Some(&e) => {
            eprintln!("cookbook: {e}");
            *said = Some(e);
        }
        Err(_) => {}
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
    /// What went to a reMarkable in the same run (`[remarkable]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    remarkable: Option<deckle_core::sync::RemarkablePush>,
    /// Books left queued for other Kindles: (queue name, the Kindle's name or id).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    waiting: Vec<(String, String)>,
    /// Why the Kitchen Book wasn't offered to this Kindle, or its delivery wasn't recorded
    /// (also said on stderr).
    #[serde(skip_serializing_if = "Option::is_none")]
    cookbook_error: Option<String>,
}

/// "2 books wait for Mom's Kindle", one line per Kindle, from (queue name, Kindle) pairs.
fn waiting_lines(waiting: &[(String, String)]) -> Vec<String> {
    let mut by: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (_, kindle) in waiting {
        *by.entry(kindle.as_str()).or_default() += 1;
    }
    by.into_iter()
        .map(|(kindle, n)| {
            let books = if n == 1 { "book waits" } else { "books wait" };
            format!("{n} {books} for {kindle}")
        })
        .collect()
}

/// One push to a reMarkable, as lines of the run's report.
fn remarkable_lines(p: &deckle_core::sync::RemarkablePush) -> String {
    let mut lines: Vec<String> = p
        .uploaded
        .iter()
        .map(|f| format!("      reMarkable -> {f}"))
        .collect();
    lines.extend(
        p.already
            .iter()
            .map(|f| format!("      reMarkable already has {f}")),
    );
    lines.extend(p.errors.iter().map(|e| format!("      reMarkable: {e}")));
    lines.join("\n")
}

impl DigestOutcome {
    fn describe(&self, queue: &deckle_core::queue::Queue) -> String {
        let main = self.describe_device(queue);
        match &self.remarkable {
            Some(p) if !main.is_empty() => format!("{main}\n{}", remarkable_lines(p)),
            Some(p) => remarkable_lines(p),
            None => main,
        }
    }

    fn describe_device(&self, queue: &deckle_core::queue::Queue) -> String {
        match &self.flush {
            Some(f)
                if f.delivered.is_empty()
                    && f.pruned.is_empty()
                    && self.highlights.is_none()
                    && self.readership.is_none()
                    && self.shelf.is_none()
                    && f.shelf_error.is_none()
                    && f.positions_error.is_none()
                    && self.waiting.is_empty() =>
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
                lines.extend(
                    waiting_lines(&self.waiting)
                        .into_iter()
                        .map(|l| format!("      {l}")),
                );
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

/// The household's subscriptions as a net: passwords from the keychain (or the environment),
/// sessions kept beside the config.
fn household_net(path: &Path, cfg: &deckle_digest::Config) -> deckle_digest::SessionNet {
    let net = household_net_quiet(path, cfg);
    // A jar that wouldn't load: the run goes on signed out, and says so.
    for s in net.status() {
        if let Some(note) = &s.jar_note {
            eprintln!("{}: {note}", s.site);
        }
    }
    net
}

/// [`household_net`] without the jar notes on stderr, for a caller that shows them itself.
fn household_net_quiet(path: &Path, cfg: &deckle_digest::Config) -> deckle_digest::SessionNet {
    deckle_digest::SessionNet::from_config(
        path,
        &cfg.subscriptions,
        &deckle_digest::secret::SystemKeyring,
        &|k| std::env::var(k).ok(),
    )
}

/// Keeps the sessions after a run and says on stderr what couldn't be kept.
fn save_jars_noted(net: &deckle_digest::SessionNet) {
    for note in net.save_jars() {
        eprintln!("{note}");
    }
}

fn session_text(state: &deckle_digest::session::SessionState) -> String {
    use deckle_digest::session::SessionState as S;
    match state {
        S::NotTried => "not tried".into(),
        S::SignedIn { service: Some(s) } => format!("signed in ({s})"),
        S::SignedIn { service: None } => "signed in".into(),
        S::LoginFailed(m) => format!("sign-in failed: {m}"),
        S::NoAdapter => "no adapter".into(),
        S::NoPassword(m) => format!("no password: {m}"),
        S::EntitlementUnknown(m) => m.clone(),
    }
}

/// `digest subscribe [site]`: list the subscriptions, or add, test or forget one.
fn digest_subscribe(
    json: bool,
    path: &Path,
    site: Option<String>,
    username: Option<String>,
    stdin: bool,
    test: bool,
    forget: bool,
) -> Result<ExitCode, String> {
    use deckle_digest::config::SubscriptionConfig;
    use deckle_digest::secret::{self, SystemKeyring};
    use deckle_digest::zeroize::Zeroizing;
    let mut cfg = load_digest_config(path)?;
    let Some(raw) = site else {
        if username.is_some() || stdin || test || forget {
            return Err("name the site, e.g. `deckle digest subscribe yourpaper.com`".into());
        }
        let statuses = household_net_quiet(path, &cfg).status();
        emit(json, &statuses, || {
            if statuses.is_empty() {
                return "no subscriptions; add one with `deckle digest subscribe <site> --username <you>`".to_owned();
            }
            statuses
                .iter()
                .map(|s| {
                    let mut line = format!(
                        "{}  {}  password: {}  session: {}",
                        s.site,
                        s.username,
                        if s.has_password { "yes" } else { "no" },
                        session_text(&s.state)
                    );
                    if let Some(note) = &s.jar_note {
                        line.push_str(&format!("\n  saved session: {note}"));
                    }
                    line
                })
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        return Ok(ExitCode::SUCCESS);
    };
    if test && forget {
        return Err("--test and --forget don't go together".into());
    }
    let site = SubscriptionConfig::normalized_site(&raw)?;
    let existing = cfg.subscriptions.iter().position(|s| s.site == site);
    if forget {
        let gone =
            deckle_digest::session::forget_subscription(path, &mut cfg, &site, &SystemKeyring)?;
        for note in &gone.notes {
            eprintln!("{note}");
        }
        emit(json, &gone, || {
            format!(
                "removed the subscription to {site}{}",
                if gone.keychain_entry {
                    " and its keychain entry"
                } else {
                    ""
                }
            )
        })?;
        return Ok(ExitCode::SUCCESS);
    }
    if test {
        let i = existing.ok_or_else(|| format!("no subscription for {site}"))?;
        let net = household_net(path, &cfg);
        let state = net.sign_in(&site);
        let user = cfg.subscriptions[i].username.clone();
        save_jars_noted(&net);
        return report_sign_in(json, &site, &user, &state);
    }
    let username = username
        .map(|u| u.trim().to_owned())
        .filter(|u| !u.is_empty());
    let sub = match (existing, username) {
        (None, None) => {
            return Err(format!(
                "a new subscription needs --username (the account you sign in to {site} with)"
            ));
        }
        (None, Some(username)) => {
            let sub = SubscriptionConfig {
                site: site.clone(),
                username,
                kind: None,
            };
            cfg.subscriptions.push(sub.clone());
            sub
        }
        (Some(i), Some(username)) => {
            cfg.subscriptions[i].username = username;
            cfg.subscriptions[i].clone()
        }
        (Some(i), None) => cfg.subscriptions[i].clone(),
    };
    let password = Zeroizing::new(if stdin {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| format!("stdin: {e}"))?;
        let pw = line.trim_end_matches(['\r', '\n']).to_owned();
        deckle_digest::zeroize::Zeroize::zeroize(&mut line);
        pw
    } else {
        rpassword::prompt_password(format!("Password for {} at {site}: ", sub.username))
            .map_err(|e| format!("couldn't read the password: {e}"))?
    });
    if password.is_empty() {
        return Err("the password is empty".into());
    }
    deckle_digest::config::save(path, &cfg)?;
    secret::set_site(&sub, &SystemKeyring, &password).map_err(|e| e.to_string())?;
    let net = household_net(path, &cfg);
    let state = net.sign_in(&site);
    save_jars_noted(&net);
    if state == deckle_digest::session::SessionState::NoAdapter {
        // The account is saved; there is just nothing to sign in with yet.
        emit(json, &state, || {
            format!(
                "saved {} at {site}. Deckle doesn't know how {site} signs in yet",
                sub.username
            )
        })?;
        return Ok(ExitCode::SUCCESS);
    }
    report_sign_in(json, &site, &sub.username, &state)
}

fn report_sign_in(
    json: bool,
    site: &str,
    username: &str,
    state: &deckle_digest::session::SessionState,
) -> Result<ExitCode, String> {
    use deckle_digest::session::SessionState as S;
    emit(json, state, || {
        format!("{site}  {username}  session: {}", session_text(state))
    })?;
    Ok(match state {
        S::SignedIn { .. } => ExitCode::SUCCESS,
        _ => ExitCode::from(1),
    })
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
#[allow(clippy::too_many_arguments)]
fn email_paper<M: deckle_digest::mail::Mailer>(
    cfg: &deckle_digest::config::EmailConfig,
    title: Option<&str>,
    stem: &str,
    date: deckle_digest::chrono::NaiveDate,
    epub: &Path,
    keys: &dyn deckle_digest::secret::Keyring,
    env: &dyn Fn(&str) -> Option<String>,
    connect: impl FnOnce(deckle_digest::zeroize::Zeroizing<String>) -> M,
) -> (Option<deckle_digest::mail::EmailReport>, bool) {
    let report = deckle_digest::mail::email_paper(cfg, title, stem, date, epub, keys, env, connect);
    let to_usb = report.as_ref().is_none_or(|r| r.to_usb(cfg));
    (report, to_usb)
}

/// Puts the paper's KFX (`None`: it went by email) and the long reads in `queue`, meant for
/// the Kindle `owner` when the paper is bound to one (many papers §4).
fn queue_edition(
    queue: &deckle_core::queue::Queue,
    paper: Option<&Path>,
    name: &str,
    replaces: Option<&str>,
    long_reads: &[LongRead],
    keep: &[String],
    owner: Option<&str>,
) -> Result<(), String> {
    if let Some(kfx) = paper {
        queue
            .add_keeping_for(kfx, name, replaces, keep, owner)
            .map_err(|e| e.to_string())?;
    }
    for (n, p, fallback) in long_reads {
        queue
            .add_prepared_for(p, fallback, n, None, owner)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The paper's build: into `converted`, always PDOC, because the cover is the front page and
/// must always show (design §5).
fn paper_options(previewer: Option<PathBuf>, keep_work_dir: bool, converted: &Path) -> RunOptions {
    RunOptions {
        preset: None,
        cover: Default::default(),
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
        originals: None,
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
        cover: Default::default(),
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
        prefix: deckle_digest::edition_prefix(cfg),
        keep: cfg.keep.max(1),
        // Editions sent again from the Morgue are the main paper's. The Kitchen Book is never
        // pruned (cookbook design §9), whatever the prefix.
        exempt: match cfg.paper {
            None => morgue_exempt(),
            Some(_) => Vec::new(),
        }
        .into_iter()
        .chain([deckle_digest::cookbook::book::DEVICE_NAME.to_owned()])
        .collect(),
    }
}

/// One prune per paper that goes to the Kindle `id`: those for any Kindle and those bound to
/// it, each by its own prefix and `keep` (many papers §4).
fn prunes_for(papers: &[deckle_digest::Config], id: &str) -> Vec<deckle_core::queue::Prune> {
    papers
        .iter()
        .filter(|p| match paper_owner(p) {
            Ok(None) => true,
            Ok(Some(owner)) => deckle_core::sync::same_kindle(&owner, id),
            Err(_) => false,
        })
        .map(digest_prune)
        .collect()
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

/// The first connected e-reader on a drive that isn't a Kindle: a Kobo, else a PocketBook
/// (other readers design §8).
fn detect_reader() -> Option<deckle_core::reader::Reader> {
    deckle_core::reader::detect().into_iter().next()
}

/// What waits for a reMarkable: the paper's PDF and the long reads' EPUBs.
fn remarkable_queue() -> deckle_core::queue::Queue {
    deckle_core::queue::Queue::new(deckle_core::queue::Queue::remarkable_dir())
}

/// Puts the paper's PDF for the reMarkable's screen (and, with `books`, the long reads) in its
/// outbox.
fn queue_remarkable(
    queue: &deckle_core::queue::Queue,
    cfg: &deckle_digest::Config,
    built: &deckle_digest::BuildReport,
    date: deckle_digest::chrono::NaiveDate,
) -> Result<(), String> {
    let rm = &cfg.remarkable;
    let pdf = built
        .pdf(rm.screen, rm.photos())
        .ok_or("the PDF edition wasn't built")?;
    let name = format!("{}{date}.pdf", built.prefix);
    queue
        .add(pdf, &name, Some(&built.prefix))
        .map_err(|e| e.to_string())?;
    if rm.books {
        for p in &built.long_reads {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            queue.add(p, &name, None).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Uploads the reMarkable outbox when `[remarkable]` is on and the tablet answers.
fn push_remarkable(cfg: &deckle_digest::Config) -> Option<deckle_core::sync::RemarkablePush> {
    if !cfg.remarkable.enabled {
        return None;
    }
    let queue = remarkable_queue();
    if queue.pending().is_empty() {
        return None;
    }
    let r = deckle_core::remarkable::detect()?;
    Some(deckle_core::sync::run_remarkable(&queue, &r))
}

/// What waits for a Kobo or a PocketBook: EPUBs, made KEPUBs for a Kobo as they go over.
fn epub_queue() -> deckle_core::queue::Queue {
    deckle_core::queue::Queue::new(deckle_core::queue::Queue::epub_dir())
}

/// Whether the paper is also queued as an EPUB: a Kobo or a PocketBook has been seen, or there
/// is no Previewer to make the Kindle's copy.
fn epub_wanted(previewer_found: bool) -> bool {
    !previewer_found || reader_seen()
}

/// A Kobo or a PocketBook has been plugged in here before, or is now.
fn reader_seen() -> bool {
    deckle_core::reader::any_seen(&deckle_core::reader::seen_file()) || detect_reader().is_some()
}

/// Puts the paper's EPUB and the long reads' in the EPUB outbox, for a Kobo or a PocketBook.
fn queue_epubs(
    queue: &deckle_core::queue::Queue,
    built: &deckle_digest::BuildReport,
    date: deckle_digest::chrono::NaiveDate,
) -> Result<(), String> {
    let name = format!("{}{date}.epub", built.prefix);
    queue
        .add(&built.epub, &name, Some(&built.prefix))
        .map_err(|e| e.to_string())?;
    for p in &built.long_reads {
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        queue.add(p, &name, None).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Flushes the EPUB outbox to `reader` (a Kobo or a PocketBook), when one is connected, with its
/// `[sync]` actions.
fn push_reader(
    queue: &deckle_core::queue::Queue,
    cfg: &deckle_digest::Config,
    reader: Option<&deckle_core::reader::Reader>,
) -> Result<Option<DigestOutcome>, String> {
    let Some(reader) = reader else {
        return Ok(None);
    };
    let actions = sync_actions(cfg, &reader.id());
    let Some(plan) = push_plan(actions, queue.pending().is_empty()) else {
        return Ok(None);
    };
    reader_sync(queue, cfg, reader, plan).map(Some)
}

/// [`sync_to`] for a Kobo or a PocketBook: push the EPUB outbox, prune, and on a Kobo read its
/// database for the highlights and the shelf. Neither has a `.yjf`, so no readership; and the
/// Morgue's notes on
/// what's on the Kindle stay the Kindle's.
fn reader_sync(
    queue: &deckle_core::queue::Queue,
    cfg: &deckle_digest::Config,
    reader: &deckle_core::reader::Reader,
    plan: deckle_core::sync::Plan,
) -> Result<DigestOutcome, String> {
    let id = reader.id();
    let now = deckle_digest::chrono::Utc::now();
    let stamp = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    if let Err(e) = deckle_core::reader::remember(&deckle_core::reader::seen_file(), reader, &stamp)
    {
        eprintln!("{}: {e}", reader.name());
    }
    let flush = deckle_core::sync::run_reader(
        queue,
        reader,
        plan,
        &digest_prune(cfg),
        &mut |i, n, name| eprintln!("push: {i}/{n} {name}"),
    )
    .map_err(|e| e.to_string())?;
    if plan.clippings && puzzle_on(cfg) && flush.clippings_error.is_none() {
        let path = cfg.puzzle_store();
        let mut store = deckle_digest::puzzle::store::Store::load(&path);
        use deckle_digest::puzzle::check::Notes;
        let notes = match flush.clippings.as_deref() {
            Some(b) => Notes::Read(b),
            None => Notes::NoFile,
        };
        deckle_digest::puzzle::check::record(
            &mut store,
            notes,
            deckle_digest::chrono::Local::now().date_naive(),
            deckle_digest::puzzle::words::Words::embedded(),
        );
        if let Err(e) = store.save(&path) {
            eprintln!("puzzle: {}: {e}", path.display());
        }
    }
    let export = cfg.sync.for_device(&id).clippings;
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
    let shelf = flush.kobo_books.clone().and_then(|books| {
        let path = cfg.shelf_store();
        let previous = deckle_digest::shelf::store::Shelf::load(&path)
            .kindles
            .get(&id)
            .cloned();
        let name = cfg.sync.device_name(&id).unwrap_or_else(|| reader.name());
        let snap = deckle_shelf::kobo_snapshot(
            books,
            flush.clippings.clone(),
            flush.clippings_error.is_some(),
            &name,
            previous,
            now,
        );
        deckle_shelf::record(&path, &id, snap)
            .inspect_err(|e| eprintln!("shelf: {e}"))
            .ok()
    });
    Ok(DigestOutcome {
        flush: Some(flush),
        highlights,
        shelf,
        ..Default::default()
    })
}

/// `deckle run … --to device` with no Kindle but a Kobo or a PocketBook: each book cleaned
/// (and made a KEPUB for a Kobo; no Previewer), sent to its `Deckle/` folder.
fn send_to_reader(
    inputs: &[PathBuf],
    reader: &deckle_core::reader::Reader,
    json: bool,
) -> Result<ExitCode, String> {
    #[derive(serde::Serialize)]
    struct Sent {
        input: PathBuf,
        delivered: Option<PathBuf>,
        error: Option<String>,
    }
    let sent: Vec<Sent> = inputs
        .iter()
        .map(|input| {
            let arg = input.to_string_lossy();
            let r = (|| {
                let source = if deckle_digest::web_book::is_article(&arg) {
                    import_article(&arg, &deckle_digest::Http, &articles_dir())?.path
                } else {
                    input.clone()
                };
                let bytes = deckle_core::reader::book(reader, &source)?;
                let stem = source
                    .file_stem()
                    .map(|s| deckle_core::deliver::sanitize(&s.to_string_lossy()))
                    .unwrap_or_else(|| "book".into());
                deckle_core::reader::deliver(reader, &bytes, &reader.file_name(&stem))
                    .map_err(|e| e.to_string())
            })();
            match r {
                Ok(p) => Sent {
                    input: input.clone(),
                    delivered: Some(p),
                    error: None,
                },
                Err(e) => Sent {
                    input: input.clone(),
                    delivered: None,
                    error: Some(e),
                },
            }
        })
        .collect();
    let failed = sent.iter().filter(|s| s.error.is_some()).count();
    emit(json, &sent, || {
        sent.iter()
            .map(|s| match (&s.error, &s.delivered) {
                (Some(e), _) => format!("FAIL  {}\n      {e}", s.input.display()),
                (None, Some(p)) => format!("OK    {}\n      -> {}", s.input.display(), p.display()),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    Ok(if failed > 0 {
        ExitCode::from(EXIT_FAILURE)
    } else {
        ExitCode::SUCCESS
    })
}

/// Flushes the outbox to `target` (a connected Kindle, or a folder), or reports it queued.
fn push_queued(
    main: &Path,
    queue: &deckle_core::queue::Queue,
    papers: &[deckle_digest::Config],
    target: Option<Target>,
) -> Result<DigestOutcome, String> {
    let cookbook = deckle_digest::cookbook::store::Store::new(
        deckle_digest::cookbook::store::Store::default_dir(),
    );
    push_queued_with(main, queue, papers, target, &cookbook)
}

/// [`push_queued`] with the cookbook folder given (`cookbook send` builds into one and must
/// offer from the same).
fn push_queued_with(
    main: &Path,
    queue: &deckle_core::queue::Queue,
    papers: &[deckle_digest::Config],
    target: Option<Target>,
    cookbook: &deckle_digest::cookbook::store::Store,
) -> Result<DigestOutcome, String> {
    let cfg = &papers[0];
    // The Kitchen Book rides along when this Kindle hasn't the newest build (cookbook §9). A
    // folder is an export, not a Kindle: it gets none.
    let id = target.as_ref().and_then(cookbook_delivery::kindle_id);
    let mut cookbook_error = None;
    if let Some(id) = &id
        && let Err(e) =
            cookbook_delivery::offer(cookbook, queue, id, cfg.sync.for_device(id).cookbook)
    {
        eprintln!("cookbook: {e}");
        cookbook_error = Some(e);
    }
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
            cookbook_error,
            ..Default::default()
        });
    };
    // The build that goes now, read before the push takes the entry.
    let pushed = id
        .as_deref()
        .and_then(|k| cookbook_delivery::queued_build(queue, k));
    let mut outcome = sync_to(main, queue, papers, &target, plan)?;
    if let (Some(flush), Some(id)) = (&outcome.flush, &id)
        && let Err(e) = cookbook_delivery::record(cookbook, id, flush, pushed.as_deref())
    {
        eprintln!("cookbook: {e}");
        cookbook_error = Some(e);
    }
    outcome.cookbook_error = cookbook_error;
    Ok(outcome)
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
        // learning reads the paper's own highlights too (interests design §5)
        clippings: a.clippings
            || puzzle_on(cfg)
            || shelf
            || deckle_readership::learn::learning_on(cfg),
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
        naming: Vec::new(),
        nudges: Vec::new(),
    }
}

/// The build's options with what Deckle noticed and the editor may name (interests design
/// §6): only with the editor on, from the paper's own `interests.json`.
fn gather_options_for(
    cfg: &deckle_digest::Config,
    date: deckle_digest::chrono::NaiveDate,
) -> deckle_digest::GatherOptions {
    let mut opts = gather_options();
    if cfg.llm.enabled {
        let learning =
            deckle_readership::Learning::load(&deckle_readership::Learning::path_for(cfg));
        opts.naming = deckle_readership::learn::naming_candidates(&learning, cfg, date)
            .into_iter()
            .map(|(key, phrase, headlines)| deckle_digest::NamingCandidate {
                key,
                phrase,
                headlines,
            })
            .collect();
    }
    if cfg.learning.enabled && cfg.learning.nudges {
        let learning =
            deckle_readership::Learning::load(&deckle_readership::Learning::path_for(cfg));
        opts.nudges = deckle_readership::learn::nudges_for(&learning, cfg, date)
            .into_iter()
            .map(|(key, name, why)| deckle_digest::Nudge { key, name, why })
            .collect();
    }
    opts
}

/// Makes the nudges tapped on the Kindle interests of `paper` (interests design §7): the
/// entry goes into its file, the acceptance into its `interests.json`. Never fails the caller.
fn accept_tapped_nudges(
    main: &Path,
    paper: &deckle_digest::Config,
    state: &deckle_readership::State,
) {
    let path = deckle_readership::Learning::path_for(paper);
    let mut learning = deckle_readership::Learning::load(&path);
    // the file as it is now, not as it was when the connection began: the app may have saved
    let mut cfg = match deckle_digest::papers::load_one(main, paper.slug()) {
        Ok(c) => c,
        Err(e) => return eprintln!("learning: {e}"),
    };
    let today = deckle_digest::chrono::Local::now().date_naive();
    let added = deckle_readership::learn::accept_nudges(&mut learning, &mut cfg, state, today);
    if added.is_empty() {
        return;
    }
    if let Err(e) = deckle_digest::papers::save(main, &cfg) {
        eprintln!("learning: {e}");
        return;
    }
    if let Err(e) = learning.save(&path) {
        eprintln!("learning: {e}");
    }
    for name in added {
        eprintln!("Added {name} to your interests (you tapped it in the paper).");
    }
}

/// Keeps the names the editor gave (interests design §6). Never fails the build.
fn record_naming(cfg: &deckle_digest::Config, gathered: &deckle_digest::Gathered) {
    if gathered.names.is_empty() {
        return;
    }
    let path = deckle_readership::Learning::path_for(cfg);
    let mut learning = deckle_readership::Learning::load(&path);
    let names: Vec<(String, String, String)> = gathered
        .names
        .iter()
        .map(|n| (n.key.clone(), n.name.clone(), n.query.clone()))
        .collect();
    if deckle_readership::learn::record_names(&mut learning, &names) > 0
        && let Err(e) = learning.save(&path)
    {
        eprintln!("learning: {e}");
    }
}

/// The paper has a puzzle page.
fn puzzle_on(cfg: &deckle_digest::Config) -> bool {
    cfg.sections.contains(&deckle_digest::SectionKind::Puzzle)
}

/// Runs `plan` on `target` over one connection.
fn sync_to(
    main: &Path,
    queue: &deckle_core::queue::Queue,
    papers: &[deckle_digest::Config],
    target: &Target,
    plan: deckle_core::sync::Plan,
) -> Result<DigestOutcome, String> {
    // The household settings (`[sync]`, the puzzle's checks, the shelf) are the main paper's.
    let cfg = &papers[0];
    let id = deckle_core::sync::device_id(target);
    // Prepared books are finished for this Kindle: its class from the connection, the owner's
    // choices for it from the app's devices.json (device-aware build §4.3).
    let registry =
        deckle_core::devices::DeviceRegistry::load(&deckle_core::devices::default_registry_file());
    let finishing = deckle_core::queue::Finishing {
        profile: deckle_core::devices::profile_for(target, &registry),
        ..Default::default()
    };
    // Each paper keeps its own readership (many papers §4): every paper's stems are read in
    // the one session, and each read is recorded where its stem came from.
    let mut states: Vec<(PathBuf, deckle_readership::State)> = papers
        .iter()
        .map(|p| {
            let path = readership_path(p);
            let state = deckle_readership::State::load(&path);
            (path, state)
        })
        .collect();
    let stems: Vec<String> = match plan.readership {
        true => states.iter().flat_map(|(_, s)| s.stems()).collect(),
        false => Vec::new(),
    };
    let mut conn = deckle_core::deliver::Conn::new(target);
    let flush = deckle_core::sync::run_with(
        queue,
        &mut conn,
        plan,
        &prunes_for(papers, &id),
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
        let now = deckle_digest::chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let mut all = deckle_readership::Recorded::default();
        for (path, state) in &mut states {
            let mine = state.stems();
            let found: Vec<_> = flush
                .positions
                .iter()
                .filter(|(stem, _)| mine.contains(stem))
                .map(|(stem, on)| (stem.clone(), on_device(on)))
                .collect();
            if found.is_empty() {
                continue;
            }
            let r = state.record_reads(&id, &found, &now);
            if let Err(e) = state.save(path) {
                eprintln!("readership: {e}");
            }
            all.opened += r.opened;
            all.unopened += r.unopened;
            all.errors.extend(r.errors);
        }
        all
    });
    record_morgue(&flush);
    let shelf = record_shelf(cfg, target, &flush);
    // What this connection teaches each paper (interests design §5): taps from the reads just
    // recorded, highlights from the clippings, authors from the shelf just stored.
    for (paper, (_, state)) in papers.iter().zip(&states) {
        let store = deckle_digest::shelf::store::Shelf::load(&paper.shelf_store());
        if let Err(e) = deckle_readership::learn::on_connection(
            paper,
            state,
            flush.clippings.as_deref(),
            Some(&store),
            deckle_digest::chrono::Utc::now(),
        ) {
            eprintln!("learning: {e}");
        }
        // a nudge tapped on the Kindle is an accept (interests design §7)
        accept_tapped_nudges(main, paper, state);
    }
    let waiting = flush
        .waiting
        .iter()
        .map(|(name, id)| {
            (
                name.clone(),
                cfg.sync.device_name(id).unwrap_or_else(|| id.clone()),
            )
        })
        .collect();
    Ok(DigestOutcome {
        flush: Some(flush),
        highlights,
        readership: recorded,
        shelf,
        waiting,
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
    cfg: &deckle_digest::Config,
) {
    let sections: Vec<(String, String)> = built
        .sections
        .iter()
        .map(|s| (s.key.clone(), s.title.clone()))
        .collect();
    let stem = deckle_digest::edition_stem(cfg, date);
    let stem = stem.as_str();
    let titles: Vec<(String, Vec<String>)> = built
        .sections
        .iter()
        .map(|s| (s.key.clone(), s.titles.clone()))
        .collect();
    let layout = std::fs::read(kfx)
        .map_err(|e| format!("{}: {e}", kfx.display()))
        .and_then(|bytes| {
            let mut l = deckle_readership::layout::layout_from_kfx(
                &bytes,
                date,
                &built.title,
                stem,
                &sections,
            )?;
            // stories are a nicety for learning: a layout without them is still a layout
            if let Err(e) = deckle_readership::layout::add_stories(&mut l, &bytes, &titles) {
                eprintln!("readership: stories: {e}");
            }
            Ok(l)
        })
        .map(|l| with_items(l, built));
    let path = readership_path(cfg);
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
fn digest_puzzle(json: bool, cfg: &deckle_digest::Config) -> Result<ExitCode, String> {
    use deckle_digest::puzzle;
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
    // The first-run import of old editions is the main paper's: another paper's are new.
    if cfg.paper.is_some() {
        return morgue
            .archive_edition(epub, &deckle_morgue::from_gathered(gathered))
            .err()
            .map(|e| format!("Morgue: couldn't save the edition ({e})"));
    }
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
    paper: Option<&str>,
    action: Option<cli::ReadershipCommand>,
) -> Result<ExitCode, String> {
    let mut cfg = paper_config(path, paper)?;
    let state_path = readership_path(&cfg);
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
            deckle_digest::papers::save(path, &cfg)?;
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
/// outbox (design §10). Says so when it found anything. Recipes for the cookbook are added
/// only with `recipes` (the watcher): fetching a backlog of them must not hold up a build.
fn scan_inbox(cfg: &deckle_digest::Config, recipes: bool) {
    let dir = deckle_digest::inbox::expand_home(&cfg.reading.inbox);
    let outbox = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir());
    let kindles: Vec<(String, String)> = cfg
        .sync
        .named()
        .map(|(id, name)| (name.to_owned(), id.to_owned()))
        .collect();
    let r = inbox::scan(
        &dir,
        &reading_queue(cfg),
        &outbox,
        std::time::SystemTime::now(),
        &kindles,
        &mut inbox::with_readers(reader_seen().then(epub_queue), inbox::pipeline()),
    );
    // Said once per run: the watcher scans every few seconds.
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        for w in &r.warnings {
            eprintln!("inbox: {w}");
        }
    });
    for folder in &r.other_folders {
        eprintln!(
            "inbox: left {} alone: no Kindle is called that (name one with `deckle devices name`)",
            folder.display()
        );
    }
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
    if recipes {
        scan_recipes_folder(&dir);
    }
}

/// Adds what was dropped in the Inbox's `Recipes/` folder to the cookbook (cookbook design
/// §8.3). An empty folder costs one directory read. The cookbook delivery picks the new
/// recipes up on its next tick.
fn scan_recipes_folder(dir: &Path) {
    let store = deckle_digest::cookbook::store::Store::new(
        deckle_digest::cookbook::store::Store::default_dir(),
    );
    let r = deckle_digest::inbox::scan_recipes(
        dir,
        &store,
        &deckle_digest::Http,
        std::time::SystemTime::now(),
    );
    if r.recipes > 0 {
        eprintln!(
            "inbox: added {} {} to the cookbook from {}",
            r.recipes,
            if r.recipes == 1 { "recipe" } else { "recipes" },
            dir.join(deckle_digest::inbox::RECIPES_FOLDER).display()
        );
    }
    for (file, why) in &r.unreadable {
        eprintln!(
            "inbox: couldn't add {} to the cookbook: {why}",
            file.display()
        );
    }
    for (file, why) in &r.retry {
        eprintln!("inbox: {} waits for the next try: {why}", file.display());
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
            "Drag these to your bookmarks bar (or make a bookmark with each as the address).\n\
             They work while `deckle digest watch` is running.\n\n\
             Save to paper (an article for tomorrow's Daily):\n{}\n\n\
             Save recipe (a recipe for the cookbook):\n{}",
            deckle_digest::bookmarklet::javascript(&token),
            deckle_digest::bookmarklet::recipe_javascript(&token)
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
            color_epub: None,
            color_long_reads: Vec::new(),
            pdfs: Vec::new(),
            bundles: Vec::new(),
            reading_outcomes: vec![
                ("a1".into(), Outcome::Printed),
                ("b2".into(), Outcome::Book),
                ("c3".into(), Outcome::Book),
            ],
            notes: Vec::new(),
            prefix: deckle_digest::EDITION_PREFIX.to_owned(),
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
        k.set(
            deckle_digest::secret::SERVICE,
            &deckle_digest::secret::account(&cfg(false)),
            PW,
        )
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
        let stem = format!("{}{}", deckle_digest::EDITION_PREFIX, date());
        let (email, to_usb) =
            email_paper(cfg, None, &stem, date(), &run.epub, keys, &no_env, |_| rec);
        queue_edition(
            &run.queue,
            to_usb.then_some(run.kfx.as_path()),
            &deckle_digest::edition_filename(date()),
            Some(deckle_digest::EDITION_PREFIX),
            &[],
            &[],
            None,
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
            None,
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
            stories: Vec::new(),
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
            titles: Vec::new(),
        };
        let built = deckle_digest::BuildReport {
            epub: PathBuf::new(),
            title: "T".into(),
            sections: vec![summary("feeds:Science", 7), summary("local", 3)],
            long_reads: Vec::new(),
            color_epub: None,
            color_long_reads: Vec::new(),
            pdfs: Vec::new(),
            bundles: Vec::new(),
            reading_outcomes: Vec::new(),
            notes: Vec::new(),
            prefix: deckle_digest::EDITION_PREFIX.to_owned(),
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
