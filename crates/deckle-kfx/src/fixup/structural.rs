// SPDX-License-Identifier: GPL-3.0-or-later
//! Structural KPF fix-ups (kfxlib `fix_kpf_prepub_book` up to the `$585` features block).

use super::{Book, FixupError, float_to_decimal};
use crate::consts::{EID_REFERENCES, GENERIC_CREATOR_VERSIONS, format_symbol};
use crate::fragment::Fragment;
use crate::ion::{Struct, Sym, Value};

fn kv(key: &str, value: Value) -> Value {
    let mut s = Struct::new();
    s.insert("$492", Value::string(key));
    s.insert("$307", value);
    Value::Struct(s)
}

pub fn fix_resource_location(s: &str) -> String {
    if s.starts_with("resource/") {
        s.to_owned()
    } else {
        format!("resource/{s}")
    }
}

/// Applies kfxlib's structural fix-ups in order.
pub fn fix_structural(book: &mut Book) -> Result<(), FixupError> {
    if !book.is_prepub {
        return Ok(());
    }
    // 1. `$417` resource ids get the `resource/` prefix.
    let raw = Sym::new("$417");
    let renames: Vec<(Sym, String)> = book
        .fragments
        .all(&raw)
        .filter_map(|f| {
            let fid = f.fid.clone()?;
            let fixed = fix_resource_location(fid.as_str());
            (fixed != fid.0).then_some((fid, fixed))
        })
        .collect();
    for (old, new) in renames {
        if let Some(mut f) = book.fragments.remove(&raw, Some(&old)) {
            f.fid = Some(book.symtab.create_local(&new));
            book.fragments.push(f);
        }
    }

    // 2. Per-fragment Ion fix-ups (everything but `$270`).
    let n = book.fragments.len();
    for i in 0..n {
        let (ftype, fid) = {
            let f = &book.fragments.0[i];
            (f.ftype.clone(), f.fid.clone())
        };
        if ftype.as_str() == "$270" {
            continue;
        }
        let value = std::mem::replace(&mut book.fragments.0[i].value, Value::Null);
        let fixed = fix_ion(book, &ftype, fid.as_ref(), value, None);
        book.fragments.0[i].value = fixed;
    }

    // 3. `$266` anchors: drop a zero `$143` offset.
    for f in book.fragments.all_mut(&Sym::new("$266")) {
        if let Some(pos) = f
            .value
            .as_struct_mut()
            .and_then(|s| s.get_mut("$183"))
            .and_then(Value::as_struct_mut)
            && pos.get("$143").and_then(Value::as_int) == Some(0)
        {
            pos.remove("$143");
        }
    }
    // 4. `$550` location map entries: drop `$178`.
    if let Some(f) = book.fragments.get_single_mut(&Sym::new("$550"))
        && let Some(list) = f.value.as_list_mut()
    {
        for lm in list {
            if let Some(s) = lm.as_struct_mut() {
                s.remove("$178");
            }
        }
    }

    // 5. `$490` book metadata defaults.
    fix_metadata(book)?;

    // 6. `$262` font declarations become root fragments; their data moves to `$418`.
    fix_fonts(book);

    // 7. `$164` pobject dimension copy and empty mime removal.
    fix_resources(book);

    // 8. `$585` content features.
    fix_features(book);
    Ok(())
}

/// Recursive port of kfxlib `kpf_fix_fragment._fix_ion_data`.
fn fix_ion(
    book: &mut Book,
    ftype: &Sym,
    fid: Option<&Sym>,
    value: Value,
    container: Option<&str>,
) -> Value {
    match value {
        Value::Annotated { annots, value } => {
            if annots.len() == 1 && annots[0].as_str() == "$608" {
                return fix_ion(book, ftype, fid, *value, container);
            }
            let annots = annots
                .into_iter()
                .map(
                    |a| match fix_ion(book, ftype, fid, Value::Symbol(a), None) {
                        Value::Symbol(s) => s,
                        _ => unreachable!(),
                    },
                )
                .collect();
            Value::Annotated {
                annots,
                value: Box::new(fix_ion(book, ftype, fid, *value, container)),
            }
        }
        Value::List(items) => {
            let list_container = match container {
                Some("$181") => Some("contains_list_"),
                Some("$141") => Some("$141"),
                _ => None,
            };
            let mut out = Vec::with_capacity(items.len());
            for (i, mut item) in items.into_iter().enumerate() {
                if container == Some("$146")
                    && let Value::Symbol(s) = &item
                    && let Some(structure) = book.fragments.get(&Sym::new("$608"), Some(s))
                {
                    item = structure.value.clone();
                }
                if (ftype.as_str() == "$609" && container == Some("contains_list_") && i == 1)
                    || (ftype.as_str() == "$538"
                        && container == Some("yj.semantics.containers_with_semantics"))
                {
                    item = symbol_id(book, item);
                }
                out.push(fix_ion(book, ftype, fid, item, list_container));
            }
            Value::List(out)
        }
        Value::SExp(items) => Value::SExp(
            items
                .into_iter()
                .map(|v| fix_ion(book, ftype, fid, v, None))
                .collect(),
        ),
        Value::Struct(s) => {
            let mut out = Struct::new();
            for (fk, fv) in s.0 {
                let mut fv = fix_ion(book, ftype, fid, fv, Some(fk.as_str()));
                let mut fk = fk;
                let key = fk.as_str();
                if key == "$597" {
                    continue;
                }
                if key == "$239"
                    && let Some(text) = value_text(&fv)
                {
                    book.symtab.create_local(&text);
                }
                if EID_REFERENCES.contains(&key)
                    && ftype.as_str() != "$597"
                    && matches!(fv, Value::Symbol(_))
                {
                    if key == "$598" {
                        fk = Sym::new("$155");
                    }
                    let is_section_ref = ftype.as_str() == "$610"
                        && fv.as_sym().is_some_and(|s| {
                            book.fragments.get(&Sym::new("$260"), Some(s)).is_some()
                        });
                    if !is_section_ref {
                        fv = symbol_id(book, fv);
                    }
                }
                let key = fk.as_str();
                if key == "$161"
                    && let Value::String(ext) = &fv
                    && let Some(sym) = format_symbol(ext)
                {
                    fv = Value::sym(sym);
                }
                if key.starts_with("yj.")
                    && [
                        "yj.authoring.",
                        "yj.conversion.",
                        "yj.print.",
                        "yj.semantics.",
                    ]
                    .iter()
                    .any(|p| key.starts_with(p))
                {
                    continue;
                }
                if book.is_illustrated_layout()
                    && ftype.as_str() == "$260"
                    && container == Some("$141")
                    && (key == "$67" || key == "$66")
                {
                    continue;
                }
                if key == "$165" {
                    match &fv {
                        Value::String(loc) => fv = Value::String(fix_resource_location(loc)),
                        other => {
                            book.warn(format!("location is not a string: {other:?}"));
                        }
                    }
                }
                if ftype.as_str() == "$157"
                    && key == "$173"
                    && let Some(fid) = fid
                    && fv.as_sym() != Some(fid)
                {
                    fv = Value::Symbol(fid.clone());
                }
                out.insert(fk, fv);
            }
            Value::Struct(out)
        }
        Value::Float(f) => Value::Decimal(float_to_decimal(f)),
        other => other,
    }
}

fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Symbol(s) => Some(s.0.clone()),
        Value::Int(i) => Some(i.to_string()),
        _ => None,
    }
}

/// kfxlib `symbol_id`: a symbol becomes its id as an Int; anything else passes through.
fn symbol_id(book: &Book, v: Value) -> Value {
    match v {
        Value::Symbol(s) => Value::Int(i64::from(book.symtab.id_of(&s).unwrap_or(0))),
        other => other,
    }
}

fn fix_metadata(book: &mut Book) -> Result<(), FixupError> {
    let kcb_tool = book
        .kcb
        .get("metadata")
        .and_then(|m| m.get("tool_name"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let kcb_version = book
        .kcb
        .get("metadata")
        .and_then(|m| m.get("tool_version"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let source = book.source_epub.clone();
    let container_id = book.container_id.clone();
    let has_asset_id = book.title_metadata("asset_id").is_some();
    let has_is_sample = book.title_metadata("is_sample").is_some();
    let has_issue_date = book.title_metadata("issue_date").is_some();
    let has_override_font = book.title_metadata("override_kindle_font").is_some();
    let has_cover = book.title_metadata("cover_image").is_some();
    let fixed_layout = book.is_fixed_layout();
    let language_ja_zh = book
        .title_metadata("language")
        .and_then(Value::as_str)
        .is_some_and(|l| l.to_lowercase().starts_with("ja-zh"));
    let (file_creator, creator_version) = (
        book.metadata_value("file_creator", "kindle_audit_metadata")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        book.metadata_value("creator_version", "kindle_audit_metadata")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
    );
    let generic = GENERIC_CREATOR_VERSIONS
        .iter()
        .any(|(c, v)| *c == file_creator && *v == creator_version);

    let Some(md) = book.fragments.get_single_mut(&Sym::new("$490")) else {
        return Ok(());
    };
    let Some(cats) = md
        .value
        .as_struct_mut()
        .and_then(|s| s.get_mut("$491"))
        .and_then(Value::as_list_mut)
    else {
        return Ok(());
    };
    for category in [
        "kindle_audit_metadata",
        "kindle_capability_metadata",
        "kindle_title_metadata",
    ] {
        let exists = cats.iter().any(|c| {
            c.as_struct()
                .and_then(|s| s.get("$495"))
                .and_then(Value::as_str)
                == Some(category)
        });
        if !exists {
            let mut s = Struct::new();
            s.insert("$495", Value::string(category));
            s.insert("$258", Value::List(Vec::new()));
            cats.push(Value::Struct(s));
        }
    }
    for cat in cats.iter_mut() {
        let Some(cs) = cat.as_struct_mut() else {
            continue;
        };
        let name = cs
            .get("$495")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let Some(list) = cs.get_mut("$258").and_then(Value::as_list_mut) else {
            continue;
        };
        match name.as_str() {
            "kindle_audit_metadata" => {
                if generic
                    && let (Some(tool), Some(ver)) = (&kcb_tool, &kcb_version)
                    && !ver.starts_with("unknown")
                {
                    let short = match tool.as_str() {
                        "Kindle Previewer 3" => "KPR",
                        "Kindle Create" => "KC",
                        t => t,
                    };
                    for m in list.iter_mut() {
                        let Some(ms) = m.as_struct_mut() else {
                            continue;
                        };
                        match ms.get("$492").and_then(Value::as_str) {
                            Some("file_creator") => ms.insert("$307", Value::string(short)),
                            Some("creator_version") => {
                                ms.insert("$307", Value::String(ver.clone()))
                            }
                            _ => {}
                        }
                    }
                }
            }
            "kindle_title_metadata" => {
                if !has_asset_id {
                    list.push(kv("asset_id", Value::String(container_id.clone())));
                }
                if !has_is_sample {
                    list.push(kv("is_sample", Value::Bool(false)));
                }
                if let Some(src) = &source {
                    if let Some(lang) = &src.original_language {
                        for m in list.iter_mut() {
                            let Some(ms) = m.as_struct_mut() else {
                                continue;
                            };
                            if ms.get("$492").and_then(Value::as_str) == Some("language") {
                                ms.insert("$307", Value::String(lang.clone()));
                                break;
                            }
                        }
                    }
                    if src.authors.len() > 1 {
                        list.retain(|m| {
                            m.as_struct()
                                .and_then(|s| s.get("$492"))
                                .and_then(Value::as_str)
                                != Some("author")
                        });
                        for a in &src.authors {
                            list.push(kv("author", Value::String(a.clone())));
                        }
                    }
                    if let Some(d) = &src.issue_date
                        && !has_issue_date
                    {
                        list.push(kv("issue_date", Value::String(d.clone())));
                    }
                }
                if language_ja_zh {
                    for m in list.iter_mut() {
                        let Some(ms) = m.as_struct_mut() else {
                            continue;
                        };
                        if ms.get("$492").and_then(Value::as_str) == Some("language")
                            && let Some(Value::String(l)) = ms.get("$307").cloned()
                        {
                            ms.insert("$307", Value::String(l[3..].replace('=', "-")));
                        }
                    }
                }
                if !has_override_font {
                    list.push(kv("override_kindle_font", Value::Bool(false)));
                }
                if !has_cover && fixed_layout {
                    // kfxlib derives a cover from the first section of fixed-layout books;
                    // Deckle targets reflowable books, so only note it.
                    book.warnings
                        .push("fixed-layout book without cover_image metadata".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn fix_fonts(book: &mut Book) {
    let font_type = Sym::new("$262");
    let fonts: Vec<Fragment> = book.fragments.all(&font_type).cloned().collect();
    for f in fonts {
        if f.fid.is_some() {
            book.fragments.remove(&font_type, f.fid.as_ref());
            book.fragments.push(Fragment::root("$262", f.value.clone()));
        }
        let Some(location) = f
            .value
            .as_struct()
            .and_then(|s| s.get("$165"))
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let loc_sym = Sym::new(&location);
        if let Some(data) = book.fragments.remove(&Sym::new("$417"), Some(&loc_sym)) {
            let fid = book.symtab.create_local(&location);
            book.fragments.push(Fragment {
                ftype: Sym::new("$418"),
                fid: Some(fid),
                value: data.value,
            });
        }
    }
}

fn fix_resources(book: &mut Book) {
    let res_type = Sym::new("$164");
    let snapshot: Vec<Fragment> = book.fragments.all(&res_type).cloned().collect();
    for f in book.fragments.all_mut(&res_type) {
        let Some(fv) = f.value.as_struct_mut() else {
            continue;
        };
        let is_pobject = fv.get("$161").and_then(Value::as_sym).map(|s| s.as_str()) == Some("$287");
        if is_pobject
            && !fv.contains("$422")
            && !fv.contains("$423")
            && let Some(referred) = fv.get("$167").cloned()
        {
            let referred: Vec<Sym> = referred
                .as_list()
                .map(|l| l.iter().filter_map(|v| v.as_sym().cloned()).collect())
                .unwrap_or_default();
            if let Some(src) = snapshot.iter().find(|r| {
                r.fid.as_ref().is_some_and(|id| referred.contains(id))
                    && r.value
                        .as_struct()
                        .is_some_and(|s| s.contains("$422") && s.contains("$423"))
            }) {
                let s = src.value.as_struct().unwrap();
                fv.insert("$422", s.get("$422").unwrap().clone());
                fv.insert("$423", s.get("$423").unwrap().clone());
            }
        }
        if fv.get("$162").and_then(Value::as_str) == Some("") {
            fv.remove("$162");
        }
    }
}

fn add_feature(features: &mut Vec<Value>, namespace: &str, name: &str, version: (i64, i64)) {
    let exists = features.iter().any(|f| {
        f.as_struct().is_some_and(|s| {
            s.get("$586").and_then(Value::as_str) == Some(namespace)
                && s.get("$492").and_then(Value::as_str) == Some(name)
        })
    });
    if exists {
        return;
    }
    let mut ver = Struct::new();
    ver.insert("$587", Value::Int(version.0));
    ver.insert("$588", Value::Int(version.1));
    let mut vwrap = Struct::new();
    vwrap.insert("version", Value::Struct(ver));
    let mut s = Struct::new();
    s.insert("$586", Value::string(namespace));
    s.insert("$492", Value::string(name));
    s.insert("$589", Value::Struct(vwrap));
    features.push(Value::Struct(s));
}

fn fix_features(book: &mut Book) {
    let illustrated = book.is_illustrated_layout();
    let fixed_layout = book.is_fixed_layout();
    let has_pdf = book.has_pdf_resource();
    let file_creator = book
        .metadata_value("file_creator", "kindle_audit_metadata")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let creator_version = book
        .metadata_value("creator_version", "kindle_audit_metadata")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let has_fixed_md = book
        .metadata_value("yj_fixed_layout", "kindle_capability_metadata")
        .is_some();
    let has_graphical = book
        .metadata_value("graphical_highlights", "kindle_capability_metadata")
        .is_some();
    let has_textbook = book
        .metadata_value("yj_textbook", "kindle_capability_metadata")
        .is_some();

    let mut canonical = if illustrated { (2, 0) } else { (1, 0) };
    if file_creator == "KC" || (file_creator == "KTC" && creator_version.as_str() >= "1.11") {
        canonical = (2, 0);
    }

    // image scans
    let mut has_hdv = false;
    let mut has_tiles = false;
    let mut rst = false;
    let res_type = Sym::new("$164");
    for f in book.fragments.all(&res_type) {
        let Some(fv) = f.value.as_struct() else {
            continue;
        };
        let w = fv.get("$422").and_then(Value::as_int).unwrap_or(0);
        let h = fv.get("$423").and_then(Value::as_int).unwrap_or(0);
        if w > 1920 || h > 1920 || fv.contains("$636") {
            has_hdv = true;
        }
        if fv.contains("$797") {
            has_tiles = true;
        }
        if !rst
            && fv.get("$161").and_then(Value::as_sym).map(|s| s.as_str()) == Some("$285")
            && let Some(loc) = fv.get("$165").and_then(Value::as_str)
            && let Some(raw) = book.fragments.get(&Sym::new("$417"), Some(&Sym::new(loc)))
            && let Some(bytes) = raw.value.as_blob()
        {
            rst = bytes
                .windows(2)
                .any(|w| w[0] == 0xFF && (0xD0..=0xD7).contains(&w[1]));
        }
    }

    let cf_type = Sym::new("$585");
    if book.fragments.get_single(&cf_type).is_none() {
        let mut s = Struct::new();
        s.insert("$590", Value::List(Vec::new()));
        book.fragments
            .push(Fragment::root("$585", Value::Struct(s)));
    }
    let cf = book.fragments.get_single_mut(&cf_type).unwrap();
    let s = cf.value.as_struct_mut().unwrap();
    s.remove("$155");
    s.remove("$598");
    if !s.contains("$590") {
        s.insert("$590", Value::List(Vec::new()));
    }
    let features = s.get_mut("$590").unwrap().as_list_mut().unwrap();
    add_feature(features, "SDK.Marker", "CanonicalFormat", canonical);
    if fixed_layout {
        if has_pdf {
            add_feature(
                features,
                "com.amazon.yjconversion",
                "yj_pdf_support",
                (1, 0),
            );
            if has_fixed_md {
                add_feature(
                    features,
                    "com.amazon.yjconversion",
                    "yj_fixed_layout",
                    (1, 0),
                );
            }
        } else if has_fixed_md {
            add_feature(
                features,
                "com.amazon.yjconversion",
                "yj_non_pdf_fixed_layout",
                (2, 0),
            );
        }
    } else if has_tiles {
        add_feature(features, "com.amazon.yjconversion", "yj_hdv", (2, 0));
    } else if has_hdv {
        add_feature(features, "com.amazon.yjconversion", "yj_hdv", (1, 0));
    }
    if rst {
        add_feature(
            features,
            "com.amazon.yjconversion",
            "yj_jpg_rst_marker_present",
            (1, 0),
        );
    }
    if has_graphical {
        add_feature(
            features,
            "com.amazon.yjconversion",
            "yj_graphical_highlights",
            (1, 0),
        );
    }
    if has_textbook {
        add_feature(features, "com.amazon.yjconversion", "yj_textbook", (1, 0));
    }
}
