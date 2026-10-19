// SPDX-License-Identifier: GPL-3.0-or-later
//! Final pass: drop unreferenced fragments, rebuild `$270`, `$419` and the symbol table, and
//! sort fragments into kfxlib's order (`check_fragment_usage(rebuild=True)`,
//! `rebuild_container_entity_map`, `check_symbol_table(rebuild=True)`).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::{Book, FixupError};
use crate::consts::{
    COMMON_FRAGMENT_REFERENCES, EID_REFERENCES, FRAGMENT_ID_KEYS, NESTED_FRAGMENT_REFERENCES,
    SPECIAL_FRAGMENT_REFERENCES, SPECIAL_PARENT_NON_REFERENCES,
};
use crate::fragment::{CONTAINER_FRAGMENT_TYPES, Fragment, FragmentList, ROOT_FRAGMENT_TYPES};
use crate::ion::{Struct, Sym, Value};

/// Fragment types kfxlib knows; anything else is kept as a root.
const KNOWN_FRAGMENT_TYPES: [&str; 36] = [
    "$ion_symbol_table",
    "$270",
    "$490",
    "$389",
    "$419",
    "$538",
    "$550",
    "$258",
    "$265",
    "$264",
    "$611",
    "$266",
    "$597",
    "$418",
    "$417",
    "$394",
    "$145",
    "$585",
    "$610",
    "$164",
    "$262",
    "$593",
    "$391",
    "$692",
    "$387",
    "$395",
    "$756",
    "$260",
    "$267",
    "$390",
    "$609",
    "$259",
    "$608",
    "$157",
    "$621",
    "$XXX",
];

/// A fragment key: (type, fid or None).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(pub String, pub Option<String>);

impl Key {
    fn of(f: &Fragment) -> Key {
        Key(f.ftype.0.clone(), f.fid.as_ref().map(|s| s.0.clone()))
    }
    fn new(ftype: &str, fid: &str) -> Key {
        Key(ftype.to_owned(), Some(fid.to_owned()))
    }
    /// kfxlib `YJFragmentKey.sort_key`: (preferred type index, fid-or-type).
    fn sort_key(&self) -> (usize, String) {
        let order = crate::fragment::PREFERRED_FRAGMENT_TYPE_ORDER
            .iter()
            .position(|t| *t == self.0)
            .unwrap_or(crate::fragment::PREFERRED_FRAGMENT_TYPE_ORDER.len());
        (order, self.1.clone().unwrap_or_else(|| self.0.clone()))
    }
}

/// kfxlib `natural_sort_key`: lowercase, digit runs left-padded to 8.
pub fn natural_sort_key(s: &str) -> String {
    let mut out = String::new();
    let lower = s.to_lowercase();
    let mut digits = String::new();
    let flush = |digits: &mut String, out: &mut String| {
        if !digits.is_empty() {
            if digits.len() < 8 {
                out.push_str(&"0".repeat(8 - digits.len()));
            }
            out.push_str(digits);
            digits.clear();
        }
    };
    for c in lower.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            flush(&mut digits, &mut out);
            out.push(c);
        }
    }
    flush(&mut digits, &mut out);
    out
}

struct Walk<'b> {
    book: &'b Book,
    ftype: String,
    mandatory: BTreeSet<Key>,
    optional: BTreeSet<Key>,
}

impl Walk<'_> {
    fn walk(&mut self, data: &Value, container: Option<&str>, parent: Option<&str>) {
        let container_name = container.unwrap_or(&self.ftype).to_owned();
        match data {
            Value::Annotated { value, .. } => self.walk(value, Some(&container_name), parent),
            Value::List(items) => items
                .iter()
                .for_each(|i| self.walk(i, Some(&container_name), parent)),
            Value::Struct(s) => {
                for (k, v) in s.iter() {
                    self.walk(v, Some(k.as_str()), Some(&container_name));
                }
            }
            Value::SExp(items) => {
                if let Some(Value::Symbol(head)) = items.first() {
                    let head = head.0.clone();
                    for i in &items[1..] {
                        self.walk(i, Some(&head), Some(&container_name));
                    }
                }
            }
            Value::String(s) => {
                if container_name == "$165" || container_name == "$636" {
                    self.symbol(s, &container_name, parent);
                }
            }
            Value::Symbol(s) => self.symbol(&s.0, &container_name, parent),
            _ => {}
        }
    }

    fn symbol(&mut self, name: &str, container: &str, parent: Option<&str>) {
        let ftype = self.ftype.as_str();
        let own_id = parent == Some(ftype)
            && FRAGMENT_ID_KEYS
                .iter()
                .any(|(t, keys)| *t == ftype && keys.contains(&container));
        if own_id {
            return;
        }
        let mut frag_ref: Option<&str> = SPECIAL_FRAGMENT_REFERENCES
            .iter()
            .find(|((t, k), _)| *t == ftype && *k == container)
            .map(|(_, r)| *r);
        if frag_ref.is_none()
            && let Some(p) = parent
            && SPECIAL_PARENT_NON_REFERENCES
                .iter()
                .any(|(t, pk)| *t == ftype && *pk == p)
        {
            return;
        }
        if frag_ref.is_none()
            && let Some(p) = parent
        {
            frag_ref = NESTED_FRAGMENT_REFERENCES
                .iter()
                .find(|((pp, k), _)| *pp == p && *k == container)
                .map(|(_, r)| *r);
        }
        if frag_ref.is_none() {
            frag_ref = COMMON_FRAGMENT_REFERENCES
                .iter()
                .find(|(k, _)| *k == container)
                .map(|(_, r)| *r);
        }
        let Some(frag_ref) = frag_ref else { return };
        if container == "name" && matches!(parent, Some("$249") | Some("$692")) {
            self.mandatory.insert(Key::new("$692", name));
        } else if container == "$165"
            && self
                .book
                .fragments
                .get(&Sym::new("$418"), Some(&Sym::new(name)))
                .is_some()
        {
            self.mandatory.insert(Key::new("$418", name));
        } else if container == "$635" {
            self.optional.insert(Key::new(frag_ref, name));
        } else {
            self.mandatory.insert(Key::new(frag_ref, name));
        }
        if frag_ref == "$260" {
            for (t, fid) in [
                ("$609", name.to_owned()),
                ("$609", format!("{name}-spm")),
                ("$597", format!("{name}-ad")),
                ("$597", name.to_owned()),
                ("$267", name.to_owned()),
                ("$387", name.to_owned()),
            ] {
                if self
                    .book
                    .fragments
                    .get(&Sym::new(t), Some(&Sym::new(&fid)))
                    .is_some()
                {
                    self.mandatory.insert(Key::new(t, &fid));
                }
            }
        }
    }
}

fn walk_fragment(book: &Book, f: &Fragment) -> (BTreeSet<Key>, BTreeSet<Key>) {
    let mut w = Walk {
        book,
        ftype: f.ftype.0.clone(),
        mandatory: BTreeSet::new(),
        optional: BTreeSet::new(),
    };
    // kfxlib walks the annotated fragment, so top-level keys get `parent == ftype`.
    w.walk(&f.value, Some(&f.ftype.0.clone()), None);
    (w.mandatory, w.optional)
}

fn find_symbols(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::Annotated { annots, value } => {
            annots.iter().for_each(|a| {
                out.insert(a.0.clone());
            });
            find_symbols(value, out);
        }
        Value::List(items) | Value::SExp(items) => items.iter().for_each(|i| find_symbols(i, out)),
        Value::Struct(s) => {
            for (k, fv) in s.iter() {
                out.insert(k.0.clone());
                find_symbols(fv, out);
            }
        }
        Value::Symbol(s) => {
            out.insert(s.0.clone());
        }
        _ => {}
    }
}

fn random_container_id() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    let tail: String = (0..28)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect();
    format!("CR!{tail}")
}

/// Generates a kfxlib-style container id (`CR!` + 28 uppercase alphanumerics).
pub fn new_container_id() -> String {
    random_container_id()
}

/// Final rebuild. `creator` goes into `$270.$587`.
pub fn rebuild(book: &mut Book, container_id: &str, creator: &str) -> Result<(), FixupError> {
    // 1. Reachability from the root fragments.
    let mut discovered: BTreeSet<Key> = BTreeSet::new();
    for f in book.fragments.iter() {
        let t = f.ftype.as_str();
        if (ROOT_FRAGMENT_TYPES.contains(&t) && t != "$419") || !KNOWN_FRAGMENT_TYPES.contains(&t) {
            discovered.insert(Key::of(f));
        }
    }
    if let Some(cover) = book.title_metadata("cover_image") {
        let name = match cover {
            Value::String(s) => Some(s.clone()),
            Value::Symbol(s) => Some(s.0.clone()),
            _ => None,
        };
        if let Some(n) = name {
            discovered.insert(Key::new("$164", &n));
        }
    }
    let mut visited: BTreeSet<Key> = BTreeSet::new();
    for t in CONTAINER_FRAGMENT_TYPES {
        visited.insert(Key(t.to_owned(), None));
        visited.insert(Key(t.to_owned(), Some(t.to_owned())));
    }
    let mut mandatory_refs: BTreeMap<Key, BTreeSet<Key>> = BTreeMap::new();
    let mut optional_refs: BTreeMap<Key, BTreeSet<Key>> = BTreeMap::new();
    let mut missing: BTreeSet<Key> = BTreeSet::new();
    while !discovered.is_empty() {
        let next_visits: BTreeSet<Key> = discovered.difference(&visited).cloned().collect();
        discovered.clear();
        let mut found: HashSet<Key> = HashSet::new();
        let snapshot: Vec<Fragment> = book
            .fragments
            .iter()
            .filter(|f| next_visits.contains(&Key::of(f)))
            .cloned()
            .collect();
        for f in &snapshot {
            let key = Key::of(f);
            let (m, o) = walk_fragment(book, f);
            visited.insert(key.clone());
            found.insert(key.clone());
            discovered.extend(m.iter().cloned());
            discovered.extend(o.iter().cloned());
            mandatory_refs.insert(key.clone(), m);
            optional_refs.insert(key, o);
        }
        for k in next_visits {
            if !found.contains(&k) {
                missing.insert(k);
            }
        }
    }
    for k in &missing {
        book.warnings.push(format!(
            "referenced fragment is missing from book: {} {:?}",
            k.0, k.1
        ));
    }

    // 2. Keep referenced fragments (dedupe identical duplicates), drop the rest.
    let mut referenced: Vec<Fragment> = Vec::new();
    let mut seen: BTreeMap<Key, Value> = BTreeMap::new();
    for f in book.fragments.iter() {
        let key = Key::of(f);
        if f.ftype.as_str() != "$262" && f.ftype.as_str() != "$387" {
            if let Some(prev) = seen.get(&key) {
                if f.ftype.as_str() == "$270" || f.ftype.as_str() == "$593" || *prev == f.value {
                    continue;
                }
                return Err(FixupError::Malformed(format!(
                    "duplicate fragment key with different content: {key:?}"
                )));
            }
            seen.insert(key.clone(), f.value.clone());
        }
        if visited.contains(&key) {
            referenced.push(f.clone());
        }
    }

    // 3. Fresh `$270`.
    let mut existing_ids: BTreeSet<String> = BTreeSet::new();
    let (mut app_version, mut package_version, mut version) =
        (None::<String>, None::<String>, None::<i64>);
    for f in book.fragments.all(&Sym::new("$270")) {
        if let Some(s) = f.value.as_struct() {
            if let Some(id) = s
                .get("$409")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
            {
                existing_ids.insert(id.to_owned());
            }
            if let Some(v) = s
                .get("$587")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
            {
                app_version = Some(v.to_owned());
            }
            if let Some(v) = s
                .get("$588")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
            {
                package_version = Some(v.to_owned());
            }
            if let Some(v) = s.get("version").and_then(Value::as_int) {
                version = Some(v);
            }
        }
    }
    referenced.retain(|f| f.ftype.as_str() != "$270");
    let container_id = if existing_ids.len() == 1 {
        existing_ids.into_iter().next().unwrap()
    } else {
        container_id.to_owned()
    };
    let mut c = Struct::new();
    c.insert("$409", Value::String(container_id.clone()));
    c.insert("$161", Value::string("KFX main"));
    c.insert(
        "$587",
        Value::String(app_version.unwrap_or_else(|| creator.to_owned())),
    );
    c.insert("$588", Value::String(package_version.unwrap_or_default()));
    c.insert("version", Value::Int(version.unwrap_or(2)));
    referenced.push(Fragment::root("$270", Value::Struct(c)));

    // 4. Sort into kfxlib's order.
    referenced.sort_by_cached_key(|f| Key::of(f).sort_key());

    // 5. `$419` container entity map with dependencies.
    let deps = entity_dependencies(&mandatory_refs, &optional_refs);
    referenced.retain(|f| f.ftype.as_str() != "$419");
    let mut entity_ids: Vec<Sym> = Vec::new();
    for f in &referenced {
        if !CONTAINER_FRAGMENT_TYPES.contains(&f.ftype.as_str())
            && let Some(fid) = &f.fid
            && !entity_ids.contains(fid)
        {
            entity_ids.push(fid.clone());
        }
    }
    let mut contents = Struct::new();
    contents.insert("$155", Value::String(container_id.clone()));
    contents.insert(
        "$181",
        Value::List(entity_ids.iter().cloned().map(Value::Symbol).collect()),
    );
    let mut map = Struct::new();
    map.insert("$252", Value::List(vec![Value::Struct(contents)]));
    if !deps.is_empty() {
        map.insert("$253", Value::List(deps));
    }
    if !entity_ids.is_empty() {
        referenced.push(Fragment::root("$419", Value::Struct(map)));
    } else {
        book.warnings
            .push("omitting container_entity_map due to lack of content".into());
    }
    book.fragments = FragmentList(referenced);
    book.container_id = container_id;

    // 6. Symbol table from the symbols actually used.
    let mut used: HashSet<String> = HashSet::new();
    for f in book.fragments.iter() {
        if CONTAINER_FRAGMENT_TYPES.contains(&f.ftype.as_str()) {
            continue;
        }
        if let Some(fid) = &f.fid {
            used.insert(fid.0.clone());
        }
        used.insert(f.ftype.0.clone());
        find_symbols(&f.value, &mut used);
    }
    let mut locals: Vec<String> = used
        .into_iter()
        .filter(|s| {
            book.symtab
                .id_of(&Sym::new(s))
                .is_some_and(|id| id >= book.symtab.local_min_id())
        })
        .collect();
    locals.sort_by_cached_key(|s| natural_sort_key(s));
    book.symtab.replace_locals(locals);
    book.fragments
        .retain(|f| f.ftype.as_str() != "$ion_symbol_table");
    book.fragments.0.insert(
        0,
        Fragment::root("$ion_symbol_table", book.symtab.import_fragment()),
    );
    Ok(())
}

/// kfxlib `determine_entity_dependencies`: `$260` -> `$164` and `$164` -> `$417` deep references.
fn entity_dependencies(
    mandatory: &BTreeMap<Key, BTreeSet<Key>>,
    optional: &BTreeMap<Key, BTreeSet<Key>>,
) -> Vec<Value> {
    let mut mandatory = mandatory.clone();
    for (k, refs) in mandatory.iter_mut() {
        if k.0 == "$387" {
            refs.clear();
        }
    }
    let mut deep: BTreeMap<Key, BTreeSet<Key>> = BTreeMap::new();
    for (k, refs) in &mandatory {
        let mut new_refs: BTreeSet<Key> = refs.clone();
        if k.0 == "$164" {
            new_refs.retain(|r| r.0 != "$164");
        }
        let mut old_refs: BTreeSet<Key> = BTreeSet::new();
        while new_refs.difference(&old_refs).next().is_some() {
            old_refs = old_refs.union(&new_refs).cloned().collect();
            new_refs = old_refs.clone();
            for r in &old_refs {
                if let Some(more) = mandatory.get(r) {
                    new_refs.extend(more.iter().cloned());
                }
            }
        }
        deep.insert(k.clone(), new_refs);
    }
    let mut out = Vec::new();
    let mut keys: Vec<&Key> = deep.keys().collect();
    keys.sort_by_cached_key(|k| k.sort_key());
    for k in keys {
        let Some(fid) = &k.1 else { continue };
        let mut mand: Vec<Value> = Vec::new();
        let mut opt: Vec<Value> = Vec::new();
        for (depends, dependant) in [("$260", "$164"), ("$164", "$417")] {
            if k.0 != depends {
                continue;
            }
            let mut refs: Vec<&Key> = deep[k].iter().collect();
            refs.sort_by_cached_key(|r| r.sort_key());
            for r in refs {
                if r.0 == dependant
                    && let Some(rf) = &r.1
                {
                    mand.push(Value::sym(rf));
                    if let Some(o) = optional.get(r) {
                        let mut orefs: Vec<&Key> = o.iter().collect();
                        orefs.sort_by_cached_key(|r| r.sort_key());
                        for orf in orefs {
                            if orf.0 == dependant
                                && let Some(of) = &orf.1
                            {
                                opt.push(Value::sym(of));
                            }
                        }
                    }
                }
            }
        }
        if !mand.is_empty() {
            let mut s = Struct::new();
            s.insert("$155", Value::sym(fid));
            s.insert("$254", Value::List(mand));
            out.push(Value::Struct(s));
        }
        if !opt.is_empty() {
            let mut s = Struct::new();
            s.insert("$155", Value::sym(fid));
            s.insert("$255", Value::List(opt));
            out.push(Value::Struct(s));
        }
    }
    out
}

#[allow(dead_code)]
fn is_eid_container(c: &str) -> bool {
    EID_REFERENCES.contains(&c)
}
