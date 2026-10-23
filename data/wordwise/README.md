# Word Wise database builder

Offline, dev-only. Builds `deckle-wordwise-en-v1.sqlite` (rules in `docs/formats/wordwise.md`).

```sh
curl -L -o kaikki-en.jsonl.gz https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl.gz
curl -L -o large_en.msgpack.gz https://raw.githubusercontent.com/rspeer/wordfreq/master/wordfreq/data/large_en.msgpack.gz
cargo run --release -p deckle-wordwise-build -- kaikki-en.jsonl.gz large_en.msgpack.gz deckle-wordwise-en-v1.sqlite 2026-10-19
gzip -9 -n -k deckle-wordwise-en-v1.sqlite      # -n: no name/time, so the hash is reproducible
shasum -a 256 deckle-wordwise-en-v1.sqlite.gz   # → deckle_core::wordwise::DB_SHA256
```

Publish the `.gz` as an asset of the `wordwise-v1` release. A new build needs a new release tag
and file name (`…-v2`) or the pinned hash will refuse it. Never let it become the Latest release:
installed apps read their updates from Latest, so publish with
`gh release create wordwise-v2 deckle-wordwise-en-v2.sqlite.gz --latest=false …` (or untick
**Set as the latest release**). The `latest-guard` workflow repairs it if you forget.

## The puzzle page's word list

`crates/deckle-digest/assets/puzzle/words.tsv.gz`: `word<TAB>clue` lines, embedded in the app
(puzzle design §4). Built from the same two downloads:

```sh
cargo run --release -p deckle-wordwise-build -- puzzle kaikki-en.jsonl.gz large_en.msgpack.gz \
    data/wordwise/puzzle-blocklist.txt words.tsv
gzip -9 -n -c words.tsv > crates/deckle-digest/assets/puzzle/words.tsv.gz
```

Then skim 50 random lines (`shuf -n 50 words.tsv`), add anything objectionable or unguessable
to `puzzle-blocklist.txt`, and rebuild.

| Built | Sources downloaded | Words (3 / 4 / 5 letters) | Gzipped |
|---|---|---|---|
| 2026-10-22 | 2026-10-22 | 3,090 (412 / 1,168 / 1,510) | 61 KB |

Data licenses: Wiktionary content via kaikki.org is CC BY-SA 4.0 (`tests/sample.jsonl` is 22
trimmed entries of it); wordfreq's word lists are CC BY-SA 4.0.
