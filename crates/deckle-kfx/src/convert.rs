// SPDX-License-Identifier: GPL-3.0-or-later
//! KPF → monolithic KFX, end to end.

use std::path::Path;

use crate::container::{KfxDoc, WriteOptions, write_kfx};
use crate::fixup::{
    Book, add_approximate_pages, create_content_fragments, create_position_and_location_maps,
    fix_navigation, fix_structural, rebuild,
};
use crate::kpf::read_kpf;
pub use crate::metadata::CdeType;
use crate::metadata::Metadata;

#[derive(Clone, Debug)]
pub struct ConvertOptions {
    pub cde_type: CdeType,
    pub asin: Option<String>,
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    pub language: Option<String>,
    /// `None`: no page list; `Some(None)`: kfxlib auto density; `Some(Some(n))`: aim for n pages.
    pub approximate_pages: Option<Option<u32>>,
    pub container_id: Option<String>,
    pub creator: Option<String>,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        ConvertOptions {
            cde_type: CdeType::Pdoc,
            asin: None,
            title: None,
            authors: None,
            language: None,
            approximate_pages: None,
            container_id: None,
            creator: None,
        }
    }
}

#[derive(Debug)]
pub struct ConvertOutput {
    pub bytes: Vec<u8>,
    pub pages: Option<u32>,
    pub warnings: Vec<String>,
}

impl ConvertOutput {
    /// kfxlib warns above 16 MiB; some devices lock up.
    pub fn size_warning(len: usize) -> Option<String> {
        if len > crate::container::MAX_KFX_CONTAINER_SIZE {
            Some(format!(
                "KFX is {:.1} MB; some devices lock up above 16 MB",
                len as f64 / 1_000_000.0
            ))
        } else {
            None
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error(transparent)]
    Kpf(#[from] crate::kpf::KpfError),
    #[error(transparent)]
    Fixup(#[from] crate::fixup::FixupError),
    #[error(transparent)]
    Metadata(#[from] crate::metadata::MetadataError),
    #[error(transparent)]
    Container(#[from] crate::container::ContainerError),
}

pub fn default_creator() -> String {
    format!("deckle-{}", env!("CARGO_PKG_VERSION"))
}

/// Converts a KPF file to monolithic KFX bytes.
pub fn kpf_to_kfx(kpf: &Path, opts: &ConvertOptions) -> Result<ConvertOutput, ConvertError> {
    let container_id = opts
        .container_id
        .clone()
        .unwrap_or_else(crate::fixup::new_container_id);
    let creator = opts.creator.clone().unwrap_or_else(default_creator);
    let kpf = read_kpf(kpf)?;
    let mut book = Book::from_kpf(kpf, &container_id);
    fix_structural(&mut book)?;
    fix_navigation(&mut book);
    let has_text = create_content_fragments(&mut book);
    create_position_and_location_maps(&mut book, has_text)?;
    let md = Metadata {
        title: opts.title.clone(),
        authors: opts.authors.clone(),
        cde_type: Some(opts.cde_type),
        asin: opts.asin.clone(),
        language: opts.language.clone(),
        cover_jpeg: None,
    };
    crate::metadata::apply(&mut book, &md)?;
    let pages = match opts.approximate_pages {
        Some(desired) => Some(add_approximate_pages(&mut book, desired)?),
        None => None,
    };
    rebuild(&mut book, &container_id, &creator)?;
    let doc = KfxDoc {
        symtab: book.symtab.clone(),
        fragments: book.fragments.clone(),
        container_id: book.container_id.clone(),
        creator: creator.clone(),
        package_version: String::new(),
    };
    let bytes = write_kfx(
        &doc,
        &WriteOptions {
            container_id: book.container_id.clone(),
            creator,
            package_version: String::new(),
        },
    )?;
    let mut warnings = book.warnings;
    if let Some(w) = ConvertOutput::size_warning(bytes.len()) {
        warnings.push(w);
    }
    Ok(ConvertOutput {
        bytes,
        pages,
        warnings,
    })
}
