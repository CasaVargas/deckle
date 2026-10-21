// SPDX-License-Identifier: GPL-3.0-or-later
//! Known compatibility issues per Kindle model, transport and firmware (design §6: warn, never
//! refuse). The table is embedded; `docs/devices.md` says how a row gets added.

use serde::Serialize;

use crate::devices::{DeviceProfile, TransportKind};

/// The part of a send a rule is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Feature {
    Cover,
    /// The book itself getting onto the device and opening.
    Kfx,
    Xray,
    WordWise,
}

/// Inclusive firmware bounds; `None` is open-ended.
#[derive(Clone, Copy, Debug)]
pub struct FirmwareRange {
    pub min: Option<&'static str>,
    pub max: Option<&'static str>,
}

#[derive(Clone, Copy, Debug)]
pub struct Rule {
    /// Case-insensitive substring of the model hint; `None`: any model.
    pub model: Option<&'static str>,
    /// `None`: any transport.
    pub transport: Option<TransportKind>,
    /// `None`: every firmware. A range never matches unknown firmware.
    pub firmware: Option<FirmwareRange>,
    pub feature: Feature,
    /// Also a warning on a send that uses `feature`; otherwise the device card only.
    pub on_send: bool,
    pub message: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub feature: Feature,
    pub on_send: bool,
    pub message: String,
}

/// Seeded from `docs/devices.md`; firmware-range rows come as owner device checks record them.
pub const RULES: &[Rule] = &[
    Rule {
        model: None,
        transport: Some(TransportKind::Mtp),
        firmware: None,
        feature: Feature::Cover,
        on_send: true,
        message: "EBOK books without a real Amazon ASIN show no cover on this Kindle: \
                  system/thumbnails isn't writable over MTP. Type the book's ASIN, or send it \
                  as PDOC.",
    },
    Rule {
        model: None,
        transport: Some(TransportKind::Mtp),
        firmware: None,
        feature: Feature::WordWise,
        on_send: false,
        message: "Word Wise needs the Kindle's dictionary staged first: system/ isn't reachable \
                  over MTP, so copy kll.en.en.klld to documents/deckle/ on the device \
                  (docs/devices.md).",
    },
    Rule {
        model: Some("scribe"),
        transport: Some(TransportKind::Mtp),
        firmware: None,
        feature: Feature::Kfx,
        on_send: false,
        message: "A USB reset drops the Scribe 2024 off the bus until it's replugged. Deckle \
                  never resets it; if another tool does, unplug it and plug it back in.",
    },
];

/// The firmware token of a `system/version.txt` line: the first word made of digits and dots
/// with at least one dot (`Kindle 5.16.2.1.1 (4184940002)` → `5.16.2.1.1`).
pub fn parse_firmware(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|w| {
            w.contains('.')
                && w.starts_with(|c: char| c.is_ascii_digit())
                && w.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
        .map(|w| w.trim_end_matches('.').to_owned())
}

/// Numeric dot-separated components; `None` when any component isn't a number.
fn components(v: &str) -> Option<Vec<u64>> {
    v.trim().split('.').map(|c| c.parse().ok()).collect()
}

/// Compares two firmware versions component by component (missing components are 0).
pub fn compare_firmware(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let (a, b) = (components(a)?, components(b)?);
    let n = a.len().max(b.len());
    let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    Some(
        (0..n)
            .map(|i| at(&a, i).cmp(&at(&b, i)))
            .find(|o| o.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal),
    )
}

impl FirmwareRange {
    /// Unknown or unparseable firmware is never in range.
    pub fn contains(&self, firmware: Option<&str>) -> bool {
        use std::cmp::Ordering::{Greater, Less};
        let Some(fw) = firmware else {
            return false;
        };
        let above_min = match self.min {
            Some(min) => matches!(compare_firmware(fw, min), Some(o) if o != Less),
            None => components(fw).is_some(),
        };
        let below_max = match self.max {
            Some(max) => matches!(compare_firmware(fw, max), Some(o) if o != Greater),
            None => true,
        };
        above_min && below_max
    }
}

impl Rule {
    pub fn matches(&self, model: &str, transport: TransportKind, firmware: Option<&str>) -> bool {
        self.model
            .is_none_or(|m| model.to_lowercase().contains(&m.to_lowercase()))
            && self.transport.is_none_or(|t| t == transport)
            && self.firmware.is_none_or(|r| r.contains(firmware))
    }
}

/// Every row of `rules` that applies to the device.
pub fn check_rules(
    rules: &[Rule],
    model: &str,
    transport: TransportKind,
    firmware: Option<&str>,
) -> Vec<Warning> {
    rules
        .iter()
        .filter(|r| r.matches(model, transport, firmware))
        .map(|r| Warning {
            feature: r.feature,
            on_send: r.on_send,
            message: r.message.to_owned(),
        })
        .collect()
}

/// [`check_rules`] over the built-in table.
pub fn check(model: &str, transport: TransportKind, firmware: Option<&str>) -> Vec<Warning> {
    check_rules(RULES, model, transport, firmware)
}

/// [`check`] for a remembered device (the device card).
pub fn check_profile(p: &DeviceProfile) -> Vec<Warning> {
    check(&p.model_hint, p.transport, p.firmware.as_deref())
}

/// What a finished send actually used, so only relevant rows become send warnings.
#[derive(Clone, Copy, Debug, Default)]
pub struct SendUse {
    /// Shipped as EBOK with a generated ASIN and no thumbnail from Deckle.
    pub ebok_without_cover_source: bool,
    pub xray: bool,
    pub word_wise: bool,
}

impl SendUse {
    fn uses(&self, f: Feature) -> bool {
        match f {
            Feature::Cover => self.ebok_without_cover_source,
            Feature::Kfx => true,
            Feature::Xray => self.xray,
            Feature::WordWise => self.word_wise,
        }
    }
}

/// The `on_send` rows matching the device whose feature this send used, as report warnings.
pub fn send_warnings(
    rules: &[Rule],
    model: &str,
    transport: TransportKind,
    firmware: Option<&str>,
    used: &SendUse,
) -> Vec<String> {
    check_rules(rules, model, transport, firmware)
        .into_iter()
        .filter(|w| w.on_send && used.uses(w.feature))
        .map(|w| w.message)
        .collect()
}
