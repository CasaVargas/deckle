// SPDX-License-Identifier: GPL-3.0-or-later
//! Child commands that never show a console window.
//!
//! The desktop app is a GUI program with no console of its own. On Windows a console program
//! it starts (`schtasks`, `powershell`, Kindle Previewer) is given a fresh console unless the
//! parent says otherwise, and that console is a black window flashing on screen: the Daily
//! tab, which asks Task Scheduler about its edition task a few times when it opens, popped
//! a burst of them on first launch. `CREATE_NO_WINDOW` is that "otherwise"; it changes
//! nothing about the command's output, which is read back through pipes as before.

use std::ffi::OsStr;
use std::process::Command;

/// `CREATE_NO_WINDOW` from `processthreadsapi.h`.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A `Command` for `program` that opens no console window on Windows.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    hide(&mut cmd);
    cmd
}

/// Marks `cmd` to open no console window on Windows; a no-op elsewhere.
pub fn hide(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
