// SPDX-License-Identifier: GPL-3.0-or-later
//! Zips rendered files into an EPUB (`mimetype` first and stored, the rest deflated).

use std::io::Write;
use std::path::Path;

use crate::render::Files;

pub fn write(path: &Path, files: &Files) -> Result<(), String> {
    let out = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        let opts = if name == "mimetype" { stored } else { deflated };
        zw.start_file(name.as_str(), opts)
            .map_err(|e| e.to_string())?;
        zw.write_all(bytes).map_err(|e| e.to_string())?;
    }
    zw.finish().map_err(|e| e.to_string())?;
    Ok(())
}
