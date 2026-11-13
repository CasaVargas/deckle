// SPDX-License-Identifier: GPL-3.0-or-later
//! Book navigation (`$389`) fix-ups and the approximate page list (kfxlib `fix_kpf_prepub_book`
//! navigation block and `create_approximate_page_list`).

use std::collections::{HashMap, HashSet};

use super::{Book, ContentChunk, FixupError, collect_position_map_info};
use crate::consts::APPROXIMATE_PAGE_LIST;
use crate::fragment::Fragment;
use crate::ion::{Struct, Sym, Value};

const TYPICAL_POSITIONS_PER_PAGE: i64 = 1850;
const MIN_POSITIONS_PER_PAGE: i64 = 1;
const MAX_POSITIONS_PER_PAGE: i64 = 50000;
const MAX_WHITE_SPACE_ADJUST: i64 = 50;

fn annotated(annot: &str, s: Struct) -> Value {
    Value::Annotated {
        annots: vec![Sym::new(annot)],
        value: Box::new(Value::Struct(s)),
    }
}

fn container_type(v: &Value) -> Option<&str> {
    v.unannotated()
        .as_struct()?
        .get("$235")?
        .as_sym()
        .map(|s| s.as_str())
}

fn new_uuid_symbol(book: &mut Book) -> Sym {
    book.symtab.create_local(&uuid::Uuid::new_v4().to_string())
}

/// Ensures a `$389` exists for every reading order and turns `page_list_entry:` landmarks into
/// a `$237` page list.
pub fn fix_navigation(book: &mut Book) {
    let nav_type = Sym::new("$389");
    if book.fragments.get_single(&nav_type).is_none() {
        let mut navs = Vec::new();
        for (name, _) in book.reading_orders() {
            let mut s = Struct::new();
            if let Some(n) = name {
                s.insert("$178", Value::Symbol(n));
            }
            s.insert("$392", Value::List(Vec::new()));
            navs.push(Value::Struct(s));
        }
        book.push_fragment(Fragment::root("$389", Value::List(navs)));
    }
    // Collect page_list_entry rows out of the landmarks containers.
    let mut new_containers: Vec<(usize, Value)> = Vec::new();
    {
        let nav = book.fragments.get_single_mut(&nav_type).unwrap();
        let Some(navs) = nav.value.as_list_mut() else {
            return;
        };
        for (ro_idx, book_nav) in navs.iter_mut().enumerate() {
            let Some(containers) = book_nav
                .as_struct_mut()
                .and_then(|s| s.get_mut("$392"))
                .and_then(Value::as_list_mut)
            else {
                continue;
            };
            let mut pages: Vec<(i64, Value)> = Vec::new();
            let mut has_page_list = false;
            for container in containers.iter_mut() {
                match container_type(container) {
                    Some("$236") => {
                        let inner = match container {
                            Value::Annotated { value, .. } => value.as_mut(),
                            v => v,
                        };
                        let Some(entries) = inner
                            .as_struct_mut()
                            .and_then(|s| s.get_mut("$247"))
                            .and_then(Value::as_list_mut)
                        else {
                            continue;
                        };
                        let mut kept = Vec::new();
                        for entry in entries.drain(..) {
                            let label = entry
                                .unannotated()
                                .as_struct()
                                .and_then(|s| s.get("$241"))
                                .and_then(Value::as_struct)
                                .and_then(|l| l.get("$244"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            if let Some(rest) = label.strip_prefix("page_list_entry:")
                                && let Some((seq, text)) = rest.split_once(':')
                                && let Ok(seq) = seq.parse::<i64>()
                            {
                                let pos = entry
                                    .unannotated()
                                    .as_struct()
                                    .and_then(|s| s.get("$246"))
                                    .cloned()
                                    .unwrap_or(Value::Null);
                                let mut label = Struct::new();
                                label.insert("$244", Value::string(text));
                                let mut p = Struct::new();
                                p.insert("$241", Value::Struct(label));
                                p.insert("$246", pos);
                                pages.push((seq, annotated("$393", p)));
                            } else {
                                kept.push(entry);
                            }
                        }
                        *entries = kept;
                    }
                    Some("$237") => has_page_list = true,
                    _ => {}
                }
            }
            if !pages.is_empty() && !has_page_list {
                pages.sort_by_key(|(seq, _)| *seq);
                let mut s = Struct::new();
                s.insert("$235", Value::sym("$237"));
                s.insert("$239", Value::Null); // filled below with a fresh uuid symbol
                s.insert(
                    "$247",
                    Value::List(pages.into_iter().map(|(_, p)| p).collect()),
                );
                new_containers.push((ro_idx, annotated("$391", s)));
            }
        }
    }
    for (ro_idx, mut container) in new_containers {
        let id = new_uuid_symbol(book);
        if let Value::Annotated { value, .. } = &mut container
            && let Some(s) = value.as_struct_mut()
        {
            s.insert("$239", Value::Symbol(id));
        }
        let nav = book.fragments.get_single_mut(&nav_type).unwrap();
        if let Some(containers) = nav.value.as_list_mut().unwrap()[ro_idx]
            .as_struct_mut()
            .and_then(|s| s.get_mut("$392"))
            .and_then(Value::as_list_mut)
        {
            containers.push(container);
        }
    }
}

/// eid -> text for every storyline entry that carries `$155` and a `$145` reference.
fn storyline_texts(book: &Book) -> HashMap<i64, Vec<char>> {
    let mut contents: HashMap<String, Vec<String>> = HashMap::new();
    for f in book.fragments.all(&Sym::new("$145")) {
        if let (Some(fid), Some(list)) = (
            &f.fid,
            f.value
                .as_struct()
                .and_then(|s| s.get("$146"))
                .and_then(Value::as_list),
        ) {
            contents.insert(
                fid.0.clone(),
                list.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
            );
        }
    }
    let mut out = HashMap::new();
    fn walk(v: &Value, contents: &HashMap<String, Vec<String>>, out: &mut HashMap<i64, Vec<char>>) {
        match v {
            Value::Annotated { value, .. } => walk(value, contents, out),
            Value::List(items) | Value::SExp(items) => {
                items.iter().for_each(|i| walk(i, contents, out))
            }
            Value::Struct(s) => {
                if let (Some(Value::Int(eid)), Some(text)) = (s.get("$155"), s.get("$145")) {
                    let resolved = match text {
                        Value::String(t) => Some(t.clone()),
                        Value::Struct(r) => {
                            let name = r.get("name").and_then(Value::as_sym).map(|s| s.0.clone());
                            let idx = r.get("$403").and_then(Value::as_int);
                            match (name, idx) {
                                (Some(n), Some(i)) => {
                                    contents.get(&n).and_then(|l| l.get(i as usize)).cloned()
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    if let Some(t) = resolved {
                        out.entry(*eid).or_insert_with(|| t.chars().collect());
                    }
                }
                for (_, fv) in s.iter() {
                    walk(fv, contents, out);
                }
            }
            _ => {}
        }
    }
    for f in book.fragments.all(&Sym::new("$259")) {
        walk(&f.value, &contents, &mut out);
    }
    out
}

/// eids defined by the section page templates (`$155` inside `$260` fragments).
fn page_template_eids(book: &Book) -> HashSet<i64> {
    let mut out = HashSet::new();
    fn walk(v: &Value, out: &mut HashSet<i64>) {
        match v {
            Value::Annotated { value, .. } => walk(value, out),
            Value::List(items) | Value::SExp(items) => items.iter().for_each(|i| walk(i, out)),
            Value::Struct(s) => {
                if let Some(Value::Int(eid)) = s.get("$155") {
                    out.insert(*eid);
                }
                for (_, fv) in s.iter() {
                    walk(fv, out);
                }
            }
            _ => {}
        }
    }
    for f in book.fragments.all(&Sym::new("$260")) {
        walk(&f.value, &mut out);
    }
    out
}

fn determine_pages(
    pos_info: &[ContentChunk],
    texts: &HashMap<i64, Vec<char>>,
    template_eids: &HashSet<i64>,
    positions_per_page: i64,
) -> (Vec<Value>, usize) {
    let mut pages: Vec<Value> = Vec::new();
    let mut new_section_pages = 0usize;
    let mut next_page_pid = 0i64;
    let mut prev_section: Option<&Sym> = None;
    for chunk in pos_info {
        if template_eids.contains(&chunk.eid) {
            continue;
        }
        let new_section = prev_section != Some(&chunk.section);
        prev_section = Some(&chunk.section);
        if new_section {
            next_page_pid = chunk.pid;
            new_section_pages += 1;
        }
        let text = texts.get(&chunk.eid);
        let mut min_chunk_offset = 0i64;
        loop {
            let mut chunk_offset = (next_page_pid - chunk.pid).max(0);
            if chunk_offset >= chunk.length {
                break;
            }
            if let Some(t) = text
                && t.get(chunk_offset as usize)
                    .is_some_and(|c| !c.is_whitespace())
            {
                let init = chunk_offset;
                loop {
                    if chunk_offset == 0 {
                        break;
                    }
                    if chunk_offset <= min_chunk_offset {
                        chunk_offset = init;
                        break;
                    }
                    if t.get(chunk_offset as usize - 1)
                        .is_some_and(|c| c.is_whitespace())
                    {
                        break;
                    }
                    chunk_offset -= 1;
                }
            }
            let mut label = Struct::new();
            label.insert("$244", Value::String((pages.len() + 1).to_string()));
            let mut pos = Struct::new();
            pos.insert("$155", Value::Int(chunk.eid));
            pos.insert("$143", Value::Int(chunk.eid_offset + chunk_offset));
            let mut p = Struct::new();
            p.insert("$241", Value::Struct(label));
            p.insert("$246", Value::Struct(pos));
            pages.push(Value::Struct(p));
            next_page_pid += positions_per_page;
            min_chunk_offset = chunk_offset + (positions_per_page - MAX_WHITE_SPACE_ADJUST).max(1);
        }
    }
    (pages, new_section_pages)
}

/// Adds an `APPROXIMATE_PAGE_LIST` page list to the single reading order's navigation.
/// `desired` = `None` picks kfxlib's auto density (1850 positions per page). Returns the
/// number of pages the book now has.
pub fn add_approximate_pages(book: &mut Book, desired: Option<u32>) -> Result<u32, FixupError> {
    let cde = book
        .title_metadata("cde_content_type")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(c) = &cde
        && !["EBOK", "EBSP", "PDOC"].contains(&c.as_str())
    {
        return Err(FixupError::Malformed(format!(
            "cannot create page numbers for KFX {c}"
        )));
    }
    if book.is_fixed_layout() {
        return Err(FixupError::Malformed(
            "approximate pages for fixed layout are not supported".into(),
        ));
    }
    let orders = book.reading_orders();
    if orders.len() != 1 {
        return Err(FixupError::Malformed(
            "page numbers need a single reading order".into(),
        ));
    }
    let ro_name = orders[0].0.clone();
    let desired_pages = desired.map(i64::from).unwrap_or(0);

    // Existing page list?
    let nav_type = Sym::new("$389");
    let mut existing_real: Option<usize> = None;
    if let Some(nav) = book.fragments.get_single_mut(&nav_type)
        && let Some(navs) = nav.value.as_list_mut()
    {
        for book_nav in navs.iter_mut() {
            let Some(bs) = book_nav.as_struct_mut() else {
                continue;
            };
            if bs.get("$178").and_then(Value::as_sym) != ro_name.as_ref() {
                continue;
            }
            let Some(containers) = bs.get_mut("$392").and_then(Value::as_list_mut) else {
                continue;
            };
            if let Some(i) = containers
                .iter()
                .position(|c| container_type(c) == Some("$237"))
            {
                let c = containers[i].unannotated().as_struct().unwrap();
                let name = c
                    .get("$239")
                    .and_then(Value::as_sym)
                    .map(|s| s.0.clone())
                    .unwrap_or_default();
                let n = c
                    .get("$247")
                    .and_then(Value::as_list)
                    .map(Vec::len)
                    .unwrap_or(0);
                if desired_pages == 0 || n as i64 == desired_pages || name != APPROXIMATE_PAGE_LIST
                {
                    existing_real = Some(n);
                } else {
                    containers.remove(i);
                }
            }
            break;
        }
    }
    if let Some(n) = existing_real {
        return Ok(n as u32);
    }

    // Position info: kfxlib walks the content; the KPF position map describes the same runs,
    // and the storyline text supplies the characters for the whitespace adjustment.
    let pos_info = pos_info_for_pages(book)?;
    let texts = storyline_texts(book);
    let template_eids = page_template_eids(book);
    let sections = book.ordered_section_names();
    if sections.is_empty() || pos_info.is_empty() {
        return Err(FixupError::Malformed(
            "no content found for page numbers".into(),
        ));
    }
    let (pages, _chapter_pages) = if desired_pages == 0 {
        determine_pages(
            &pos_info,
            &texts,
            &template_eids,
            TYPICAL_POSITIONS_PER_PAGE,
        )
    } else {
        let (mut lo, mut hi) = (MIN_POSITIONS_PER_PAGE, MAX_POSITIONS_PER_PAGE);
        let mut best = determine_pages(
            &pos_info,
            &texts,
            &template_eids,
            TYPICAL_POSITIONS_PER_PAGE,
        );
        while lo <= hi {
            let ppp = (lo + hi) / 2;
            best = determine_pages(&pos_info, &texts, &template_eids, ppp);
            let n = best.0.len() as i64;
            if n == desired_pages {
                break;
            } else if n > desired_pages {
                lo = ppp + 1;
            } else {
                hi = ppp - 1;
            }
        }
        best
    };
    let count = pages.len() as u32;
    if pages.is_empty() {
        return Ok(0);
    }
    let name = book.symtab.create_local(APPROXIMATE_PAGE_LIST);
    let mut s = Struct::new();
    s.insert("$235", Value::sym("$237"));
    s.insert("$239", Value::Symbol(name));
    s.insert(
        "$247",
        Value::List(
            pages
                .into_iter()
                .map(|p| annotated("$393", p.as_struct().cloned().unwrap()))
                .collect(),
        ),
    );
    let container = annotated("$391", s);
    if book.fragments.get_single(&nav_type).is_none() {
        let mut bn = Struct::new();
        if let Some(n) = &ro_name {
            bn.insert("$178", Value::Symbol(n.clone()));
        }
        bn.insert("$392", Value::List(Vec::new()));
        book.push_fragment(Fragment::root("$389", Value::List(vec![Value::Struct(bn)])));
    }
    let nav = book.fragments.get_single_mut(&nav_type).unwrap();
    for book_nav in nav.value.as_list_mut().unwrap().iter_mut() {
        let Some(bs) = book_nav.as_struct_mut() else {
            continue;
        };
        if bs.get("$178").and_then(Value::as_sym) != ro_name.as_ref() {
            continue;
        }
        if let Some(containers) = bs.get_mut("$392").and_then(Value::as_list_mut) {
            containers.push(container);
        }
        break;
    }
    Ok(count)
}

/// The section of each eid, from `$264`: each section's eids, an eid or a `[start, count]` run.
/// Kept as the runs `(start, end, section)` in the order they came, not a key an eid, so a run's
/// memory doesn't grow with its length (size-limits design). A later entry wins over an earlier
/// one for an eid in both, as inserting each eid into a map did.
struct EidSections {
    runs: Vec<(i64, i64, Sym)>,
}

impl EidSections {
    /// The section of each of `eids`: the last run holding it, found in one sweep of the runs
    /// and the eids, both in eid order.
    fn sections_of(&self, eids: &[i64]) -> Vec<Option<Sym>> {
        use std::collections::BinaryHeap;
        let mut by_start: Vec<usize> = (0..self.runs.len()).collect();
        by_start.sort_by_key(|&i| self.runs[i].0);
        let mut asked: Vec<usize> = (0..eids.len()).collect();
        asked.sort_by_key(|&q| eids[q]);
        // Runs begun so far, the latest first; one that ended is dropped when it comes up.
        let mut open: BinaryHeap<(usize, i64)> = BinaryHeap::new();
        let mut next = 0;
        let mut out = vec![None; eids.len()];
        for q in asked {
            let eid = eids[q];
            while next < by_start.len() && self.runs[by_start[next]].0 <= eid {
                let i = by_start[next];
                open.push((i, self.runs[i].1));
                next += 1;
            }
            while open.peek().is_some_and(|&(_, end)| end <= eid) {
                open.pop();
            }
            out[q] = open.peek().map(|&(i, _)| self.runs[i].2.clone());
        }
        out
    }

    #[cfg(test)]
    fn section(&self, eid: i64) -> Option<Sym> {
        self.sections_of(&[eid]).pop().flatten()
    }

    /// How many entries are held.
    #[cfg(test)]
    fn held(&self) -> usize {
        self.runs.len()
    }
}

fn eid_sections(book: &Book) -> Result<EidSections, FixupError> {
    let mut runs: Vec<(i64, i64, Sym)> = Vec::new();
    let mut ranged = 0u64;
    let held = super::positions::content_size(book);
    if let Some(pm) = book
        .fragments
        .get_single(&Sym::new("$264"))
        .and_then(|f| f.value.as_list())
    {
        for sect in pm {
            let Some(s) = sect.as_struct() else { continue };
            let Some(name) = s.get("$174").and_then(Value::as_sym) else {
                continue;
            };
            for e in s.get("$181").and_then(Value::as_list).into_iter().flatten() {
                match e {
                    Value::Int(i) => {
                        runs.push((*i, i.saturating_add(1), name.clone()));
                    }
                    Value::List(range) if range.len() == 2 => {
                        if let (Some(start), Some(n)) = (range[0].as_int(), range[1].as_int()) {
                            // An element a position: the runs together can't pass what the
                            // content holds.
                            ranged = ranged.saturating_add(n.max(0) as u64);
                            if ranged > held {
                                return Err(FixupError::Malformed(format!(
                                    "the book's position ranges pass the {held} its text and \
                                     elements can hold"
                                )));
                            }
                            if n > 0 {
                                runs.push((start, start.saturating_add(n), name.clone()));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(EidSections { runs })
}

/// Content runs for page numbering. Before the position map is built we read the KPF's own
/// map; afterwards we rebuild the same runs from `$265`/`$264`.
fn pos_info_for_pages(book: &mut Book) -> Result<Vec<ContentChunk>, FixupError> {
    if book.fragments.get_single(&Sym::new("$611")).is_some() {
        return collect_position_map_info(book);
    }
    let Some(pim) = book.fragments.get_single(&Sym::new("$265")) else {
        return Ok(Vec::new());
    };
    let Some(entries) = pim.value.as_list() else {
        return Ok(Vec::new());
    };
    let eid_section = eid_sections(book)?;
    let mut out = Vec::new();
    for w in entries.windows(2) {
        let (Some(a), Some(b)) = (w[0].as_struct(), w[1].as_struct()) else {
            continue;
        };
        let pid = a.get("$184").and_then(Value::as_int).unwrap_or(0);
        let next = b.get("$184").and_then(Value::as_int).unwrap_or(pid);
        let eid = a.get("$185").and_then(Value::as_int).unwrap_or(0);
        let offset = a.get("$143").and_then(Value::as_int).unwrap_or(0);
        out.push(ContentChunk {
            pid,
            eid,
            eid_offset: offset,
            length: next.saturating_sub(pid),
            section: Sym::new(""),
        });
    }
    let eids: Vec<i64> = out.iter().map(|c| c.eid).collect();
    for (c, s) in out.iter_mut().zip(eid_section.sections_of(&eids)) {
        if let Some(s) = s {
            c.section = s;
        }
    }
    super::positions::check_positions(book, &out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fragment::FragmentList;
    use crate::symtab::SymbolTable;

    /// A book of `text` characters of cheap repeated text, with `$264` as given.
    fn book(text: usize, sections: Vec<(&str, Vec<Value>)>) -> Book {
        let mut fragments = FragmentList::new();
        let mut content = Struct::new();
        content.insert("name", Value::sym("content_1"));
        content.insert("$146", Value::List(vec![Value::String("x".repeat(text))]));
        fragments.push(Fragment::new(
            "$145",
            Some("content_1"),
            Value::Struct(content),
        ));
        let map = sections
            .into_iter()
            .map(|(name, eids)| {
                let mut s = Struct::new();
                s.insert("$174", Value::sym(name));
                s.insert("$181", Value::List(eids));
                Value::Struct(s)
            })
            .collect();
        fragments.push(Fragment::root("$264", Value::List(map)));
        Book {
            fragments,
            symtab: SymbolTable::yj_default(),
            kcb: serde_json::Value::Null,
            source_epub: None,
            container_id: String::new(),
            is_prepub: false,
            warnings: Vec::new(),
        }
    }

    fn run(start: i64, n: i64) -> Value {
        Value::List(vec![Value::Int(start), Value::Int(n)])
    }

    #[test]
    fn a_run_of_eids_is_held_as_one_entry() {
        // Two million characters of text (cheap), and a run of nearly as many eids: held as the
        // run, not a key an eid.
        let b = book(
            2_000_000,
            vec![
                ("s1", vec![run(100, 1_900_000), Value::Int(5)]),
                ("s2", vec![Value::Int(150), run(1_900_050, 100)]),
            ],
        );
        let sections = eid_sections(&b).unwrap();
        assert!(sections.held() <= 4, "{} entries held", sections.held());
        let at = |eid| sections.section(eid).map(|s| s.as_str().to_owned());
        assert_eq!(at(5).as_deref(), Some("s1"));
        assert_eq!(at(100).as_deref(), Some("s1"));
        assert_eq!(at(1_900_049).as_deref(), Some("s1"));
        assert_eq!(at(1_900_099).as_deref(), Some("s2"));
        assert_eq!(at(99), None);
        // A later entry wins, as inserting into a map did: 150 was put in s2 after the s1 run,
        // and so were 1_900_100 and on.
        assert_eq!(at(150).as_deref(), Some("s2"));
        assert_eq!(at(1_900_100).as_deref(), Some("s2"));
        assert_eq!(at(1_900_149).as_deref(), Some("s2"));
        assert_eq!(at(1_900_150), None);
    }

    #[test]
    fn page_template_elements_count_toward_the_bound() {
        // A fixed-layout shape: no text, a section page template (`$260`) of 500 elements,
        // each with its eid, and 500 positions. The bound has to count the template.
        let mut b = book(0, vec![]);
        let elements: Vec<Value> = (0..500)
            .map(|i| {
                let mut s = Struct::new();
                s.insert("$155", Value::Int(1000 + i));
                Value::Struct(s)
            })
            .collect();
        let mut template = Struct::new();
        template.insert("$174", Value::sym("s1"));
        template.insert("$141", Value::List(elements));
        b.fragments
            .push(Fragment::new("$260", Some("s1"), Value::Struct(template)));
        let chunk = |length| ContentChunk {
            pid: 0,
            eid: 1000,
            eid_offset: 0,
            length,
            section: Sym::new("s1"),
        };
        super::super::positions::check_positions(&b, &[chunk(500)]).unwrap();
        assert!(super::super::positions::check_positions(&b, &[chunk(5_000)]).is_err());
    }

    #[test]
    fn runs_past_the_text_are_still_refused() {
        let b = book(1_000, vec![("s1", vec![run(0, 1_000_000)])]);
        assert!(eid_sections(&b).is_err());
    }
}
