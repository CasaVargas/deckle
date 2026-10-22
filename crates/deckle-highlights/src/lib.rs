// SPDX-License-Identifier: GPL-3.0-or-later
//! Kindle highlights out as Markdown (highlights export design): parse `My Clippings.txt`, group
//! it into books, and keep one append-only Markdown file per book.

pub mod parse;

pub use parse::{Added, Entry, Kind, ParseError, Parsed, parse};
