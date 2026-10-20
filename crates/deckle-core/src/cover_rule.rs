// SPDX-License-Identifier: GPL-3.0-or-later
//! Design §5: covers are non-negotiable. Decides PDOC vs EBOK, which ASIN to stamp, and
//! whether Deckle writes the cover thumbnail itself, per device class.

use deckle_kfx::convert::CdeType;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceClass {
    /// USB mass-storage Kindle; `thumbnails_writable` says `system/thumbnails` is reachable.
    MassStorage { thumbnails_writable: bool },
    /// MTP-only Kindle (Scribe 2024, Colorsoft, Paperwhite 2024): no thumbnail folder.
    Mtp,
    /// Export to a folder: the device is unknown.
    Folder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchResult {
    None,
    /// A real Amazon ASIN candidate; `verified` means spike 2 confirmed the device keeps our
    /// sidecars for that kind of ASIN (a per-device-profile flag).
    Candidate {
        asin: String,
        verified: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Override {
    None,
    PreferFeatures,
    PreferCover,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum AsinChoice {
    Generated,
    Real(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Decision {
    pub cde: CdeType,
    pub write_thumbnail: bool,
    pub asin: AsinChoice,
    pub reason: String,
}

/// An ASIN the owner typed (design §3): 10 letters or digits, uppercased.
pub fn manual_asin(input: &str) -> Result<String, String> {
    let asin = input.trim().to_ascii_uppercase();
    if asin.len() == 10 && asin.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(asin)
    } else {
        Err(format!(
            "\"{}\" is not an ASIN: it must be 10 letters or digits (e.g. B0…)",
            input.trim()
        ))
    }
}

pub fn decide(device: &DeviceClass, matched: &MatchResult, user: Override) -> Decision {
    let thumbs = matches!(
        device,
        DeviceClass::MassStorage {
            thumbnails_writable: true
        }
    );
    match user {
        Override::PreferCover => {
            return Decision {
                cde: CdeType::Pdoc,
                write_thumbnail: false,
                asin: AsinChoice::Generated,
                reason: "Your override: cover first (PDOC). X-Ray and Word Wise withheld.".into(),
            };
        }
        Override::PreferFeatures => {
            // The owner accepted the grey-cover risk; a real ASIN is strictly better than a
            // generated one here (and it is how spike 2 gets run).
            let asin = match matched {
                MatchResult::Candidate { asin, .. } => AsinChoice::Real(asin.clone()),
                MatchResult::None => AsinChoice::Generated,
            };
            return Decision {
                cde: CdeType::Ebok,
                write_thumbnail: thumbs,
                reason: if let AsinChoice::Real(a) = &asin {
                    format!(
                        "Your override: features first (EBOK) as Amazon edition {a}; the Kindle \
                         fetches the store cover."
                    )
                } else if thumbs {
                    "Your override: features first (EBOK) with Deckle's thumbnail.".into()
                } else {
                    "Your override: features first (EBOK). The cover may show grey on this Kindle."
                        .into()
                },
                asin,
            };
        }
        Override::None => {}
    }
    if thumbs {
        return Decision {
            cde: CdeType::Ebok,
            write_thumbnail: true,
            asin: AsinChoice::Generated,
            reason: "Everything: EBOK for X-Ray and Word Wise, cover from Deckle's thumbnail."
                .into(),
        };
    }
    match (device, matched) {
        (DeviceClass::Folder, _) => Decision {
            cde: CdeType::Pdoc,
            write_thumbnail: false,
            asin: AsinChoice::Generated,
            reason: "Exported as PDOC so the cover shows on any Kindle; X-Ray and Word Wise withheld.".into(),
        },
        (_, MatchResult::Candidate { asin, verified: true }) => Decision {
            cde: CdeType::Ebok,
            write_thumbnail: false,
            asin: AsinChoice::Real(asin.clone()),
            reason: format!("Matched Amazon edition {asin}: EBOK, the Kindle fetches the store cover."),
        },
        (_, MatchResult::Candidate { asin, verified: false }) => Decision {
            cde: CdeType::Pdoc,
            write_thumbnail: false,
            asin: AsinChoice::Generated,
            reason: format!(
                "Matched {asin}, but this Kindle has not been verified to keep sidecars for a real ASIN (spike 2); PDOC keeps the cover, X-Ray and Word Wise withheld."
            ),
        },
        (_, MatchResult::None) => Decision {
            cde: CdeType::Pdoc,
            write_thumbnail: false,
            asin: AsinChoice::Generated,
            reason: "No thumbnail folder on this Kindle: PDOC keeps the cover; X-Ray and Word Wise withheld until an Amazon edition is matched.".into(),
        },
    }
}
