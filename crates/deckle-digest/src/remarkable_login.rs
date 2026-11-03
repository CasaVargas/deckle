// SPDX-License-Identifier: GPL-3.0-or-later
//! Signing in to a reMarkable over SSH (fixed-layout spec §12.5): the password from the
//! keychain, the host key trusted on first use and then pinned beside `digest.toml`. Shared by
//! the command line and the app's Devices tab.

use std::path::{Path, PathBuf};

/// The reMarkable's pinned SSH host key: `remarkable-hostkey.json` beside `digest.toml`.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct HostKeyPin {
    pub host: String,
    pub fingerprint: String,
}

pub fn hostkey_path(config: &Path) -> PathBuf {
    config
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("remarkable-hostkey.json")
}

/// Connects to the reMarkable over SSH as `[remarkable]` says, with the password from the
/// keychain (or `DECKLE_REMARKABLE_PASSWORD` without one) and the host key trusted on first
/// use, then pinned (fixed-layout spec §12.5). The fingerprint comes back with the shell.
pub fn connect_with(
    rm: &crate::config::RemarkableConfig,
    config: &Path,
) -> Result<(deckle_core::remarkable_ssh::SshShell, String), String> {
    use crate::secret::{self, SystemKeyring};
    let host = rm.ssh_host();
    let (password, _) =
        secret::remarkable_password(host, &SystemKeyring, &|k| std::env::var(k).ok())
            .map_err(|e| e.to_string())?;
    let pin_file = hostkey_path(config);
    let pinned: Option<HostKeyPin> = std::fs::read(&pin_file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .filter(|p: &HostKeyPin| p.host == host);
    let c = deckle_core::remarkable_ssh::connect(
        host,
        rm.ssh_port(),
        &password,
        pinned.as_ref().map(|p| p.fingerprint.as_str()),
        std::time::Duration::from_secs(5),
    )
    .map_err(|e| e.to_string())?;
    if pinned.is_none() {
        let pin = HostKeyPin {
            host: host.to_owned(),
            fingerprint: c.fingerprint.clone(),
        };
        std::fs::write(
            &pin_file,
            serde_json::to_vec_pretty(&pin).expect("two strings"),
        )
        .map_err(|e| format!("couldn't pin the host key at {}: {e}", pin_file.display()))?;
    }
    Ok((c.shell, c.fingerprint))
}

pub fn connect(
    rm: &crate::config::RemarkableConfig,
    config: &Path,
) -> Result<deckle_core::remarkable_ssh::SshShell, String> {
    connect_with(rm, config).map(|(s, _)| s)
}
