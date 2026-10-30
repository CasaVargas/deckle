// SPDX-License-Identifier: GPL-3.0-or-later
//! The reader server's card (KOReader hub design §A.4): whether a password is set, and the two
//! URLs to type into KOReader. The on switch, port and username are `[reader_server]`, saved
//! with the rest of the Daily settings; the watcher picks them up on its next tick.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ReaderServerStatus {
    /// A password is set (its MD5 is kept; nothing reads it back to the window).
    pub password_set: bool,
    /// KOReader's catalog URL.
    pub opds: String,
    /// KOReader's sync server URL.
    pub sync: String,
}

fn status_at(main: &std::path::Path) -> ReaderServerStatus {
    let port = deckle_digest::Config::load(main)
        .map(|c| c.reader_server.port)
        .unwrap_or(deckle_digest::config::READER_SERVER_PORT);
    let (opds, sync) = deckle_readerserver::urls(port);
    ReaderServerStatus {
        password_set: deckle_readerserver::auth::load_key(&deckle_readerserver::auth::key_path(
            main,
        ))
        .is_some(),
        opds,
        sync,
    }
}

pub fn reader_server_status() -> ReaderServerStatus {
    status_at(&deckle_digest::config::default_path())
}

/// Keeps the password's MD5 beside `digest.toml`, owner-only.
pub fn reader_server_set_password(password: String) -> Result<ReaderServerStatus, String> {
    let main = deckle_digest::config::default_path();
    deckle_readerserver::auth::save_password(
        &deckle_readerserver::auth::key_path(&main),
        &password,
    )
    .map_err(|e| e.to_string())?;
    Ok(status_at(&main))
}
