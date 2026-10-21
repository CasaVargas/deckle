# Plain text in — design

**Status:** approved by owner (2026-10-21) for [#86](https://github.com/CasaVargas/deckle/issues/86), the first slice of [#24](https://github.com/CasaVargas/deckle/issues/24) ("More source formats").
FB2, ODT, RTF and MOBI/AZW3 get their own issues later ·
**Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-docx-import-design.md` (the `import` stage)

Drop a `.txt` file (a Project Gutenberg download, an exported draft, a long saved post) and it
reaches the Kindle as a real book: paragraphs instead of hard-wrapped lines, chapters in the
contents, and the title and author it states.

## 1. Decision

- **TXT first, and one format per issue after it.** #24 bundles five formats with no
  acceptance criteria. TXT is the cheapest to do well, it's the most common Gutenberg
  download, and it proves that the import stage generalises beyond DOCX. Proposed order for the
  rest: FB2, ODT, RTF, then MOBI/AZW3 (DRM-free only, which is a §2 non-goal check).
- **A new crate, `crates/deckle-txt`**, the same shape as `deckle-docx` (`is_text(path)`,
  `convert(txt, epub) -> Result<ImportReport, TxtError>`), with no dependency on the rest of
  Deckle. The pipeline's `import` stage dispatches on the extension.
- **Heuristics are conservative.** When in doubt, keep a line break, and don't invent a
  chapter. A wrongly joined paragraph is easy to notice; a heading invented in the middle of a
  sentence is worse.
- **Not Markdown.** `.md` is #23 and needs its own dialect decisions. A `.txt` that happens to
  contain Markdown is converted as plain text.

## 2. Rules

**Encoding.**
1. A BOM (UTF-8, UTF-16 LE/BE) wins.
2. Otherwise valid UTF-8 is read as UTF-8.
3. Otherwise the file is decoded as Windows-1252, with a warning ("decoded as Windows-1252").

Uses `encoding_rs` (new dependency). Line endings `\r\n`, `\r` and `\n` are all accepted.

**Project Gutenberg.** If the file has `*** START OF THE PROJECT GUTENBERG EBOOK` and `*** END
OF …` markers (any capitalisation, `THIS` or `THE`), only the text between them is the book.
`Title:`, `Author:` and `Language:` lines before the start marker set the metadata. The licence
text is dropped. Nothing else is assumed about the format.

**Paragraphs.** The file is split into blocks at blank lines (lines that are empty or only
whitespace).
- **Hard-wrapped:** blank lines separate paragraphs, and at least 60% of the lines in
  multi-line blocks are between 50 and 80 characters long. The lines in each block are joined
  with a space, and a trailing hyphen between two lowercase letters is kept (`well-\nknown` →
  `well-known`). Hyphens are never removed, because there's no way to tell a broken word from
  a real compound without a dictionary.
- **One paragraph per line:** there are no blank lines, or most blocks are a single line. Each
  line becomes a paragraph.
- **Verse or short lines:** any other block (most lines under 50 characters) keeps its line
  breaks as `<br/>`.

**Chapters.** A block is a heading only when it is a single line of 60 characters or fewer, has
a blank line on each side, and matches one of these patterns:
- `Chapter|Part|Book|Volume|Prologue|Epilogue|Preface|Introduction|Afterword|Appendix`,
  optionally followed by a number, numeral or word (any capitalisation), with an optional
  subtitle after `.`, `:` or `—`;
- a Roman numeral (I–CC) or an Arabic number alone, optionally with a trailing `.`;
- a line in all caps (at least three letters) directly before one of the above, which is
  merged into it as a subtitle.

Each `Part`/`Book`/`Volume` is a level-1 contents entry. Other matches are level 2 under the
current part, or level 1 when the book has no parts. Each heading starts a new XHTML file. A
book with no headings is one contents entry (the title), split into roughly 200 KB XHTML files
at paragraph boundaries so Previewer gets reasonably sized chapters.

**Scene breaks.** A block that is only `*`, `#`, `~` or `-` characters (with optional spaces,
at least three) becomes `<hr class="scene"/>`.

**Metadata.**
- Title: Gutenberg `Title:`, else the file stem.
- Author: Gutenberg `Author:`, else none.
- Language: Gutenberg `Language:` mapped to a BCP 47 tag, else `en`.
- Identifier: `urn:deckle:txt:` plus a hash of title and author, the same scheme as DOCX, so a
  re-sent file stays the same book.
- There's no cover; the typographic cover generator (#27) gives it one.

**Errors.** Specific errors for an empty file, a file that is mostly binary (more than 1% NUL or
control bytes), and a file over 50 MB.

## 3. Where it sits

```
book.txt ──import (deckle-txt)──▶ imported.epub ──inspect──▶ normalize ──▶ …
```

`pipeline::run` checks `deckle_docx::is_word` and then `deckle_txt::is_text` (extension `.txt`,
any case). Every later stage is unchanged. The library accepts `.txt` as a source alongside
EPUB and DOCX (`deckle_library::is_source`). The desktop drop zone follows in the same
increment if DOCX has landed there by then; otherwise it's CLI only, matching DOCX.

## 4. Verification

`fixtures/txt/` is made by `tools/ref/make_txt_fixtures.py` (standard library only):
- `gutenberg-mini.txt`: Gutenberg markers, header fields, two parts, four chapters, a scene
  break and hard-wrapped lines;
- `unwrapped.txt`: one paragraph per line, in Windows-1252, with curly quotes;
- `verse.txt`: short lines;
- `utf16.txt`: UTF-16 LE with a BOM;
- `noheadings.txt`: about 500 KB with no headings.

Crate tests check the contents order and nesting, paragraph joining (and when lines are not
joined), `<br/>` in verse, scene breaks, the Gutenberg licence being stripped, metadata, the
encoding fallback warning, the file splitting, and each refusal. The pipeline and CLI tests run
`gutenberg-mini.txt` end to end through the fake Previewer. A real Gutenberg novel through real
Previewer is checked by hand.

## 5. Implementation notes (2026-10-21, #86)

Calls made where §2 left room, so the code and this document agree:

- **Order of the paragraph rules.** The hard-wrapped test runs first, so a dialogue-heavy
  Gutenberg novel (many one-line blocks) still joins its wrapped paragraphs. A multi-line block
  in a hard-wrapped book stays verse when more than half its lines are under 50 characters (a
  signature, an address).
- **Long unwrapped lines.** In a book that is neither hard-wrapped nor one paragraph per line, a
  multi-line block whose lines are mostly 50 characters or longer becomes one paragraph per line,
  not verse.
- **All-caps subtitle.** Rendered as `Chapter 4<br/><span class="subtitle">THE LAST PROOF</span>`;
  the contents entry reads `Chapter 4 — THE LAST PROOF`. A line that is itself a heading
  (`PART TWO`) is never merged.
- **Roman numerals** must be canonical (`IIII`, `MCMXCIX` and mixed case are not headings).
- **Splitting at ~200 KB** applies only to a book with no headings; a long headed chapter stays
  one file.
- **Unknown `Language:`** falls back to `en` with a warning.
- **Checked by hand:** Gutenberg #1342 (*Pride and Prejudice*, 772 KB) imports with the Preface
  and 61 chapters in the contents and no warnings. Its first heading reads `Chapter I.]`,
  because an illustration caption wraps it in the source. Real Previewer wasn't available on
  this machine, so that end of the check is still to do.
