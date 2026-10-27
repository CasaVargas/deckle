// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kitchen Book: a cookbook built from saved recipe links
//! (docs/superpowers/specs/2026-10-26-cookbook-design.md).

pub mod add;
pub mod filing;
pub mod index;
pub mod picture;
pub mod recipe;
pub mod store;

pub use filing::{Chapter, EVERYTHING_ELSE, default_chapters, file_into};
pub use recipe::{Group, Recipe, iso_minutes, parse_recipe};
