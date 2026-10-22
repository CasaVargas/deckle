// SPDX-License-Identifier: GPL-3.0-or-later
//! Kindle highlights out as Markdown (highlights export design): parse `My Clippings.txt`, group
//! it into books, and keep one append-only Markdown file per book.

pub mod book;
pub mod parse;
pub mod render;

pub use book::{Book, EDITIONS_KEY, Item, edition_date, entry_id, group};
pub use parse::{Added, Entry, Kind, ParseError, Parsed, parse};
pub use render::{passage_start, render_items, render_new_file};
