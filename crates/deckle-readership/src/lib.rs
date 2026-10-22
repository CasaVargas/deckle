// SPDX-License-Identifier: GPL-3.0-or-later
//! How far each Deckle Daily edition was read (docs/superpowers/specs/2026-10-22-readership-report-design.md):
//! the Kindle's `.yjf` reading position, set against where each section of the edition starts.

pub mod krds;
pub mod layout;
pub mod state;

pub use krds::{KrdsError, Positions, parse_yjf, position_id};
pub use state::{OnDevice, Recorded, State, state_path};
