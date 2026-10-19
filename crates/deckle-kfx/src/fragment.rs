// SPDX-License-Identifier: GPL-3.0-or-later
//! KFX fragments: an annotated Ion value identified by type (`ftype`) and, for non-root types,
//! an id (`fid`). Mirrors kfxlib's `YJFragment` / `YJFragmentList`.

use crate::ion::{Sym, Value};

/// Fragment types that hold raw bytes rather than Ion.
pub const RAW_FRAGMENT_TYPES: [&str; 2] = ["$418", "$417"];

/// Types that live at container level rather than as book entities.
pub const CONTAINER_FRAGMENT_TYPES: [&str; 4] = ["$270", "$593", "$ion_symbol_table", "$419"];

/// Root fragment types: identified by type alone (no fid).
pub const ROOT_FRAGMENT_TYPES: [&str; 17] = [
    "$ion_symbol_table",
    "$270",
    "$490",
    "$389",
    "$419",
    "$585",
    "$538",
    "$262",
    "$593",
    "$550",
    "$258",
    "$265",
    "$264",
    "$395",
    "$390",
    "$621",
    "$611",
];

/// kfxlib's `PREFERED_FRAGMENT_TYPE_ORDER`: the order fragments are written in a KFX.
pub const PREFERRED_FRAGMENT_TYPE_ORDER: [&str; 36] = [
    "$ion_symbol_table",
    "$270",
    "$593",
    "$585",
    "$490",
    "$258",
    "$538",
    "$389",
    "$390",
    "$260",
    "$259",
    "$608",
    "$145",
    "$756",
    "$692",
    "$157",
    "$391",
    "$266",
    "$394",
    "$264",
    "$265",
    "$550",
    "$609",
    "$621",
    "$611",
    "$610",
    "$597",
    "$267",
    "$387",
    "$395",
    "$262",
    "$164",
    "$418",
    "$417",
    "$419",
    "$XXX_unused_placeholder",
];

pub fn is_root_type(ftype: &str) -> bool {
    ROOT_FRAGMENT_TYPES.contains(&ftype)
}

pub fn is_raw_type(ftype: &str) -> bool {
    RAW_FRAGMENT_TYPES.contains(&ftype)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub ftype: Sym,
    /// `None` for root/singleton fragments.
    pub fid: Option<Sym>,
    pub value: Value,
}

impl Fragment {
    pub fn new(ftype: &str, fid: Option<&str>, value: Value) -> Self {
        Fragment {
            ftype: Sym::new(ftype),
            fid: fid.map(Sym::new),
            value,
        }
    }

    pub fn root(ftype: &str, value: Value) -> Self {
        Self::new(ftype, None, value)
    }

    /// The fragment's key as a comparable tuple (type order, fid text).
    pub fn sort_key(&self) -> (usize, String) {
        let order = PREFERRED_FRAGMENT_TYPE_ORDER
            .iter()
            .position(|t| *t == self.ftype.as_str())
            .unwrap_or(PREFERRED_FRAGMENT_TYPE_ORDER.len());
        // kfxlib sorts by (type index, fid) where a root fragment's "fid" is its type.
        let fid = self
            .fid
            .as_ref()
            .map(|f| f.0.clone())
            .unwrap_or_else(|| self.ftype.0.clone());
        (order, fid)
    }

    pub fn same_key(&self, ftype: &Sym, fid: Option<&Sym>) -> bool {
        self.ftype == *ftype && self.fid.as_ref() == fid
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FragmentList(pub Vec<Fragment>);

impl FragmentList {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn push(&mut self, f: Fragment) {
        self.0.push(f);
    }
    pub fn iter(&self) -> impl Iterator<Item = &Fragment> {
        self.0.iter()
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Fragment> {
        self.0.iter_mut()
    }
    /// All fragments of a type.
    pub fn all<'a>(&'a self, ftype: &'a Sym) -> impl Iterator<Item = &'a Fragment> + 'a {
        self.0.iter().filter(move |f| f.ftype == *ftype)
    }
    pub fn all_mut<'a>(
        &'a mut self,
        ftype: &'a Sym,
    ) -> impl Iterator<Item = &'a mut Fragment> + 'a {
        self.0.iter_mut().filter(move |f| f.ftype == *ftype)
    }
    /// The fragment with this exact (type, fid) key.
    pub fn get(&self, ftype: &Sym, fid: Option<&Sym>) -> Option<&Fragment> {
        self.0.iter().find(|f| f.same_key(ftype, fid))
    }
    pub fn get_mut(&mut self, ftype: &Sym, fid: Option<&Sym>) -> Option<&mut Fragment> {
        self.0.iter_mut().find(|f| f.same_key(ftype, fid))
    }
    /// The single root fragment of a type (fid `None`).
    pub fn get_single(&self, ftype: &Sym) -> Option<&Fragment> {
        self.get(ftype, None)
    }
    pub fn get_single_mut(&mut self, ftype: &Sym) -> Option<&mut Fragment> {
        self.get_mut(ftype, None)
    }
    /// Removes and returns the first fragment with this key.
    pub fn remove(&mut self, ftype: &Sym, fid: Option<&Sym>) -> Option<Fragment> {
        let i = self.0.iter().position(|f| f.same_key(ftype, fid))?;
        Some(self.0.remove(i))
    }
    pub fn retain(&mut self, keep: impl FnMut(&Fragment) -> bool) {
        self.0.retain(keep);
    }
    /// Sorts into kfxlib's preferred order (stable).
    pub fn sort_preferred(&mut self) {
        self.0.sort_by_cached_key(Fragment::sort_key);
    }
}
