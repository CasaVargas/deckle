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
    /// Run the whole pipeline on one or more EPUBs.
    Run {
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
        /// This Kindle keeps Deckle's sidecars for real ASINs (spike 2 passed).
        #[arg(long)]
        verified_asin: bool,
        #[arg(long)]
        keep_work_dir: bool,
    },
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
