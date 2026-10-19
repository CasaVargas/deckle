// SPDX-License-Identifier: GPL-3.0-or-later
//! Position and location maps (kfxlib `yj_position_location.py`, KPF branch), plus the `$593`
//! format-capabilities rewrite that depends on them.

use std::collections::HashMap;

use super::{Book, FixupError};
use crate::fragment::Fragment;
use crate::ion::{Struct, Sym, Value};

/// Positions per Kindle "location".
pub const KFX_POSITIONS_PER_LOCATION: i64 = 110;

/// One run of content: `length` positions starting at `pid`, inside element `eid` at `eid_offset`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentChunk {
    pub pid: i64,
    pub eid: i64,
    pub eid_offset: i64,
    pub length: i64,
    pub section: Sym,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PositionReport {
    pub has_spim: bool,
    pub has_pid_offset: bool,
    /// True when a `$550` had to be generated (approximate locations).
    pub generated_location_map: bool,
}

fn eid_int(book: &Book, v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        Value::Symbol(s) => i64::from(book.symtab.id_of(s).unwrap_or(0)),
        _ => 0,
    }
}

/// Walks the KPF's per-section position id maps (`$609` + `$611`) into content chunks
/// (kfxlib `collect_position_map_info`, pre-publication branch).
pub fn collect_position_map_info(book: &mut Book) -> Result<Vec<ContentChunk>, FixupError> {
    let mut pos_info = Vec::new();
    let Some(counts) = book.fragments.get_single(&Sym::new("$611")) else {
        return Ok(pos_info);
    };
    let mut section_count: HashMap<String, i64> = HashMap::new();
    if let Some(list) = counts
        .value
        .as_struct()
        .and_then(|s| s.get("$181"))
        .and_then(Value::as_list)
    {
        for sm in list {
            let Some(sm) = sm.as_struct() else { continue };
            if let (Some(name), Some(n)) = (
                sm.get("$174").and_then(Value::as_sym),
                sm.get("$144").and_then(Value::as_int),
            ) {
                section_count.insert(name.0.clone(), n);
            }
        }
    }
    let mut eid_start_pos: HashMap<i64, i64> = HashMap::new();
    let mut section_start_pid = 0i64;
    for section in book.ordered_section_names() {
        let spim_fid = Sym(format!("{}-spm", section.as_str()));
        let Some(spim) = book.fragments.get(&Sym::new("$609"), Some(&spim_fid)) else {
            book.warnings.push(format!(
                "section_position_id_map missing for section {}",
                section.as_str()
            ));
            continue;
        };
        let count = *section_count.get(section.as_str()).unwrap_or(&0);
        let mut contains: Vec<Value> = spim
            .value
            .as_struct()
            .and_then(|s| s.get("$181"))
            .and_then(Value::as_list)
            .cloned()
            .unwrap_or_default();
        contains.push(Value::List(vec![Value::Int(count + 1), Value::Int(0)]));
        let (mut pid, mut eid, mut eid_offset) = (0i64, 0i64, 0i64);
        for (i, pi) in contains.iter().enumerate() {
            let (next_pid, next_eid, next_offset) = match pi {
                Value::List(items) if (2..=3).contains(&items.len()) => (
                    items[0].as_int().unwrap_or(0),
                    eid_int(book, &items[1]),
                    items.get(2).and_then(Value::as_int).unwrap_or(0),
                ),
                Value::Int(p) => (*p, eid + 1, 0),
                Value::Struct(s) => (
                    s.get("$184").and_then(Value::as_int).unwrap_or(0),
                    s.get("$185").map(|v| eid_int(book, v)).unwrap_or(0),
                    s.get("$143").and_then(Value::as_int).unwrap_or(0),
                ),
                other => {
                    return Err(FixupError::Malformed(format!(
                        "bad section_position_id_map entry {other:?}"
                    )));
                }
            };
            let next_pid = next_pid - 1; // one-based pids in KPF
            if i > 0 {
                eid_start_pos.entry(eid).or_insert(pid);
                pos_info.push(ContentChunk {
                    pid: pid + section_start_pid,
                    eid,
                    eid_offset,
                    length: next_pid - pid,
                    section: section.clone(),
                });
            }
            pid = next_pid;
            eid = next_eid;
            eid_offset = next_offset;
        }
        if eid != 0 || eid_offset != 0 {
            book.warnings.push(format!(
                "section_position_id_map last eid is {eid}+{eid_offset} (should be zero)"
            ));
        }
        section_start_pid += count;
    }
    Ok(pos_info)
}

/// Iteration order of a CPython `set` of small non-negative ints built by inserting `values`
/// in order. Needed because kfxlib writes `$264` from a set.
pub fn cpython_int_set_order(values: &[i64]) -> Vec<i64> {
    const MINSIZE: usize = 8;
    const LINEAR_PROBES: usize = 9;
    const PERTURB_SHIFT: u32 = 5;
    fn insert(table: &mut [Option<i64>], key: i64) -> bool {
        let mask = table.len() - 1;
        let hash = key as u64; // CPython hash(int) for small non-negative ints
        let mut i = (hash as usize) & mask;
        if table[i].is_none() {
            table[i] = Some(key);
            return true;
        }
        let mut perturb = hash;
        loop {
            if table[i] == Some(key) {
                return false;
            }
            if i + LINEAR_PROBES <= mask {
                for j in 1..=LINEAR_PROBES {
                    match table[i + j] {
                        None => {
                            table[i + j] = Some(key);
                            return true;
                        }
                        Some(k) if k == key => return false,
                        _ => {}
                    }
                }
            }
            perturb >>= PERTURB_SHIFT;
            i = ((i as u64)
                .wrapping_mul(5)
                .wrapping_add(1)
                .wrapping_add(perturb) as usize)
                & mask;
            if table[i].is_none() {
                table[i] = Some(key);
                return true;
            }
        }
    }
    fn resize(table: &[Option<i64>], used: usize) -> Vec<Option<i64>> {
        let minused = if used > 50000 { used * 2 } else { used * 4 };
        let mut newsize = MINSIZE;
        while newsize <= minused {
            newsize <<= 1;
        }
        let mut new_table = vec![None; newsize];
        for k in table.iter().flatten() {
            insert(&mut new_table, *k);
        }
        new_table
    }
    let mut table: Vec<Option<i64>> = vec![None; MINSIZE];
    let mut used = 0usize;
    for &v in values {
        if insert(&mut table, v) {
            used += 1;
            let mask = table.len() - 1;
            if used * 5 >= mask * 3 {
                table = resize(&table, used);
            }
        }
    }
    table.into_iter().flatten().collect()
}

/// Writes `$264` and `$265` from the chunks and removes the KPF-only mapping fragments
/// (kfxlib `create_position_map`).
pub fn create_position_map(book: &mut Book, pos_info: &[ContentChunk]) -> (bool, bool) {
    for t in ["$264", "$265", "$609", "$610", "$611"] {
        let ty = Sym::new(t);
        book.fragments.retain(|f| f.ftype != ty);
    }
    let mut section_eids: HashMap<String, Vec<i64>> = HashMap::new();
    for c in pos_info {
        section_eids
            .entry(c.section.0.clone())
            .or_default()
            .push(c.eid);
    }
    let mut position_map = Vec::new();
    for section in book.ordered_section_names() {
        let eids = section_eids
            .get(section.as_str())
            .map(|v| cpython_int_set_order(v))
            .unwrap_or_default();
        let mut s = Struct::new();
        s.insert(
            "$181",
            Value::List(eids.into_iter().map(Value::Int).collect()),
        );
        s.insert("$174", Value::Symbol(section));
        position_map.push(Value::Struct(s));
    }
    book.push_fragment(Fragment::root("$264", Value::List(position_map)));

    let mut position_id_map = Vec::new();
    let mut has_offset = false;
    let mut pid = 0i64;
    for c in pos_info {
        let mut s = Struct::new();
        s.insert("$184", Value::Int(pid));
        s.insert("$185", Value::Int(c.eid));
        if c.eid_offset != 0 {
            s.insert("$143", Value::Int(c.eid_offset));
            has_offset = true;
        }
        position_id_map.push(Value::Struct(s));
        pid += c.length;
    }
    let mut end = Struct::new();
    end.insert("$184", Value::Int(pid));
    end.insert("$185", Value::Int(0));
    position_id_map.push(Value::Struct(end));
    book.push_fragment(Fragment::root("$265", Value::List(position_id_map)));
    (false, has_offset)
}

/// One location every 110 positions, restarting at each section (kfxlib
/// `generate_approximate_locations`). Returned chunks carry `length` 0.
pub fn generate_approximate_locations(pos_info: &[ContentChunk]) -> Vec<ContentChunk> {
    let mut pid = 0i64;
    let mut next_loc = 0i64;
    let mut current_section: Option<&Sym> = None;
    let mut out = Vec::new();
    for c in pos_info {
        let mut eid_loc_offset = 0i64;
        let mut loc_pid = pid;
        if current_section != Some(&c.section) {
            next_loc = loc_pid;
            current_section = Some(&c.section);
        }
        loop {
            if loc_pid == next_loc {
                out.push(ContentChunk {
                    pid: loc_pid,
                    eid: c.eid,
                    eid_offset: c.eid_offset + eid_loc_offset,
                    length: 0,
                    section: c.section.clone(),
                });
                next_loc += KFX_POSITIONS_PER_LOCATION;
            }
            let eid_remaining = c.length - eid_loc_offset;
            let loc_remaining = next_loc - loc_pid;
            if eid_remaining <= loc_remaining {
                break;
            }
            eid_loc_offset += loc_remaining;
            loc_pid = next_loc;
        }
        pid += c.length;
    }
    out
}

/// Writes `$550` from locations (kfxlib `create_location_map`).
pub fn create_location_map(book: &mut Book, locs: &[ContentChunk]) {
    for t in ["$550", "$621"] {
        let ty = Sym::new(t);
        book.fragments.retain(|f| f.ftype != ty);
    }
    let locations: Vec<Value> = locs
        .iter()
        .map(|l| {
            let mut s = Struct::new();
            s.insert("$155", Value::Int(l.eid));
            s.insert("$143", Value::Int(l.eid_offset));
            Value::Struct(s)
        })
        .collect();
    let mut m = Struct::new();
    m.insert("$182", Value::List(locations));
    book.push_fragment(Fragment::root("$550", Value::List(vec![Value::Struct(m)])));
}

/// Position maps, location map (when absent), `$395` default and the `$593` rewrite.
pub fn create_position_and_location_maps(
    book: &mut Book,
    has_text_block: bool,
) -> Result<PositionReport, FixupError> {
    let pos_info = collect_position_map_info(book)?;
    book.is_prepub = false;
    let (has_spim, has_pid_offset) = create_position_map(book, &pos_info);
    let mut report = PositionReport {
        has_spim,
        has_pid_offset,
        generated_location_map: false,
    };

    let print_replica = {
        let textbook = book
            .metadata_value("yj_textbook", "kindle_capability_metadata")
            .is_some();
        let fixed = book
            .metadata_value("yj_fixed_layout", "kindle_capability_metadata")
            .and_then(Value::as_int);
        fixed == Some(2) || (textbook && fixed != Some(3))
    };
    let magazine = book
        .title_metadata("cde_content_type")
        .and_then(Value::as_str)
        == Some("MAGZ");
    if book.fragments.get_single(&Sym::new("$550")).is_none() && !(print_replica || magazine) {
        let locs = generate_approximate_locations(&pos_info);
        create_location_map(book, &locs);
        report.generated_location_map = true;
    }
    if book.fragments.get_single(&Sym::new("$395")).is_none() {
        let mut s = Struct::new();
        s.insert("$247", Value::List(Vec::new()));
        book.push_fragment(Fragment::root("$395", Value::Struct(s)));
    }
    let fc_type = Sym::new("$593");
    book.fragments.retain(|f| f.ftype != fc_type);
    let mut fc = Vec::new();
    let cap = |name: &str, version: i64| {
        let mut s = Struct::new();
        s.insert("$492", Value::string(name));
        s.insert("version", Value::Int(version));
        Value::Struct(s)
    };
    if report.has_spim {
        fc.push(cap("kfxgen.positionMaps", 2));
    }
    if report.has_pid_offset {
        fc.push(cap("kfxgen.pidMapWithOffset", 1));
    }
    if has_text_block {
        fc.push(cap("kfxgen.textBlock", 1));
    }
    book.push_fragment(Fragment::root("$593", Value::List(fc)));
    Ok(report)
}
