// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle-wordwise-build <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <out.sqlite> [version]`
//! `deckle-wordwise-build puzzle <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <blocklist.txt> <out.tsv>`
//! `deckle-wordwise-build crossword <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <blocklist.txt> <out.tsv>`

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("puzzle") {
        return puzzle(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("crossword") {
        return crossword(&args[1..]);
    }
    let [jsonl, wordfreq, out, rest @ ..] = args.as_slice() else {
        eprintln!(
            "usage: deckle-wordwise-build <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <out.sqlite> [version]\n       deckle-wordwise-build puzzle <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <blocklist.txt> <out.tsv>\n       deckle-wordwise-build crossword <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <blocklist.txt> <out.tsv>"
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
    let blocked = read_blocklist(blocklist)?;
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

/// The crossword list as `word\tzipf\tclue` lines (puzzle books §3.3; gzip it with
/// `gzip -9 -n`).
fn crossword(args: &[String]) -> Result<()> {
    let [jsonl, wordfreq, blocklist, out] = args else {
        eprintln!(
            "usage: deckle-wordwise-build crossword <kaikki.jsonl[.gz]> <large_en.msgpack.gz> <blocklist.txt> <out.tsv>"
        );
        std::process::exit(2);
    };
    let freq = load_freq(wordfreq)?;
    let blocked = read_blocklist(blocklist)?;
    let words = deckle_wordwise_build::crossword_words(open_jsonl(jsonl)?, &freq, &blocked);
    let mut f =
        std::io::BufWriter::new(File::create(out).with_context(|| format!("create {out}"))?);
    let mut lens = [0usize; 16];
    for (w, z, c) in &words {
        writeln!(f, "{w}\t{z:.2}\t{c}")?;
        lens[w.len()] += 1;
    }
    f.flush()?;
    eprintln!("{} words → {out}", words.len());
    for (n, k) in lens.iter().enumerate().filter(|(_, k)| **k > 0) {
        eprintln!("  {n:>2} letters: {k}");
    }
    Ok(())
}

fn read_blocklist(blocklist: &str) -> Result<HashSet<String>> {
    Ok(std::fs::read_to_string(blocklist)
        .with_context(|| format!("open {blocklist}"))?
        .lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect())
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
