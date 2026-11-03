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

#[test]
fn puzzles_sudoku_writes_a_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let out = puzzles(&[
        "sudoku",
        "--count",
        "2",
        "--difficulty",
        "hard",
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
            .ends_with("Sudoku 2026-11-01 0000.pdf"),
        "{v}"
    );
    assert_eq!(v["pages"], 4, "title, two puzzles, one page of answers");
    // a sudoku is always 9×9: there's no --size to give
    let sized = puzzles(&["sudoku", "--size", "9", "-o", dir.path().to_str().unwrap()]);
    assert!(!sized.status.success());
}

#[test]
fn crossword_words_file_with_clues() {
    let dir = tempfile::tempdir().unwrap();
    let words = dir.path().join("apple.txt");
    std::fs::write(
        &words,
        "Jobs: Co-founder who returned in 1997\nWoz: The other Steve\nLisa: 1983 computer\nNewton: Early handheld\niPod: A thousand songs\nSiri: Voice assistant\nPixar: Studio he ran\nSwift: Language of 2014\nMacintosh: The 1984 computer\nWWDC\n",
    )
    .unwrap();
    let out = puzzles(&[
        "crossword",
        "--count",
        "1",
        "--size",
        "11",
        "--words",
        words.to_str().unwrap(),
        "--title",
        "Apple History Crossword",
        "--seed",
        "3",
        "--date",
        "2026-11-01",
        "-o",
        dir.path().join("out").to_str().unwrap(),
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = v["warnings"].to_string();
    assert!(warnings.contains("MACINTOSH"), "{warnings}");
    assert!(
        warnings.contains("WWDC") && warnings.contains("no clue"),
        "{warnings}"
    );
}

const BUNDLE: &str = "deckle-puzzles-word-search-2026-11-01-0000.decklepuzzles";

#[test]
fn as_bundle_writes_a_decklepuzzles_folder_and_never_over_one() {
    let dir = tempfile::tempdir().unwrap();
    fn args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
        [
            &[
                "search",
                "--count",
                "2",
                "--seed",
                "7",
                "--date",
                "2026-11-01",
                "--as",
                "bundle",
            ][..],
            extra,
        ]
        .concat()
    }
    let d = dir.path().to_str().unwrap().to_owned();
    let out = puzzles(&args(&["-o", &d, "--json"]));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let path = dir.path().join(BUNDLE);
    assert_eq!(v["path"], path.to_str().unwrap());
    assert_eq!(v["format"], "bundle");
    assert_eq!(v["puzzles"], 2);
    assert!(v.get("pages").is_none(), "a bundle has no pages: {v}");
    let book: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("book.json")).unwrap()).unwrap();
    assert_eq!(book["format"], "deckle-puzzles");
    assert_eq!(book["book"]["kind"], "search");
    assert_eq!(book["book"]["seed"], "0000000000000007");
    assert_eq!(book["puzzles"].as_array().unwrap().len(), 2);
    let cover = std::fs::read(path.join("cover.jpg")).unwrap();
    assert!(cover.starts_with(&[0xff, 0xd8]), "a JPEG");
    assert!(!dir.path().join(format!(".{BUNDLE}.tmp")).exists());
    assert!(!dir.path().join(BOOK).exists(), "no PDF beside it");

    // the same bundle again is refused, then replaced
    let again = puzzles(&args(&["-o", &d]));
    assert!(!again.status.success());
    let err = String::from_utf8_lossy(&again.stderr);
    assert!(
        err.contains("is already there") && err.contains("--replace"),
        "{err}"
    );
    let replaced = puzzles(&args(&["--replace", "--to", &d]));
    assert!(
        replaced.status.success(),
        "{}",
        String::from_utf8_lossy(&replaced.stderr)
    );
    let text = String::from_utf8_lossy(&replaced.stdout);
    assert!(text.contains("(2 puzzles, seed 7)"), "{text}");
    let remake = text
        .lines()
        .find(|l| l.starts_with("remake it with: "))
        .unwrap();
    assert!(
        remake.contains("--seed 7 --date 2026-11-01 --as bundle --to "),
        "{text}"
    );

    // a Kindle can't take a bundle
    let device = puzzles(&args(&["--to", "device"]));
    assert!(!device.status.success());
    let err = String::from_utf8_lossy(&device.stderr);
    assert!(
        err.contains("not a Kindle") && err.contains("--to <folder>"),
        "{err}"
    );
}

#[test]
fn puzzles_solved_reads_the_returns_beside_a_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap().to_owned();
    let out = puzzles(&[
        "maze",
        "--count",
        "2",
        "--size",
        "8",
        "--seed",
        "7",
        "--date",
        "2026-11-01",
        "--as",
        "bundle",
        "-o",
        &d,
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let bundle = std::path::PathBuf::from(v["path"].as_str().unwrap());
    let book: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("book.json")).unwrap()).unwrap();
    let id = book["book"]["id"].as_str().unwrap().to_owned();
    // no returns yet
    let out = puzzles(&["solved", bundle.to_str().unwrap(), "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["of"], 2);
    assert!(v.get("done").is_none(), "{v}");
    // the iPad solved one
    let returns = dir.path().join(".deckle-returns/ipad-1/puzzles");
    std::fs::create_dir_all(&returns).unwrap();
    std::fs::write(
        returns.join(format!("{id}.json")),
        serde_json::json!({
            "format": "deckle-puzzle-returns", "version": "1.0",
            "reader": { "id": "ipad-1", "name": "Jon's iPad", "app": "Deckle Reader" },
            "book": id, "updated_at": "2026-11-01T20:00:00Z",
            "puzzles": [
                { "n": 1, "done": true, "seconds": 90, "last": "2026-11-01T20:00:00Z" },
                { "n": 2, "done": false, "seconds": 30, "last": "2026-11-01T20:01:00Z" }
            ]
        })
        .to_string(),
    )
    .unwrap();
    let out = puzzles(&["solved", bundle.join("book.json").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("1 of 2 solved on Jon's iPad (2 min)"),
        "{text}"
    );
}
