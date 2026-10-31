// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings → Library → Shelves from calibre (calibre bridge design §6.4): the rules, and what
//! they'd change on the plugged-in reader, with Fix shelves to change it now.

use deckle_library::shelves::{self, Config, Target};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ShelvesView {
    pub config: Config,
    /// The plugged-in Kobo or PocketBook: its key and name.
    pub reader: Option<(String, String)>,
    /// What the rules would change on it.
    pub targets: Vec<Target>,
    /// Shelves each rule makes (its name, or each series), with how many calibre books.
    pub sets: Vec<(String, usize)>,
    pub error: Option<String>,
}

fn library_json() -> std::path::PathBuf {
    deckle_library::config::default_path()
}

fn reader() -> Option<deckle_core::reader::Reader> {
    deckle_core::reader::detect().into_iter().next()
}

fn view(apply: bool) -> ShelvesView {
    let config = Config::load(&shelves::config_path(&library_json()));
    let mut v = ShelvesView {
        config,
        reader: None,
        targets: Vec::new(),
        sets: Vec::new(),
        error: None,
    };
    let sets = match shelves::sets(&library_json(), deckle_library::sources::home().as_deref()) {
        Ok(s) => s,
        Err(e) => {
            v.error = Some(e);
            return v;
        }
    };
    v.sets = sets
        .iter()
        .map(|s| (s.name.clone(), s.books.len()))
        .collect();
    if let Some(r) = reader() {
        let kobo = match &r {
            deckle_core::reader::Reader::Kobo(k) => Some(k),
            _ => None,
        };
        v.targets = shelves::for_drive(
            &v.config,
            &sets,
            r.root(),
            &r.id(),
            kobo,
            apply,
            &deckle_core::devices::now_iso(),
        );
        v.reader = Some((r.id(), r.name()));
    }
    v
}

pub fn shelves_status() -> ShelvesView {
    view(false)
}

/// Saves the rules and the automatic switch; answers with the preview.
pub fn shelves_set(config: Config) -> Result<ShelvesView, String> {
    for r in &config.rules {
        if let Some(why) = shelves::check(&r.from) {
            return Err(format!("{}: {why}", r.from));
        }
    }
    let path = shelves::config_path(&library_json());
    let mut saved = Config::load(&path);
    saved.rules = config.rules;
    saved.auto = config.auto;
    saved.save(&path).map_err(|e| e.to_string())?;
    Ok(view(false))
}

/// Lets Deckle write shelves on Kobo `key`, then makes them.
pub fn shelves_kobo_ok(key: String) -> Result<ShelvesView, String> {
    let path = shelves::config_path(&library_json());
    let mut c = Config::load(&path);
    if !c.kobo_ok.contains(&key) {
        c.kobo_ok.push(key);
    }
    c.save(&path).map_err(|e| e.to_string())?;
    Ok(view(true))
}

/// Fix shelves: the rules on the plugged-in reader now.
pub fn shelves_apply() -> Result<ShelvesView, String> {
    if reader().is_none() {
        return Err("plug in a Kobo or a PocketBook".into());
    }
    Ok(view(true))
}
