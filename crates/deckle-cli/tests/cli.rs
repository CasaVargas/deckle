// SPDX-License-Identifier: GPL-3.0-or-later
use assert_cmd::Command;

#[test]
fn help_lists_all_subcommands() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    for sub in ["inspect", "normalize", "convert", "send", "devices", "run"] {
        assert!(s.contains(sub), "missing subcommand {sub} in:\n{s}");
    }
}

#[test]
fn unimplemented_subcommand_exits_2() {
    Command::cargo_bin("deckle")
        .unwrap()
        .args(["devices"])
        .assert()
        .code(2);
}
