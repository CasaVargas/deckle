// SPDX-License-Identifier: GPL-3.0-or-later
//! Whether Deckle could build a book on a Kindle better now (Kindle-updates design §2–3):
//! an older build (engine stamps), or a sidecar the send shipped that is gone from the Kindle.

use serde::Serialize;

use crate::SendRecord;

/// What a send would be built with today.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Current {
    pub xray_engine: u32,
    /// `None`: no Word Wise database yet, so Word Wise isn't judged.
    pub wordwise_db: Option<String>,
}

impl Current {
    pub fn now(wordwise: &deckle_core::wordwise::WordWiseOptions) -> Self {
        Current {
            xray_engine: deckle_xray::ENGINE,
            wordwise_db: deckle_core::wordwise::current_database_version(wordwise),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reason {
    /// Sent before Deckle stamped engine versions.
    Unstamped,
    XRayEngine {
        sent: u32,
        now: u32,
    },
    WordWiseDatabase {
        sent: String,
        now: String,
    },
    MissingXRay,
    MissingWordWise,
}

impl Reason {
    pub fn describe(&self) -> String {
        match self {
            Reason::Unstamped => "sent before Deckle tracked versions".into(),
            Reason::XRayEngine { sent, now } => format!("X-Ray engine {sent} → {now}"),
            Reason::WordWiseDatabase { sent, now } => {
                format!("Word Wise database {sent} → {now}")
            }
            Reason::MissingXRay => "its X-Ray file is gone from the Kindle".into(),
            Reason::MissingWordWise => "its Word Wise file is gone from the Kindle".into(),
        }
    }

    /// An "Older build" reason (else a missing feature).
    pub fn is_older(&self) -> bool {
        matches!(
            self,
            Reason::Unstamped | Reason::XRayEngine { .. } | Reason::WordWiseDatabase { .. }
        )
    }
}

/// Why `send` (the last send of this book to this Kindle) can be updated, given which
/// sidecars the Kindle has now. Empty: current.
pub fn freshness(
    send: &SendRecord,
    on_device_xray: bool,
    on_device_word_wise: bool,
    current: &Current,
) -> Vec<Reason> {
    let mut out = Vec::new();
    if !send.stamped {
        out.push(Reason::Unstamped);
    } else {
        if send.xray
            && let Some(sent) = send.xray_engine
            && sent < current.xray_engine
        {
            out.push(Reason::XRayEngine {
                sent,
                now: current.xray_engine,
            });
        }
        if send.word_wise
            && let (Some(sent), Some(now)) = (&send.wordwise_db, &current.wordwise_db)
            && sent != now
        {
            out.push(Reason::WordWiseDatabase {
                sent: sent.clone(),
                now: now.clone(),
            });
        }
    }
    if send.xray && !on_device_xray {
        out.push(Reason::MissingXRay);
    }
    if send.word_wise && !on_device_word_wise {
        out.push(Reason::MissingWordWise);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Current {
        Current {
            xray_engine: 2,
            wordwise_db: Some("2026.10+3".into()),
        }
    }

    fn sent(xray_engine: Option<u32>, db: Option<&str>) -> SendRecord {
        SendRecord {
            stamped: true,
            xray: xray_engine.is_some(),
            word_wise: db.is_some(),
            xray_engine,
            wordwise_db: db.map(str::to_owned),
            ..Default::default()
        }
    }

    #[test]
    fn current_stamps_are_current() {
        assert!(freshness(&sent(Some(2), Some("2026.10+3")), true, true, &now()).is_empty());
        // Nothing shipped, nothing to judge.
        assert!(freshness(&sent(None, None), false, false, &now()).is_empty());
    }

    #[test]
    fn an_unstamped_record_is_an_older_build() {
        let old = SendRecord {
            xray: true,
            ..Default::default()
        };
        assert_eq!(freshness(&old, true, false, &now()), [Reason::Unstamped]);
        assert!(Reason::Unstamped.is_older());
    }

    #[test]
    fn older_engines_and_other_databases_are_older_builds() {
        assert_eq!(
            freshness(&sent(Some(1), Some("2026.09+1")), true, true, &now()),
            [
                Reason::XRayEngine { sent: 1, now: 2 },
                Reason::WordWiseDatabase {
                    sent: "2026.09+1".into(),
                    now: "2026.10+3".into()
                }
            ]
        );
        assert_eq!(
            freshness(&sent(Some(1), None), true, false, &now())[0].describe(),
            "X-Ray engine 1 → 2"
        );
    }

    #[test]
    fn word_wise_is_not_judged_without_a_current_database() {
        let no_db = Current {
            xray_engine: 2,
            wordwise_db: None,
        };
        assert!(freshness(&sent(None, Some("2026.09+1")), false, true, &no_db).is_empty());
    }

    #[test]
    fn a_shipped_sidecar_gone_from_the_kindle_is_missing() {
        assert_eq!(
            freshness(&sent(Some(2), Some("2026.10+3")), false, false, &now()),
            [Reason::MissingXRay, Reason::MissingWordWise]
        );
        assert!(!Reason::MissingXRay.is_older());
    }
}
