// SPDX-License-Identifier: GPL-3.0-or-later
fn main() -> std::process::ExitCode {
    deckle_cli::main_with(std::env::args_os())
}
