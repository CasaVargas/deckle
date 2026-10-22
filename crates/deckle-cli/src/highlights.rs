// SPDX-License-Identifier: GPL-3.0-or-later
//! Highlights out as Markdown (highlights export design): where Deckle keeps its state; the
//! export itself is `deckle_highlights::export_clippings`.

use std::path::PathBuf;

pub use deckle_highlights::{Outcome, export_clippings as export};

/// `highlights.json`, next to `library.json` and `digest.toml`.
pub fn state_path() -> PathBuf {
    deckle_library::config::default_path().with_file_name("highlights.json")
}
