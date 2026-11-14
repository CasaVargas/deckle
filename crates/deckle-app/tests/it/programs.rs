// SPDX-License-Identifier: GPL-3.0-or-later
//! Every place the workspace starts a program, reviewed. The web UI must never choose which
//! program the server runs (only saved config, the environment or Deckle's own binary may), so
//! a new place, or a new caller of a lookup that takes a configured program, fails here until
//! someone has checked where its program comes from and added it below.

use std::collections::BTreeMap;
use std::path::Path;

/// What starts or picks a program.
const PATTERNS: [&str; 4] = [
    "Command::new(",
    "quiet::command(",
    "previewer::locate(",
    "find_calibredb(",
];

/// `file: count`, with where the program comes from.
const REVIEWED: &[(&str, usize, &str)] = &[
    (
        "deckle-app/src/calibre.rs",
        1,
        "calibredb from calibre-writeback.json; web can't set it",
    ),
    (
        "deckle-app/src/commands.rs",
        4,
        "previewer from settings.json (web keeps saved); open/explorer/xdg-open",
    ),
    (
        "deckle-app/src/daily.rs",
        4,
        "the app's own binary; open/explorer/xdg-open",
    ),
    (
        "deckle-app/src/morgue.rs",
        4,
        "the app's own binary; open/explorer/xdg-open",
    ),
    (
        "deckle-cli/src/calibre.rs",
        1,
        "calibredb from calibre-writeback.json",
    ),
    (
        "deckle-cli/src/cookbook_delivery.rs",
        1,
        "previewer from the CLI's options",
    ),
    ("deckle-cli/src/daemon.rs", 1, "current_exe"),
    (
        "deckle-cli/src/inbox.rs",
        1,
        "previewer from the CLI's options",
    ),
    (
        "deckle-cli/src/lib.rs",
        6,
        "previewer from --previewer or settings.json",
    ),
    ("deckle-cli/src/press_worker.rs", 1, "current_exe"),
    ("deckle-comic/src/cbr.rs", 2, "bsdtar/tar, or DECKLE_BSDTAR"),
    (
        "deckle-core/src/pipeline.rs",
        1,
        "previewer from the run's options",
    ),
    ("deckle-core/src/previewer.rs", 2, "the located Previewer"),
    ("deckle-core/src/quiet.rs", 1, "the wrapper itself"),
    ("deckle-core/src/usb_serial.rs", 1, "fixed system tools"),
    (
        "deckle-digest/src/schedule.rs",
        19,
        "id, launchctl, schtasks",
    ),
    (
        "deckle-library/src/shelves.rs",
        1,
        "calibredb from calibre-writeback.json",
    ),
    (
        "deckle-library/src/writeback.rs",
        4,
        "calibredb from calibre-writeback.json",
    ),
    ("deckle-xray/src/llm.rs", 2, "sysctl, powershell"),
];

fn scan(dir: &Path, crates: &Path, found: &mut BTreeMap<String, usize>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            // Tests may start whatever they like.
            if path
                .file_name()
                .is_some_and(|n| n == "tests" || n == "target")
            {
                continue;
            }
            scan(&path, crates, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            // A Windows checkout ends its lines with CRLF.
            let text = std::fs::read_to_string(&path)
                .unwrap()
                .replace("\r\n", "\n");
            // Unit tests at the end of a file don't count.
            let code = text.split("#[cfg(test)]\nmod tests").next().unwrap();
            let n: usize = PATTERNS.iter().map(|p| code.matches(p).count()).sum();
            if n > 0 {
                let rel = path.strip_prefix(crates).unwrap();
                found.insert(rel.to_string_lossy().replace('\\', "/"), n);
            }
        }
    }
}

#[test]
fn every_program_the_workspace_starts_is_reviewed() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut found = BTreeMap::new();
    for krate in std::fs::read_dir(crates).unwrap() {
        let src = krate.unwrap().path().join("src");
        if src.is_dir() {
            scan(&src, crates, &mut found);
        }
    }
    let reviewed: BTreeMap<String, usize> = REVIEWED
        .iter()
        .map(|(f, n, _)| (f.to_string(), *n))
        .collect();
    assert_eq!(
        found, reviewed,
        "a place that starts a program changed: check where its program comes from (never a \
         web request), then update REVIEWED in this test"
    );
}
