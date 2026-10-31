// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's commands without Tauri: the desktop app wraps them as Tauri commands, and the web
//! UI serves the same functions over HTTP. Every function here blocks; callers run it off their
//! async runtime.

pub mod books;
pub mod capabilities;
pub mod commands;
pub mod cookbook;
pub mod daily;
pub mod dispatch;
pub mod events;
pub mod kindle;
pub mod koreader;
pub mod morgue;
pub mod next;
pub mod press;
pub mod reader_server;
pub mod readership;
pub mod shelf;
pub mod state;
pub mod upgrades;
pub mod wireless;

use std::sync::{Mutex, MutexGuard};

/// Take it through [`config_write`]; public so a test can see it held.
pub static CONFIG_WRITE: Mutex<()> = Mutex::new(());

/// Held while a command rewrites `digest.toml`: one lock for the whole process, whichever
/// front end the command came from.
pub fn config_write() -> Result<MutexGuard<'static, ()>, String> {
    CONFIG_WRITE
        .lock()
        .map_err(|_| "Configuration is busy; reopen settings.".into())
}
