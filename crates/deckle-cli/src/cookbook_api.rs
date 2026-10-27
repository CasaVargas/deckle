// SPDX-License-Identifier: GPL-3.0-or-later
//! The desktop's window onto the cookbook's delivery (cookbook design §8.2): the CLI's own
//! functions, made public. Nothing here decides anything; `deckle cookbook send` and `list`
//! run the same code.

use std::path::{Path, PathBuf};

use deckle_digest::config::SyncConfig;
use deckle_digest::cookbook::store::Store;
use serde::Serialize;

pub use super::cookbook_delivery::{KindleState, KindleStatus};

/// What [`send_now`] did, as `deckle cookbook send --json` says it (less the push's details).
#[derive(Debug, Serialize)]
pub struct SendSummary {
    /// The build that was sent or offered.
    pub build: String,
    /// "sent", "up_to_date", "off", "all_current", "queued" or "exported".
    pub result: &'static str,
    /// The Kindle's name (or sync id), when one took part.
    pub kindle: Option<String>,
    /// The EPUB, for an export.
    pub exported: Option<PathBuf>,
    /// The line `deckle cookbook send` prints.
    pub message: String,
    /// Recipes in the book, when this call had to build it first.
    pub built: Option<usize>,
    #[serde(skip)]
    pub(crate) outcome: Option<super::DigestOutcome>,
}

/// `digest.toml`'s `[sync]` at `path`: the household's settings and the Kindles it names. No
/// file is no settings; one that won't load is an error.
pub fn sync_config_at(path: &Path) -> Result<SyncConfig, String> {
    if !path.exists() {
        return Ok(Default::default());
    }
    Ok(deckle_digest::Config::load(path)?.sync)
}

/// Where each Kindle stands with the cookbook (`cookbook list --json`'s `kindles`). A record that
/// can't be read is an error, never "Never sent".
pub fn statuses(store: &Store, sync: &SyncConfig) -> Result<Vec<KindleStatus>, String> {
    super::cookbook_delivery::statuses(store, sync)
}

/// Whether the book has changes no build has picked up, and why the last attempt failed (only
/// said while changes wait): `cookbook list --json`'s `pending_build` and `build_error`.
pub fn unbuilt(store: &Store) -> (bool, Option<String>) {
    super::cookbook_delivery::unbuilt(store)
}

/// Builds the book if it changed and sends it to the connected Kindle, or queues it for each
/// Kindle's next plug-in: `deckle cookbook send`, run in-process.
pub fn send_now(store: &Store) -> Result<SendSummary, String> {
    super::cookbook::send_core(store, None, &mut |_| {})
}

/// Whether the background watcher (which rebuilds the book and delivers it when a Kindle
/// connects) is installed.
pub fn watcher_installed() -> bool {
    deckle_digest::schedule::status().watch
}
