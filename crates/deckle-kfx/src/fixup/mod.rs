// SPDX-License-Identifier: GPL-3.0-or-later
//! Turns a pre-publication KPF into the fragment set of a device KFX. Port of kfxlib's
//! `kpf_book.fix_kpf_prepub_book` and the structure/position helpers it calls.

mod content;
mod navigation;
mod positions;
mod rebuild;
mod structural;

pub use content::{create_content_fragments, section_story_names};
pub use navigation::{add_approximate_pages, fix_navigation};
pub use positions::{
    ContentChunk, PositionReport, collect_position_map_info, cpython_int_set_order,
    create_location_map, create_position_and_location_maps, create_position_map,
    generate_approximate_locations,
};
pub use rebuild::{natural_sort_key, new_container_id, rebuild};
pub use structural::fix_structural;

use crate::fragment::{Fragment, FragmentList};
use crate::ion::{Decimal, Sym, Value};
use crate::kpf::Kpf;
use crate::source_epub::SourceEpub;
use crate::symtab::SymbolTable;

#[derive(Debug, thiserror::Error)]
pub enum FixupError {
    #[error("fix-up failed: {0}")]
    Malformed(String),
    #[error("Ion error: {0}")]
    Ion(#[from] crate::ion::IonError),
}

/// A book under conversion.
#[derive(Debug, Clone)]
pub struct Book {
    pub fragments: FragmentList,
    pub symtab: SymbolTable,
    pub kcb: serde_json::Value,
    pub source_epub: Option<SourceEpub>,
    /// Becomes the container id and the `asset_id` metadata.
    pub container_id: String,
    pub is_prepub: bool,
    pub warnings: Vec<String>,
}

impl Book {
    pub fn from_kpf(kpf: Kpf, container_id: &str) -> Self {
        let source_epub = kpf.source_epub.as_deref().map(SourceEpub::parse);
        Book {
            fragments: kpf.fragments,
            symtab: kpf.symtab,
            kcb: kpf.kcb,
            source_epub,
            container_id: container_id.to_owned(),
            is_prepub: kpf.is_prepub,
            warnings: Vec::new(),
        }
    }

    pub fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    /// `$490` lookup: value of `name` within `category` (kfxlib `get_metadata_value`),
    /// falling back to the `$258` metadata fragment for known keys.
    pub fn metadata_value(&self, name: &str, category: &str) -> Option<&Value> {
        if let Some(md) = self.fragments.get_single(&Sym::new("$490"))
            && let Some(cats) = md
                .value
                .as_struct()
                .and_then(|s| s.get("$491"))
                .and_then(Value::as_list)
        {
            for cat in cats {
                let Some(cat) = cat.as_struct() else { continue };
                if cat.get("$495").and_then(Value::as_str) != Some(category) {
                    continue;
                }
                if let Some(kvs) = cat.get("$258").and_then(Value::as_list) {
                    for kv in kvs {
                        let Some(kv) = kv.as_struct() else { continue };
                        if kv.get("$492").and_then(Value::as_str) == Some(name) {
                            return kv.get("$307");
                        }
                    }
                }
            }
        }
        if let Some(sym) = crate::consts::metadata_symbol(name)
            && let Some(md) = self.fragments.get_single(&Sym::new("$258"))
        {
            return md.value.as_struct().and_then(|s| s.get(sym));
        }
        None
    }

    pub fn title_metadata(&self, name: &str) -> Option<&Value> {
        self.metadata_value(name, "kindle_title_metadata")
    }

    /// `$585` lookup (kfxlib `get_feature_value`, default namespace).
    pub fn feature_value(&self, feature: &str, namespace: &str) -> Option<&Value> {
        let cf = self.fragments.get_single(&Sym::new("$585"))?;
        let list = cf.value.as_struct()?.get("$590")?.as_list()?;
        list.iter().find_map(|f| {
            let f = f.as_struct()?;
            if f.get("$586").and_then(Value::as_str) == Some(namespace)
                && f.get("$492").and_then(Value::as_str) == Some(feature)
            {
                f.get("$589")
            } else {
                None
            }
        })
    }

    pub fn is_fixed_layout(&self) -> bool {
        self.metadata_value("yj_fixed_layout", "kindle_capability_metadata")
            .is_some()
    }

    pub fn is_illustrated_layout(&self) -> bool {
        self.feature_value("yj.illustrated_layout", "com.amazon.yjconversion")
            .is_some()
    }

    pub fn has_pdf_resource(&self) -> bool {
        self.fragments.all(&Sym::new("$164")).any(|f| {
            f.value
                .as_struct()
                .and_then(|s| s.get("$161"))
                .and_then(Value::as_sym)
                .map(|s| s.as_str())
                == Some("$565")
        })
    }

    /// Reading orders from `$538.$169`: (name, section names).
    pub fn reading_orders(&self) -> Vec<(Option<Sym>, Vec<Sym>)> {
        let Some(dd) = self.fragments.get_single(&Sym::new("$538")) else {
            return Vec::new();
        };
        let Some(ros) = dd
            .value
            .as_struct()
            .and_then(|s| s.get("$169"))
            .and_then(Value::as_list)
        else {
            return Vec::new();
        };
        ros.iter()
            .filter_map(|ro| {
                let ro = ro.as_struct()?;
                let name = ro.get("$178").and_then(Value::as_sym).cloned();
                let sections = ro
                    .get("$170")
                    .and_then(Value::as_list)
                    .map(|l| l.iter().filter_map(|v| v.as_sym().cloned()).collect())
                    .unwrap_or_default();
                Some((name, sections))
            })
            .collect()
    }

    /// Section names in reading order, then any remaining `$260` fragments (kfxlib
    /// `ordered_section_names`).
    pub fn ordered_section_names(&self) -> Vec<Sym> {
        let mut order: Vec<Sym> = Vec::new();
        for (_, sections) in self.reading_orders() {
            for s in sections {
                if !order.contains(&s) {
                    order.push(s);
                }
            }
        }
        let mut rest: Vec<Sym> = self
            .fragments
            .all(&Sym::new("$260"))
            .filter_map(|f| f.fid.clone())
            .filter(|f| !order.contains(f))
            .collect();
        rest.sort();
        order.extend(rest);
        order
    }

    pub fn push_fragment(&mut self, f: Fragment) {
        self.fragments.push(f);
    }
}

/// kfxlib converts every float to `Decimal("%g" % x)`, with |x| < 0.001 becoming 0.
pub fn float_to_decimal(x: f64) -> Decimal {
    if !x.is_finite() || x.abs() < 0.001 {
        return Decimal {
            coefficient: 0,
            exponent: 0,
        };
    }
    let text = format_g6(x);
    parse_decimal_literal(&text)
}

/// Python's `"%g" % x`: 6 significant digits, trailing zeros removed, exponent form outside
/// 1e-4 <= |x| < 1e6.
fn format_g6(x: f64) -> String {
    let exp = x.abs().log10().floor() as i32;
    // Rounding to 6 significant digits can bump the exponent (e.g. 999999.5 -> 1e+06).
    let sci = format!("{:.5e}", x); // like 1.23457e6
    let (mant, e) = sci.split_once('e').unwrap();
    let e: i32 = e.parse().unwrap();
    let exp = if e != exp { e } else { exp };
    if !(-4..6).contains(&exp) {
        let mant = strip_zeros(mant);
        format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        let decimals = (5 - exp).max(0) as usize;
        strip_zeros(&format!("{x:.decimals$}"))
    }
}

fn strip_zeros(s: &str) -> String {
    if s.contains('.') {
        let t = s.trim_end_matches('0');
        t.trim_end_matches('.').to_owned()
    } else {
        s.to_owned()
    }
}

/// Parses a decimal literal like `-1.25`, `100`, `1e-05`, `1.23457e+06` keeping the digits as
/// written (Python `Decimal(str)` semantics).
fn parse_decimal_literal(text: &str) -> Decimal {
    let (mant, exp) = match text.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().unwrap_or(0)),
        None => (text, 0),
    };
    let negative = mant.starts_with('-');
    let mant = mant.trim_start_matches(['-', '+']);
    let (int_part, frac_part) = mant.split_once('.').unwrap_or((mant, ""));
    let digits: String = format!("{int_part}{frac_part}");
    let coefficient: i128 = digits.parse().unwrap_or(0);
    let coefficient = if negative { -coefficient } else { coefficient };
    Decimal {
        coefficient,
        exponent: exp - frac_part.len() as i32,
    }
}
