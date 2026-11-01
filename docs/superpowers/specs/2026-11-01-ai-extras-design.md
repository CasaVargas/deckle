# AI extras: chapter recaps first — design

**Status:** draft, 2026-11-01 · **Owner:** Jonathan (CasaVargas LLC) · **Issue:** #29
**Builds on:** `2026-10-18-deckle-design.md` (§7 LLM policy), `2026-10-30-whos-who-design.md`
(page injection), `2026-10-24-daily-llm-editor-design.md` (the shared `llm::complete`)

Issue #29 asks for four optional, LLM-written extras: a book summary, a glossary, a "previously in
this series" recap and end-of-chapter summaries. This design settles where the model runs, drops
the glossary, orders the other three, and specifies the first one — **chapter recaps** — in full.

## 1. Decisions

| Question | Decision | Who |
|---|---|---|
| Where the model runs | The existing LLM setting (Anthropic, OpenAI, or any OpenAI-compatible server such as Ollama or LM Studio). No new key field, no bundled model. | Owner (accepted the recommendation) |
| Glossary | Dropped: the Who's who page (EPUB readers) and X-Ray (Kindle) already are one. | Owner |
| Order | Chapter recaps → book summary → series recap. | Builder's call, owner did not object |
| Where a chapter recap goes | At the end of its chapter, in the text. | Owner |

### Why no bundled model

Bundling llama.cpp would make "local" one click, but adds a multi-gigabyte download, separate
Metal/CUDA/CPU builds for the Tauri app and the Docker image, and model updates Deckle would have
to ship. Ollama and LM Studio already give the owner a local, free, private path through the
setting Deckle has. No option involves Deckle charging per use (one-time purchase, files first).

What each option costs the owner, for a ~100k-word novel (~130k tokens in, ~6k out):

| Setting | Per book | Trade-off |
|---|---|---|
| Anthropic, Sonnet 5.5 ($2 / $10 per M tokens) | ≈ $0.30 | Chapter text goes to Anthropic |
| Anthropic, Haiku 5.5 ($0.10 / $0.50) | ≈ $0.02 | Same; weaker prose |
| Local Ollama, ~8B model | $0 | Owner installs Ollama and a ~5 GB model; measured 18 min for a 108k-word book with `qwen3:8b` on an M4 Mac mini (16 GB), and it gets some details wrong |

Re-sending a book costs nothing: recaps are cached (§6).

## 2. Goals, non-goals, anti-goals

**Goals**

- A short, clearly labelled, AI-written recap at the end of each chapter, so a reader coming back
  to a book after a break can catch up from where they stopped.
- Works with a local model on its default context, not only with hosted models.
- Off by default; on, it never costs a book: any failure ships the book without recaps and says so.

**Non-goals (this increment)**

- The book summary and the series recap (§9 sketches them; each gets its own increment).
- Translated labels: "Chapter recap" is English; the recap text itself is in the book's language.
- Recaps in the Daily or its long reads.
- Editing recaps by hand.

**Anti-goals** — failures even if the code works:

- A recap that reveals anything after its chapter (a spoiler).
- AI text a reader could mistake for the author's.
- A send that silently costs money or silently sends text off the machine: the setting says so
  (§8).

## 3. Amendment to the v1 LLM policy

The v1 design §7 says the LLM is used only by X-Ray. The Daily editor already widened that; this
adds a third use, chapter recaps, under the same rules: opt-in, default off, the owner's configured
provider only, and nothing breaks without it.

## 4. Where it runs

A new module `deckle_core::recaps`, shaped like `whoswho`: EPUB bytes in, EPUB bytes out.

```rust
pub struct RecapOptions { pub llm: LlmConfig, pub cache_dir: Option<PathBuf> }
pub struct RecapSummary { pub chapters: usize, pub recapped: usize, pub cached: usize,
                          pub model: String, pub warnings: Vec<String> }
pub fn add(bytes: &[u8], opts: &RecapOptions) -> Result<(Vec<u8>, Option<RecapSummary>), String>;
pub fn add_with(bytes, opts, post: &mut Post<'_>) -> …;   // injected HTTP for tests
```

Callers:

- **Kindle** (`pipeline::run`): after `normalize`, before `previewer::convert`, the normalized EPUB
  is rewritten with recaps. `RunOptions` gains `recaps: Option<PathBuf>` (the cache directory;
  `None` = off); it does nothing unless `llm` is also set (then a warning: "Chapter recaps need an LLM; set one in Settings").
- **Other readers** (`reader::book_epub`): same, after normalize and before the Who's who page.
  Increment 2 (§10).
- **CLI**: `deckle run … --recaps` (uses the run's `--llm…` options). Until increment 2, a run to
  a Kobo, PocketBook or other EPUB reader with `--recaps` ships the book without recaps and warns
  "Chapter recaps go to Kindle only for now".

**X-Ray and Word Wise see the recaps.** On Kindle they are built from the KFX text after
Previewer, so a name in a recap counts as one more X-Ray mention and recap words can get Word
Wise hints. That is accepted: a recap repeats names the chapter just used, and keeping the text out
would need a marker that survives Previewer plus a filter in the run extraction. Revisit if it
shows up on a device. The same holds for Who's who links in increment 2.
- **Desktop**: a `chapter_recaps: bool` setting, default `false`, next to the LLM settings.

Report: `RunReport` gains `recaps: Option<RecapSummary>`; warnings go into `report.warnings`.

## 5. Chapters

- A **chapter** is a spine document split at each `h1`/`h2`, the same rule as Who's who (one file
  often holds several chapters).
- Text is the chapter's body text (skipping `script`, `style`, `svg`, `math`), entities decoded.
- **Length is counted in words**, where a word is a whitespace-separated run of text, except that
  each Han, kana, Hangul or Thai character counts as one word on its own. This counts Chinese,
  Japanese, Korean and Thai sensibly; their per-character token cost is close enough to an English
  word's for the part sizes below.
- A chapter gets a recap only if it has **at least 800 words**. That skips the title page,
  copyright, dedication, contents, short epigraph sections and the Who's who page. (Builder's call
  on the number; the test fixtures pin it.)
- A chapter's **label** is the text of the `h1`/`h2` that opens it; else the spine document's nav
  label (when the document holds one chapter); else "Chapter N" (N counting recap-eligible
  chapters). Who's who's `nav_labels` maps a whole file to one label, so it can't name the second
  chapter of a file.
- Non-linear spine items (`linear="no"`) are skipped.
- **Front and back matter is skipped**, however long. A chapter counts as matter when its heading
  (or, before the first heading, the `<body>`) sits inside an element marked by `epub:type` or
  `role="doc-…"` as front or back matter (`frontmatter`, `backmatter`, `copyright-page`,
  `colophon`, `acknowledgments`, `toc`, `index`, `bibliography`, `glossary`, `endnotes` and the
  like), or by Project Gutenberg's `pg-boilerplate` class; or when its heading names it
  ("Project Gutenberg", "copyright", "acknowledg…", "about the author", "bibliography",
  "colophon"). Found on a real run: the 2,913-word Gutenberg licence got a "recap".
- A fixed-layout book (`rendition:layout` = `pre-paginated`) gets no recaps at all (issue #20's
  books are pictures, not prose).
- A book already carrying recaps (any `deckle-recap` id) comes back unchanged.

## 6. Generating a recap

**Prompt.** System: "You write short chapter recaps for an e-reader. Answer with the recap text
only." User message, per chapter:

> Book: {title} · Author: {author} · Language: {lang} · Chapter: {label}
> Write a recap of this chapter in 2–4 sentences, in the book's language, present tense. Say only
> what happens in this chapter. Do not guess at or hint about anything that happens later. Do not
> judge or praise the writing. {chapter text}

Only the chapter's own text is sent — never later chapters — which is what makes it spoiler-safe.
The chapter label is as §5.

**Long chapters.** A local model on a 4k context can't take a whole chapter. The chapter is split
on paragraph boundaries into parts of at most:

- **2,000 words** on a self-hosted server (the `local` test `llm.rs` already uses: OpenAI provider
  with a non-default endpoint);
- **40,000 words** on a hosted provider (effectively one request per chapter).

Each part is recapped in 1–2 sentences with the same prompt; when there is more than one part, a
final request turns the part recaps (in order) into the 2–4 sentence chapter recap.

**Budget.** `max_tokens` 4096 per request. The recap itself is ~150 tokens; the headroom is for
reasoning models, whose thinking counts against the cap: the hosted OpenAI default (`gpt-5-mini`)
gets no `reasoning_effort: none` (only local servers do), and Anthropic models run adaptive
thinking when `thinking` is omitted. A cap of a few hundred would truncate before any answer. The
prompt, not the cap, keeps the recap short. A truncated answer counts as a failure
(`LlmError::Truncated`). `llm.rs` is not changed.

**Transport.** `deckle_xray::llm::complete`, so auth, the reasoning-effort tweak for local servers
and the 180 s timeout are shared. Requests run one at a time, in order.

**Answer cleanup.** Trim; strip a leading "Recap:"/"Summary:" label and surrounding quotes or
markdown emphasis; an empty answer is a failure.

**Cache.** `{cache_dir}/recaps/{key}.txt`, one plain-text file per chapter, where `key` is the
SHA-256 of: a prompt version constant, provider, model, and the chapter text. `cache_dir` defaults
to `wordwise::default_cache_dir()`. A hit skips the request; a re-send of the same book with the
same model costs nothing, and a run that failed halfway resumes where it stopped. `None` disables
the cache (tests).

**Failures.** A chapter whose request fails gets no recap and adds one warning line (`"Chapter
recap skipped for {label}: {error}"`, at most 3 such lines, then "…and N more"). If every request
fails the book comes back unchanged. `add` never returns `Err` for an LLM problem — only for an
EPUB it can't read, which the caller turns into a warning.

## 7. The recap in the book

Inserted at the end of the chapter: before the next `h1`/`h2` in the same document, else before
`</body>`.

```html
<aside class="deckle-recap" id="deckle-recap-dK-sJ" role="note"
       style="margin-top: 2em; padding-top: 0.6em; border-top: 1px solid currentColor; font-size: 0.9em;">
  <p style="font-weight: bold; margin: 0;">Chapter recap</p>
  <p style="margin: 0.3em 0 0;">{recap text, XML-escaped}</p>
  <p style="font-size: 0.8em; font-style: italic; margin: 0.3em 0 0;">AI-generated summary ({model}), not part of the original book.</p>
</aside>
```

- `<aside>` **without** `epub:type`: `epub:type="footnote"`/`"note"` would make KOReader, Kobo and
  Apple Books hide it as a popup. `role="note"` is for screen readers.
- No heading element, so the nav, the NCX, Who's who's chapter split and Kindle's TOC don't treat
  it as a chapter.
- Inline styles only, so the book's stylesheets aren't touched.
- The label names the model (`claude-sonnet-5-5`, `qwen3:8b`, …) so the reader knows what wrote it.
- `K`/`J` are the spine index and the chapter's `h1`/`h2` count, as in Who's who.

## 8. Privacy and the setting's wording

Recaps send each chapter's text to the configured LLM provider. The desktop toggle's help text:

> "Adds a short AI-written recap at the end of each chapter. Uses your LLM setting: with a hosted
> provider the book's text is sent to it and billed to your key; with a local server (Ollama, LM
> Studio) nothing leaves this computer."

The CLI `--recaps` help says the same in one line. Nothing is logged except counts and errors;
recap text lives only in the book and the cache.

## 9. Later extras (sketch, not in this increment)

- **Book summary.** Specified in §13 (increment 3).
- **Series recap.** Specified in §14 (increment 4).

Each gets its own spec section and increment.

## 10. Increments

1. **This PR:** this spec; `deckle_core::recaps` with tests (mocked `Post`); Kindle pipeline
   wiring behind `RunOptions.recaps`; CLI `--recaps`.
2. Other readers (`book_epub`) and the desktop setting and toggle, with the §8 copy (PR #282).
   The Inbox and the wireless reader server have no LLM setting, so they don't add recaps.
3. Book summary (§13, PR #286). 4. Series recap (§14).

## 11. Tests the build must add

- A three-chapter EPUB (one chapter under 800 words) with a mocked `Post`: the two long chapters
  each end with one `deckle-recap` aside holding the mocked text, XML-escaped; the short one has
  none; nav and spine are unchanged.
- One file holding two `h2` chapters: each gets its own recap before the next heading, and each
  request names its own heading as the chapter label.
- Word counting: a Japanese chapter of 1,000 kana/kanji characters with no spaces counts as 1,000
  words.
- The request's `max_tokens` (or `max_completion_tokens` on hosted OpenAI) is 4096.
- The request body sent for a chapter contains the "Do not guess at or hint about anything that
  happens later" instruction and that chapter's text only (no text from later chapters).
- Local endpoint + a 5,000-word chapter: three part requests then one merge request; hosted: one
  request.
- One chapter's request fails: the others still get recaps, one warning; all fail: bytes unchanged.
- Cache: a second run with the same cache dir makes zero requests and produces identical bytes.
- Already recapped → unchanged; fixed-layout → unchanged.
- With `recaps` off (the default), existing pipeline tests pass unchanged.

## 12. Left to the builder

How the chapter splitter is shared with `whoswho` (extract a common helper or reuse its scanner);
the exact cleanup regexes; the warning wording beyond §6.

## 13. Book summary (increment 3)

A back-matter **Summary** page: what happens in the whole book, ending included, written in one
request from the chapter recaps. It is for a reader who has finished a book, or who returns to a
series and wants the last volume back in a page.

### 13.1 Decisions (builder's calls; the owner can overturn any)

| Question | Decision |
|---|---|
| What it's made from | The chapter recaps of §6, from the cache or freshly made. The book's text is never sent a second time. |
| Setting | A separate `book_summary` switch. With chapter recaps off, the recaps are still made and cached, but not put in the chapters. |
| Where it goes | A new page at the end of the spine, after the last chapter and before the Who's who page, listed last but one in the nav and the NCX as "Summary". |
| Spoilers | Allowed and announced: the page opens with "This summary covers the whole book, including the ending." |
| Partial recaps | No summary when any eligible chapter has no recap. A summary of part of a book reads like a summary of all of it. |

### 13.2 Where it runs

In `recaps::add`, after the chapter recaps, so each caller (Kindle `pipeline`, `reader::book_epub`)
gets it with no new stage. `RecapOptions` gains two fields:

```rust
pub struct RecapOptions { pub llm: LlmConfig, pub cache_dir: Option<PathBuf>,
                          pub chapters: bool,   // recaps go in the chapters (§7)
                          pub summary: bool }   // the Summary page (§13)
```

`RunOptions.recaps` becomes `Option<recaps::Extras>`, where `Extras { cache_dir, chapters,
summary }` is `RecapOptions` without the LLM, which comes from `RunOptions.llm`. `None` is off,
as before. `RecapSummary` gains `summary: bool` (the page went in). `recapped` now counts recaps
made, whether or not they went into the chapters.

- **CLI:** `deckle run … --book-summary`, which works with or without `--recaps`.
- **Desktop:** `Settings.book_summary: bool`, default `false`. Its switch sits in the Chapter
  recaps card, and the Online & privacy row covers both.

### 13.3 Generating it

- **Input:** the recaps in reading order, each as `{label}: {recap}`.
- **Prompt:** the system prompt is "You write book summaries for an e-reader. Answer with the
  summary text only." The user message:

  > Book: {title} · Author: {author} · Language: {lang}
  > These are recaps of each chapter of the book, in order. Write a summary of the whole book in
  > 2–3 short paragraphs, in the book's language, past tense, ending included. Do not judge or
  > praise the writing. {recaps}

- **Long books on a local server:** when the recaps run past 2,000 words, they are grouped in
  order into parts of at most 2,000 words. Each part gets a one-paragraph summary, and a final
  request combines them, the same pattern as §6's long chapters. A hosted provider gets one
  request (the 40,000-word limit).
- **Cleanup, budget and transport:** as §6. Paragraphs are split on blank lines.
- **Cache:** `{cache_dir}/recaps/summary-{key}.txt`, where `key` is the SHA-256 of the prompt
  version, the provider, the model and the joined recaps.
- **Minimum:** at least 2 recapped chapters. A one-chapter book's recap is already its summary.

### 13.4 The page

`deckle-summary.xhtml`, next to the OPF:

```html
<section epub:type="appendix" id="deckle-summary"><h1>Summary</h1>
  <p><i>This summary covers the whole book, including the ending.</i></p>
  <p>{paragraph}</p>…
  <p style="font-size: 0.8em; font-style: italic;">AI-generated summary ({model}), not part of the original book.</p>
</section>
```

- Its `h1` makes it a chapter to Who's who, which is accepted (names in it count as mentions, as
  they do for recaps, §4). It is added after the recap pass, so it never gets a recap of its own.
- A book that already has `deckle-summary.xhtml` keeps it, and no new summary is made. A book that
  already has recaps but no summary comes back unchanged (§5), and gets no summary either.

### 13.5 Failures

A failed summary request is one warning, "Book summary skipped: {error}", and the book keeps
everything else. Missing recaps give the warning "Book summary skipped: {n} chapters have no
recap". A fixed-layout book gets no summary.

### 13.6 Tests

- Three long chapters with summary on: three recap requests, then one summary request that holds
  all three recaps in order. A `deckle-summary.xhtml` sits last in the spine, the manifest and the
  nav, says "including the ending", and parses as XML. The NCX entry is written by the same code
  as Who's who's, and that code is tested there.
- `chapters: false, summary: true`: the chapters have no asides, and the Summary page is there.
- One recap fails: no summary, and the warning names the count.
- Local server with recaps over 2,000 words: the part requests, then a merge request.
- Cache: a second run makes zero requests and produces identical bytes.
- The summary request fails: the recaps are still in, with one warning.
- A book that already has a Summary page keeps it, and makes no requests.
- A one-chapter book gets no summary.
- `reader::book_epub` with summary on: the Summary page is in the book. Who's who runs after
  recaps, so its page comes later.

## 14. Series recap (increment 4)

A front-matter **Previously** page in book N of a series: what happened in the earlier books,
endings included, just before chapter 1. For a reader returning to a series after months away.

### 14.1 Decisions (builder's calls; the owner can overturn any)

| Question | Decision |
|---|---|
| How earlier books are found | From the recap cache, not the library. Every run that recaps every eligible chapter of a book with series metadata writes a **book record**, and book N reads the records of the same series and author with a lower number. This lives in `deckle_core` with no library plumbing, so Kindle, other readers and the CLI all get it. |
| What earlier books must have had | Sent with any of the three extras on: chapter recaps, the summary, or this switch. |
| Setting | A separate `series_recap` switch. On a book in a series, it makes the recaps for the book's record even when chapter recaps are off (it does not put them in the chapters). On a book with no series metadata it does nothing and costs nothing. |
| Where the page goes | Before the spine document of the first chapter long enough for a recap, so after the cover, title, copyright and contents pages. It is listed in the nav before that chapter's entry, and in the NCX it gets that chapter's `playOrder`. |
| Missing earlier books | The page covers the books it found, and names them. A warning says how many earlier books had no record. |

### 14.2 The book record

`{cache_dir}/recaps/books/{key}.json`, where `key` is the SHA-256 of the lowercased author, the
lowercased series name and the series number:

```json
{ "title": "…", "author": "…", "series": "…", "index": 2.0, "model": "…",
  "recaps": ["Chapter 1: …", "…"], "summary": "…" }
```

- `summary` is set when the run made one, else `null`.
- It is written only when every eligible chapter has a recap. Sending the same book again
  overwrites its record.
- Series metadata is read as `inspect` reads it: `calibre:series`, `calibre:series_index`, or
  EPUB 3 `belongs-to-collection` with `group-position`. A book with no series number gets no
  record and no Previously page.
- Matching is on author and series name, both lowercased with whitespace collapsed. The record
  must have a lower number than the book being sent.

### 14.3 Generating it

- **Input:** each earlier book, in series order, as `Book {n}: {title}` followed by its summary,
  or by its recaps when it has no summary.
- **Prompt:** the system prompt is the summary one (§13.3). The user message:

  > Book: {title} · Author: {author} · Language: {lang} · Series: {series}, book {n}
  > These are summaries of the earlier books in this series, in order. Write a recap of what
  > happened in them for a reader about to start this book, in 2–4 short paragraphs, in this
  > book's language, past tense, including how each book ended. Do not judge or praise the
  > writing. {input}

- Nothing from book N's text is sent, which is what keeps the page free of spoilers for book N.
- **Long input on a local server:** parts, then a merge, as §13.3.
- **Cache:** `previously-{key}.txt`, keyed on the prompt version, the provider, the model and
  the input.

### 14.4 The page

`deckle-previously.xhtml`, next to the OPF:

```html
<section epub:type="prologue" id="deckle-previously"><h1>Previously</h1>
  <p><i>What happened in the earlier books of {series}: {Book 1: Title}, {Book 2: Title}.</i></p>
  <p>{paragraph}</p>…
  <p style="font-size: 0.8em; font-style: italic;">AI-generated summary ({model}), not part of the original book.</p>
</section>
```

- `epub:type="prologue"` is not `"footnote"` or `"note"`, so no reader hides it as a popup.
- It is added after the recap pass, so it gets no recap of its own. Its names count for Who's
  who and X-Ray, as with the summary (§13.4).
- A book that already has `deckle-previously.xhtml` keeps it.

### 14.5 Failures

- No earlier records: "Series recap skipped: no earlier book of {series} was sent with recaps".
- A failed request: "Series recap skipped: {error}".
- Earlier books missing (fewer records than the book's number minus one, for whole numbers):
  "Series recap covers {k} of {n} earlier books".
- None of these ever costs the book.

### 14.6 Tests

- Book 1 of a series, sent with recaps, leaves a record. Book 2 with `series` on then gets a
  Previously page that holds book 1's recaps. The page is before chapter 1 in the spine, before
  its entry in the nav, and has a NCX `playOrder` lower than chapter 1's, with later points
  renumbered. The request holds no text from book 2.
- Book 1's summary is used when its record has one.
- No record: a warning and no page. Another author's series of the same name doesn't match.
- Book 3 with only book 1 recorded: the page names book 1, plus the "covers 1 of 2" warning.
- `series` alone on a book without series metadata makes no requests.
- A failed request: a warning, and the book keeps everything else.
- A cached Previously page makes zero requests on a second run.
