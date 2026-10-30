// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle pipeline stages.

pub mod accessibility;
pub mod bind;
pub mod clippings;
pub mod compat;
pub mod cover_rule;
pub mod covers;
pub mod deliver;
pub mod devices;
pub mod epub;
#[cfg(test)]
mod fake_mtp;
pub mod import;
pub mod inspect;
pub mod inventory;
pub mod kepub;
pub mod kobo;
pub mod matcher;
pub mod metadata_fill;
pub mod normalize;
pub mod notes;
pub mod pipeline;
pub mod preflight;
pub mod preset;
pub mod previewer;
pub mod queue;
pub mod quiet;
pub mod reader;
pub mod remarkable;
pub mod sync;
pub mod thumbnail;
pub mod typography;
pub mod usb_serial;
pub mod vocab;
pub mod whoswho;
pub mod wordwise;
pub mod xray;
pub mod yjf;
