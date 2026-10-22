// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle.json`, schema 1 (library design §3). `metadata` caches the source; `id`, `added`,
//! `asin` and `sends` belong to the sidecar. Fields Deckle doesn't know are kept on rewrite.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SCHEMA: u32 = 1;
pub const FILE_NAME: &str = "deckle.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub schema: u32,
    /// The generated ASIN's seed: the EPUB's first identifier, else `title|author`.
    pub id: String,
    /// The source file's name in the book's folder.
    pub source: String,
    pub sha256: String,
    pub added: String,
    /// The `B0…DECK` ASIN every build of this book carries.
    pub asin: String,
    #[serde(default)]
    pub metadata: Metadata,
    /// The latest send per device.
    #[serde(default)]
    pub sends: Vec<SendRecord>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub title: String,
    pub authors: Vec<String>,
    pub author_sort: Option<String>,
    pub series: Option<SeriesRef>,
    pub language: String,
    pub isbn: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeriesRef {
    pub name: String,
    pub index: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendRecord {
    /// The device's profile key (`ms:…`, `mtp:…`), or `folder:<path>` for an export.
    pub device: String,
    pub device_name: String,
    /// The file name on the device (in `documents/`) or in the folder.
    pub file: String,
    pub at: String,
    /// `PDOC` | `EBOK`
    pub cde: Option<String>,
    pub asin: Option<String>,
    pub xray: bool,
    pub word_wise: bool,
    /// Written by a Deckle that stamps engines (Kindle-updates design §3); `false` on older
    /// records.
    pub stamped: bool,
    /// `deckle_xray::ENGINE` when X-Ray shipped.
    pub xray_engine: Option<u32>,
    /// The Word Wise database's `version+build` when Word Wise shipped.
    pub wordwise_db: Option<String>,
    /// The KFX's end position (`erl`) when sent, so a reading position becomes a percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Sidecar {
    /// Replaces the send to the same device, or adds one.
    pub fn record(&mut self, send: SendRecord) {
        self.sends.retain(|s| s.device != send.device);
        self.sends.push(send);
    }

    /// Drops the send to `device` named `file`; true when there was one.
    pub fn forget(&mut self, device: &str, file: &str) -> bool {
        let before = self.sends.len();
        self.sends
            .retain(|s| !(s.device == device && s.file == file));
        self.sends.len() != before
    }

    pub fn send_to(&self, device: &str) -> Option<&SendRecord> {
        self.sends.iter().find(|s| s.device == device)
    }
}
