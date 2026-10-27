// SPDX-License-Identifier: GPL-3.0-or-later
//! The cookbook's plain-file store (cookbook design §5): one JSON file per recipe, the book's
//! settings in `cookbook.toml`, the pictures beside them. No database, no hidden state.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::filing::{Chapter, default_chapters, file_into};
use super::recipe::Recipe;
use crate::reading::{fnv, normalize_url};

/// The longest note kept, in characters (§5).
const NOTE_MAX: usize = 1000;

/// Which picture a recipe (or the whole book) prints (§5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Picture {
    Site,
    Illustration,
    None,
    /// A recipe's own setting: follow the book's.
    Default,
}

/// A recipe as saved: the parsed fields, and the owner's (`recipes/<id>.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    #[serde(flatten)]
    pub recipe: Recipe,
    /// A chapter id. May name a chapter since deleted from the config; the build files such a
    /// recipe under "Everything else" without rewriting the file.
    pub chapter: String,
    #[serde(default = "default_picture")]
    pub picture: Picture,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub title_override: Option<String>,
    pub added: DateTime<Utc>,
}

fn default_picture() -> Picture {
    Picture::Default
}

/// The illustrations settings of `cookbook.toml` (§7.2). Off unless `enabled`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IllustrationsConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
}

/// `cookbook.toml`: the book's title, chapters and defaults (§5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookConfig {
    pub title: String,
    pub subtitle: String,
    pub picture: Picture,
    pub chapters: Vec<Chapter>,
    pub illustrations: IllustrationsConfig,
}

impl Default for BookConfig {
    fn default() -> Self {
        BookConfig {
            title: "The Kitchen Book".to_owned(),
            subtitle: "Recipes, gathered".to_owned(),
            picture: Picture::Site,
            chapters: default_chapters(),
            illustrations: IllustrationsConfig::default(),
        }
    }
}

/// What [`Store::put`] did.
#[derive(Clone, Debug, PartialEq)]
pub enum Stored {
    Added(Saved),
    Updated(Saved),
}

/// The three pictures a recipe can have in `images/` (§5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    /// `<id>.jpg`: as printed (grayscale, cropped).
    Printed,
    /// `<id>.orig.jpg`: the site photo as fetched.
    Orig,
    /// `<id>.ill.jpg`: the generated illustration.
    Ill,
}

/// The cookbook folder.
#[derive(Clone, Debug)]
pub struct Store {
    pub dir: PathBuf,
}

/// The id of the recipe at `key` (its page's URL, or a saved page's absolute path): the first
/// 12 hex digits of the FNV hash of the normalized key.
pub fn recipe_id(key: &str) -> String {
    format!("{:012x}", fnv(&normalize_url(key)) >> 16)
}

fn is_id(s: &str) -> bool {
    s.len() == 12
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn cap_note(note: &str) -> String {
    note.chars().take(NOTE_MAX).collect()
}

/// Writes `text` to a temp file beside `path`, then renames it into place.
pub(crate) fn write_atomic(path: &Path, text: impl AsRef<[u8]>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text.as_ref()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The serializer writes a list of tables as one inline array; turn those (and inline tables)
/// at the top level into `[[chapters]]` and `[illustrations]`, as the owner would write them.
fn spread_tables(table: &mut toml_edit::Table) {
    use toml_edit::{ArrayOfTables, Item, Value};
    for (_, item) in table.iter_mut() {
        let spread = match &*item {
            Item::Value(Value::InlineTable(t)) => Some(Item::Table(t.clone().into_table())),
            Item::Value(Value::Array(a))
                if !a.is_empty() && a.iter().all(|v| v.is_inline_table()) =>
            {
                let mut tables = ArrayOfTables::new();
                for v in a.iter().filter_map(Value::as_inline_table) {
                    tables.push(v.clone().into_table());
                }
                Some(Item::ArrayOfTables(tables))
            }
            _ => None,
        };
        if let Some(spread) = spread {
            *item = spread;
        }
    }
}

impl Store {
    /// `DECKLE_COOKBOOK_DIR`, else `cookbook/` beside `digest.toml`.
    pub fn default_dir() -> PathBuf {
        if let Some(p) = std::env::var_os("DECKLE_COOKBOOK_DIR").filter(|p| !p.is_empty()) {
            return PathBuf::from(p);
        }
        crate::config::default_path()
            .parent()
            .map(|d| d.join("cookbook"))
            .unwrap_or_else(|| PathBuf::from("cookbook"))
    }

    pub fn new(dir: PathBuf) -> Self {
        Store { dir }
    }

    fn config_path(&self) -> PathBuf {
        self.dir.join("cookbook.toml")
    }

    fn recipe_path(&self, id: &str) -> PathBuf {
        self.dir.join("recipes").join(format!("{id}.json"))
    }

    /// The book's settings; the defaults when `cookbook.toml` doesn't exist.
    pub fn config(&self) -> Result<BookConfig, String> {
        let path = self.config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BookConfig::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Writes `cfg` into `cookbook.toml`, keeping the comments, key order and every value that
    /// didn't change, as `config::save` does for `digest.toml`.
    pub fn save_config(&self, cfg: &BookConfig) -> Result<(), String> {
        let path = self.config_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut new = toml_edit::ser::to_document(cfg).map_err(|e| e.to_string())?;
        spread_tables(new.as_table_mut());
        crate::config::merge_table(doc.as_table_mut(), new.as_table());
        write_atomic(&path, doc.to_string())
    }

    /// Saves the recipe read from `key` (§5). A new one is filed into a chapter (`chapter`, or
    /// by its categories and title) and stamped `now`. Saving one again refreshes the parsed
    /// fields and keeps the owner's, except a `chapter` or `note` given now.
    pub fn put(
        &self,
        recipe: Recipe,
        key: &str,
        chapter: Option<&str>,
        note: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Stored, String> {
        let id = recipe_id(key);
        let note = note.map(cap_note);
        let stored = match self.find(&id)? {
            Some(old) => Stored::Updated(Saved {
                recipe,
                chapter: chapter.map(str::to_owned).unwrap_or(old.chapter),
                note: note.or(old.note),
                ..old
            }),
            None => {
                let chapter = match chapter {
                    Some(c) => c.to_owned(),
                    None => file_into(&recipe.category, &recipe.title, &self.config()?.chapters),
                };
                Stored::Added(Saved {
                    id,
                    recipe,
                    chapter,
                    picture: Picture::Default,
                    note,
                    title_override: None,
                    added: now,
                })
            }
        };
        let (Stored::Added(saved) | Stored::Updated(saved)) = &stored;
        self.write(saved)?;
        Ok(stored)
    }

    fn write(&self, saved: &Saved) -> Result<(), String> {
        let text = serde_json::to_string_pretty(saved).map_err(|e| e.to_string())?;
        write_atomic(&self.recipe_path(&saved.id), &text)?;
        self.mark_dirty()
    }

    /// Every saved recipe, oldest first (ties by id), so the order never depends on the
    /// directory listing. A recipe file that doesn't parse is an error naming it.
    pub fn all(&self) -> Result<Vec<Saved>, String> {
        let dir = self.dir.join("recipes");
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        };
        let mut out = Vec::new();
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|x| x == "json") {
                out.extend(Self::read(&path)?);
            }
        }
        out.sort_by(|a, b| a.added.cmp(&b.added).then_with(|| a.id.cmp(&b.id)));
        Ok(out)
    }

    fn read(path: &Path) -> Result<Option<Saved>, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// The id `id_or_url` names: a 12-digit id as it is, anything else as a URL or path.
    fn id_of(id_or_url: &str) -> String {
        if is_id(id_or_url) {
            id_or_url.to_owned()
        } else {
            recipe_id(id_or_url)
        }
    }

    /// One recipe, by id or by the URL it was saved from.
    pub fn find(&self, id_or_url: &str) -> Result<Option<Saved>, String> {
        Self::read(&self.recipe_path(&Self::id_of(id_or_url)))
    }

    /// Writes the owner's edits to a recipe already saved (its note is capped).
    pub fn update(&self, saved: &Saved) -> Result<(), String> {
        let mut saved = saved.clone();
        saved.note = saved.note.as_deref().map(cap_note);
        self.write(&saved)
    }

    /// Deletes a recipe's file and its pictures. `false` when there was no such recipe.
    pub fn remove(&self, id_or_url: &str) -> Result<bool, String> {
        let id = Self::id_of(id_or_url);
        let path = self.recipe_path(&id);
        let existed = match std::fs::remove_file(&path) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let images = self.dir.join("images");
        if let Ok(entries) = std::fs::read_dir(&images) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(&id) {
                    std::fs::remove_file(entry.path())
                        .map_err(|e| format!("{}: {e}", entry.path().display()))?;
                }
            }
        }
        if existed {
            self.mark_dirty()?;
        }
        Ok(existed)
    }

    pub fn image_path(&self, id: &str, kind: ImageKind) -> PathBuf {
        let name = match kind {
            ImageKind::Printed => format!("{id}.jpg"),
            ImageKind::Orig => format!("{id}.orig.jpg"),
            ImageKind::Ill => format!("{id}.ill.jpg"),
        };
        self.dir.join("images").join(name)
    }

    fn dirty_path(&self) -> PathBuf {
        self.dir.join("dirty")
    }

    /// Notes that the book needs rebuilding (`dirty` in the folder).
    pub fn mark_dirty(&self) -> Result<(), String> {
        write_atomic(&self.dirty_path(), "")
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_path().exists()
    }

    pub fn clear_dirty(&self) -> Result<(), String> {
        match std::fs::remove_file(self.dirty_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", self.dirty_path().display())),
        }
    }
}
