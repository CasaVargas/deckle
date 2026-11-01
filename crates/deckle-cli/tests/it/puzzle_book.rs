// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle puzzles search` (puzzle books §5.2): a word-search book written to a folder.

use assert_cmd::Command;

fn puzzles(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("deckle")
        .unwrap()
        .arg("puzzles")
        .args(args)
        .output()
        .unwrap()
}

const BOOK: &str = "Word Search 2026-11-01 0000.pdf";

#[test]
fn puzzles_search_writes_a_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let out = puzzles(&[
        "search",
        "--count",
        "2",
        "--seed",
        "7",
        "--date",
        "2026-11-01",
        "-o",
        dir.path().to_str().unwrap(),
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let path = dir.path().join(BOOK);
    assert_eq!(v["path"], path.to_str().unwrap());
    assert_eq!(v["pages"], 4, "title, two puzzles, one page of answers");
    assert_eq!(v["puzzles"], 2);
    assert_eq!(v["seed"], 7);
    assert_eq!(v["date"], "2026-11-01");
    assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF-"));
}

#[test]
fn a_second_run_is_refused_without_replace() {
    let dir = tempfile::tempdir().unwrap();
    let args = |extra: &[&'static str]| {
        let mut a = vec![
            "search",
            "--count",
            "1",
            "--seed",
            "7",
            "--date",
            "2026-11-01",
        ];
        a.extend_from_slice(extra);
        a
    };
    let d = dir.path().to_str().unwrap().to_owned();
    assert!(
        puzzles(&[args(&["-o"]), vec![&d]].concat())
            .status
            .success()
    );
    let again = puzzles(&[args(&["-o"]), vec![&d]].concat());
    assert!(!again.status.success());
    let err = String::from_utf8_lossy(&again.stderr);
    assert!(
        err.contains("is already there") && err.contains("--replace"),
        "{err}"
    );
    assert!(
        puzzles(&[args(&["--replace", "-o"]), vec![&d]].concat())
            .status
            .success()
    );
}

#[test]
fn to_a_folder_delivers_and_prints_how_to_remake_it() {
    let dir = tempfile::tempdir().unwrap();
    let out = puzzles(&[
        "search",
        "--count",
        "1",
        "--size",
        "10",
        "--seed",
        "7",
        "--date",
        "2026-11-01",
        "--to",
        dir.path().to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(dir.path().join(BOOK).exists());
    assert!(text.contains("3 pages, 1 puzzle, seed 7"), "{text}");
    // the folder is quoted where it needs to be (a Windows path's `C:\`)
    let remake = text
        .lines()
        .find(|l| l.starts_with("remake it with: "))
        .unwrap();
    assert!(
        remake.contains(
            "deckle puzzles search --count 1 --size 10 --difficulty medium --seed 7 --date 2026-11-01 --to "
        ),
        "{text}"
    );
    assert!(remake.contains(&dir.path().display().to_string()), "{text}");
}

#[test]
fn bad_size_says_the_range() {
    let dir = tempfile::tempdir().unwrap();
    let out = puzzles(&["search", "--size", "30", "-o", dir.path().to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("from 8 to 20"), "{err}");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "nothing written"
    );
}

#[test]
fn own_words_file_themes_the_book() {
    let dir = tempfile::tempdir().unwrap();
    let words = dir.path().join("tech.txt");
    std::fs::write(
        &words,
        "python\nkernel\nrouter\npixel\nlaptop\nserver\ncache\nsocket\nbinary\ncursor\n",
    )
    .unwrap();
    let out_dir = dir.path().join("out");
    let out = puzzles(&[
        "search",
        "--count",
        "1",
        "--words",
        words.to_str().unwrap(),
        "--title",
        "Tech Search",
        "--date",
        "2026-11-01",
        "-o",
        out_dir.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["path"]
            .as_str()
            .unwrap()
            .contains("Tech Search 2026-11-01 "),
        "{v}"
    );
}

#[test]
fn puzzles_crossword_writes_a_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let out = puzzles(&[
        "crossword",
        "--count",
        "2",
        "--size",
        "9",
        "--seed",
        "7",
        "--date",
        "2026-11-01",
        "-o",
        dir.path().to_str().unwrap(),
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["path"]
            .as_str()
            .unwrap()
            .ends_with("Crossword 2026-11-01 0000.pdf"),
        "{v}"
    );
    assert_eq!(v["puzzles"], 2);
    assert!(v["pages"].as_u64().unwrap() >= 4, "{v}");
}

#[test]
fn crossword_bad_size_says_the_range() {
    let dir = tempfile::tempdir().unwrap();
    let out = puzzles(&[
        "crossword",
        "--size",
        "16",
        "-o",
        dir.path().to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("from 5 to 15 for a crossword"), "{err}");
}
