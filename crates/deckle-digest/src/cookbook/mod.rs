// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kitchen Book: a cookbook built from saved recipe links
//! (docs/superpowers/specs/2026-10-26-cookbook-design.md).

pub mod recipe;

pub use recipe::{Group, Recipe, iso_minutes, parse_recipe};
