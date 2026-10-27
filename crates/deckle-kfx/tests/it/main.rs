// SPDX-License-Identifier: GPL-3.0-or-later
//! All integration tests in one binary: one link instead of one per file.

mod container_roundtrip;
mod fixtures_present;
mod fixup_content;
mod fixup_navigation;
mod fixup_positions;
mod fixup_structural;
mod ion_decode;
mod ion_roundtrip;
mod kpf_read;
mod kpf_to_kfx_oracle;
mod metadata;
mod symtab;
mod text_runs;
