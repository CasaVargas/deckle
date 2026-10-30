// SPDX-License-Identifier: GPL-3.0-or-later
//! The state every command can reach.

use std::path::PathBuf;
use std::sync::Mutex;

/// App state: the settings file location and what the command line asked for.
pub struct AppState {
    pub settings_file: PathBuf,
    pub devices_file: PathBuf,
    pub preload: Mutex<Vec<String>>,
    /// `--export-to <dir>`: run the preloaded books into this folder as soon as they load.
    pub preload_target: Mutex<Option<String>>,
    /// `--daily` / `--kindle`: open on that tab (else `books`).
    pub start_tab: &'static str,
}
