// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings → KOReader (KOReader plugins design §4, phase 4): the plugins on the connected
//! reader, or on a folder that's a copy of one, and the Deckle plugin. Each change answers with
//! the page as it now is.

use std::path::{Path, PathBuf};

use deckle_core::koreader_plugins::{self as kp, Listing};
use deckle_core::koreader_store as ks;
pub use deckle_core::koreader_store::{Found, Update};
use serde::Serialize;

/// The page's state.
#[derive(Clone, Debug, Default, Serialize)]
pub struct KoreaderView {
    /// What the reader is: Kindle, Kobo, PocketBook, or the folder's name. `None`: nothing found.
    pub device: Option<String>,
    pub root: Option<PathBuf>,
    /// KOReader's plugins and patches there; `None` when the reader has no KOReader.
    pub listing: Option<Listing>,
    /// The Deckle plugin built into this Deckle.
    pub deckle_version: String,
    /// Why the Deckle plugin can't be installed now (the reader server is off, say); `None`: it can.
    pub deckle_blocked: Option<String>,
    /// Why nothing was found, in plain words.
    pub note: Option<String>,
}

/// What an install would do, shown before it's done.
#[derive(Clone, Debug, Serialize)]
pub struct KoreaderPreview {
    pub name: String,
    pub fullname: String,
    pub version: Option<String>,
    pub replaces: bool,
    pub from: String,
    pub release: Option<String>,
    /// The release Deckle reviewed (a Recommended pin, or the Deckle plugin itself).
    pub reviewed: bool,
}

/// The reader to work on: `folder` when given, else a Kindle on USB storage, else a Kobo or a
/// PocketBook. A Kindle over MTP isn't reached (KOReader hub design §D).
fn root(folder: Option<&str>) -> Result<(PathBuf, String), String> {
    if let Some(f) = folder.filter(|f| !f.trim().is_empty()) {
        let dir = PathBuf::from(f);
        if !dir.is_dir() {
            return Err(format!("{} is not a folder", dir.display()));
        }
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Folder".into());
        return Ok((dir, name));
    }
    if let Some(d) = deckle_core::deliver::detect_mass_storage()
        .into_iter()
        .next()
    {
        return Ok((d.root, "Kindle".into()));
    }
    match deckle_core::reader::detect().into_iter().next() {
        Some(r) => Ok((r.root().to_path_buf(), r.name())),
        None => Err(
            "No e-reader is plugged in. Plug one in over USB, or choose a folder that's a \
                     copy of one."
                .into(),
        ),
    }
}

fn data(folder: Option<&str>) -> Result<(PathBuf, String), String> {
    let (root, device) = root(folder)?;
    let data = kp::data_dir(&root)
        .ok_or_else(|| format!("The {device} has no KOReader (no koreader folder found)."))?;
    Ok((data, device))
}

/// What the Deckle plugin would be told; KEPUB on a Kobo (by its drive's layout).
fn plugin_settings(data: &Path, device: &str) -> Result<kp::DeckleSettings, String> {
    let main = deckle_digest::config::default_path();
    let cfg = deckle_digest::Config::load(&main).map_err(|e| e.to_string())?;
    let rs = &cfg.reader_server;
    let kobo = device == "Kobo" || kp::is_kobo(data);
    deckle_readerserver::plugin_settings(rs.enabled, rs.port, &rs.username, &main, kobo)
}

/// The page for the reader (or `folder`).
pub fn koreader_status(folder: Option<String>) -> KoreaderView {
    let mut view = KoreaderView {
        deckle_version: env!("CARGO_PKG_VERSION").into(),
        ..Default::default()
    };
    match root(folder.as_deref()) {
        Ok((root, device)) => {
            let data = kp::data_dir(&root);
            view.listing = data.as_deref().map(kp::list);
            match &data {
                Some(d) => view.deckle_blocked = plugin_settings(d, &device).err(),
                None => {
                    view.note = Some(format!(
                        "The {device} has no KOReader (no koreader folder found)."
                    ))
                }
            }
            view.device = Some(device);
            view.root = Some(root);
        }
        Err(e) => view.note = Some(e),
    }
    view
}

/// What installing `source` (a .zip, a folder, `owner/repo` or its URL, or `deckle`) would do.
pub fn koreader_preview(folder: Option<String>, source: String) -> Result<KoreaderPreview, String> {
    let (data, device) = data(folder.as_deref())?;
    let (p, reviewed) = prepare(&data, &device, &source)?;
    Ok(KoreaderPreview {
        reviewed,
        replaces: p.replaces.is_some(),
        name: p.name,
        fullname: p.fullname,
        version: p.version,
        from: p.source.from,
        release: p.source.release,
    })
}

/// The plugin `source` names, and whether it's a release Deckle reviewed: the Deckle plugin, a
/// Recommended pin (checked by hash), or anything else as it is.
fn prepare(data: &Path, device: &str, source: &str) -> Result<(kp::Prepared, bool), String> {
    if source.trim() == kp::DECKLE {
        plugin_settings(data, device)?;
        return Ok((kp::deckle_plugin(data), true));
    }
    let mut fetch = ks::cached(ks::cache_dir(), kp::http_fetch);
    let (list, _) = ks::recommended(&mut fetch);
    ks::prepare_any(data, source.trim(), &list, &mut fetch)
}

/// Installs `source` (after the page showed its preview). The Deckle plugin gets its settings.
pub fn koreader_install(folder: Option<String>, source: String) -> Result<KoreaderView, String> {
    let (data, device) = data(folder.as_deref())?;
    let (p, _) = prepare(&data, &device, &source)?;
    let deckle = p.name == kp::DECKLE;
    kp::install(&data, &p)?;
    if deckle {
        kp::write_deckle_settings(&data, &plugin_settings(&data, &device)?)?;
    }
    Ok(koreader_status(folder))
}

/// Installs a plugin again from where it came from.
pub fn koreader_update(folder: Option<String>, name: String) -> Result<KoreaderView, String> {
    let (data, _) = data(folder.as_deref())?;
    let source = kp::list(&data)
        .plugins
        .into_iter()
        .find(|p| p.name == name)
        .and_then(|p| p.source)
        .map(|s| s.from)
        .ok_or_else(|| {
            format!("Deckle didn't install {name}, so it doesn't know where it came from.")
        })?;
    koreader_install(folder, source)
}

pub fn koreader_remove(folder: Option<String>, name: String) -> Result<KoreaderView, String> {
    let (data, _) = data(folder.as_deref())?;
    kp::remove(&data, &name)?;
    Ok(koreader_status(folder))
}

pub fn koreader_rollback(folder: Option<String>, name: String) -> Result<KoreaderView, String> {
    let (data, _) = data(folder.as_deref())?;
    kp::rollback(&data, &name)?;
    Ok(koreader_status(folder))
}

pub fn koreader_set_enabled(
    folder: Option<String>,
    name: String,
    on: bool,
) -> Result<KoreaderView, String> {
    let (data, _) = data(folder.as_deref())?;
    kp::set_enabled(&data, &name, on)?;
    Ok(koreader_status(folder))
}

pub fn koreader_set_patches(folder: Option<String>, on: bool) -> Result<KoreaderView, String> {
    let (data, _) = data(folder.as_deref())?;
    kp::set_patches(&data, on)?;
    Ok(koreader_status(folder))
}

/// The Recommended list, and whether it's the live one.
#[derive(Clone, Debug, Serialize)]
pub struct StoreList {
    pub live: bool,
    pub plugins: Vec<ks::Pick>,
}

/// The store goes online only through these, when the owner opens it or searches.
pub fn koreader_store_list() -> StoreList {
    let mut fetch = ks::cached(ks::cache_dir(), kp::http_fetch);
    let (list, live) = ks::recommended(&mut fetch);
    StoreList {
        live,
        plugins: list.plugins,
    }
}

pub fn koreader_store_search(text: String, updated: bool) -> Result<Vec<ks::Found>, String> {
    let mut fetch = ks::cached(ks::cache_dir(), kp::http_fetch);
    let sort = if updated {
        ks::Sort::Updated
    } else {
        ks::Sort::Stars
    };
    ks::search(&text, sort, &mut fetch)
}

#[derive(Clone, Debug, Serialize)]
pub struct StoreReadme {
    pub readme: String,
    pub latest: Option<String>,
}

pub fn koreader_store_readme(repo: String) -> Result<StoreReadme, String> {
    let mut fetch = ks::cached(ks::cache_dir(), kp::http_fetch);
    Ok(StoreReadme {
        readme: ks::readme(&repo, &mut fetch)?,
        latest: ks::latest_release(&repo, &mut fetch),
    })
}

/// Installed plugins with a newer release (for Recommended ones, a newer pin).
pub fn koreader_store_updates(folder: Option<String>) -> Result<Vec<ks::Update>, String> {
    let (data, _) = data(folder.as_deref())?;
    let mut fetch = ks::cached(ks::cache_dir(), kp::http_fetch);
    let (list, _) = ks::recommended(&mut fetch);
    Ok(ks::updates(&kp::list(&data), &list, &mut fetch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kobo(root: &Path) -> String {
        let ko = root.join("KOBOeReader/.adds/koreader");
        for (p, t) in [
            ("plugins/kosync.koplugin/main.lua", "--"),
            ("plugins/zen.koplugin/main.lua", "-- zen"),
            (
                "plugins/zen.koplugin/_meta.lua",
                "return { fullname = \"Zen UI\" }",
            ),
            ("settings.reader.lua", "return {}"),
        ] {
            std::fs::create_dir_all(ko.join(p).parent().unwrap()).unwrap();
            std::fs::write(ko.join(p), t).unwrap();
        }
        root.join("KOBOeReader").display().to_string()
    }

    #[test]
    fn a_folder_shows_its_plugins_and_changes_answer_with_the_new_page() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = Some(kobo(tmp.path()));
        let v = koreader_status(folder.clone());
        assert_eq!(v.device.as_deref(), Some("KOBOeReader"));
        let names: Vec<_> = v
            .listing
            .unwrap()
            .plugins
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["kosync", "zen"]);
        assert!(kp::is_kobo(&tmp.path().join("KOBOeReader/.adds/koreader")));

        let v = koreader_remove(folder.clone(), "zen".into()).unwrap();
        assert_eq!(v.listing.unwrap().plugins.len(), 1);
        let v = koreader_rollback(folder.clone(), "zen".into()).unwrap();
        assert_eq!(v.listing.unwrap().plugins.len(), 2);
        let err = koreader_set_enabled(folder.clone(), "zen".into(), false).unwrap_err();
        assert!(err.contains("Deckle plugin"), "{err}");
        let v = koreader_set_patches(folder, false).unwrap();
        assert!(v.listing.unwrap().patches_disabled);
    }

    #[test]
    fn a_folder_without_koreader_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let v = koreader_status(Some(tmp.path().display().to_string()));
        assert!(v.listing.is_none());
        assert!(v.note.unwrap().contains("no KOReader"));
        let v = koreader_status(Some("/no/such/folder".into()));
        assert!(v.note.unwrap().contains("not a folder"));
    }
}
