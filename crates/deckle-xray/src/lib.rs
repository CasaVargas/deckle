// SPDX-License-Identifier: GPL-3.0-or-later
//! Kindle X-Ray sidecars: the `.asc` writer, entity extraction, and description sources.

pub mod asc;
pub mod extract;
pub mod llm;
pub mod wiki;

/// What a book's X-Ray is built with. Bump it by hand when extraction, descriptions or the
/// sidecar change what a book gets: books sent with an older one show as updatable
/// (Kindle-updates design §3).
pub const ENGINE: u32 = 1;
