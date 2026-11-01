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

/// `--direction`: a comic's reading order (fixed-layout spec §3).
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectionArg {
    /// Left to right.
    Ltr,
    /// Right to left: manga.
    Rtl,
}

impl From<DirectionArg> for deckle_comic::Direction {
    fn from(d: DirectionArg) -> Self {
        match d {
            DirectionArg::Ltr => deckle_comic::Direction::LeftToRight,
            DirectionArg::Rtl => deckle_comic::Direction::RightToLeft,
        }
    }
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
    /// Read an EPUB's metadata, structure and problems; a Word document, text file or PDF
    /// reports the EPUB it imports as.
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
    /// List connected Kindles with their names and sync ids, or name one.
    Devices {
        #[command(subcommand)]
        action: Option<DevicesCommand>,
    },
    /// Run the whole pipeline on one or more EPUBs, Word documents (.docx), OpenDocument text (.odt), RTF (.rtf), DRM-free Kindle books (.mobi, .azw, .azw3, .prc), text files (.txt), FictionBook (.fb2, .fb2.zip), Markdown files or notes folders,
    /// text PDFs (.pdf, reflowed; needs PDFium), comics (.cbz, .cbr, a folder of page images or a
    /// fixed-layout AZW3; an AZW3 on a Kindle) or web articles.
    Run {
        /// EPUBs; Word documents (.docx); OpenDocument text (.odt/.ott/.fodt); RTF (.rtf); DRM-free Kindle books (.mobi/.azw/.azw3/.prc); text files (.txt); FictionBook (.fb2/.fb2.zip); Markdown (.md/.markdown) or flat notes folders; text PDFs (.pdf); comics (.cbz/.cbr) or folders of page images; or web articles:
        /// an address, a .webloc/.url link file or a saved .html page, each built into its own
        /// book first.
        epubs: Vec<PathBuf>,
        /// Built-in font, spacing and enhancement defaults: academic, novel, manual.
        #[arg(long)]
        preset: Option<deckle_core::preset::Preset>,
        /// A folder path, or `device` for the first connected Kindle.
        #[arg(long, default_value = "device")]
        to: String,
        /// The Kindle to send to, by its name (`deckle devices name`) or sync id. When it isn't
        /// connected the books wait in the outbox for it: `deckle digest push` or `watch`
        /// delivers them when it is.
        #[arg(long)]
        kindle: Option<String>,
        /// `auto` lets the cover rule decide; `pdoc`/`ebok` override it.
        #[arg(long, value_enum, default_value_t = CdeArg::Auto)]
        cde: CdeArg,
        /// Approximate page numbers: a count, `auto`, or `off`.
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
        /// (off without a preset). Use `--typography=false` to override a preset.
        /// Code and non-English quotes are left alone.
        #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
        typography: Option<bool>,
        /// Don't make a typographic cover for a book that has none (one is made by default).
        #[arg(long)]
        no_generated_cover: bool,
        /// The generated cover's template: `classic`, `band` or `rule` (default: chosen per
        /// book by its identifier).
        #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(["classic", "band", "rule"]))]
        cover_style: Option<String>,
        /// Turn footnotes and endnotes into Kindle pop-up notes (off without a preset).
        /// Use `--popup-notes=false` to override a preset. Notes the book
        /// marks always convert; others only when the note links back to its reference.
        #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
        popup_notes: Option<bool>,
        /// This Kindle keeps Deckle's sidecars for real ASINs (spike 2 passed).
        #[arg(long)]
        verified_asin: bool,
        /// The book's Amazon ASIN, e.g. the Kindle edition's B0… from its Amazon page; replaces
        /// `--match`. With `--cde ebok` the Kindle fetches the store cover. One EPUB only.
        #[arg(long)]
        asin: Option<String>,
        /// X-Ray sidecar for EBOK books: `auto` (default) or `off`.
        #[arg(long, value_enum)]
        xray: Option<XRayArg>,
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
        #[arg(long = "word-wise", value_enum)]
        word_wise: Option<XRayArg>,
        /// Highest Word Wise difficulty to hint, 1 (rarest words only) to 5 (all).
        #[arg(long = "word-wise-level", default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=5))]
        word_wise_level: u8,
        /// A local Word Wise database instead of the cached download.
        #[arg(long = "wordwise-db")]
        wordwise_db: Option<PathBuf>,
        #[arg(long)]
        keep_work_dir: bool,
        /// Reading order for comics (.cbz, .cbr, page folders): `rtl` for manga, `ltr`
        /// otherwise. Without it, `ComicInfo.xml` decides, else left to right.
        #[arg(long, value_enum)]
        direction: Option<DirectionArg>,
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
    /// Bind several files into one book, in the order given: EPUBs, Word, OpenDocument and RTF files, DRM-free Kindle books, text files, FB2,
    /// text PDFs and web articles (an address, a .webloc/.url link file or a saved .html page).
    /// Each becomes a part with its own entry in the contents. Writes an EPUB; send it with
    /// `deckle run`.
    Bind {
        /// The files, in reading order (at least two).
        inputs: Vec<PathBuf>,
        /// The book's title.
        #[arg(long)]
        title: String,
        /// The book's author (default: the author every part shares, if they share one).
        #[arg(long)]
        author: Option<String>,
        /// Where to write the EPUB (default: "<title>.epub" here).
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// What's on a Kindle: list its books, or remove one.
    Kindle {
        /// `device` for the first connected Kindle, or a folder (an export).
        #[arg(long, default_value = "device", global = true)]
        to: String,
        #[command(subcommand)]
        action: KindleCommand,
    },
    /// The OPDS catalog and KOReader sync on the home network, served by the watcher.
    ReaderServer {
        #[command(subcommand)]
        action: ReaderServerCommand,
    },
    /// A Kobo's shelves and KOReader's collections from calibre's tags, series, authors,
    /// columns or saved searches.
    Shelves {
        #[command(subcommand)]
        action: ShelvesCommand,
    },
    /// Reading data written back into your calibre library, through calibredb.
    Calibre {
        #[command(subcommand)]
        action: CalibreCommand,
    },
    /// calibre's wireless connection, served by the watcher: KOReader takes books over Wi-Fi.
    Wireless {
        #[command(subcommand)]
        action: WirelessCommand,
    },
    /// Highlights and notes on a Kindle, as Markdown files (one per book).
    Highlights {
        /// `device` for the first connected Kindle, or a folder standing in for documents/.
        #[arg(long, default_value = "device", global = true)]
        from: String,
        #[command(subcommand)]
        action: HighlightsCommand,
    },
    /// The words looked up on a Kindle (its Vocabulary Builder).
    Words {
        /// `device` for the first connected Kindle, or a copy of a Kindle's drive.
        #[arg(long, default_value = "device", global = true)]
        from: String,
        #[command(subcommand)]
        action: WordsCommand,
    },
    /// KOReader on a Kobo, a PocketBook, a Kindle on USB storage or in a folder: back up its
    /// reading state and put it back.
    Koreader {
        #[command(subcommand)]
        action: KoreaderCommand,
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
        /// Which paper (`deckle digest papers`): `main` is digest.toml's, else
        /// `papers/<slug>.toml`. `push` and `watch` always serve every paper.
        #[arg(long, global = true)]
        paper: Option<String>,
        #[command(subcommand)]
        action: DigestCommand,
    },
    /// The Kitchen Book: a cookbook built from the recipes you save.
    Cookbook {
        #[command(subcommand)]
        action: CookbookCommand,
    },
}

/// Which picture a recipe prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum PictureArg {
    /// The site's photo.
    Site,
    /// A generated illustration.
    Illustration,
    /// No picture.
    None,
    /// Whatever the book's setting says.
    Default,
}

#[derive(Subcommand, Debug)]
pub enum CookbookCommand {
    /// Keep recipes from web addresses, link files or saved pages, and file each into a
    /// chapter. Prints the chapter and the picture.
    Add {
        /// Web addresses, `.webloc`/`.url` link files or saved pages (`.html`).
        targets: Vec<String>,
        /// A chapter id (`deckle cookbook chapters`); default: filed by its categories and title.
        #[arg(long)]
        chapter: Option<String>,
        /// A note to print with the recipe.
        #[arg(long)]
        note: Option<String>,
    },
    /// The recipes in the book: id, title, chapter, added, structured or text, picture.
    List {
        /// Only this chapter's recipes (a chapter id).
        #[arg(long)]
        chapter: Option<String>,
    },
    /// File a recipe into another chapter.
    Move {
        /// A recipe's id, or the address it was saved from.
        who: String,
        /// A chapter id (`deckle cookbook chapters`).
        chapter: String,
    },
    /// Delete a recipe and its pictures.
    Remove {
        /// A recipe's id, or the address it was saved from.
        who: String,
    },
    /// Set a recipe's note; an empty string clears it.
    Note {
        /// A recipe's id, or the address it was saved from.
        who: String,
        text: String,
    },
    /// Write the book as an EPUB (`cookbook.epub`) in a folder, without sending it anywhere.
    /// Prints how many recipes in how many chapters, then the path.
    Build {
        /// The folder to write `cookbook.epub` into (made if it isn't there).
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Build the book if it changed (a book that hasn't is sent as it is) and send it. With a
    /// Kindle connected it goes now, unless that Kindle already has this build or is left out
    /// (`cookbook = false`); with none connected, it waits for each Kindle's next plug-in.
    /// `--to <folder>` exports `cookbook.epub` there instead and records nothing.
    Send {
        /// `device` (the default) for a connected Kindle, or a folder to export the EPUB to.
        #[arg(long)]
        to: Option<String>,
    },
    /// Choose a recipe's picture.
    Picture {
        /// A recipe's id, or the address it was saved from.
        who: String,
        #[arg(value_enum)]
        choice: PictureArg,
    },
    /// The book's chapters, in book order, with their recipe counts. Add, rename, remove and
    /// reorder them by editing cookbook.toml.
    Chapters,
    /// Draw an engraving-style illustration of a recipe (needs `[illustrations] enabled = true`
    /// in cookbook.toml and OPENAI_API_KEY). Sends the recipe's title, chapter name and up to
    /// eight key ingredients to the configured image endpoint, nothing else. The site's photo
    /// is kept; the recipe prints the drawing when its picture is `illustration`.
    Illustrate {
        /// A recipe's id, or the address it was saved from.
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        who: Option<String>,
        /// Every recipe without an illustration yet, one at a time.
        #[arg(long)]
        all: bool,
        /// Draw again where there already is an illustration.
        #[arg(long)]
        redraw: bool,
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
    /// Upgrade books Deckle didn't send from their originals on this computer (a calibre
    /// library, the folders in library.json's find_in): each comes back with Enhanced
    /// Typesetting, page numbers, X-Ray and Word Wise. The old copy and its .sdr are kept, so
    /// `put-back` undoes it. Books with DRM and store books are never touched.
    Upgrade {
        /// Only these books (as `books` names them); a book you're partway through has to be
        /// named, or `--include-reading` given.
        files: Vec<String>,
        /// Show what would be upgraded and change nothing.
        #[arg(long)]
        dry_run: bool,
        /// Don't ask first.
        #[arg(long)]
        yes: bool,
        /// Also books you're partway through (each opens at the start afterwards).
        #[arg(long)]
        include_reading: bool,
        #[arg(long)]
        previewer: Option<PathBuf>,
    },
    /// Undo an upgrade: the old copy goes back, Deckle's copy goes.
    PutBack { file: String },
    /// Copy every DRM-free book and its .sdr (reading position, highlights) into a new folder.
    /// Amazon's own books are left out: Amazon downloads them again. So is the paper.
    Backup {
        /// Where the backup's folder goes (default: kindle-backups/ beside library.json).
        #[arg(long)]
        into: Option<PathBuf>,
    },
    /// Put a backup's books on this Kindle. Books already there are left as they are.
    Restore { dir: PathBuf },
}

#[derive(Subcommand, Debug)]
pub enum HighlightsCommand {
    /// Books with highlights, how many, and how many are new.
    List,
    /// Write new highlights into the folder (remembered after the first --to).
    Export {
        #[arg(long)]
        to: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum KoreaderCommand {
    /// Copy every book's sidecar (progress, highlights, notes) and KOReader's settings,
    /// history, statistics and vocabulary into a new folder, with a manifest.json.
    Backup {
        /// `device` for the connected reader, or a folder (a copy of a reader's drive).
        #[arg(long, default_value = "device")]
        from: String,
        /// Where backups go (default: ~/Deckle/KOReader backups). Each lands in
        /// <device>/<YYYY-MM-DD HHMMSS>/ under it.
        #[arg(long)]
        to: Option<PathBuf>,
    },
    /// Put a backup's sidecars back beside their books, found by path or else by KOReader's
    /// partial MD5. A newer sidecar on the reader is kept.
    Restore {
        /// The backup's folder (the one holding manifest.json).
        backup: PathBuf,
        /// `device` for the connected reader, or a folder.
        #[arg(long, default_value = "device")]
        to: String,
        /// Also put back KOReader's settings, history, statistics and vocabulary.
        #[arg(long)]
        settings: bool,
        /// Overwrite files on the reader even when they're newer than the backup's.
        #[arg(long)]
        force: bool,
    },
    /// The plugin store: plugins Deckle recommends (pinned and checked), and GitHub's
    /// koreader-plugin topic (not reviewed). Goes online only when run.
    Store {
        /// `device` for the connected reader, or a folder (for install and updates).
        #[arg(long, default_value = "device", global = true)]
        from: String,
        #[command(subcommand)]
        action: StoreCommand,
    },
    /// KOReader's plugins on the reader: list, install, update, remove, roll back.
    Plugins {
        /// `device` for the connected reader, or a folder (a copy of a reader's drive).
        #[arg(long, default_value = "device", global = true)]
        from: String,
        #[command(subcommand)]
        action: PluginsCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum StoreCommand {
    /// The plugins Deckle recommends, each pinned to a reviewed release.
    List,
    /// GitHub repositories with the koreader-plugin topic (not reviewed by Deckle).
    Search {
        /// Words to look for; nothing lists them all.
        text: Vec<String>,
        /// Newest first, not most starred.
        #[arg(long)]
        updated: bool,
    },
    /// A repository's README and newest release.
    Show { repo: String },
    /// Install a recommended plugin (by name or repository: the reviewed release), or any
    /// repository's newest release.
    Install {
        plugin: String,
        #[arg(long)]
        yes: bool,
    },
    /// Installed plugins with a newer release (for recommended ones, a newer reviewed one).
    Updates,
}

#[derive(Subcommand, Debug)]
pub enum PluginsCommand {
    /// Every plugin (built in and added, on or off) and patch.
    List,
    /// Install a plugin from a .zip, a folder, or a GitHub repository (owner/repo or its URL):
    /// its newest release, else its default branch.
    Install {
        source: String,
        /// Don't ask before writing to the reader.
        #[arg(long)]
        yes: bool,
    },
    /// Install a plugin again from where it came from (or from SOURCE).
    Update {
        name: String,
        source: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Remove an added plugin (kept in deckle-backup/plugins, so `rollback` brings it back).
    Remove { name: String },
    /// Put back the copy of a plugin that the last install, update or remove kept.
    Rollback { name: String },
    /// Switch a plugin on from KOReader's next start (applied by the Deckle plugin).
    Enable { name: String },
    /// Switch a plugin off from KOReader's next start (applied by the Deckle plugin).
    Disable { name: String },
    /// Switch every patch in patches/ on or off.
    Patches {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum WordsCommand {
    /// Every English word looked up, once each, as a CSV Anki imports (File → Import):
    /// word, short definition, the sentence, the book and the day. Overwrites FILE.
    Anki {
        #[arg(long)]
        to: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum DevicesCommand {
    /// Name the connected Kindle ("Mom's Kindle"), so books can be sent to it by name. An empty
    /// name clears it.
    Name {
        name: String,
        /// With several Kindles connected: the one to name, by its current name or sync id.
        #[arg(long)]
        kindle: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum LibraryCommand {
    /// Search titles, authors and series in the library and connected Kindles.
    Search {
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
    },
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
    #[command(hide = true)]
    PressWorker {
        #[arg(long)]
        job: PathBuf,
    },
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
        /// Also write the edition bundle (`deckle-daily-YYYY-MM-DD.deckle`), the paper as data
        /// for a native reader, beside the EPUB.
        #[arg(long)]
        bundle: bool,
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
        /// Run even when the presses are stopped for this date (`digest stop`).
        #[arg(long)]
        force: bool,
    },
    /// What the paper keeps of one story page and the boilerplate it drops (newsletter
    /// sign-ups, "Advertisement", share prompts…), to tune the rules.
    Lint {
        /// A story's address, or a saved `.html` page.
        page: String,
    },
    /// Tomorrow's paper, so far: the press, the front page as it stands, the page plan and
    /// this week's reads.
    Next {
        /// Check the feeds for new headlines first (each at most twice an hour).
        #[arg(long)]
        fetch: bool,
    },
    /// Stop the presses: skip the next scheduled edition, once. `--undo` restarts them.
    Stop {
        #[arg(long)]
        undo: bool,
        /// The edition to skip, YYYY-MM-DD (default: the next scheduled one).
        #[arg(long)]
        date: Option<String>,
    },
    /// Deliver queued editions to a connected Kindle and prune old ones. Does nothing (and
    /// succeeds) when no Kindle is connected, so it is safe to run on a timer.
    Push {
        /// `device` for the first connected Kindle, or a folder standing in for its documents.
        #[arg(long, default_value = "device")]
        to: String,
    },
    /// Make the paper arrive by itself: the edition daily at a time, and the watcher from
    /// login on (macOS LaunchAgents, Windows Task Scheduler). No option: show what's set.
    Schedule {
        /// Time for the edition, 24-hour, e.g. 05:30. Saved in the paper's file as
        /// `[schedule] at`; every paper's job is then installed.
        #[arg(long, conflicts_with = "off")]
        at: Option<String>,
        /// With --at: the days it runs, e.g. `sat,sun` (default: every day).
        #[arg(long, requires = "at")]
        days: Option<String>,
        /// Remove every paper's job and the watcher; with --paper, only that paper's job.
        #[arg(long)]
        off: bool,
    },
    /// Stay running and push queued editions whenever a Kindle connects.
    Watch {
        /// Seconds between checks.
        #[arg(long, default_value_t = 20)]
        interval: u64,
    },
    /// Stay running and print every paper on its schedule (containers and servers: no launchd,
    /// Task Scheduler or cron).
    Daemon {
        /// Seconds between checks.
        #[arg(long, default_value_t = crate::DAEMON_TICK)]
        tick: u64,
    },
    /// Email the paper to your Kindle (`[email]` in digest.toml): the SMTP password and a test.
    Email {
        #[command(subcommand)]
        action: EmailCommand,
    },
    /// Papers you subscribe to: sign Deckle in with your own account.
    Subscribe {
        /// The paper's site, e.g. gazette.example. Alone: list every subscription and its state.
        site: Option<String>,
        #[arg(long)]
        username: Option<String>,
        /// Read the password from one line of stdin instead of asking.
        #[arg(long)]
        stdin: bool,
        /// Sign in now and report the result, without changing anything.
        #[arg(long)]
        test: bool,
        /// Remove the account and its keychain entry.
        #[arg(long)]
        forget: bool,
    },
    /// Today's puzzle (made and kept if there's none yet) and yesterday's answers.
    Puzzle,
    /// What From Your Shelf would print now, from the Kindle snapshots (`--forget` deletes them).
    Shelf {
        /// Delete every stored snapshot, highlights and looked-up words included.
        #[arg(long)]
        forget: bool,
    },
    /// The Morgue: every edition kept in a folder (`[morgue]` in digest.toml), searchable.
    Morgue {
        #[command(subcommand)]
        action: MorgueCommand,
    },
    /// How far you read the last editions, per section, with suggested edits to the paper.
    Readership {
        #[command(subcommand)]
        action: Option<ReadershipCommand>,
    },
    /// The papers this Mac prints (many papers): digest.toml's and each `papers/<slug>.toml`.
    Papers {
        #[command(subcommand)]
        action: Option<PapersCommand>,
    },
    /// Things you care about, each its own section, hunted across the news every morning.
    Interests {
        #[command(subcommand)]
        action: Option<InterestsCommand>,
    },
}

#[derive(Subcommand, Debug)]
pub enum InterestsCommand {
    /// Add one; the search is the name in quotes unless --query says otherwise.
    Add {
        /// The section's title, e.g. "Carolina Panthers".
        name: String,
        /// Your own Google News search instead of the quoted name (OR and -word work).
        #[arg(long)]
        query: Option<String>,
        /// Another search, merged in; a story naming it is relevant. Repeatable.
        #[arg(long = "also")]
        also: Vec<String>,
        /// A phrase that drops a story from the section. Repeatable.
        #[arg(long = "not")]
        not: Vec<String>,
        /// Stories kept (default 5).
        #[arg(long)]
        max_items: Option<usize>,
    },
    /// Remove one by name.
    Remove { name: String },
    /// Make something Deckle noticed an interest (`deckle digest interests` lists them).
    Accept { phrase: String },
    /// Set something Deckle noticed aside for 90 days.
    Dismiss { phrase: String },
    /// The stories behind something Deckle noticed, or behind an interest it suggested.
    Why { name: String },
}

#[derive(Subcommand, Debug)]
pub enum PapersCommand {
    /// Start another paper, `papers/<slug>.toml`, from the main paper's place.
    New {
        /// Its name, e.g. "The Kid Courier".
        #[arg(long)]
        title: String,
        /// Its file name (default: made from the title, "kid-courier").
        #[arg(long)]
        slug: Option<String>,
        /// Who it's for, e.g. "Maya".
        #[arg(long = "for")]
        reader: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum MorgueCommand {
    /// The editions kept in a month (default: this month).
    List {
        /// `YYYY-MM`.
        #[arg(long)]
        month: Option<String>,
    },
    /// Stories matching every word, newest edition first.
    Search {
        #[arg(required = true)]
        words: Vec<String>,
    },
    /// One edition: its weather, lead, how far you read, highlights and whether it's on the Kindle.
    Show {
        /// `YYYY-MM-DD`.
        date: String,
    },
    /// Send an edition to the Kindle again (queued for the next plug-in; kept a week there).
    Send {
        /// `YYYY-MM-DD`.
        date: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReadershipCommand {
    /// Make suggested edit N (as numbered in the report) to digest.toml.
    Apply { n: usize },
    /// Hide suggestion N for the next 14 editions.
    Dismiss { n: usize },
}

#[derive(Subcommand, Debug)]
pub enum ReaderServerCommand {
    /// Set the password KOReader signs in with (asked for without echo; its MD5 is kept).
    Password {
        /// Read one line from stdin instead, for scripts.
        #[arg(long)]
        stdin: bool,
    },
    /// Whether it's on, and the catalog and sync URLs to type into KOReader.
    Status,
}

#[derive(Subcommand, Debug)]
pub enum ShelvesCommand {
    /// The rules, numbered.
    List,
    /// A rule: shelf NAME from FROM, which is tag:<t>, series:<s> (series:* for one shelf a
    /// series), author:<a>, column:#<label>=<value> or search:<saved search>.
    Add {
        name: String,
        from: String,
        /// Only for these readers (kobo/<serial>, koreader/<id>); every reader without it.
        #[arg(long)]
        reader: Vec<String>,
    },
    /// Remove rule N; Deckle's shelves from it go the next time.
    Remove { n: usize },
    /// Apply the rules when a reader is plugged in and over Wi-Fi (`on`/`off`).
    Auto { on: String },
    /// Let Deckle write shelves on a Kobo (one signed in to a Kobo account may sync them).
    KoboOk { key: String },
    /// What the rules would change on the plugged-in reader.
    Preview {
        /// A folder standing in for a reader's drive.
        #[arg(long)]
        folder: Option<std::path::PathBuf>,
    },
    /// Change it now.
    Apply {
        #[arg(long)]
        folder: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum CalibreCommand {
    /// The library, calibredb, the columns chosen, and the next batch.
    Status,
    /// Write reading data when the watcher runs (once a minute at most).
    On,
    /// Stop writing.
    Off,
    /// Choose the column for progress, read, last_read, rating or highlights (`none` for none).
    Map { field: String, column: String },
    /// Write through calibre's content server (`http://host:8080/#library_id`), so calibre
    /// can stay open; `none` writes to the library folder.
    Server {
        url: Option<String>,
        #[arg(long)]
        username: Option<String>,
    },
    /// The content server's password, kept in the system keychain.
    Password {
        #[arg(long)]
        stdin: bool,
    },
    /// What the next batch would write, what's left alone, and what needs confirming.
    Preview,
    /// Write the next batch now.
    Write,
    /// A book matched only by title and author is the same book (a number from the preview,
    /// or `all`).
    Confirm { n: String },
    /// Add #deckle_progress, #deckle_read, #deckle_last_read, #deckle_rating and
    /// #deckle_highlights, and use them.
    /// calibre must be closed.
    CreateColumns,
    /// Highlights also into calibre's own annotations (`on`/`off`): calibre 9.18 or later.
    Annotations { on: String },
    /// Where calibredb is, when it isn't found (`none` to look again).
    Calibredb { path: Option<std::path::PathBuf> },
}

#[derive(Subcommand, Debug)]
pub enum WirelessCommand {
    /// Whether it's listening, the address to type into a reader, the readers seen, the queue.
    Status,
    /// Set the password a reader must give (asked for without echo), or clear it.
    Password {
        /// Read one line from stdin instead, for scripts.
        #[arg(long)]
        stdin: bool,
        /// No password.
        #[arg(long, conflicts_with = "stdin")]
        clear: bool,
    },
    /// Send books to a reader when it next connects: EPUBs cleaned as for a Kobo; Word, PDF,
    /// text and Markdown files imported first.
    Queue {
        /// The reader: its name, its id, or `any` for the next to connect.
        reader: String,
        #[arg(required = true)]
        books: Vec<std::path::PathBuf>,
        /// Send the files exactly as they are.
        #[arg(long)]
        as_is: bool,
    },
    /// Take books off a reader when it next connects, by their paths on it.
    Remove {
        reader: String,
        #[arg(required = true)]
        lpaths: Vec<String>,
    },
    /// Run the server in the foreground (the watcher runs it when `[wireless]` is on).
    Serve,
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
        Some("off") => Ok(None),
        Some(n) => n
            .parse::<u32>()
            .map(|n| Some(Some(n)))
            .map_err(|_| format!("--pages expects a number, `auto`, or `off`, got {n:?}")),
    }
}
