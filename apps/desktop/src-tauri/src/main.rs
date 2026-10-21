// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    // `deckle-desktop digest …` / `save …`: the command line, no window. The schedule runs the
    // app this way (Daily design §5c).
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if matches!(
        args.get(1).and_then(|a| a.to_str()),
        Some("digest" | "save")
    ) {
        return deckle_cli::main_with(args);
    }
    deckle_desktop_lib::run();
    std::process::ExitCode::SUCCESS
}
