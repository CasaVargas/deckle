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
| Local Ollama, ~8B model | $0 | Owner installs Ollama and a ~5 GB model; roughly 10–15 min per novel on an M4 (estimate) |

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

- **Book summary.** A back-matter "Summary" page (spoilers allowed: it's after the end), made by one
  request over the cached chapter recaps — no second pass over the book. A front-matter version
  would have to be premise-only and mostly duplicates the publisher's description, so it's off the
  table unless the owner asks.
- **Series recap.** A front-matter "Previously" page in volume N, made from the cached recaps of
  volumes 1…N-1 found through the library's series metadata. Needs those volumes to have been sent
  with recaps on (or a "recap this book only" action).

Each gets its own spec section and increment.

## 10. Increments

1. **This PR:** this spec; `deckle_core::recaps` with tests (mocked `Post`); Kindle pipeline
   wiring behind `RunOptions.recaps`; CLI `--recaps`.
2. Other readers (`book_epub`, the Inbox, the wireless reader server) and the desktop setting and
   toggle, with the §8 copy.
3. Book summary. 4. Series recap.

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
