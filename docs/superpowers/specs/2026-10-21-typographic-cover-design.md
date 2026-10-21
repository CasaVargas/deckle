# Typographic cover generator — design

**Status:** approved by owner (2026-10-21) for [#27](https://github.com/prjoni99/deckle/issues/27) ("Automatic typographic cover generator") ·
**Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-18-deckle-design.md` (§1 goal 1, §5 the cover rule) ·
`2026-10-21-docx-import-design.md` (DOCX has no cover)

When a book has no cover, Deckle makes a clean one from the title and author, so it never shows
up as a grey tile. DOCX and TXT imports, older EPUBs and web books all get a real cover.

## 1. Decision

- **On by default, and only when the book has no cover.** Design goal 1 says every book shows a
  cover, always, and today a coverless book breaks that and gets a warning ("thumbnail not
  written: the book has no cover"). An existing cover is never replaced or restyled. The
  opt-out is `deckle run --no-generated-cover` and the desktop setting *Make a cover when a
  book has none* (on).
  - This bends normalize's "passthrough" promise the same way JPEG cover conversion already
    does: it fixes something the Kindle needs. Typography stays opt-in because it rewrites
    the text; a cover adds one image and touches no text.
- **Three templates, chosen per book by a stable hash of its identifier.** A shelf of imported
  documents shouldn't all look the same, and the same book always gets the same cover.
  `--cover-style classic|band|rule` chooses one explicitly.
- **One size: 1264 × 1680, grayscale JPEG.** That's the 3:4 ratio Kindles display, and the
  same size the Daily cover already uses. Previewer and the Kindle scale it for each device.
  Per-device sizing was considered and rejected: the KFX is built once for each device class,
  not each model, and the thumbnail path already resizes.
- **A new crate, `crates/deckle-cover`,** owns the renderer and the fonts. It's pure: it takes
  metadata in and gives JPEG bytes back, with no I/O. It uses `ab_glyph` and `image`, which are
  already dependencies. The fonts are Old Standard (OFL, already bundled in `deckle-digest`)
  for the serif and Inter (OFL, new) for the sans. `deckle-digest` keeps its own copy for now.
  Moving it onto `deckle-cover` is a follow-up cleanup, not part of this issue.
- Rejected: colour or illustrated covers (most target devices are e-ink, and an illustration
  needs assets or an LLM), and fetching a cover online (that's the Match stage's job for EBOK,
  and online lookups stay opt-in).

## 2. Templates

All three: white background, black type, 64 px margins, centred, no images. Text is fitted by
shrinking the font size until it fits; it is never cut off.

| Name | Layout |
|---|---|
| `classic` | Title in Old Standard, upper-middle, up to 4 lines, 96–160 px. A thin rule. Author in small caps below, 48–64 px. Series ("Hainish Cycle · 5") in italics near the foot. |
| `band` | A black band across the upper third with the title reversed out in white Inter Bold. The author underneath in Inter Regular. The series in the band's lower edge. |
| `rule` | Title in Inter Bold, ranged left with a heavy left rule. The author at the foot, ranged left. The series above the author. |

- **Text:** the title from the OPF `dc:title`, with any subtitle after `:` set smaller on its
  own line. Authors from `dc:creator`: up to two names, joined by "and", otherwise "First
  Author et al.". The series from `inspect`'s `series`.
- **Long titles:** below a template's size range (60 % of its largest size) a title takes
  more lines, within the template's height, rather than shrink out of legibility on a tile.
- **Scripts:** Old Standard covers Latin and Cyrillic; Inter covers Latin, Greek and Cyrillic.
  (As built: the bundled Old Standard has no Greek, so when the hashed template can't set a
  title, `Template::choose` moves on to the next template that can. An explicit
  `--cover-style` is kept and the title left off.) Any other script (CJK,
  Arabic, Hebrew, Devanagari…) isn't drawn as missing-glyph boxes. If any of the title's
  letters fall outside the bundled fonts, the title is left off and the cover shows the
  remaining lines (author, series) that do render, plus a warning ("cover title not renderable
  in the bundled fonts"). System fonts are never used, so covers stay identical on every
  machine. Bundling a CJK font (about 10 MB or more) is out of scope.

## 3. Where it sits

```
inspect (no cover) ──▶ deckle_cover::render(&CoverText, style) ──▶ NormalizeOptions.generated_cover
                                                                 └▶ normalize adds cover.jpg
```

- `pipeline::run` renders the cover only when `inspect` reports no `cover_href` and the option
  is on.
- `NormalizeOptions` gets `generated_cover: Option<GeneratedCover>` (the JPEG and the
  template's name). Normalize adds `deckle-cover.jpg` to the manifest with
  `properties="cover-image"`, adds `<meta name="cover">` for EPUB 2 readers, and adds a
  `deckle-cover.xhtml` first in the spine. The `deckle-` prefix follows the synthesized
  `deckle-nav.xhtml` and can't collide with a book's own files.
- `RunOptions.generated_cover: Option<Option<Template>>`: `None` off, `Some(None)` by hash,
  `Some(Some(t))` template `t` (the `pages` idiom).
- `NormalizeReport.cover_generated: Option<String>` records the template that was used. The CLI
  shows `Cover: generated (classic)`.
- Everything downstream (the cover rule, the thumbnail, the library's `source_cover`) sees an
  ordinary cover. The library keeps the source unchanged, so the generated cover is made again
  on each send. That's deterministic, so it comes out the same each time.
- The Daily and web books already have their own covers, so they're unaffected.

## 4. Acceptance

- `deckle-cover` unit tests:
  - each template renders a 1264×1680 grayscale JPEG for short, long (more than 120
    characters), subtitled, multi-author and series titles;
  - rendering is byte-identical for the same input;
  - the template chosen by hash is stable;
  - a CJK title gives a cover without the title, and a warning.
- Golden images: one PNG per template in `crates/deckle-cover/tests/golden/`, compared with a
  per-pixel tolerance so a font rasteriser update doesn't fail the test for invisible
  differences.
- Pipeline:
  - the DOCX fixture with the option on gets a cover, and the thumbnail warning is gone;
  - with `--no-generated-cover`, the output is byte-identical to today's;
  - an EPUB that has a cover comes out byte-identical with the option on or off.
- By hand: an imported DOCX on a Scribe (MTP, PDOC) and a Paperwhite (mass storage) shows the
  generated cover in the library view.
