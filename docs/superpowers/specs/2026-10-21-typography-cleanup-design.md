# Typography cleanup — design

**Status:** first increment of #30 ("Typography cleanup pass"), built in #30's PR ·
**Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-18-deckle-design.md` (stage 2, normalize)

Turn typewriter text into book text: straight quotes become curly ones, double hyphens become
dashes, three dots become an ellipsis, and stray empty paragraphs go. It's opt-in, and it runs
inside normalize, so every source gets it: EPUB, Word documents, web articles.

## 1. Decision

- **EPUB text pass, opt-in, off by default.** `deckle run --typography`; desktop setting
  *Tidy typography* (off). Normalize's promise ("everything else passes through byte for byte")
  holds unless you turn it on.
- **Paragraph-join repair is out of this increment.** It only matters for PDF and DOCX
  conversions with broken line structure. There is no PDF importer yet (#18), and DOCX structure
  recovery is #25. It moves to those issues.
- Rejected: on by default. That breaks the passthrough promise and risks mangling non-English
  quotes, poetry and code. It can be revisited once the pass has been used on real books.

## 2. What changes

Applied to the text inside `<body>` of every XHTML document in the manifest, and to `<text>`
labels in the NCX, so the contents list matches the headings.

| Input | Output | Rule |
|---|---|---|
| `"` / `&quot;` | `“` or `”` | open after start, whitespace, an opening bracket, dash or quote; close otherwise |
| `'` / `&apos;` / `&#39;` | `‘`, `’` | apostrophe between letters (`don’t`), before a digit (`’90s`), and in `’tis`, `’twas`, `’til`, `’em`, `’cause`, `’round`, `’n’`; otherwise open/close like `"` |
| `---`, `--` | `—` | em dash |
| word` - `word | word` – `word | spaced hyphen becomes a spaced en dash |
| `...`, `. . .` | `…` | ellipsis |
| `<p></p>`, `<p/>`, `<p> </p>` | removed | truly empty paragraphs (nothing visible) |
| two or more `<p>&nbsp;</p>` in a row | one | runs collapse to a single blank line, so a scene break survives |

**Quote context crosses inline tags:** `<i>"Hello,</i>" she said` closes after `</i>`. It
resets at block boundaries (`p`, `div`, `h1`–`h6`, `li`, `blockquote`, `td`, `th`, `dd`, `dt`,
`br`, `figcaption`, `section`, `aside`).

**Never touched:**
- `pre`, `code`, `kbd`, `samp`, `tt`, `var`, `script`, `style`, `math`, `svg`;
- attributes, comments, CDATA, processing instructions;
- the OPF (titles stay as the publisher wrote them, so Amazon matching is unaffected);
- CSS;
- any entity other than the quote entities.

**Kept:** a paragraph with an `id` (a link target), or with any child element (`img`, `a`,
`span`, `br`…).

**Language:** quotes are curled only when the document is English, or says no language. The
language comes from `xml:lang`/`lang` on `<html>`, else the OPF `dc:language`. Other languages
(German `„“`, French `« »`…) get dashes, ellipses and empty-paragraph cleanup only. Their quotes
are left straight rather than guessed wrong.

**Idempotent:** running the pass twice gives the same bytes as once.

## 3. Where it sits

New module `crates/deckle-core/src/typography.rs`, a pure string → string pass with its own
counters. `normalize` calls it after script and remote-reference removal when
`NormalizeOptions::typography` is set.

```
RunOptions.typography ─▶ NormalizeOptions.typography ─▶ typography::tidy_xhtml / tidy_ncx
                                                     └▶ NormalizeReport.typography: Option<TypographyReport>
```

`TypographyReport { quotes, dashes, ellipses, empty_paragraphs }` is `None` when the pass is
off. The CLI result shows `Typography: 412 quotes, 31 dashes, 4 ellipses, 2 empty paragraphs`.
The Inbox, re-process and the Daily keep it off for now.

## 4. Known limits

- Primes: `5'10"` becomes `5’10”`, not `5′10″`.
- A leading apostrophe outside the short list (`'s'pose`) becomes an opening quote.
- Number ranges (`1-5`) stay hyphenated. A hyphen can't be told from a range without a
  dictionary.
- The desktop result line doesn't show the counts yet; the CLI and `--json` do.

## 5. Acceptance

- Flag off: the output is byte-identical to today, and the existing `cargo test -p deckle-core`
  passes.
- Flag on: quotes, dashes and ellipses as in §2, in text only. `pre`/`code`/`script`/`style`
  content and attributes come out byte-identical.
- Empty paragraphs go. Paragraphs with an `id` or a child element stay. A single
  `<p>&nbsp;</p>` scene break stays.
- A `lang="de"` document keeps its straight quotes but gets dashes.
- Two runs give the same output as one, and the report counts match.
- `deckle run --typography` and the desktop toggle (off by default).
