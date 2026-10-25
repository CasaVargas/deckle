// SPDX-License-Identifier: GPL-3.0-or-later
//! The committed MOBI fixtures, built from the crate's fixture module so they stay in step.

use deckle_mobi::fixture::{Compression, Kf8, Mobi6, build_kf8, build_mobi6};

pub fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    fn adler(data: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for &x in data {
            a = (a + u32::from(x)) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }
    let chunk = |kind: &[u8], data: &[u8]| {
        let mut c = kind.to_vec();
        c.extend(data);
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend(&c);
        out.extend(crc(&c).to_be_bytes());
        out
    };
    // Each row: filter byte 0, then the pixels.
    let row: Vec<u8> = std::iter::once(0).chain(rgb.repeat(w as usize)).collect();
    let raw = row.repeat(h as usize);
    // A stored (uncompressed) zlib stream.
    let mut z = vec![0x78, 0x01, 0x01];
    z.extend((raw.len() as u16).to_le_bytes());
    z.extend((!(raw.len() as u16)).to_le_bytes());
    z.extend(&raw);
    z.extend(adler(&raw).to_be_bytes());
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    out.extend(chunk(b"IHDR", &ihdr));
    out.extend(chunk(b"IDAT", &z));
    out.extend(chunk(b"IEND", &[]));
    out
}

pub const MINI6_HTML: &str = "<html><head><guide><reference type=\"toc\" title=\"Contents\" {{filepos:<h2>Chapter One}} /></guide></head><body>\
<h1 align=center>The Deckle MOBI Mini</h1>\
<p>Go to <a {{filepos:<h2>Chapter Two}}>the second chapter</a>.</p>\
<mbp:pagebreak/>\
<h2>Chapter One</h2>\
<p>Unquoted <font size=+1><b>bold</b></font> and <i>italic</i> text<br>with a break.\
<p align=right>Right aligned\
<center>Centred</center>\
<p><img recindex=\"00001\" alt=\"A grey plate\"></p>\
<mbp:pagebreak/>\
<h2>Chapter Two</h2>\
<p>Back to <a {{filepos:<h2>Chapter One}}>chapter one</a> and <a href=\"https://example.com/press\">the press</a>.</p>\
<script>alert('no')</script><table><tr><td colspan=2>Wide cell</td></tr></table>\
</body></html>";

pub fn mini6(compression: Compression) -> Vec<u8> {
    let plate = png(8, 8, [128, 128, 128]);
    let cover = png(60, 90, [40, 60, 90]);
    build_mobi6(&Mobi6 {
        title: "The Deckle MOBI Mini",
        authors: &["Ada Testwright"],
        html: MINI6_HTML,
        images: &[&plate, &cover],
        toc: &[
            ("Chapter One", 0, "<h2>Chapter One"),
            ("Chapter Two", 0, "<h2>Chapter Two"),
        ],
        cover: Some(2),
        compression,
        ..Default::default()
    })
}

pub fn mini_azw3() -> Vec<u8> {
    let doc = |title: &str, body: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title><link rel=\"stylesheet\" type=\"text/css\" href=\"kindle:flow:0001?mime=text/css\"/></head><body aid=\"0\">{body}</body></html>"
        )
    };
    let cover = png(60, 90, [90, 60, 40]);
    let p0 = doc(
        "Cover",
        "<p><img src=\"kindle:embed:0001?mime=image/png\" alt=\"\"/></p>",
    );
    let p1 = doc(
        "One",
        "<h2 id=\"c1\">Chapter One</h2><p>It was a <a href=\"{{pos:2:<h2}}\">dark</a> night.</p>",
    );
    let p2 = doc(
        "Two",
        "<h2 id=\"c2\">Chapter Two</h2><p>Then the storm came over the hill.</p>",
    );
    let parts = [p0.as_str(), p1.as_str(), p2.as_str()];
    build_kf8(&Kf8 {
        title: "The Deckle AZW3 Mini",
        authors: &["Ada Testwright"],
        parts: &parts,
        css: "p { margin: 0 }",
        images: &[&cover],
        toc: &[("Chapter One", 0, 1, "<h2"), ("Chapter Two", 0, 2, "<h2")],
        cover: Some(1),
        ..Default::default()
    })
}
