// SPDX-License-Identifier: GPL-3.0-or-later
//! Builders for synthetic KRDS files.
#![allow(dead_code)]

pub fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/yjf")
            .join(name),
    )
    .unwrap()
}

pub const SIGNATURE: [u8; 8] = [0, 0, 0, 0, 0, 0x1a, 0xb1, 0x26];

pub fn int(v: i32) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(v.to_be_bytes());
    b
}

pub fn long(v: i64) -> Vec<u8> {
    let mut b = vec![2];
    b.extend(v.to_be_bytes());
    b
}

pub fn dbl(v: f64) -> Vec<u8> {
    let mut b = vec![4];
    b.extend(v.to_be_bytes());
    b
}

pub fn utf(s: &str) -> Vec<u8> {
    let mut b = vec![3, 0];
    b.extend((s.len() as u16).to_be_bytes());
    b.extend(s.as_bytes());
    b
}

/// A named object: begin (-2), its name as utf without the type byte, values, end (-1).
pub fn obj(name: &str, values: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0xfe, 0];
    b.extend((name.len() as u16).to_be_bytes());
    b.extend(name.as_bytes());
    for v in values {
        b.extend(v);
    }
    b.push(0xff);
    b
}

/// A whole file: signature, a version int, the number of objects, then the objects.
pub fn synthetic(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut b = SIGNATURE.to_vec();
    b.extend(long(1)); // the header value every real file carries
    b.extend(int(objects.len() as i32));
    for o in objects {
        b.extend(o);
    }
    b
}

/// A minimal `.yjf` whose furthest and last positions are both `pos`.
pub fn yjf(pos: i64) -> Vec<u8> {
    let p = format!("AQoEAAAAAAAA:{pos}");
    synthetic(&[
        obj("fpr", &[utf(&p), long(-1), int(-1), utf(""), utf("")]),
        obj("lpr", &[int(2), utf(&p), long(1_792_620_390_980)]),
    ])
}

pub fn d(s: &str) -> chrono::NaiveDate {
    s.parse().unwrap()
}

/// A layout for `date` shipped as `deckle-daily-<date>`, sections "front", "s1", "s2"… at
/// `starts`, ending at 10 000.
pub fn layout(date: &str, starts: &[i64]) -> deckle_readership::layout::Layout {
    use deckle_readership::layout::{Layout, SectionStart};
    Layout {
        date: d(date),
        title: "The Deckle Daily".into(),
        file: format!("deckle-daily-{date}"),
        end: 10_000,
        epub: None,
        sections: starts
            .iter()
            .enumerate()
            .map(|(i, &start)| SectionStart {
                key: if i == 0 {
                    "front".into()
                } else {
                    format!("s{i}")
                },
                name: format!("Section {i}"),
                start,
                items: None,
                stories: Vec::new(),
            })
            .collect(),
    }
}

pub const NOW: &str = "2026-10-22T07:02:11Z";
