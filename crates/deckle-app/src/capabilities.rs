// SPDX-License-Identifier: GPL-3.0-or-later
//! What this install can do, reported rather than guessed: the web UI keeps a feature that
//! needs something missing on screen, with the reason.

use serde::Serialize;

use crate::commands::AppState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub web: bool,
    /// Kindle Previewer is found, at the settings' path or where Deckle looks for it.
    pub previewer: bool,
    /// Kindles can be reached over USB.
    pub usb: bool,
    /// Passwords can be kept in the system keychain.
    pub keychain: bool,
    /// The app can update itself.
    pub updater: bool,
    /// A file can be shown in the file manager.
    pub reveal: bool,
    /// `"install"`: the edition runs as a system job; `"config"`: the daemon reads
    /// `[schedule]` from the config.
    pub schedule: &'static str,
}

pub fn capabilities(web: bool, state: &AppState) -> Capabilities {
    Capabilities {
        web,
        previewer: crate::commands::previewer_status(state).found,
        usb: !web || kindle_visible(),
        keychain: !web,
        updater: !web,
        reveal: !web,
        schedule: if web { "config" } else { "install" },
    }
}

/// The desktop always reaches USB, Kindle plugged in or not. A container reaches it only when
/// the device was passed through, which shows as a Kindle it can see now.
fn kindle_visible() -> bool {
    !deckle_core::deliver::detect_mass_storage().is_empty()
        || !deckle_transport::usb::detect().is_empty()
}
