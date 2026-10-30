// SPDX-License-Identifier: GPL-3.0-or-later
//! Where a command's progress events go: the desktop window, or the web UI's event stream.

/// Sends `event` with its JSON `payload` to whoever is listening.
pub trait Events: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// Nobody is listening (tests, the CLI).
pub struct NoEvents;

impl Events for NoEvents {
    fn emit(&self, _event: &str, _payload: serde_json::Value) {}
}
