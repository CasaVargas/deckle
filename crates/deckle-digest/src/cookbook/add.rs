// SPDX-License-Identifier: GPL-3.0-or-later
//! Adding a recipe from a web address, a link file or a saved page, with the site's photo
//! (cookbook design §4.3, §7.1).

use chrono::{DateTime, Utc};

use super::filing::file_into;
pub use super::picture::site_picture;
use super::recipe::{Recipe, parse_recipe};
use super::store::{ImageKind, Store, Stored, recipe_id, write_atomic};
use crate::article;
use crate::fetch::Net;
use crate::reading::normalize_url;
use crate::web_book::{Source, source};

/// Said when a recipe is saved with no printed picture.
pub const NO_PICTURE: &str = "No picture on the page";
/// Said when saving a recipe again found no usable photo, and its earlier picture stays.
pub const KEPT_PICTURE: &str = "Kept the earlier picture";

/// What the owner chose when adding.
pub struct AddOptions<'a> {
    /// A chapter id, else the recipe is filed by its categories and title.
    pub chapter: Option<&'a str>,
    pub note: Option<&'a str>,
}

/// What [`add`] did.
#[derive(Clone, Debug)]
pub struct AddOutcome {
    pub stored: Stored,
    /// Why there is no printed picture, when there isn't.
    pub picture_note: Option<&'static str>,
}

/// What adding would do, without doing it (the desktop Add card).
#[derive(Clone, Debug)]
pub struct Preview {
    pub recipe: Recipe,
    pub chapter: String,
    /// The picture as it would print.
    pub picture_jpeg: Option<Vec<u8>>,
    /// Already in the book.
    pub exists: bool,
}

/// Why adding failed: the moment (worth trying again) or the content (it won't get better).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddError {
    /// The page, its Google News link, or a file on disk couldn't be got. A server's refusal
    /// (404, 403, 410…) isn't this: asking again won't change it.
    Fetch(String),
    /// Not a recipe, too little text, or the book couldn't be written.
    Other(String),
}

impl AddError {
    pub fn into_string(self) -> String {
        match self {
            AddError::Fetch(e) | AddError::Other(e) => e,
        }
    }
}

impl std::fmt::Display for AddError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddError::Fetch(e) | AddError::Other(e) => f.write_str(e),
        }
    }
}

/// A failed fetch: worth retrying, unless the server answered with a 4xx that asking again
/// won't change. `Net`s report a status as `http status: N` (ureq's wording).
fn fetch_error(e: String) -> AddError {
    let status = e
        .split("http status: ")
        .nth(1)
        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse::<u16>().ok());
    match status {
        Some(code) if (400..500).contains(&code) && code != 408 && code != 429 => {
            AddError::Other(e)
        }
        _ => AddError::Fetch(e),
    }
}

/// The recipe at `target`, the key it is stored under, and the site photo's bytes (when it
/// has one that loads). A saved page is read from disk, never fetched; its photo is read from
/// beside it.
fn read(target: &str, net: &dyn Net) -> Result<(Recipe, String, Option<Vec<u8>>), AddError> {
    let (html, base, key) = match source(target).map_err(AddError::Other)? {
        Source::Url(url) => {
            let url = if article::is_google_news(&url) {
                article::resolve_google_news(&url, net)
                    .ok_or_else(|| AddError::Fetch("couldn't follow the Google News link".into()))?
            } else {
                url
            };
            let bytes = net.get_page(&url).map_err(fetch_error)?;
            let key = normalize_url(&url);
            (String::from_utf8_lossy(&bytes).into_owned(), url, key)
        }
        Source::Page(file) => {
            // An iCloud file that hasn't downloaded yet fails here, and may not later.
            let bytes = std::fs::read(&file).map_err(|e| AddError::Fetch(e.to_string()))?;
            let base = url::Url::from_file_path(&file).map_err(|_| {
                AddError::Other(format!("{}: not an absolute path", file.display()))
            })?;
            let key = file.to_string_lossy().into_owned();
            (
                String::from_utf8_lossy(&bytes).into_owned(),
                base.to_string(),
                key,
            )
        }
    };
    let recipe = parse_recipe(&html, &base).map_err(AddError::Other)?;
    let photo = recipe
        .image_url
        .as_deref()
        .and_then(|u| article::load_photo(u, net));
    Ok((recipe, key, photo))
}

/// Saves the recipe at `target` into `store`, with its photo: as fetched in
/// `images/<id>.orig.jpg` (the bytes are kept as they came, whatever their format) and as
/// printed in `images/<id>.jpg`. The pictures are written first, each atomically, so a disk
/// failure stores nothing. A photo that is missing, won't load or is too small leaves a new
/// recipe without a picture ([`NO_PICTURE`]); adding one again then keeps the earlier picture
/// ([`KEPT_PICTURE`]), and otherwise refreshes both files.
pub fn add(
    store: &Store,
    target: &str,
    net: &dyn Net,
    opts: &AddOptions,
    now: DateTime<Utc>,
) -> Result<AddOutcome, String> {
    add_classified(store, target, net, opts, now).map_err(AddError::into_string)
}

/// [`add`], saying whether a failure was the fetch (try again later) or the content.
pub fn add_classified(
    store: &Store,
    target: &str,
    net: &dyn Net,
    opts: &AddOptions,
    now: DateTime<Utc>,
) -> Result<AddOutcome, AddError> {
    let (recipe, key, photo) = read(target, net)?;
    let id = recipe_id(&key);
    let (orig_path, printed_path) = (
        store.image_path(&id, ImageKind::Orig),
        store.image_path(&id, ImageKind::Printed),
    );
    let picture_note = match (&photo, photo.as_deref().and_then(site_picture)) {
        (Some(orig), Some(printed)) => {
            let fresh = !orig_path.exists();
            write_atomic(&orig_path, orig).map_err(AddError::Other)?;
            if let Err(e) = write_atomic(&printed_path, printed) {
                if fresh {
                    let _ = std::fs::remove_file(&orig_path);
                }
                return Err(AddError::Other(e));
            }
            None
        }
        _ if printed_path.is_file() => Some(KEPT_PICTURE),
        _ => Some(NO_PICTURE),
    };
    let stored = store
        .put(recipe, &key, opts.chapter, opts.note, now)
        .map_err(AddError::Other)?;
    Ok(AddOutcome {
        stored,
        picture_note,
    })
}

/// Fetches and reads `target` and says where it would be filed, writing nothing.
pub fn preview(target: &str, net: &dyn Net, store: &Store) -> Result<Preview, String> {
    let (recipe, key, photo) = read(target, net).map_err(AddError::into_string)?;
    let existing = store.find(&recipe_id(&key))?;
    let chapter = match &existing {
        Some(old) => old.chapter.clone(),
        None => file_into(&recipe.category, &recipe.title, &store.config()?.chapters),
    };
    Ok(Preview {
        recipe,
        chapter,
        picture_jpeg: photo.as_deref().and_then(site_picture),
        exists: existing.is_some(),
    })
}
