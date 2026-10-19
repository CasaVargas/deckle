// SPDX-License-Identifier: GPL-3.0-or-later
mod cli;

use std::process::ExitCode;

use clap::Parser;

/// Exit code for subcommands whose body is not implemented yet.
const EXIT_NOT_IMPLEMENTED: u8 = 2;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let cli::Command::Devices = cli.command else {
        eprintln!("not implemented yet");
        return ExitCode::from(EXIT_NOT_IMPLEMENTED);
    };
    eprintln!("not implemented yet");
    ExitCode::from(EXIT_NOT_IMPLEMENTED)
}
