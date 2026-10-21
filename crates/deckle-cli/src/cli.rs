// SPDX-License-Identifier: GPL-3.0-or-later
//! Command line definition.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// Deckle: turn EPUBs into store-quality Kindle books and send them to a Kindle.
#[derive(Parser, Debug)]
#[command(name = "deckle", version, about)]
pub struct Cli {
    /// Print machine-readable JSON reports instead of text.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdeArg {
    /// Let the cover rule decide (design §5).
    Auto,
    Pdoc,
    Ebok,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum XRayArg {
    Off,
    Auto,
}

impl From<XRayArg> for deckle_core::pipeline::XRayMode {
    fn from(a: XRayArg) -> Self {
        match a {
            XRayArg::Off => Self::Off,
            XRayArg::Auto => Self::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum LlmArg {
    Off,
    Anthropic,
    Openai,
}

impl LlmArg {
    pub fn config(
        self,
        model: Option<String>,
        endpoint: Option<String>,
    ) -> Option<deckle_core::xray::LlmConfig> {
        use deckle_xray::llm::Provider;
        let provider = match self {
            LlmArg::Off => return None,
            LlmArg::Anthropic => Provider::Anthropic,
            LlmArg::Openai => Provider::OpenAi,
        };
        Some(deckle_core::xray::LlmConfig {
            provider,
            model: model.unwrap_or_default(),
            endpoint: endpoint.unwrap_or_default(),
            api_key: String::new(),
        })
    }
}

impl CdeArg {
    pub fn to_override(self) -> Option<deckle_kfx::convert::CdeType> {
        match self {
            CdeArg::Auto => None,
            CdeArg::Pdoc => Some(deckle_kfx::convert::CdeType::Pdoc),
            CdeArg::Ebok => Some(deckle_kfx::convert::CdeType::Ebok),
        }
    }
}

impl From<CdeArg> for deckle_kfx::convert::CdeType {
    fn from(c: CdeArg) -> Self {
        match c {
            CdeArg::Ebok => deckle_kfx::convert::CdeType::Ebok,
            _ => deckle_kfx::convert::CdeType::Pdoc,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Read an EPUB's metadata, structure and problems.
    Inspect { epub: PathBuf },
    /// Write a cleaned EPUB that Kindle Previewer converts well.
    Normalize {
        epub: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Convert an EPUB (via Kindle Previewer) or a KPF into a monolithic KFX.
    Convert {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = CdeArg::Pdoc)]
        cde: CdeArg,
        #[arg(long)]
        asin: Option<String>,
        /// Approximate page numbers: a count, or `auto`.
        #[arg(long)]
        pages: Option<String>,
        /// Remove author metadata (reproduces kfxlib's default output).
        #[arg(long)]
        no_authors: bool,
        #[arg(long)]
        container_id: Option<String>,
        #[arg(long)]
        creator: Option<String>,
        #[arg(long)]
        previewer: Option<PathBuf>,
    },
    /// Copy a KFX to a folder or a connected mass-storage Kindle.
    Send {
        kfx: PathBuf,
        /// A folder path, or `device` for the first connected Kindle.
        #[arg(long)]
        to: String,
    },
    /// List connected Kindles.
    Devices,
    /// Run the whole pipeline on one or more EPUBs, Word documents (.docx), text files (.txt) or
    /// web articles.
    Run {
        /// EPUBs; Word documents (.docx); text files (.txt); or web articles: an address, a
        /// .webloc/.url link file or a saved .html page, each built into its own book first.
        epubs: Vec<PathBuf>,
        /// A folder path, or `device` for the first connected Kindle.
        #[arg(long, default_value = "device")]
        to: String,
        /// `auto` lets the cover rule decide; `pdoc`/`ebok` override it.
        #[arg(long, value_enum, default_value_t = CdeArg::Auto)]
        cde: CdeArg,
        /// Approximate page numbers: a count, or `auto`.
        #[arg(long)]
        pages: Option<String>,
        #[arg(long)]
        previewer: Option<PathBuf>,
        /// Look up an Amazon edition on Open Library when the Kindle needs a real ASIN (network).
        #[arg(long = "match")]
        amazon_match: bool,
        /// Fill the title, author, series, description and subjects a book leaves empty from
        /// Open Library, by ISBN or title search (network, off by default). The book's own metadata
        /// always wins.
        #[arg(long)]
        fill_metadata: bool,
        /// Tidy the text: curly quotes, dashes, ellipses, and stray empty paragraphs removed
        /// (off by default). Code and non-English quotes are left alone.
        #[arg(long)]
        typography: bool,
        /// Turn footnotes and endnotes into Kindle pop-up notes (off by default). Notes the book
        /// marks always convert; others only when the note links back to its reference.
        #[arg(long)]
        popup_notes: bool,
        /// This Kindle keeps Deckle's sidecars for real ASINs (spike 2 passed).
        #[arg(long)]
        verified_asin: bool,
        /// The book's Amazon ASIN, e.g. the Kindle edition's B0… from its Amazon page; replaces
        /// `--match`. With `--cde ebok` the Kindle fetches the store cover. One EPUB only.
        #[arg(long)]
        asin: Option<String>,
        /// X-Ray sidecar for EBOK books: `auto` (default) or `off`.
        #[arg(long, value_enum, default_value_t = XRayArg::Auto)]
        xray: XRayArg,
        /// LLM for X-Ray descriptions (key from ANTHROPIC_API_KEY / OPENAI_API_KEY).
        #[arg(long, value_enum, default_value_t = LlmArg::Off)]
        llm: LlmArg,
        /// Model name for `--llm` (provider default when omitted).
        #[arg(long)]
        llm_model: Option<String>,
        /// Base URL for `--llm openai` (OpenAI-compatible servers, e.g. http://localhost:11434/v1).
        #[arg(long)]
        llm_endpoint: Option<String>,
        /// Wikipedia summaries for X-Ray terms (network).
        #[arg(long)]
        wikipedia: bool,
        /// Word Wise sidecar for EBOK books: `auto` (default) or `off`. The first use downloads
        /// Deckle's Word Wise database (about 12 MB) unless `--wordwise-db` is given.
        #[arg(long = "word-wise", value_enum, default_value_t = XRayArg::Auto)]
        word_wise: XRayArg,
        /// Highest Word Wise difficulty to hint, 1 (rarest words only) to 5 (all).
        #[arg(long = "word-wise-level", default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=5))]
        word_wise_level: u8,
        /// A local Word Wise database instead of the cached download.
        #[arg(long = "wordwise-db")]
        wordwise_db: Option<PathBuf>,
        #[arg(long)]
        keep_work_dir: bool,
    },
    /// Save articles for tomorrow's paper (Your Reading): URLs or saved .html pages.
    Save {
        /// URLs, or saved web pages (.html).
        targets: Vec<String>,
        #[arg(long)]
        title: Option<String>,
        /// Show the queue: pending first, then the last 7 days.
        #[arg(long)]
        list: bool,
        /// Remove an entry by id or URL.
        #[arg(long)]
        remove: Option<String>,
        /// Print the bookmarklet to drag to your browser's bookmarks bar.
        #[arg(long)]
        bookmarklet: bool,
    },
    /// What's on a Kindle: list its books, or remove one.
    Kindle {
        /// `device` for the first connected Kindle, or a folder (an export).
        #[arg(long, default_value = "device", global = true)]
        to: String,
        #[command(subcommand)]
        action: KindleCommand,
    },
    /// Your library: one folder per book, with a deckle.json beside it. Every book `run` sends
    /// is added once the library is set up.
    Library {
        #[command(subcommand)]
        action: LibraryCommand,
    },
    /// The Deckle Daily: a newspaper built from your sources (weather, local news, feeds,
    /// lottery, horoscope, on this day).
    Digest {
        /// The digest config (default: `digest.toml` in Deckle's config folder).
        #[arg(long, global = true)]
        config: Option<PathBuf>,
        #[command(subcommand)]
        action: DigestCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum KindleCommand {
    /// Every book in documents/: mode, X-Ray and Word Wise, size, and free space.
    Books,
    /// Remove a book (as `books` names it) with its .sdr folder.
    Remove {
        file: String,
        /// Don't ask first.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum LibraryCommand {
    /// Use DIR as the library (created when missing) and remember it.
    Init {
        dir: PathBuf,
        /// Don't add the books `run` sends; only `library add` adds.
        #[arg(long)]
        no_auto_add: bool,
    },
    /// Add EPUBs or Word documents (an edited copy of a book updates it in place).
    Add { books: Vec<PathBuf> },
    /// Every book: author, title, series, and where it was last sent.
    List {
        /// Group the books under series headings, books outside a series last.
        #[arg(long)]
        series: bool,
    },
    /// Every series with its missing numbers, or one series (NAME, or a unique start of it) in
    /// reading order.
    Series {
        name: Option<String>,
        /// Only series with gaps, and their missing numbers.
        #[arg(long)]
        missing: bool,
    },
    /// Adopt books put in the folder by hand, refresh changed ones, report problems.
    Scan,
    /// Rebuild and re-send the books on a Kindle that Deckle could build better now (an
    /// older X-Ray engine or Word Wise database, or a sidecar gone), each in the mode it was
    /// sent in. Lists them, then asks.
    Reprocess {
        /// `device` for the first connected Kindle, or a folder.
        #[arg(long, default_value = "device")]
        to: String,
        /// Show what would be updated and change nothing.
        #[arg(long)]
        dry_run: bool,
        /// Don't ask first.
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        previewer: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum DigestCommand {
    /// Write a commented starter config.
    Init {
        /// Replace an existing config.
        #[arg(long)]
        force: bool,
    },
    /// Build today's edition as an EPUB.
    Build {
        /// Folder for the EPUB.
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        /// Edition date, YYYY-MM-DD (default: today). Sources still return today's data.
        #[arg(long)]
        date: Option<String>,
    },
    /// Build today's edition and convert it (forced Cover/PDOC). With `--to device` it goes to
    /// the outbox and on to a connected Kindle now, or to the next one that connects (`push`,
    /// `watch`). Only the newest `keep` editions stay on the target.
    Run {
        /// A folder path, or `device` for a Kindle (now, or when one connects).
        #[arg(long, default_value = "device")]
        to: String,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        previewer: Option<PathBuf>,
        #[arg(long)]
        keep_work_dir: bool,
    },
    /// Deliver queued editions to a connected Kindle and prune old ones. Does nothing (and
    /// succeeds) when no Kindle is connected, so it is safe to run on a timer.
    Push,
    /// Make the paper arrive by itself: the edition daily at a time, and the watcher from
    /// login on (macOS LaunchAgents, Windows Task Scheduler). No option: show what's set.
    Schedule {
        /// Daily time for the edition, 24-hour, e.g. 05:30.
        #[arg(long, conflicts_with = "off")]
        at: Option<String>,
        /// Remove both jobs.
        #[arg(long)]
        off: bool,
    },
    /// Stay running and push queued editions whenever a Kindle connects.
    Watch {
        /// Seconds between checks.
        #[arg(long, default_value_t = 20)]
        interval: u64,
    },
    /// Email the paper to your Kindle (`[email]` in digest.toml): the SMTP password and a test.
    Email {
        #[command(subcommand)]
        action: EmailCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum EmailCommand {
    /// Save the SMTP password in the system keychain (asked for without echo).
    Password {
        /// Read one line from stdin instead, for scripts.
        #[arg(long)]
        stdin: bool,
    },
    /// Send a one-page test EPUB to the Kindle address and show the SMTP result.
    Test,
    /// Delete the SMTP password from the system keychain.
    Forget,
}

/// `--pages` parsing: absent → none; `auto` → Some(None); a number → Some(Some(n)).
pub fn parse_pages(arg: Option<&str>) -> Result<Option<Option<u32>>, String> {
    match arg {
        None => Ok(None),
        Some("auto") => Ok(Some(None)),
        Some(n) => n
            .parse::<u32>()
            .map(|n| Some(Some(n)))
            .map_err(|_| format!("--pages expects a number or `auto`, got {n:?}")),
    }
}
