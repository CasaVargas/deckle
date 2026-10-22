// SPDX-License-Identifier: GPL-3.0-or-later
//! Kindle highlights out as Markdown (highlights export design): parse `My Clippings.txt`, group
//! it into books, and keep one append-only Markdown file per book.

pub mod book;
pub mod export;
pub mod parse;
pub mod render;
pub mod run;
pub mod state;

pub use book::{Book, EDITIONS_KEY, Item, edition_date, entry_id, group};
pub use export::{ExportReport, Pending, Written, export, file_name, pending};
pub use parse::{Added, Entry, Kind, ParseError, Parsed, parse};
pub use render::{passage_start, render_items, render_new_file};
pub use run::{Outcome, export_clippings};
pub use state::{BookState, State};
