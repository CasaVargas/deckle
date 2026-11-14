# Books, in full

The README has the overview. This page lists everything Deckle turns into a book, and the book
styles.

## What a book gets

- **Enhanced Typesetting (KFX).** Kindle Previewer makes a KPF; Deckle packs it into the monolithic
  KFX a Kindle reads, with approximate page numbers.
- **Covers that stick.** On Kindles where it can, Deckle writes its own cover thumbnail and puts it
  back on every send.
- **X-Ray.** The people and terms in the book. Optionally a model (Ollama on this computer,
  Anthropic, or any OpenAI-compatible endpoint) writes spoiler-free descriptions, and
  Wikipedia adds summaries. A Kobo or PocketBook gets the same people and terms as a Who's who
  page at the back, with each name's first mention in a chapter linked to it.
- **Word Wise.** Hints from a Wiktionary-derived English lexicon, pointed at the Kindle's own
  dictionary when it has one.
- **Recaps, without spoilers.** Off by default. With the model you set up for X-Ray, a short
  labeled recap at the end of each chapter, written from that chapter alone; a Summary page at
  the back; and for a later book in a series, a Previously page before chapter one, made only
  from the earlier books' recaps. On every reader, and a book is never summarised twice.
- **Book styles.** Novel, Academic or Manual, per book: the paragraph style, pop-up notes, pages
  and X-Ray set the way that kind of book reads.
- **Cleanups.** Typography fixes, footnotes as pop-up notes, language tags repaired, images missing
  alt text reported, and missing details filled from Open Library if you turn that on.

## What you can send

- **EPUB.** The usual case: cleaned up, enriched and sent.
- **Word, OpenDocument, RTF, FictionBook and plain text** (`.docx`, `.odt`, `.ott`, `.fodt`,
  `.rtf`, `.fb2`, `.fb2.zip`, `.txt`): headings become the contents, and tables, pictures and
  footnotes come through.
- **DRM-free Kindle files** (`.mobi`, `.azw`, `.azw3`, `.prc`) rebuilt as real KFX with page
  numbers, X-Ray and Word Wise. DRM-protected files are refused, never decrypted.
- A file nested more than 256 levels deep (tags in tags, groups in groups) is refused with a
  message saying so. Real books stay under 30.
- **PDFs, reflowed.** Columns read in order, even a journal's narrow gutter, running heads and
  page numbers dropped, broken words joined, citations kept in their sentence; tables, equations
  and figures kept as pictures. A paper's first page reads title, dates, authors, abstract.
  Never as good as a real ebook, and the run says so. A scan, with no text to reflow, goes as
  its page pictures, the way a comic does (no OCR).
- **Markdown and notes folders.** A file or a flat folder of notes becomes one book: headings,
  tables, local images and links between notes, a chapter per file. See [markdown.md](markdown.md).
- **Comics, manga and picture books** (`.cbz`, `.cbr`, a folder of page images, a fixed-layout
  EPUB or a fixed-layout AZW3) keep their pages as they are: on a Kindle as an AZW3, since Kindle
  Previewer makes no KFX for fixed layout. A CBR needs the system's `bsdtar` (built into macOS
  and Windows as `tar`; `libarchive-tools` on Linux). Pages are fitted to the screen of the Kindle
  or Kobo that's plugged in (nineteen Kindles and thirty Kobos known by model), spreads are split
  at import, and `ComicInfo.xml` gives the title and series. Manga marked right to left reads right
  to left; a **Reads right to left** checkbox on the book's row (or `deckle run --direction rtl`)
  sets it for a comic that doesn't say. A Kobo or PocketBook gets a fixed-layout EPUB; a reMarkable
  (`--to remarkable`) an image-only PDF fitted to its screen, since its EPUB reader reflows. No
  X-Ray, Word Wise or page numbers.
- **Puzzle books to write on.** The **Puzzles** tab (and `deckle puzzles`) makes a book of
  crosswords (5×5 to 15×15), word searches, sudoku, word ladders, cryptograms, nonograms (your
  own photo as the first) or mazes, as a PDF that takes pen ink on a Kindle Scribe, sized for a
  Scribe, a reMarkable, an iPad or A5 paper. A crossword or word search is **themed on what you
  follow**: the headline words of your interest sections or the whole week's paper, clued from
  the headlines; the words you looked up on the Kindle; your cookbook; your own list; or a
  subject the AI writes about. A theme you can't use yet says why. With the AI on, the clues can
  be rewritten as trivia, cryptic, or for a kid. The first page is previewed before you make it,
  each step of the making is shown as it goes, and **Your books** lists every book you've made,
  so **Make again** gives the same one. For an iPad, **Send to** its export folder (or
  `--as bundle`) writes the same book as data its reader solves, Sunday's book can ride along
  with Sunday's paper, and what was solved there comes back to the tab. See
  [puzzle-books.md](puzzle-books.md).
- **A PDF as it is.** `deckle send` copies a PDF, TXT, AZW3 or MOBI to a Kindle or a folder
  untouched, which is how a document that takes ink gets onto a Scribe.
- **Web articles.** A URL, a `.webloc`/`.url` link or a saved `.html` page.
- **Several files, one book.** Bind any of the above in your order, each with its own entry in the
  contents: a course packet, a week of long reads, a report in pieces.
- A finished `.kfx` is sent as it is.

## Size limits

A file that would make Deckle use more memory than these is refused, with an error that names
the limit, instead of slowing the computer or crashing. Real books are far inside them.

| What | Limit |
|---|---|
| EPUB, KPF, CBZ: one file inside the archive, unpacked | 256 MB |
| EPUB, KPF: the whole archive, unpacked | 1 GB |
| CBZ, CBR: the whole archive, unpacked | 4 GB |
| CBZ, CBR: one page | 256 MB |
| DOCX: the whole document, unpacked, pictures included | 50 MB |
| ODT, FB2, RTF, TXT, MOBI, Markdown: the file (ODT unpacked) | 50 MB |
| ODT, FB2, RTF: one embedded picture | 10 MB |
| PDF: the file | 200 MB |
| TXT: chapters (past this, the rest stays in the last one) | 10,000 |
| MOBI, AZW3: one embedded font, unpacked | 32 MB |
| MOBI, AZW3: all embedded fonts, unpacked | 256 MB |
| MOBI, AZW3: the text, unpacked | 256 MB |
| PDF: page width (wider pages are laid out as 14,400 pt) | 14,400 pt |
| PDF: a page rendered as a picture (a larger one renders smaller) | 100 megapixels |
| A picture in a book, a cover or a web photo (not comic pages) | 100 megapixels |
| KFX, KPF: symbols in the symbol table | 1,000,000 |
| KFX, KPF: values decoded from the file (paragraphs, styles, positions…), and a value for each 64 bytes of a name repeated in it | 5,000,000 (about 1.6 GB of decoded tree at the limit). This bounds what the decoder builds; copies the conversion makes of parts of it afterwards are not counted yet (jloutsch/deckle#79) |
| KFX, KPF: positions and page ranges, all together | one a character of the book's text, one an element |

Comic pages have no pixel limit of their own: one page decodes to at most 512 MiB, and fitting
it to a screen can briefly take about twice that. The numbers are in one place,
`crates/deckle-limits`.

## The library

**The library is a plain folder** (`Author/Title/` plus a `deckle.json`), no database. Deckle shows
it beside the queue with covers from calibre and the folders you name, groups series with the gaps
called out, searches across your library, your Kindles and past sends, and tells you which books
on a Kindle it could now build better.

## Book styles

Choose **Book options → Book style** in the app, or `deckle run book.epub --preset academic`
(`novel` and `manual` too). Academic uses spaced serif paragraphs and pop-up notes; Novel uses
indented serif paragraphs and X-Ray when eligible; Manual uses spaced sans-serif paragraphs and
keeps existing note handling. All use automatic pages and default Word Wise off. The app remembers
the choice for that source path and previews it in Before you send.

CLI flags override preset defaults:

```sh
deckle run novel.epub --preset novel --popup-notes=false --pages off --xray off
```

Styles use generic fonts and relative spacing; Kindle reader settings can affect their
appearance. Fixed-layout books skip styling with a warning. Presets apply to Kindle/KFX
conversion; the direct Kobo/PocketBook route doesn't support them yet. The full defaults are in
the [preset design](superpowers/specs/2026-10-24-book-style-presets-design.md).
