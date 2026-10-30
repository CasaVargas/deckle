# Who's who: X-Ray for every e-reader — design

**Status:** accepted, 2026-10-30 (the owner chose it from the e-reader parity list and handed the
design calls to the builder) · **Date:** 2026-10-30 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-23-other-readers-design.md` (§3: no X-Ray off Kindle, §8 Kobo, phase 3
PocketBook)

Kindle X-Ray is a Kindle file format, so a Kobo or PocketBook book gets nothing from the people
and terms Deckle already finds. This puts the same entities inside the book itself, where any
EPUB reader can show them: a **Who's who** page at the end, and the first mention of each name in
each chapter linked to its entry.

## 1. Goals

- Any EPUB reader gets the people and terms X-Ray finds, with their descriptions.
- Readers that show note popups (KOReader, Kobo with KEPUB, PocketBook) show the entry in place
  when a linked name is tapped; the rest jump to the page and back.
- **Kindle is unchanged, byte for byte.** The Kindle path keeps its `.asc` sidecar and never gets
  the page.

## 2. Non-goals

- No spoiler-aware entries. Descriptions are the ones the Kindle sidecar gets (first-mention
  quote, Wikipedia for terms when opted in, the owner's LLM when configured), which has the same
  limitation.
- No translated UI strings yet: the page's headings are English. The book's language still goes in
  `xml:lang`.
- No Who's who in the Daily or its long reads: the paper has no recurring cast.

## 3. Where it runs

`deckle_core::reader::book_epub(source, whos_who)` gains an `Option<&XRayOptions>`. `Some` adds
the page after normalize and before the reader's `prepare` (so a Kobo's KEPUB spans the page
too). Callers:

- **`deckle run … --to device` to a Kobo or PocketBook** (`send_to_reader`): `Some` unless
  `--xray off`, with the run's `--llm…` and `--wikipedia` options.
- **The Inbox's e-reader copy** (`inbox::with_readers`): `Some(XRayOptions::default())`, the
  heuristic with first-mention quotes, matching the Inbox's Kindle options (no LLM, no
  Wikipedia).

A failure to build the page is a warning, never a failed book: the book ships without it.

## 4. Text and entities

- The spine's XHTML documents, in reading order. Text is read from each document's `<body>` in a
  single pass, skipping `script`, `style`, `svg` and `math`. Entities are decoded (`&amp;` → `&`).
- Each text node becomes one `deckle_kfx::text::Run`. Its `pid` is the decoded character offset
  into the whole book, and a separator keeps runs apart. `deckle_xray::extract::heuristic` then
  works unchanged.
- The LLM and Wikipedia steps are shared with the Kindle build: `deckle_core::xray` gains
  `entities_with(…) -> Entities { list, source, warnings }`, which `build_with` now calls before
  writing the `.asc`. The Kindle output doesn't change.
- **Too few:** with fewer than 3 entities the page isn't added (a pamphlet or a cookbook gains
  nothing).

## 5. Links in the text

- For each entity and each **chapter**, the first occurrence becomes
  `<a class="deckle-ww" epub:type="noteref" href="<page>#deckle-ww-N" id="deckle-ww-N-dK-sJ">name</a>`.
  `K` is the document's spine index. `J` counts the `h1`/`h2` headings before the mention in that
  document, because one file often holds several chapters (Gutenberg books do). The occurrence is
  found by its `pid`, so it's the mention the heuristic counted, aliases included.
- Never linked: text already inside an `<a>`, a heading (`h1`–`h6`), `title`, `pre`, `code` or
  `table`; an occurrence that spans two text nodes.
- The link's only styling is an inline `color: inherit; text-decoration: none; border-bottom: 1px
  dotted`, so the page reads the same and the book's stylesheets aren't touched.
- The documents' `epub` namespace is declared on `<html>` when missing.

## 6. The page

`deckle-whoswho.xhtml`, beside the OPF, appended to the manifest and the end of the spine
(`linear="yes"`) and to the nav document's TOC (and the NCX, when there is one) as "Who's who".

```html
<section epub:type="appendix" id="deckle-whoswho">
  <h1>Who's who</h1>
  <h2>People</h2>
  <ol epub:type="endnotes" class="deckle-ww-list">
    <li epub:type="endnote" id="deckle-ww-0">
      <p><b>Elizabeth Bennet</b> — description.</p>
      <p class="deckle-ww-back"><a href="ch1.xhtml#deckle-ww-0-d1-s1">First mentioned in Chapter 1</a></p>
    </li>
  </ol>
  <h2>Places and terms</h2>
  …
</section>
```

- People first, then terms; within each, by first appearance.
- "First mentioned in …" uses the nav label of the document holding the first link, else
  "the text".
- A Wikipedia description ends with "From Wikipedia (CC BY-SA)." linked to the article, the
  attribution the licence asks for.
- `list-style: none`. Every id starts `deckle-ww-`, a prefix no book uses, so no clash check.
- A book that already has the page (a second pass) comes back unchanged.

## 7. Tests the build must add

- A three-chapter EPUB with recurring names: the page is in the manifest, spine end and nav; each
  chapter links the first mention of each name once; a name in a heading or an existing link isn't
  linked; entries are People then terms.
- Fewer than 3 entities: the EPUB comes back unchanged.
- `&amp;` and other entities in text: offsets still land on the right words.
- The Kindle `.asc` from `build_with` is byte-identical before and after the refactor (the existing
  X-Ray tests pass unchanged).
- A Kobo copy: `kepub::convert` of a book with the page still opens, and its links still resolve.

## 8. Device checks owed (`docs/devices.md`)

Whether tapping a linked name opens a popup or jumps, on a Kobo (KEPUB), a PocketBook, KOReader and
Apple Books. A jump with the back link is the floor everywhere.
