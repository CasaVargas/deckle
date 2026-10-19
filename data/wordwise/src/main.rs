// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle-wordwise-build <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <out.sqlite> [version]`

use std::fs::File;
use std::io::{BufRead, BufReader};

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [jsonl, wordfreq, out, rest @ ..] = args.as_slice() else {
        eprintln!(
            "usage: deckle-wordwise-build <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <out.sqlite> [version]"
        );
        std::process::exit(2);
    };
    let version = rest.first().cloned().unwrap_or_else(|| "dev".into());
    let freq = deckle_wordwise_build::load_wordfreq(
        File::open(wordfreq).with_context(|| format!("open {wordfreq}"))?,
    )?;
    eprintln!("wordfreq: {} words", freq.len());
    let file = File::open(jsonl).with_context(|| format!("open {jsonl}"))?;
    let reader: Box<dyn BufRead> = if jsonl.ends_with(".gz") {
        Box::new(BufReader::with_capacity(
            1 << 20,
            flate2::read::GzDecoder::new(file),
        ))
    } else {
        Box::new(BufReader::with_capacity(1 << 20, file))
    };
    let stats = deckle_wordwise_build::build(reader, &freq, out.as_ref(), &version)?;
    eprintln!(
        "{} entries read, {} senses, {} forms → {out}",
        stats.entries, stats.senses, stats.forms
    );
    Ok(())
}
