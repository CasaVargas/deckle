// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader's collections from calibre's shelves, over USB (calibre bridge design §6.2). KOReader
//! may write `settings/collection.lua` from memory after Deckle did, so Deckle doesn't: it
//! leaves the changes in `deckle-collections.lua` for the Deckle plugin to make through
//! KOReader's own `ReadCollection` at its next start.

use std::path::{Path, PathBuf};

use crate::koreader::lua::{self, Key, Value};
use crate::shelves::{Diff, Shelves};

/// The changes waiting for the Deckle plugin, in KOReader's folder.
pub const PENDING: &str = "deckle-collections.lua";

/// Where the drive's root is on the reader itself, by where KOReader is: a Kobo's
/// `/mnt/onboard/`, a Kindle's `/mnt/us/`, a PocketBook's `/mnt/ext1/`.
pub fn device_root(root: &Path, data: &Path) -> &'static str {
    let rel = data
        .strip_prefix(root)
        .unwrap_or(data)
        .to_string_lossy()
        .replace('\\', "/");
    match rel.trim_matches('/') {
        "koreader" => "/mnt/us/",
        "applications/koreader" => "/mnt/ext1/",
        _ => "/mnt/onboard/",
    }
}

/// `<KOReader>/deckle-shelves.json`: the collections Deckle made.
pub fn ours_file(data: &Path) -> PathBuf {
    data.join("deckle-shelves.json")
}

/// KOReader's collections (`settings/collection.lua`), each book as a path from the drive's
/// root. Books elsewhere (another card) are left out.
pub fn read(data: &Path, device_root: &str) -> Result<Shelves, String> {
    let file = data.join("settings").join("collection.lua");
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Shelves::new()),
        Err(e) => return Err(format!("{}: {e}", file.display())),
    };
    let Value::Table(colls) = lua::parse(&text)? else {
        return Ok(Shelves::new());
    };
    let mut out = Shelves::new();
    for (k, v) in colls {
        let (Key::Str(name), Value::Table(items)) = (k, v) else {
            continue;
        };
        let books = out.entry(name).or_default();
        for (key, item) in items {
            if !matches!(key, Key::Num(_)) {
                continue; // `settings`
            }
            if let Some(path) = item
                .get("file")
                .and_then(Value::as_str)
                .and_then(|f| f.strip_prefix(device_root))
            {
                books.insert(path.to_owned());
            }
        }
    }
    Ok(out)
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 32 => out.push_str(&format!("\\{:03}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Leaves `d` for the Deckle plugin, books as paths on the reader. Replaces what was waiting:
/// `d` is worked out from the collections as they are, which only the plugin changes.
pub fn queue(data: &Path, d: &Diff, device_root: &str) -> Result<(), String> {
    let path = data.join(PENDING);
    if d.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    let list = |names: &[String]| {
        names
            .iter()
            .map(|n| quote(n))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let books = |m: &Shelves| {
        m.iter()
            .map(|(n, b)| {
                let files = b
                    .iter()
                    .map(|p| quote(&format!("{device_root}{p}")))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("        [{}] = {{ {files} }},\n", quote(n))
            })
            .collect::<String>()
    };
    let text = format!(
        "-- Collection changes from Deckle (calibre's shelves); the Deckle plugin makes them when KOReader starts\nreturn {{\n    [\"create\"] = {{ {} }},\n    [\"delete\"] = {{ {} }},\n    [\"add\"] = {{\n{}    }},\n    [\"remove\"] = {{\n{}    }},\n}}\n",
        list(&d.create),
        list(&d.delete),
        books(&d.add),
        books(&d.remove)
    );
    let tmp = data.join(format!(".{PENDING}.tmp"));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

/// What's waiting for the plugin, books as paths from the drive's root; `None`: nothing.
pub fn waiting(data: &Path, device_root: &str) -> Option<Diff> {
    let v = lua::parse(&std::fs::read_to_string(data.join(PENDING)).ok()?).ok()?;
    let names = |k: &str| -> Vec<String> {
        match v.get(k) {
            Some(Value::Table(t)) => t
                .iter()
                .filter_map(|(_, x)| x.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        }
    };
    let books = |k: &str| -> Shelves {
        let mut out = Shelves::new();
        if let Some(Value::Table(t)) = v.get(k) {
            for (name, files) in t {
                let (Key::Str(name), Value::Table(files)) = (name, files) else {
                    continue;
                };
                out.insert(
                    name.clone(),
                    files
                        .iter()
                        .filter_map(|(_, f)| {
                            f.as_str()?.strip_prefix(device_root).map(str::to_owned)
                        })
                        .collect(),
                );
            }
        }
        out
    };
    Some(Diff {
        create: names("create"),
        delete: names("delete"),
        add: books("add"),
        remove: books("remove"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_read_as_paths_and_changes_wait_for_the_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let data = root.join(".adds/koreader");
        std::fs::create_dir_all(data.join("settings")).unwrap();
        std::fs::write(
            data.join("settings/collection.lua"),
            r#"-- settings
return {
    ["favorites"] = {
        [1] = { ["file"] = "/mnt/onboard/Books/Emma.epub", ["order"] = 1 },
        ["settings"] = { ["order"] = 1 },
    },
    ["Sci-Fi"] = {
        [1] = { ["file"] = "/mnt/onboard/Books/Dune.epub", ["order"] = 1 },
        [2] = { ["file"] = "/mnt/sd/Other.epub", ["order"] = 2 },
    },
}
"#,
        )
        .unwrap();
        let dev = device_root(root, &data);
        assert_eq!(dev, "/mnt/onboard/");
        assert_eq!(device_root(root, &root.join("koreader")), "/mnt/us/");
        assert_eq!(
            device_root(root, &root.join("applications/koreader")),
            "/mnt/ext1/"
        );
        let c = read(&data, dev).unwrap();
        assert_eq!(
            c["favorites"].iter().collect::<Vec<_>>(),
            ["Books/Emma.epub"]
        );
        assert_eq!(
            c["Sci-Fi"].iter().collect::<Vec<_>>(),
            ["Books/Dune.epub"],
            "another card's book is left out"
        );

        let d = Diff {
            create: vec!["Jane \"Austen\"".into()],
            delete: vec!["Old".into()],
            add: [(
                "Jane \"Austen\"".to_owned(),
                ["Books/Emma.epub".to_owned()].into(),
            )]
            .into(),
            remove: [("Sci-Fi".to_owned(), ["Books/Dune.epub".to_owned()].into())].into(),
        };
        queue(&data, &d, dev).unwrap();
        let text = std::fs::read_to_string(data.join(PENDING)).unwrap();
        assert!(text.contains("\"/mnt/onboard/Books/Emma.epub\""), "{text}");
        assert!(text.contains(r#"["Jane \"Austen\""]"#), "{text}");
        assert_eq!(waiting(&data, dev), Some(d));
        queue(&data, &Diff::default(), dev).unwrap();
        assert!(!data.join(PENDING).exists());
        assert_eq!(waiting(&data, dev), None);
    }
}
