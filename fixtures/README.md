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

## `docx/`

| File | What | How it was made |
|---|---|---|
| `deckle-mini.docx` | Word document: Title, Heading 1–4, bold/italic, nested bullet list, numbered list, header-row table, two PNGs in `word/media`, three footnotes, core properties | `python3 -I tools/ref/make_mini_docx.py fixtures/docx/deckle-mini.docx` |

## `notes/`

Pop-up notes fixtures (#31), all made by
`python3 -I tools/ref/make_notes_epubs.py fixtures/notes` (deterministic zips).

| File | What |
|---|---|
| `semantic.epub` | EPUB 3 with `epub:type`/ARIA-marked noterefs and notes that aren't asides yet (a `div`, a `p`, an `li` in an `ol start="2"`), plus one pair already in the pop-up form |
| `calibre.epub` | EPUB 2, no epub namespace: `<a class="footnote-link" href="#fn1"><sup>1</sup></a>` with `↩` backlinks; a note cited twice; a ref whose id is on its wrapping span |
| `indesign.epub` | Refs in two chapters to an InDesign-style endnotes chapter whose anchors link back with the same digit; a heading other links point at; one one-way link to an appendix paragraph |
| `pandoc.epub` | `footnote-ref` links to an `ol` under `section.footnotes` with `↩︎` backlinks |
| `trap.epub` | Exponents, a numbered list and digit links that aren't notes: must come out byte-identical |

## `txt/`

Made by `python3 -I tools/ref/make_txt_fixtures.py fixtures/txt` (standard library only).

| File | What |
|---|---|
| `gutenberg-mini.txt` | Project Gutenberg markers and licence, `Title:`/`Author:`/`Language:` header, two parts, four chapters (one with an all-caps subtitle line before it), a scene break, hard-wrapped lines, a `well-`/`known` line-end hyphen, a short-line block; CRLF line endings |
| `unwrapped.txt` | One paragraph per line, no blank lines, Windows-1252 with curly quotes |
| `verse.txt` | Two stanzas of short lines |
| `utf16.txt` | UTF-16 LE with a BOM, accented letters |
| `noheadings.txt` | About 500 KB of hard-wrapped paragraphs with no headings |

## Not in the repo

`pride-and-prejudice.epub` (Gutenberg #1342, epub3 images) converts to a 47 MB KPF and a 25 MB
KFX; use it locally for size and perf checks:
`curl -L -o pp.epub https://www.gutenberg.org/ebooks/1342.epub3.images`.
