// SPDX-License-Identifier: GPL-3.0-or-later
//! Device profiles (design §6): every Kindle Deckle has seen, with its transport, whether its
//! thumbnail folder is writable, the owner's per-device override, and whether spike 2 passed.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cover_rule::{DeviceClass, Override};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    MassStorage,
    Mtp,
}

/// A device as detected right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// Stable key: `ms:<model hint>` for mass storage, `mtp:<serial>` for MTP.
    pub key: String,
    pub root: Option<String>,
    pub model_hint: String,
    pub transport: TransportKind,
    pub thumbnails_writable: bool,
    pub serial: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceProfile {
    pub key: String,
    pub model_hint: String,
    pub transport: TransportKind,
    pub thumbnails_writable: bool,
    pub serial: Option<String>,
    pub last_root: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
    /// `pdoc` | `ebok` chosen by the owner for this device.
    pub cde_override: Option<String>,
    /// Spike 2 passed on this device: a real ASIN keeps our sidecars and shows the store cover.
    pub verified_real_asin: bool,
}

impl Default for DeviceProfile {
    fn default() -> Self {
        DeviceProfile::new("", "")
    }
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // civil date from days since epoch (Howard Hinnant's algorithm), UTC
    let days = (secs / 86400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, mi, s) = ((secs % 86400) / 3600, (secs % 3600) / 60, secs % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

impl DeviceProfile {
    pub fn new(key: &str, model_hint: &str) -> Self {
        let now = now_iso();
        DeviceProfile {
            key: key.to_owned(),
            model_hint: model_hint.to_owned(),
            transport: TransportKind::MassStorage,
            thumbnails_writable: false,
            serial: None,
            last_root: None,
            first_seen: now.clone(),
            last_seen: now,
            cde_override: None,
            verified_real_asin: false,
        }
    }

    pub fn device_class(&self) -> DeviceClass {
        match self.transport {
            TransportKind::MassStorage => DeviceClass::MassStorage {
                thumbnails_writable: self.thumbnails_writable,
            },
            TransportKind::Mtp => DeviceClass::Mtp,
        }
    }

    pub fn user_override(&self) -> Override {
        match self.cde_override.as_deref() {
            Some("ebok") => Override::PreferFeatures,
            Some("pdoc") => Override::PreferCover,
            _ => Override::None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeviceRegistry {
    pub profiles: Vec<DeviceProfile>,
}

impl DeviceRegistry {
    pub fn load(file: &Path) -> Self {
        std::fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> Result<(), String> {
        if let Some(p) = file.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            file,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }

    /// Records a sighting: refreshes the live facts, keeps the owner's choices.
    pub fn remember(&mut self, seen: &Seen) -> DeviceProfile {
        let now = now_iso();
        let idx = match self.profiles.iter().position(|p| p.key == seen.key) {
            Some(i) => i,
            None => {
                self.profiles
                    .push(DeviceProfile::new(&seen.key, &seen.model_hint));
                self.profiles.len() - 1
            }
        };
        let p = &mut self.profiles[idx];
        p.model_hint = seen.model_hint.clone();
        p.transport = seen.transport;
        p.thumbnails_writable = seen.thumbnails_writable;
        p.serial = seen.serial.clone();
        p.last_root = seen.root.clone();
        p.last_seen = now;
        p.clone()
    }

    pub fn get(&self, key: &str) -> Option<&DeviceProfile> {
        self.profiles.iter().find(|p| p.key == key)
    }

    pub fn set_override(&mut self, key: &str, cde: Option<&str>) {
        if let Some(p) = self.profiles.iter_mut().find(|p| p.key == key) {
            p.cde_override = cde.map(str::to_owned);
        }
    }

    pub fn set_verified_real_asin(&mut self, key: &str, verified: bool) {
        if let Some(p) = self.profiles.iter_mut().find(|p| p.key == key) {
            p.verified_real_asin = verified;
        }
    }
}

/// The `Seen` record for a mass-storage device.
pub fn seen_mass_storage(d: &crate::deliver::Device) -> Seen {
    Seen {
        key: format!("ms:{}", d.model_hint),
        root: Some(d.root.to_string_lossy().into_owned()),
        model_hint: d.model_hint.clone(),
        transport: TransportKind::MassStorage,
        thumbnails_writable: d.thumbnails_writable,
        serial: None,
    }
}
