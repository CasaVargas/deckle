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

## `fb2/`

Made by `python3 -I tools/ref/make_fb2_fixtures.py fixtures/fb2` (standard library only).

| File | What |
|---|---|
| `mini.fb2` | UTF-8 FictionBook: two authors (one by nickname), series #2, two genres, an annotation, a PNG cover; a body title and epigraph; two parts of two chapters; a poem, a table, a cite, sub/sup/strikethrough/code; a block image whose `content-type` lies, an inline GIF, an internal link, an external link, two notes (one link without `type="note"`); an unused binary |
| `mini.fb2.zip` | `mini.fb2`, zipped |
| `cp1251.fb2` | A short Russian book declared and encoded `windows-1251` |
| `broken.fb2` | `mini.fb2` cut off mid-section |

## `odt/`

Made by `python3 -I tools/ref/make_odt_fixtures.py fixtures/odt` (standard library only).

| File | What |
|---|---|
| `mini.odt` | `meta.xml` (title, creator and initial creator, language, description, keywords, date); Title and Subtitle paragraphs; Heading 1–4 with two level-1 chapters; formatting from automatic styles and an inherited common text style; `text:s`, tab, line break; a numbered list starting at 3 with a nested bullet list; a quotation; a header-row table with a spanned cell; an inline GIF and a paragraph-anchored PNG with alt text; a bookmark and a link to it, an external link, a page-number field; two footnotes and an endnote; a stored table of contents, a comment and a tracked deletion that must not appear |
| `mini.fodt` | The same document as flat ODF, pictures as base64 |
| `sheet.ods` | A spreadsheet mimetype |
| `encrypted.odt` | A manifest with `encryption-data` |

## `rtf/`

Made by `python3 -I tools/ref/make_rtf_fixtures.py fixtures/rtf` (standard library only; the 8×8
JPEG inside was made once with macOS `sips` and is embedded in the script as hex).

| File | What |
|---|---|
| `mini.rtf` | `\info` (title, author, subject, keywords, revision time); a stylesheet with Title, Quote and heading 1–4; an `\outlinelevel0` heading; every character format, alignment, `\line`, `\tab`; `\'e9`, `\u8364?`, curly quotes and dashes; a numbered list starting at 3 with a nested bullet list via `\listtable`; a header-row table with a horizontal merge; a hex PNG in `\shppict` with alt text and a `\nonshppict` WMF copy, and a JPEG as `\bin`; a bookmark, a link to it, a web link and a PAGE field; two footnotes and an endnote; a header, a hidden run, a deleted revision, a comment and a stored TOC field that must not appear |
| `cyrillic.rtf` | `\ansicpg1251`, `\fcharset204`, `\deflang1049`, Cyrillic as `\'hh`, author in `\info` |
| `japanese.rtf` | A `\fcharset128` font with Shift-JIS `\'hh` pairs, `\deflang1041` |
| `unclosed.rtf` | Missing its final brace |

## Not in the repo

`pride-and-prejudice.epub` (Gutenberg #1342, epub3 images) converts to a 47 MB KPF and a 25 MB
KFX; use it locally for size and perf checks:
`curl -L -o pp.epub https://www.gutenberg.org/ebooks/1342.epub3.images`.
