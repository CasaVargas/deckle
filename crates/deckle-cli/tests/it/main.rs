// SPDX-License-Identifier: GPL-3.0-or-later
//! All integration tests in one binary: one link instead of one per file.

mod accessibility;
mod backup;
mod bind;
mod cli;
mod cookbook;
#[cfg(unix)]
mod cookbook_delivery;
mod daemon;
mod export;
mod fb2;
mod highlights;
mod interests;
mod kfx_anchors;
#[cfg(unix)]
mod kindles;
mod kobo;
mod koreader;
#[cfg(unix)]
mod learn;
mod lint;
mod markdown;
mod mobi;
mod morgue;
mod odt;
#[cfg(unix)]
mod papers;
mod pocketbook;
mod press;
mod puzzle;
mod reader_server;
mod readership;
mod remarkable;
mod rtf;
mod search;
mod series;
mod shelf;
mod upgrade;
mod words;

mod press_worker;
