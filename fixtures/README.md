# Fixtures

Small, repo-sized inputs and golden outputs for the pipeline tests. Everything here was
generated on 2026-10-18 with the commands below, and can be regenerated.

## `mini/`

| File | What | How it was made |
|---|---|---|
| `deckle-mini.epub` | 3-chapter EPUB 3, PNG cover, nav TOC, series metadata, author file-as | `python3 -I tools/ref/make_mini_epub.py fixtures/mini/deckle-mini.epub` |
| `deckle-mini.kpf` | Kindle Previewer 4.0.3 output for the EPUB (Enhanced Typesetting: Supported) | `KindlePreviewer4CLI deckle-mini.epub --convert --locale en --output out` (output dir must exist) |
| `Summary_Log.csv`, `deckle-mini_epub_log.csv` | Previewer's logs for that run | same run |
| `deckle-mini-pdoc.golden.kfx` | Monolithic KFX from the KPF, PDOC, ASIN `B0DECK000002`, kfxlib approximate page list (45 pages) | `tools/ref/kpf2kfx.py <KFX-Input dir> deckle-mini.kpf out.kfx PDOC B0DECK000002` |
| `deckle-mini-pdoc-nopages.golden.kfx` | Same, without the approximate page list | `tools/ref/kpf2kfx_opts.py <KFX-Input dir> deckle-mini.kpf out.kfx PDOC B0DECK000002 none` |
| `deckle-mini-ebok.golden.kfx` | Same as the pdoc golden but EBOK | `tools/ref/kpf2kfx_opts.py <KFX-Input dir> deckle-mini.kpf out.kfx EBOK B0DECK000002 0` |

The goldens were produced by **kfxlib 20260827** (GPL-3, from the KFX Input plugin mirror
`github.com/frutadoconde/KFX-Input` @ `57fd6cb3`), the reference implementation the Rust port
in `crates/deckle-kfx` is tested against. kfxlib randomizes two values per run, so tests read
them from the golden instead of hard-coding:

- container id / `asset_id`: `$270.$409` (e.g. `CR!O5YUJ1MC7JEKRQFQYI9ZGZTPVE42` in the pdoc golden);
- `$587` creator string: `kfxlib-20260827`.

Tests inject those values so container output can be compared byte-for-byte.

## Not in the repo

`pride-and-prejudice.epub` (Gutenberg #1342, epub3 images) converts to a 47 MB KPF and a 25 MB
KFX; use it locally for size and perf checks:
`curl -L -o pp.epub https://www.gutenberg.org/ebooks/1342.epub3.images`.
