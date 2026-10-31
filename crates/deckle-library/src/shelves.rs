// SPDX-License-Identifier: GPL-3.0-or-later
//! Shelves from calibre (calibre bridge design §6.1): rules like "tag Sci-Fi → shelf Sci-Fi",
//! worked out against the calibre library (read-only) and narrowed to the books on a reader.
//! The readers' sides are `deckle_core::kobo_shelves` and `deckle_core::koreader_shelves`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use deckle_core::shelves::RuleSet;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

/// One rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// The shelf or collection on the reader. Ignored for `series:*`, which names one per series.
    pub name: String,
    /// `tag:<t>`, `series:<s>` (or `series:*`), `author:<a>`, `column:#<label>=<value>`, or
    /// `search:<saved search>`.
    pub from: String,
    /// Reader keys (`kobo/<serial>`, `koreader/<id>`, `pocketbook/…`), or `all`.
    #[serde(default = "all")]
    pub readers: Vec<String>,
}

fn all() -> Vec<String> {
    vec!["all".into()]
}

/// `calibre-shelves.json` beside `library.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub rules: Vec<Rule>,
    /// Kobos whose owner said yes to Deckle writing shelves (Kobo's own account sync may move
    /// them): `kobo/<serial>`.
    pub kobo_ok: Vec<String>,
    /// Apply the rules to a reader when it's plugged in (and in a wireless session).
    pub auto: bool,
}

pub fn config_path(library_json: &Path) -> PathBuf {
    library_json.with_file_name("calibre-shelves.json")
}

impl Config {
    pub fn load(path: &Path) -> Config {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }
}

/// Why `from` isn't a rule Deckle knows, or `None`.
pub fn check(from: &str) -> Option<String> {
    let Some((kind, value)) = from.split_once(':') else {
        return Some("a rule is kind:value, e.g. tag:Sci-Fi".into());
    };
    if value.trim().is_empty() {
        return Some(format!("{kind}: needs a value"));
    }
    match kind {
        "tag" | "series" | "author" | "search" => None,
        "column" => {
            (!value.trim_start_matches('#').contains('=')).then(|| "column:#label=value".into())
        }
        other => Some(format!(
            "{other}: one of tag, series, author, column, search"
        )),
    }
}

/// Each shelf a rule makes, with the calibre ids of its books. `search` runs a saved search
/// (`calibredb search`), giving calibre book ids; `None` when it can't.
pub fn evaluate(
    rules: &[Rule],
    library: &Path,
    search: &mut dyn FnMut(&str) -> Option<Vec<i64>>,
) -> Result<Vec<RuleSet>, String> {
    let db = Connection::open_with_flags(
        crate::sources::calibre_uri(&library.join("metadata.db")),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    let uuids: BTreeMap<i64, String> = db
        .prepare("SELECT id, uuid FROM books WHERE uuid IS NOT NULL")
        .and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .map_err(|e| e.to_string())?;
    let ids = |sql: &str, v: &str| -> Result<BTreeSet<String>, String> {
        let mut q = db.prepare(sql).map_err(|e| e.to_string())?;
        let rows = q
            .query_map([v], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        Ok(rows
            .flatten()
            .filter_map(|id| uuids.get(&id).cloned())
            .collect())
    };
    let mut out = Vec::new();
    let set = |rule: &Rule, name: String, books: BTreeSet<String>| RuleSet {
        name,
        readers: rule.readers.clone(),
        books,
    };
    for rule in rules {
        let Some((kind, value)) = rule.from.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match kind {
            "tag" => out.push(set(rule, rule.name.clone(), ids(
                "SELECT l.book FROM books_tags_link l JOIN tags t ON t.id = l.tag WHERE t.name = ?1 COLLATE NOCASE",
                value,
            )?)),
            "author" => out.push(set(rule, rule.name.clone(), ids(
                "SELECT l.book FROM books_authors_link l JOIN authors a ON a.id = l.author WHERE a.name = ?1 COLLATE NOCASE",
                value,
            )?)),
            "series" if value == "*" => {
                let mut q = db
                    .prepare("SELECT s.name, l.book FROM books_series_link l JOIN series s ON s.id = l.series")
                    .map_err(|e| e.to_string())?;
                let mut by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
                for (name, id) in q
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                    .map_err(|e| e.to_string())?
                    .flatten()
                {
                    if let Some(u) = uuids.get(&id) {
                        by.entry(name).or_default().insert(u.clone());
                    }
                }
                out.extend(by.into_iter().map(|(n, b)| set(rule, n, b)));
            }
            "series" => out.push(set(rule, rule.name.clone(), ids(
                "SELECT l.book FROM books_series_link l JOIN series s ON s.id = l.series WHERE s.name = ?1 COLLATE NOCASE",
                value,
            )?)),
            "column" => {
                let Some((label, want)) = value.trim_start_matches('#').split_once('=') else { continue };
                let col = crate::calibre::columns(library)?.into_iter().find(|c| c.label == label.trim());
                let Some(col) = col else {
                    return Err(format!("calibre has no column #{label}"));
                };
                let table = format!("custom_column_{}", col.id);
                let sql = if col.normalized {
                    format!("SELECT l.book FROM books_{table}_link l JOIN {table} t ON t.id = l.value WHERE t.value = ?1 COLLATE NOCASE")
                } else if col.datatype == "bool" {
                    format!("SELECT book FROM {table} WHERE value = (CASE lower(?1) WHEN 'yes' THEN 1 WHEN 'true' THEN 1 ELSE 0 END)")
                } else {
                    format!("SELECT book FROM {table} WHERE CAST(value AS TEXT) = ?1 COLLATE NOCASE")
                };
                out.push(set(rule, rule.name.clone(), ids(&sql, want.trim())?));
            }
            "search" => {
                if let Some(found) = search(value) {
                    out.push(set(rule, rule.name.clone(), found.iter().filter_map(|id| uuids.get(id).cloned()).collect()));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The rules worked out against the calibre library `library_json` names, saved searches
/// through `calibredb` when it's there.
pub fn sets(library_json: &Path, home: Option<&Path>) -> Result<Vec<RuleSet>, String> {
    let cfg = Config::load(&config_path(library_json));
    if cfg.rules.is_empty() {
        return Ok(Vec::new());
    }
    let lib = crate::writeback::library_root(&crate::LibraryConfig::load(library_json), home)
        .ok_or("no calibre library found: choose it in Settings → Library")?;
    let wb = crate::writeback::Writeback::load(&crate::writeback::config_path(library_json));
    let exe = crate::writeback::find_calibredb(wb.calibredb.as_deref());
    let mut search = |name: &str| -> Option<Vec<i64>> {
        let exe = exe.as_ref()?;
        let args = vec![
            "search".to_owned(),
            "--with-library".into(),
            lib.to_string_lossy().into_owned(),
            format!("search:\"{}\"", name.replace('"', "")),
        ];
        let out = crate::writeback::system_runner(exe, &args).ok()?;
        Some(
            out.trim()
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect(),
        )
    };
    evaluate(&cfg.rules, &lib, &mut search)
}

/// What the rules do on one reader's shelves (a Kobo's) or collections (KOReader's).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Target {
    /// `kobo` or `koreader`.
    pub kind: String,
    pub key: String,
    pub diff: deckle_core::shelves::Diff,
    /// Written (a Kobo) or left for the Deckle plugin (KOReader).
    pub applied: bool,
    /// Why not, or what to do next.
    pub note: Option<String>,
}

/// The rules on the drive at `root` (reader `key`): its Kobo shelves when it's a Kobo, its
/// KOReader collections when KOReader is there. With `apply`, writes them.
pub fn for_drive(
    cfg: &Config,
    sets: &[RuleSet],
    root: &Path,
    key: &str,
    kobo: Option<&deckle_core::kobo::Kobo>,
    apply: bool,
    now: &str,
) -> Vec<Target> {
    use deckle_core::{kobo_shelves, koreader_plugins, koreader_shelves, shelves};
    let books = shelves::books_on(root);
    let wanted = shelves::wanted(sets, key, &books);
    let mut out = Vec::new();
    if let Some(k) = kobo {
        let mut t = Target {
            kind: "kobo".into(),
            key: key.into(),
            ..Default::default()
        };
        match kobo_shelves::read(k) {
            Err(e) => t.note = Some(e),
            Ok(current) => {
                let file = kobo_shelves::ours_file(k);
                let ours = shelves::ours(&file);
                t.diff = shelves::diff(&wanted, &current, &ours);
                if apply && !t.diff.is_empty() {
                    if !cfg.kobo_ok.iter().any(|x| x == key) {
                        t.note = Some(
                            "Say yes once for this Kobo first: a Kobo signed in to a Kobo account may sync shelves with Kobo's servers"
                                .into(),
                        );
                    } else {
                        match kobo_shelves::apply(k, &t.diff, now) {
                            Ok(_) => {
                                t.applied = true;
                                let _ = shelves::record_ours(&file, &ours, &t.diff, &wanted);
                                t.note =
                                    Some("Eject the Kobo so it reads its shelves again".into());
                            }
                            Err(e) => t.note = Some(e),
                        }
                    }
                }
            }
        }
        out.push(t);
    }
    if let Some(data) = koreader_plugins::data_dir(root) {
        let mut t = Target {
            kind: "koreader".into(),
            key: key.into(),
            ..Default::default()
        };
        let dev = koreader_shelves::device_root(root, &data);
        match koreader_shelves::read(&data, dev) {
            Err(e) => t.note = Some(e),
            Ok(current) => {
                let file = koreader_shelves::ours_file(&data);
                let ours = shelves::ours(&file);
                t.diff = shelves::diff(&wanted, &current, &ours);
                let plugin = koreader_plugins::list(&data)
                    .plugins
                    .iter()
                    .any(|p| p.name == koreader_plugins::DECKLE && p.enabled);
                if apply {
                    if !plugin {
                        t.note = Some(
                            "KOReader's collections need the Deckle plugin to make them: install it on Settings → KOReader, or use the calibre connection over Wi-Fi"
                                .into(),
                        );
                    } else {
                        match koreader_shelves::queue(&data, &t.diff, dev) {
                            Ok(()) => {
                                t.applied = !t.diff.is_empty();
                                if t.applied {
                                    let _ = shelves::record_ours(&file, &ours, &t.diff, &wanted);
                                    t.note =
                                        Some("KOReader makes them the next time it starts".into());
                                }
                            }
                            Err(e) => t.note = Some(e),
                        }
                    }
                }
            }
        }
        out.push(t);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibre::fixture;
    use deckle_core::shelves::{OnDrive, wanted};

    fn library(root: &Path) {
        fixture::library(
            root,
            &[
                ("shelf", "Shelf", "text", false),
                ("fav", "Favorite", "bool", false),
            ],
            &[
                (1, "u-dune", "Dune", "Frank Herbert"),
                (2, "u-messiah", "Dune Messiah", "Frank Herbert"),
                (3, "u-emma", "Emma", "Jane Austen"),
            ],
        );
        let db = Connection::open(root.join("metadata.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE books_tags_link (id INTEGER PRIMARY KEY, book INTEGER, tag INTEGER);
             CREATE TABLE series (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE books_series_link (id INTEGER PRIMARY KEY, book INTEGER, series INTEGER);
             INSERT INTO tags VALUES (1, 'Sci-Fi'), (2, 'Classics');
             INSERT INTO books_tags_link (book, tag) VALUES (1, 1), (2, 1), (3, 2);
             INSERT INTO series VALUES (1, 'Dune');
             INSERT INTO books_series_link (book, series) VALUES (1, 1), (2, 1);
             INSERT INTO custom_column_1 (book, value) VALUES (3, 'Bedside');
             INSERT INTO custom_column_2 (book, value) VALUES (1, 1);",
        )
        .unwrap();
    }

    fn rule(name: &str, from: &str) -> Rule {
        Rule {
            name: name.into(),
            from: from.into(),
            readers: vec!["all".into()],
        }
    }

    #[test]
    fn rules_pick_calibres_books_and_the_readers_books_make_the_shelves() {
        let tmp = tempfile::tempdir().unwrap();
        library(tmp.path());
        let mut rules = vec![
            rule("Science fiction", "tag:sci-fi"),
            rule("", "series:*"),
            rule("Austen", "author:Jane Austen"),
            rule("Bedside", "column:#shelf=bedside"),
            rule("Favorites", "column:#fav=yes"),
            rule("To read", "search:Unread"),
        ];
        let mut searched = Vec::new();
        let mut search = |name: &str| {
            searched.push(name.to_owned());
            Some(vec![2, 3])
        };
        let ev = evaluate(&rules, tmp.path(), &mut search).unwrap();
        assert_eq!(searched, ["Unread"]);
        let books = vec![
            OnDrive {
                path: "Books/Dune.epub".into(),
                calibre_id: "u-dune".into(),
            },
            OnDrive {
                path: "Books/Emma.epub".into(),
                calibre_id: "u-emma".into(),
            },
        ];
        let w = wanted(&ev, "kobo/N1", &books);
        let got: Vec<(&str, Vec<&str>)> = w
            .iter()
            .map(|(n, b)| (n.as_str(), b.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            got,
            [
                ("Austen", vec!["Books/Emma.epub"]),
                ("Bedside", vec!["Books/Emma.epub"]),
                ("Dune", vec!["Books/Dune.epub"]),
                ("Favorites", vec!["Books/Dune.epub"]),
                ("Science fiction", vec!["Books/Dune.epub"]),
                ("To read", vec!["Books/Emma.epub"]),
            ]
        );
        // A rule for another reader doesn't apply here.
        rules[0].readers = vec!["koreader/abc".into()];
        let ev = evaluate(&rules, tmp.path(), &mut |_| Some(vec![])).unwrap();
        assert!(!wanted(&ev, "kobo/N1", &books).contains_key("Science fiction"));
        assert!(wanted(&ev, "koreader/ABC", &books).contains_key("Science fiction"));
    }

    #[test]
    fn rules_are_checked() {
        assert_eq!(check("tag:Sci-Fi"), None);
        assert_eq!(check("series:*"), None);
        assert!(check("column:#shelf").is_some());
        assert!(check("genre:x").is_some());
        assert!(check("tag:").is_some());
        assert!(check("nothing").is_some());
    }
}

#[cfg(test)]
mod drive_tests {
    use super::*;
    use deckle_core::shelves::RuleSet;

    fn epub(path: &Path, uuid: &str) {
        use std::io::Write;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("mimetype", o).unwrap();
        z.write_all(b"application/epub+zip").unwrap();
        z.start_file("META-INF/container.xml", o).unwrap();
        z.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
        z.start_file("c.opf", o).unwrap();
        write!(z, r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:opf="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title><dc:language>en</dc:language><dc:identifier id="i">x</dc:identifier><dc:identifier opf:scheme="calibre">{uuid}</dc:identifier></metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#).unwrap();
        z.start_file("a.xhtml", o).unwrap();
        z.write_all(b"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>x</p></body></html>")
            .unwrap();
        z.finish().unwrap();
    }

    #[test]
    fn a_kobo_with_koreader_gets_shelves_once_allowed_and_koreader_collections_with_the_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        epub(&root.join("Books/Dune.epub"), "u-dune");
        epub(&root.join("Books/Other.epub"), "u-other");
        std::fs::create_dir_all(root.join(".kobo")).unwrap();
        std::fs::write(
            root.join(".kobo/version"),
            "N1,4.1.15,4.38.21908,4.1.15,4.1.15,00000000-0000-0000-0000-000000000390\n",
        )
        .unwrap();
        let db = rusqlite::Connection::open(root.join(".kobo/KoboReader.sqlite")).unwrap();
        db.execute_batch(
            "CREATE TABLE content (ContentID TEXT, ContentType TEXT, Title TEXT, Attribution TEXT, ___PercentRead INTEGER, DateLastRead TEXT, ReadStatus INTEGER, BookID TEXT);
             CREATE TABLE Bookmark (BookmarkID TEXT, VolumeID TEXT, ContentID TEXT, Text TEXT, Annotation TEXT, DateCreated TEXT);
             CREATE TABLE Shelf (CreationDate TEXT, Id TEXT, InternalName TEXT, LastModified TEXT, Name TEXT, Type TEXT, _IsDeleted BOOL, _IsVisible BOOL, _IsSynced BOOL);
             CREATE TABLE ShelfContent (ShelfName TEXT, ContentId TEXT, DateModified TEXT, _IsDeleted BOOL, _IsSynced BOOL);",
        )
        .unwrap();
        drop(db);
        let kobo = deckle_core::reader::detect_in(&[root.to_path_buf()])
            .into_iter()
            .find_map(|r| match r {
                deckle_core::reader::Reader::Kobo(k) => Some(k),
                _ => None,
            })
            .unwrap();
        let data = root.join(".adds/koreader");
        std::fs::create_dir_all(data.join("settings")).unwrap();
        std::fs::write(data.join("settings.reader.lua"), "return {}").unwrap();
        let sets = vec![RuleSet {
            name: "Sci-Fi".into(),
            readers: vec!["all".into()],
            books: ["u-dune".to_owned()].into(),
        }];
        let mut cfg = Config::default();

        // A preview writes nothing.
        let t = for_drive(
            &cfg,
            &sets,
            root,
            "kobo/N1",
            Some(&kobo),
            false,
            "2026-10-31T12:00:00Z",
        );
        assert_eq!(t.len(), 2);
        assert!(t.iter().all(|x| !x.applied && x.diff.create == ["Sci-Fi"]));

        // Applied: the Kobo waits for a yes; KOReader for the plugin.
        let t = for_drive(
            &cfg,
            &sets,
            root,
            "kobo/N1",
            Some(&kobo),
            true,
            "2026-10-31T12:00:00Z",
        );
        assert!(t[0].note.as_deref().unwrap().contains("Say yes once"));
        assert!(t[1].note.as_deref().unwrap().contains("Deckle plugin"));
        cfg.kobo_ok.push("kobo/N1".into());
        std::fs::create_dir_all(data.join("plugins/deckle.koplugin")).unwrap();
        std::fs::write(
            data.join("plugins/deckle.koplugin/_meta.lua"),
            "return { name = \"deckle\", fullname = \"Deckle\" }",
        )
        .unwrap();
        std::fs::write(data.join("plugins/deckle.koplugin/main.lua"), "return {}").unwrap();
        let t = for_drive(
            &cfg,
            &sets,
            root,
            "kobo/N1",
            Some(&kobo),
            true,
            "2026-10-31T12:00:00Z",
        );
        assert!(t[0].applied, "{:?}", t[0].note);
        assert!(t[1].applied, "{:?}", t[1].note);
        assert_eq!(
            deckle_core::kobo_shelves::read(&kobo).unwrap()["Sci-Fi"]
                .iter()
                .collect::<Vec<_>>(),
            ["Books/Dune.epub"]
        );
        let waiting = deckle_core::koreader_shelves::waiting(&data, "/mnt/onboard/").unwrap();
        assert_eq!(
            waiting.add["Sci-Fi"].iter().collect::<Vec<_>>(),
            ["Books/Dune.epub"]
        );
        assert!(
            deckle_core::shelves::ours(&deckle_core::kobo_shelves::ours_file(&kobo))
                .contains("Sci-Fi")
        );

        // Again: the Kobo has them now; nothing to do there.
        let t = for_drive(
            &cfg,
            &sets,
            root,
            "kobo/N1",
            Some(&kobo),
            false,
            "2026-10-31T12:01:00Z",
        );
        assert!(t[0].diff.is_empty());
    }
}
