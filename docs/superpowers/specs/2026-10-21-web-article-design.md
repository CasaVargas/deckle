# Web article → book — design

**Status:** scope set by owner on #21 ("CLI plus desktop drop", 2026-10-21); built in #21's PR ·
**Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-20-save-to-paper-design.md` (the extractor, long reads, link files),
`2026-10-21-docx-import-design.md` (import before the pipeline)

Give Deckle a web address, a `.webloc`/`.url` link file or a saved `.html` page, and one
clean article goes to the Kindle as its own book, right now. It doesn't wait for tomorrow's
paper.

## 1. Intent and why

Save-to-paper (#61) handles "read this tomorrow". This handles "read this now": a long
essay you want on the Kindle before you leave the desk. The extractor, the link-file parsing
and the one-article EPUB writer already exist for long reads. What's missing is a way to
build one article on demand and send it through the normal pipeline.

Success: `deckle run https://example.com/essay` puts the essay on the Kindle with its
photos and a "Deckle Reads" cover. Dropping a `.webloc` on the desktop app does the same.

## 2. Goals, non-goals

**Goals**

1. `deckle run` accepts `http(s)://` addresses and `.webloc`, `.url`, `.html` and `.htm`
   files, mixed freely with EPUBs and Word documents. Every `run` option applies (`--to`,
   `--cde`, X-Ray, Word Wise, …).
2. The article is extracted whole, with no 6,000-word cap. Photos are fetched and turned to
   grayscale JPEGs, like long reads (`[photos]` defaults).
3. A saved `.html` page is extracted from the file and never fetched. Its photos load from
   beside it.
4. Articles under 120 words (paywalls, sign-in pages, videos) fail visibly with that reason.
   They write no book.
5. The book's identifier is `urn:deckle:article:` plus a hash of the address (or the file's
   path for a saved page). Sending the same article twice gives the same book, not two.
6. Desktop: drop or choose `.webloc`, `.url`, `.html` or `.htm` files, or paste an
   address (⌘V / Ctrl+V on the Books tab). Each becomes a row with its title and cover,
   and sends like any other book.

**Non-goals (this increment)**

- Several articles bound into one book (#22).
- Dragging a link from a browser onto the window. The webview only reports dropped files,
  so this means a `.webloc` from the address bar, or a paste.
- Text-only `.txt` link lists. The Inbox takes them; here they'd make `run` ambiguous.
- Word Wise and X-Ray tuned for articles. They run as for any other book (#26).

## 3. Where it sits

```
URL / .webloc / .url ──fetch──▶ extract ──▶ article.epub ──▶ (the usual pipeline) ──▶ deliver
saved .html ───────────read───┘
```

`deckle-core` doesn't depend on `deckle-digest`, so this import step runs before the pipeline:

- `deckle_digest::web_book`:
  - `is_article(target)` says whether a `run` argument is one of the inputs above.
  - `build(target, cfg, net, dir)` resolves it (link-file parsing from `inbox.rs`, Google
    News links followed), extracts it, and writes `dir/<slug>.epub` with
    `longread::write_article` (the long-read writer, with the identifier passed in).
    `cfg` is the Daily config when there is one, else the defaults, so photo settings match
    the paper's.
- **CLI:** `run` builds each article into Deckle's cache (`…/articles/`) and passes the EPUB
  to `pipeline::run`. The report shows the original argument as the input. A failed import
  is a `FAIL` line, like any other failure.
- **Desktop:** an `import_articles` command (blocking work off the main thread) builds the
  EPUBs into the same cache. It returns ordinary book rows that point at them, so planning
  and sending don't change. A failed import is an error row with the reason.

## 4. Verification

- `deckle-digest`: an address served by a fixture `Net`, a `.webloc`, a `.url` and a saved
  `.html` each build a valid EPUB with the extracted title, byline and photos. The saved page
  never touches the network. A stub page fails with the 120-word reason. The same address
  gets the same identifier twice.
- `deckle-cli`: `run` with a saved `.html` page goes end to end through the fake Previewer
  to a folder.
- Desktop: `import_articles` on a saved page returns a row with the article's title and a
  cover, and a stub page returns an error row. The drop, picker and paste paths are checked
  by hand in the running app.

All HTTP (here, the Daily, Open Library, Word Wise, Wikipedia, the LLM) checks certificates
against the OS store (`ureq` platform verifier), so a TLS-inspecting proxy the machine trusts
(e.g. Zscaler) works instead of failing with `UnknownIssuer`.
