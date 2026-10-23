# The PDF edition: the paper laid out for a tablet's page

Status: accepted, 2026-10-23, the owner having delegated the open questions of the other-readers design (§13 there). It's the typesetting spec that §9 of `2026-10-23-other-readers-design.md` asks for before code, for phase 4: a reMarkable, and `format = "pdf"` export folders.

## 1. Why

A reMarkable's EPUB reader doesn't handle a newspaper well: it loses the tables, the masthead, the photos' placement and the puzzle grid. A PDF laid out for the tablet's own page keeps them, and lets the owner write on the paper, the crossword included. The same PDF also serves an iPad or a Boox through an export folder.

## 2. Goals

- **One PDF per edition, for one screen.** The page is the screen's size, so nothing is zoomed or panned.
- **The paper as the EPUB has it:**
  - the front page: masthead, date, weather, the lead, the tiles and "Inside today";
  - every section on a page of its own, in the same order;
  - each story with its headline, meta line, body and photos;
  - the weather strip and the puzzle grid as tables;
  - the weather icons.
- **Navigable.** A PDF outline (Front page, each section, each story) and links from the front page to the sections.
- **Readable text.** The paper's own fonts (Old Standard, with Unifraktur for the masthead) are embedded with a ToUnicode map, so text can be searched, copied and read back.
- **No new heavy dependency, no network, no browser.** The PDF is built on the machine like the EPUB.
- **The EPUB is untouched:** the Kindle, Kobo, PocketBook and email paper stay exactly as they are.

## 3. Non-goals

- **No PDF of ordinary books.** Books reflow; a reMarkable gets them as EPUB (other readers §13).
- **No columns, justification or hyphenation.** These pages are 6 to 8 inches wide, a single ragged-right column reads best there, and both of the others need a hyphenation dictionary.
- **No font subsetting yet.** The four fonts add about 400 KB compressed per PDF; subsetting can come later.

## 4. Engine: Deckle's own layout on `pdf-writer`

The other-readers spec weighed two options:

- **Embedding Typst** gives real line breaking and hyphenation. But it brings in a couple of hundred crates and a long build. It would also need the paper's XHTML turned into Typst markup, which is a renderer of its own.
- **A small layout engine on a low-level writer** is what this spec chooses: `pdf-writer` (MIT/Apache-2.0, from the Typst project, with a handful of dependencies). Deckle lays the pages out itself, from the same XHTML the EPUB carries, so every section that renders in the EPUB renders here without code of its own.

The engine's pieces, in `deckle-digest::pdf`:

1. **Fonts:** Old Standard Regular, Bold and Italic, and Unifraktur Maguntia, through `ab_glyph` (already a dependency) for glyph ids and advances.
   - Each is written as a Type0/CIDFontType2 font with `Identity-H` encoding.
   - Its `W` array lists the glyphs used, and its ToUnicode CMap maps each glyph used back to its character.
   - A character the font lacks is transliterated (`deunicode`) before it's measured.
2. **XHTML to blocks.** Each story's body becomes blocks:
   - paragraphs of styled runs (regular, bold, italic, size), for `p`, the headings, list items, `blockquote`, `div` and `figcaption`;
   - images, from a photo token or an `<img>` naming an edition asset;
   - rules;
   - tables.
   
   `em`/`i`, `strong`/`b`, `small` and the `meta`, `kicker`, `credit`, `note` and `small` classes set the style. Anything else is read for its text. A body that won't parse as XML falls back to its text, paragraph by paragraph.
3. **Layout:**
   - greedy line breaking per paragraph; a word wider than the line is broken by character;
   - page breaking that keeps a headline with the line after it;
   - images scaled to the column width, at most 45% of the page's height;
   - tables of equal-width columns. A `grid` table (the crossword) has square cells, `td.b` filled black and `span.n` as a small number in the corner.
4. **Writing:** each page is a content stream of text, images, lines and filled rectangles.
   - Photos are embedded as JPEG (`DCTDecode`), and PNG icons as deflated RGB on white.
   - The outline and the front page's link annotations point at the anchors the layout recorded.

## 5. Pages

- **Size:** from a named screen, in points: the screen's pixels ÷ its dpi × 72.

  | Screen | Pixels | dpi | Page (pt) |
  |---|---|---|---|
  | `remarkable-2` (default) | 1404 × 1872 | 226 | 447 × 596 |
  | `remarkable-paper-pro` | 1620 × 2160 | 229 | 509 × 679 |
  | `remarkable-paper-pro-move` | 954 × 1696 | 264 | 260 × 463 |
  | `ipad` (10.9″) | 1640 × 2360 | 264 | 447 × 644 |
  | `a5` | — | — | 420 × 595 |

  All of them still need checking on the devices (other readers spike 4).
- **Margins:** 6% of the page's width at the sides, with room for the footer at the foot.
- **Type:** body 10 pt on 13.5 pt (scaled with the page's width for `remarkable-paper-pro-move`), headlines 15 pt bold, section heads 20 pt bold over a rule, meta lines 8.5 pt italic.
- **Footer:** "<paper> · <date> · <page>" in 7.5 pt.
- **The front page** has the ears line, the masthead in Unifraktur sized to fit the width, the dateline between rules, the weather line, the lead with its teaser, and the tiles as "Section — headline" lines. "Inside today" lists each section, linked to its first page.

## 6. Photos

The PDF takes `photos = "gray" | "color"` as the EPUB does (other readers §6.1). The default is gray, color for `remarkable-paper-pro` and `ipad`.

## 7. Where it's used

- `[[export]] format = "pdf"`, with `screen` (default `remarkable-2`).
- `[remarkable]` (other readers §9): the paper as this PDF, at `screen`.

## 8. Tests

- **A fixture build's PDF:**
  - starts with `%PDF-` and ends with `%%EOF`;
  - has one page per section at least;
  - has the outline;
  - read back through PDFium (`deckle-pdf`), holds the paper's title, every section title and a story's text.
- **Layout:** line breaking within the width; a long word broken; a headline never last on a page; a grid table's black cells drawn as filled rectangles.
- **Fonts:** a character outside the font is transliterated; ToUnicode maps each glyph used.
- **Screens:** the presets' page sizes; an unknown name is a config error.

## 9. Sizing

M–L: one module of about 1,000 lines, one dependency, and two config keys.

## 10. Build notes

- **Built as specified.** On the fixture edition, a reMarkable 2 PDF is 9 pages and about 390 KB. It was rendered to images and checked by eye: the masthead, the rules, the weather strip with its icons, the section heads, the crossword with its black squares and clue numbers, and a footer on every page.
- **A lone small line** (a credit, a note) may use one and a half meta lines of the space above the footer, rather than taking a page of its own.
- **`remarkable-2`** is spelled with its hyphen in the config (`#[serde(rename)]`); kebab-case alone would give `remarkable2`.
- **The outline** has the stories under each section as well as the sections (PDFium counts 25 bookmarks on the fixture: the front page, 7 sections and 17 stories).
