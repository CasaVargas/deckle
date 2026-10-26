# Books tab redesign: shelf, To send tray, book details

Mockups: the "Deckle Books Tab Redesign" canvas, artboards A (shelf + tray) and A (book
details). Direction B (table view) and batch editing are not in this plan.

## 1. Why

The Books tab grew one feature at a time and it shows:

- The library's "Search library and Kindles" button and "Back to library" have no class, so
  the Mac draws its own blue push buttons over Deckle's look. The search results are raw
  headings and lists.
- The top bar holds four outlined buttons (Add books, Add notes folder, Add article, Clear
  done) beside the tabs.
- Sending lives in a footer, split from its menu, away from the queue it sends.
- Each queued row stacks chips (mode, preset, cover pick, notes) that say little on their own.
- Fixing a book's details is the fourth pane of Before you send, only for library books, and
  only title, authors and series.

## 2. Goals and non-goals

Goals:

- No native-styled controls anywhere on the tab; one consistent button set.
- One way in (an **Add** menu), one way out (a **Send** button at the foot of To send).
- A book's details are one click away from its cover or its queue row, and editing them is
  safe: the original file and calibre are never written, and nothing changes until **Save**.
- More details can be edited: description, tags, publisher, published date, language, ISBN.
- Open Library can suggest missing details, field by field, under the existing metadata
  setting. Nothing is accepted for you.

Non-goals: the table view, editing several books at once, new network features beyond the
lookup Deckle already has, and the Daily and Kindle tabs (except where they share the footer,
§4).

## 3. The look

- A base reset: every `button` gets `appearance: none`, a transparent background and no
  border, so an unclassed button can't fall back to the native look. Buttons come in three
  kinds: **primary** (terracotta, at most one per surface), **secondary** (panel fill, line
  border) and **quiet** (text only).
- The tabs become a segmented control.
- Headings stay serif. The mockups use Newsreader; bundling it (OFL, a local woff2, no
  network) is a separate decision. Until then `ui-serif, Georgia` stays.
- Queue rows say one plain sentence with a mark, not chips: "✓ X-Ray, Word Wise, page
  numbers", "✓ Keeps its cover", "! No author — add one before sending". Warnings stay amber,
  failures red, and the full report stays one click away.

## 4. Layout

- **Top bar**: Deckle, the tabs, search (Books tab), **Add ▾** (Files…, Web article…, Notes
  folder…) and Settings. "Clear done" moves to the To send header as **Clear**.
- **Library** (left), as now: grid of covers, series with gaps. The filter chips become a
  segmented control (All / Not on Kindle), with series grouping as a toggle beside it.
- **Selection bar**: picking covers shows a floating bar: "*n* selected · Edit details ·
  Clear · **Add to To send**". It replaces the "Add *n* to To send" button in the header.
  "Edit details" only shows for one book.
- **Search**: typing filters the library at once, as it does now. Searching connected
  Kindles stays, as a quiet "Also search Kindles" action under the field once there's text,
  and its results use the library's own row styles.
- **To send tray** (right): header with a count and **Clear**; the rows; a drop hint; then at
  the foot the Kindle picker (status dot, name, free space when known) and a full-width
  **Send *n* books to Kindle**, with **Export to folder…** and **Bind into one book…** as
  quiet actions below it. With no library the tray is the whole tab, as now.
- **Footer**: the Books tab no longer has one. The Kindle tab keeps its device picker and
  version line; the version also stays in Settings › About.

## 5. Book details

One drawer for one book, opened from a library cover (selection bar → Edit details, or
double-click) or a queue row's **Review**. It replaces Before you send, keeping its panes as
tabs:

- **Details**: the editor (below).
- **Cover**: Before you send's cover pane, as is.
- **On the Kindle**: Before you send's plan and page preview, as is.

Cover and On the Kindle preview the queued book, so a book opened from the library that isn't
in To send (or one that can't be converted, or is sent as is) has only Details. A queue row's
title opens On the Kindle, and its menu's **Edit details…** opens Details.

The editor:

- Header: the cover, title, author, format and pages, and where the book lives. For calibre
  or a folder: "From calibre — calibre is never changed" with the fields read-only and a note
  on how to edit (§7).
- Fields: Title; Authors as removable chips; Series and Number (number disabled without a
  series); Description; Publisher and Published; Language and ISBN; Tags as chips; and under
  **More**, sort title, sort author and the Amazon ASIN (moved from the row menu).
- A changed field shows "● Edited". **Undo changes** puts the form back; **Save** writes the
  library copy, then refreshes the row and cover. A failed save keeps the form and says why.
- Closing with unsaved changes asks once: Save, Discard, or Keep editing.

## 6. Suggestions

With **Fill missing details** on (Settings › Books, the existing `metadata_fill`), opening
the editor looks the book up with the same Open Library code a send uses, once per opening.

- Only empty fields get a suggestion, as with fill-at-send: the book's own data wins.
- Each suggestion sits under its field: "Suggested: 15 Sep 2020 · **Use**". A banner offers
  **Use all *n***. Using one fills the field and marks it edited; it's saved only on **Save**.
- With the setting off, there's a quiet "Look up on Open Library" link instead, which says it
  goes online and does nothing until clicked.
- Open Library returns title, authors, series, description and subjects (as tags). Part 5
  reads the edition record's publisher, publish date (free text, made a W3C date) and language
  (MARC, made a tag like `en`) too. Those come only from the edition, which is found by ISBN;
  a title search finds a work, which has none, so a book without an ISBN gets no publisher,
  date or language suggestion. An ISBN is never suggested: the only edition Deckle can find is
  the one the book's own ISBN names. The send-time fill doesn't use the new fields.
- The title isn't suggested: every book in the library has one.

## 7. Rulings

- **Where edits go**: as now, only the library copy's OPF and sidecar. Calibre and folder
  books are read-only in the editor until they're in Deckle's library. A "Copy into Deckle's
  library" action is a later decision and isn't in this plan.
- **ASIN** moves into the editor; the row menu keeps Prefer features / Prefer cover, Book
  style, Details and Remove.
- **Before you send** stops being its own panel; its code becomes the drawer's tabs.

## 8. Pull requests

1. **Look and frame**: the button reset and three button kinds, segmented tabs, the Add
   menu, the To send tray with the Kindle picker and Send at its foot, no Books footer, the
   restyled search button and results. No behavior changes.
2. **Library**: the segmented filter, the selection bar, search under the top bar with "Also
   search Kindles", one-line queue statuses.
3. **Details drawer**: Before you send becomes the drawer with three tabs; the editor with
   today's fields (title, authors, series, number), dirty marks, undo and the close prompt;
   ASIN moved in.
4. **More fields**: `Details` gains description, tags, publisher, published, language, ISBN,
   sort title and sort author; the editor shows them, read from the copy's OPF
   (`library_read_details`). Built:
   - Each new field is optional: absent leaves the book's own, empty removes it, so an older
     caller can't wipe what it didn't send. If the OPF can't be read, the editor offers only
     title, authors and series.
   - `rewrite_opf` writes dc:description, dc:subject (one per tag), dc:publisher, the
     publication dc:date (other EPUB 2 dates stay), dc:language (never removed: every EPUB
     needs one), an ISBN as `urn:isbn:…` (valid in EPUB 2 and 3; the package's unique
     identifier is never touched), `calibre:title_sort`, and the first author's sort name
     (`file-as` refines in EPUB 3, `opf:file-as` in EPUB 2 when the OPF declares `opf`).
   - The library and the editor refuse the same things: a published value that isn't a year
     or a W3C date, a language that isn't a tag like en-GB, an ISBN that isn't 10 or 13 digits.
5. **Suggestions**: a `library_suggest_details` command over `metadata_fill::lookup`, per-field
   **Use** and **Use all**, the off-setting link, and edition-record publisher, date, language
   and ISBN.

## 9. Verification

- Unit tests: `rewrite_opf` round-trips every new field, leaves unrelated metadata alone, and
  is idempotent; suggestions only cover empty fields; the dirty-state and close-prompt logic.
- Existing desktop tests (`bookStatus`, `beforeSend`, `libraryView`) keep passing, updated
  where the copy changes.
- Each PR is checked in the running app, light and dark, at the app's minimum window width,
  with a library and without one.
