// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle pipeline stages.

pub mod compat;
pub mod cover_rule;
pub mod deliver;
pub mod devices;
pub mod epub;
#[cfg(test)]
mod fake_mtp;
pub mod inspect;
pub mod inventory;
pub mod matcher;
pub mod metadata_fill;
pub mod normalize;
pub mod notes;
pub mod pipeline;
pub mod previewer;
pub mod queue;
pub mod sync;
pub mod thumbnail;
pub mod typography;
pub mod wordwise;
pub mod xray;
