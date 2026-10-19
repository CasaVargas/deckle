// SPDX-License-Identifier: GPL-3.0-or-later
//! Locates files under `<workspace>/fixtures/mini/` for tests and examples.

use std::path::PathBuf;

/// Absolute path of a file in the `fixtures/mini` directory of the repository.
pub fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/mini")
        .join(name)
}
