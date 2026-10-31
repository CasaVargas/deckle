// SPDX-License-Identifier: GPL-3.0-or-later
//! Shelves on a Kobo (calibre bridge design §6.3): read from a copy of `KoboReader.sqlite`, and
//! written to the Kobo's own, which nothing else in Deckle does. Only `Shelf` and `ShelfContent`
//! are touched, after a backup beside it, in one transaction, and only when the tables are
//! what the firmware has had for years (calibre's Kobo driver writes the same rows).

use std::path::{Path, PathBuf};

use crate::kobo::Kobo;
use crate::shelves::{Diff, Shelves};

/// Backups of the database kept beside it.
const KEEP_BACKUPS: usize = 3;
/// How a sideloaded book's `ContentId` starts.
pub const ONBOARD: &str = "file:///mnt/onboard/";

fn db_path(kobo: &Kobo) -> PathBuf {
    kobo.root.join(".kobo").join("KoboReader.sqlite")
}

/// `.kobo/deckle-shelves.json`: the shelves Deckle made on this Kobo.
pub fn ours_file(kobo: &Kobo) -> PathBuf {
    kobo.root.join(".kobo").join("deckle-shelves.json")
}

/// The columns each table must have.
const NEED: [(&str, &[&str]); 2] = [
    (
        "Shelf",
        &[
            "Name",
            "InternalName",
            "CreationDate",
            "LastModified",
            "_IsDeleted",
            "_IsVisible",
            "_IsSynced",
        ],
    ),
    (
        "ShelfContent",
        &[
            "ShelfName",
            "ContentId",
            "DateModified",
            "_IsDeleted",
            "_IsSynced",
        ],
    ),
];

fn columns(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    let Ok(mut q) = conn.prepare(&format!("PRAGMA table_info({table})")) else {
        return Vec::new();
    };
    q.query_map([], |r| r.get::<_, String>(1))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// Why the tables aren't what Deckle writes, if they aren't.
fn check(conn: &rusqlite::Connection) -> Result<(), String> {
    for (table, need) in NEED {
        let have = columns(conn, table);
        if have.is_empty() {
            return Err(format!("this Kobo's database has no {table} table"));
        }
        if let Some(missing) = need.iter().find(|c| !have.iter().any(|h| h == *c)) {
            return Err(format!(
                "this Kobo's {table} table has no {missing} column; nothing written"
            ));
        }
    }
    Ok(())
}

/// The shelves on the Kobo (not deleted), with their sideloaded books' paths.
pub fn read(kobo: &Kobo) -> Result<Shelves, String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let copy = tmp.path().join("KoboReader.sqlite");
    std::fs::copy(db_path(kobo), &copy).map_err(|e| format!("{}: {e}", db_path(kobo).display()))?;
    for side in ["-wal", "-shm"] {
        let from = db_path(kobo).with_file_name(format!("KoboReader.sqlite{side}"));
        if from.is_file() {
            let _ = std::fs::copy(&from, tmp.path().join(format!("KoboReader.sqlite{side}")));
        }
    }
    let conn =
        rusqlite::Connection::open_with_flags(&copy, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    check(&conn)?;
    let mut out = Shelves::new();
    let mut q = conn
        .prepare("SELECT Name FROM Shelf WHERE _IsDeleted <> 'true' AND Name IS NOT NULL")
        .map_err(|e| e.to_string())?;
    for name in q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .flatten()
    {
        out.entry(name).or_default();
    }
    let mut q = conn
        .prepare("SELECT ShelfName, ContentId FROM ShelfContent WHERE _IsDeleted <> 'true'")
        .map_err(|e| e.to_string())?;
    for (shelf, id) in q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .flatten()
    {
        if let (Some(books), Some(path)) = (out.get_mut(&shelf), id.strip_prefix(ONBOARD)) {
            books.insert(path.to_owned());
        }
    }
    Ok(out)
}

/// Copies the database aside before a write, keeping the newest [`KEEP_BACKUPS`].
fn backup(kobo: &Kobo, now: &str) -> Result<PathBuf, String> {
    let db = db_path(kobo);
    let stamp: String = now.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let to = db.with_file_name(format!("KoboReader.sqlite.deckle-{stamp}"));
    std::fs::copy(&db, &to).map_err(|e| format!("backup {}: {e}", to.display()))?;
    let mut old: Vec<PathBuf> = std::fs::read_dir(db.parent().unwrap_or(Path::new(".")))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("KoboReader.sqlite.deckle-"))
        })
        .collect();
    old.sort();
    while old.len() > KEEP_BACKUPS {
        let _ = std::fs::remove_file(old.remove(0));
    }
    Ok(to)
}

/// Writes `d` to the Kobo: shelves made and books added as the firmware does, books taken off
/// and shelves deleted by marking them `_IsDeleted`, all unsynced. `now` is ISO 8601.
pub fn apply(kobo: &Kobo, d: &Diff, now: &str) -> Result<PathBuf, String> {
    let saved = backup(kobo, now)?;
    let mut conn = rusqlite::Connection::open(db_path(kobo)).map_err(|e| e.to_string())?;
    check(&conn)?;
    let shelf_cols = columns(&conn, "Shelf");
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| {
            format!(
                "the Kobo's database is busy ({e}); eject it from the Kobo's screen and try again"
            )
        })?;
    let e = |e: rusqlite::Error| e.to_string();
    for name in &d.create {
        // Newer firmware gives each shelf an Id and a Type; older has neither.
        let mut cols = vec![
            "Name",
            "InternalName",
            "CreationDate",
            "LastModified",
            "_IsDeleted",
            "_IsVisible",
            "_IsSynced",
        ];
        let mut vals: Vec<String> = vec![
            name.clone(),
            name.clone(),
            now.into(),
            now.into(),
            "false".into(),
            "true".into(),
            "false".into(),
        ];
        if shelf_cols.iter().any(|c| c == "Id") {
            cols.push("Id");
            vals.push(uuid::Uuid::new_v4().to_string());
        }
        if shelf_cols.iter().any(|c| c == "Type") {
            cols.push("Type");
            vals.push("UserTag".into());
        }
        // A deleted shelf of that name comes back.
        tx.execute(
            "DELETE FROM Shelf WHERE Name = ?1 AND _IsDeleted = 'true'",
            [name],
        )
        .map_err(e)?;
        let marks = vec!["?"; cols.len()].join(", ");
        tx.execute(
            &format!("INSERT INTO Shelf ({}) VALUES ({marks})", cols.join(", ")),
            rusqlite::params_from_iter(vals.iter()),
        )
        .map_err(e)?;
    }
    for (shelf, books) in &d.add {
        for path in books {
            let id = format!("{ONBOARD}{path}");
            tx.execute(
                "DELETE FROM ShelfContent WHERE ShelfName = ?1 AND ContentId = ?2",
                [shelf, &id],
            )
            .map_err(e)?;
            tx.execute(
                "INSERT INTO ShelfContent (ShelfName, ContentId, DateModified, _IsDeleted, _IsSynced) VALUES (?1, ?2, ?3, 'false', 'false')",
                [shelf, &id, &now.to_owned()],
            )
            .map_err(e)?;
        }
        tx.execute(
            "UPDATE Shelf SET LastModified = ?2, _IsSynced = 'false' WHERE Name = ?1",
            [shelf, &now.to_owned()],
        )
        .map_err(e)?;
    }
    for (shelf, books) in &d.remove {
        for path in books {
            tx.execute(
                "UPDATE ShelfContent SET _IsDeleted = 'true', _IsSynced = 'false', DateModified = ?3 WHERE ShelfName = ?1 AND ContentId = ?2",
                [shelf, &format!("{ONBOARD}{path}"), &now.to_owned()],
            )
            .map_err(e)?;
        }
    }
    for shelf in &d.delete {
        tx.execute(
            "UPDATE Shelf SET _IsDeleted = 'true', _IsSynced = 'false', LastModified = ?2 WHERE Name = ?1",
            [shelf, &now.to_owned()],
        )
        .map_err(e)?;
        tx.execute(
            "UPDATE ShelfContent SET _IsDeleted = 'true', _IsSynced = 'false' WHERE ShelfName = ?1",
            [shelf],
        )
        .map_err(e)?;
    }
    tx.commit().map_err(e)?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kobo::tests::fake_kobo;
    use crate::shelves::diff;
    use std::collections::BTreeSet;

    const SHELVES: &str = "CREATE TABLE Shelf (CreationDate TEXT, Id TEXT, InternalName TEXT, LastModified TEXT, Name TEXT, Type TEXT, _IsDeleted BOOL, _IsVisible BOOL, _IsSynced BOOL);
         CREATE TABLE ShelfContent (ShelfName TEXT, ContentId TEXT, DateModified TEXT, _IsDeleted BOOL, _IsSynced BOOL);
         INSERT INTO Shelf (Name, InternalName, _IsDeleted, _IsVisible, _IsSynced) VALUES ('Mine', 'Mine', 'false', 'true', 'true');
         INSERT INTO ShelfContent VALUES ('Mine', 'file:///mnt/onboard/Books/Emma.epub', '', 'false', 'true');";

    #[test]
    fn shelves_are_written_as_the_firmware_does_after_a_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let k = fake_kobo(tmp.path(), SHELVES);
        let current = read(&k).unwrap();
        assert_eq!(
            current["Mine"].iter().collect::<Vec<_>>(),
            ["Books/Emma.epub"]
        );

        let wanted: Shelves = [
            ("Sci-Fi".to_owned(), ["Books/Dune.epub".to_owned()].into()),
            ("Mine".to_owned(), ["Books/Dune.epub".to_owned()].into()),
        ]
        .into();
        let d = diff(&wanted, &current, &BTreeSet::new());
        let saved = apply(&k, &d, "2026-10-31T12:00:00Z").unwrap();
        assert!(saved.is_file());
        let after = read(&k).unwrap();
        assert_eq!(
            after["Sci-Fi"].iter().collect::<Vec<_>>(),
            ["Books/Dune.epub"]
        );
        // The owner's shelf was joined, not emptied.
        assert_eq!(after["Mine"].len(), 2);
        let conn = rusqlite::Connection::open(db_path(&k)).unwrap();
        let (typ, synced, id): (String, String, String) = conn
            .query_row(
                "SELECT Type, _IsSynced, Id FROM Shelf WHERE Name = 'Sci-Fi'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (typ.as_str(), synced.as_str(), id.len()),
            ("UserTag", "false", 36)
        );

        // Deckle's shelf, its rule gone: deleted the firmware's way.
        let ours: BTreeSet<String> = ["Sci-Fi".to_owned()].into();
        let d = diff(&Shelves::new(), &after, &ours);
        assert_eq!(d.delete, ["Sci-Fi"]);
        apply(&k, &d, "2026-10-31T12:05:00Z").unwrap();
        let again = read(&k).unwrap();
        assert!(!again.contains_key("Sci-Fi") && again.contains_key("Mine"));
        let deleted: String = conn
            .query_row(
                "SELECT _IsDeleted FROM Shelf WHERE Name = 'Sci-Fi'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, "true");
    }

    #[test]
    fn an_unexpected_database_is_never_written_and_backups_are_kept_to_three() {
        let tmp = tempfile::tempdir().unwrap();
        let k = fake_kobo(
            tmp.path(),
            "CREATE TABLE Shelf (Name TEXT); CREATE TABLE ShelfContent (ShelfName TEXT);",
        );
        let err = read(&k).unwrap_err();
        assert!(err.contains("no InternalName column"), "{err}");
        let d = Diff {
            create: vec!["X".into()],
            ..Default::default()
        };
        assert!(apply(&k, &d, "2026-10-31T12:00:00Z").is_err());

        let tmp = tempfile::tempdir().unwrap();
        let k = fake_kobo(tmp.path(), SHELVES);
        for i in 0..5 {
            apply(&k, &Diff::default(), &format!("2026-10-31T12:00:0{i}Z")).unwrap();
        }
        let backups = std::fs::read_dir(tmp.path().join(".kobo"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".deckle-"))
            .count();
        assert_eq!(backups, 3);
    }
}
