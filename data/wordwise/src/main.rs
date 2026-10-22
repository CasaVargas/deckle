// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle-wordwise-build <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <out.sqlite> [version]`
//! `deckle-wordwise-build puzzle <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <blocklist.txt> <out.tsv>`

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("puzzle") {
        return puzzle(&args[1..]);
    }
    let [jsonl, wordfreq, out, rest @ ..] = args.as_slice() else {
        eprintln!(
            "usage: deckle-wordwise-build <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <out.sqlite> [version]\n       deckle-wordwise-build puzzle <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <blocklist.txt> <out.tsv>"
        );
        std::process::exit(2);
    };
    let version = rest.first().cloned().unwrap_or_else(|| "dev".into());
    let freq = load_freq(wordfreq)?;
    let stats = deckle_wordwise_build::build(open_jsonl(jsonl)?, &freq, out.as_ref(), &version)?;
    eprintln!(
        "{} entries read, {} senses, {} forms → {out}",
        stats.entries, stats.senses, stats.forms
    );
    Ok(())
}

/// The puzzle page's word list as `word\tclue` lines (gzip it with `gzip -9 -n`).
fn puzzle(args: &[String]) -> Result<()> {
    let [jsonl, wordfreq, blocklist, out] = args else {
        eprintln!(
            "usage: deckle-wordwise-build puzzle <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <blocklist.txt> <out.tsv>"
        );
        std::process::exit(2);
    };
    let freq = load_freq(wordfreq)?;
    let blocked: HashSet<String> = std::fs::read_to_string(blocklist)
        .with_context(|| format!("open {blocklist}"))?
        .lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let words = deckle_wordwise_build::puzzle_words(open_jsonl(jsonl)?, &freq, &blocked);
    let mut f =
        std::io::BufWriter::new(File::create(out).with_context(|| format!("create {out}"))?);
    for (w, c) in &words {
        writeln!(f, "{w}\t{c}")?;
    }
    f.flush()?;
    eprintln!("{} words → {out}", words.len());
    Ok(())
}

fn load_freq(wordfreq: &str) -> Result<HashMap<String, f64>> {
    let freq = deckle_wordwise_build::load_wordfreq(
        File::open(wordfreq).with_context(|| format!("open {wordfreq}"))?,
    )?;
    eprintln!("wordfreq: {} words", freq.len());
    Ok(freq)
}

fn open_jsonl(jsonl: &str) -> Result<Box<dyn BufRead>> {
    let file = File::open(jsonl).with_context(|| format!("open {jsonl}"))?;
    Ok(if jsonl.ends_with(".gz") {
        Box::new(BufReader::with_capacity(
            1 << 20,
            flate2::read::GzDecoder::new(file),
        ))
    } else {
        Box::new(BufReader::with_capacity(1 << 20, file))
    })
}
