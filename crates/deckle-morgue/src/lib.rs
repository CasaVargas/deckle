// SPDX-License-Identifier: GPL-3.0-or-later
//! The Morgue (docs/superpowers/specs/2026-10-22-morgue-design.md): every Deckle Daily edition
//! kept in a plain folder, with a small JSON sidecar to search.

pub mod sidecar;

pub use sidecar::{Files, Lead, SectionText, Sidecar, Story, TEXT_CAP, from_gathered, plain_text};
