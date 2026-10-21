# X-Ray sidecar notes (M3, 2026-10-19)

Working notes for `crates/deckle-xray` and the `enrich` stage in `deckle-core`. Authority:
WordDumb (GPL-3), which has generated device-accepted X-Ray files for years, and kfxlib for the
position model. Confidence tags: **device** / **reference** / **inferred**.

Any source gets X-Ray, not just EPUB: Word documents, web articles and long reads are turned into
an EPUB before the shared stages, and `enrich` reads the text from the KFX (see
`docs/superpowers/specs/2026-10-21-source-agnostic-enrichment-design.md`; PDF is #18, Markdown #23).

## Where it lives (reference)

- `documents/<book>.sdr/XRAY.entities.<ASIN>.asc`, next to `documents/<book>.kfx`. The `.sdr`
  folder name is the KFX file name with the extension swapped.
- The book must be **EBOK** and the `<ASIN>` in the file name must equal the ASIN stamped in the
  KFX metadata. PDOC books never get X-Ray, so Deckle only builds the sidecar when the cover rule
  (design §5) or an override ships the book as EBOK.
- Deckle writes the book first, then the `.sdr` folder (mass storage: plain files; MTP:
  `ensure_folder` + `send_file`, replacing a same-named file). A Kindle indexes the sidecar when
  it indexes the book.

## Schema (reference, WordDumb)

SQLite, `PRAGMA user_version = 1`. Tables:

| table | columns | notes |
|---|---|---|
| `book_metadata` | `srl, erl, has_images, has_excerpts, show_spoilers_default, num_people, num_terms, num_images, preview_images` | one row: `srl 0`, `erl` = end position of the text, flags 0, counts |
| `type` | `id, label, singular_label, icon, top_mentioned_entities` | `1 = People (14/15)`, `2 = Terms (16/17)`; `top_mentioned_entities` = comma-separated top-10 entity ids by count |
| `entity` | `id, label, loc_label, type, count, has_info_card` | `id 0` is the fixed `loc_label 1, has_info_card 0` row; real entities from 1 |
| `entity_description` | `text, source_wildcard, source, entity` | one per entity |
| `source` | `id, label, url, license_label, license_url` | fixed rows `0, 1, 2`; Deckle uses `0 = Deckle` (quote), `1 = Wikipedia`, `2 = MediaWiki` |
| `occurrence` | `entity, start, length` | one per mention, `start` = KFX position, `length` in characters |
| `excerpt`, `entity_excerpt` | | present but empty (`has_excerpts 0`) |
| `string` | `id, language, text` | the UI strings from WordDumb's `x_ray_strings.json` (ids 0–20, 1000, 1001) plus `21` = Wikipedia URL template, `22` = MediaWiki URL template |

Indices on `entity(type)`, `entity_excerpt(entity)`, `occurrence(start)`. `PRAGMA optimize` at
the end. Byte layout does not matter; the device reads it with SQLite.

## Positions (reference, kfxlib; parity proved by page numbers)

KFX text lives in storyline content runs. `deckle_kfx::text::content_runs(kfx)` walks the
`$145` content fragments, the `$259` storylines (eid → text) and `$265` (pid per eid) and
returns `Vec<Run { pid, text }>` plus `erl` = the larger of the last `$265` position and the
end of the last text run (pid + length in code points), so no occurrence lies past it. An
occurrence's `start` is `run.pid + char_offset` and `length` is the label's length in code
points; both count characters, not bytes.

## Entity extraction (inferred, tuned on Pride and Prejudice)

`deckle_xray::extract::heuristic(runs, lang)`:

- Capitalised n-grams of 1–3 words; honorifics (`Mr. Mrs. Miss Lady Sir Dr. Colonel …`) bridge a
  `. ` gap and start a name; a plain word before an honorific ("Consider Mr. Collins") does not.
- Stop lists: function words, sentence adverbs, speech verbs, numbers, days, months, front and
  back matter ("Chapter", "Project", "Gutenberg", "Illustrations" …). All-uppercase tokens
  (drop-cap remnants such as `HEN`) are skipped. Possessives are stripped.
- Noise filter: a single-word label whose lowercase form occurs at least half as often as the
  capitalised one is a common word at sentence starts ("Allow", "Having"), not a name. "Paris"
  never appears lowercase, so it stays.
- Aliases: honorific forms and full names are canonical; a bare surname ("Darcy") joins
  "Mr. Darcy" when that form has 3+ mentions and at least a third of the token's count; a token
  too rare to stand alone joins any matching form. One-off full names ("Elizabeth Bennet" once)
  are not entities but count as mentions of each recurring word in them.
- Minimum 2 mentions. **Person** when the cluster has an honorific form or the name sits next
  to a speech verb ("said", "cried", "laughed" …); otherwise **Term**.
- Description fallback: the sentence of the first mention (spoiler-free by construction); a
  `.` after an honorific or a single initial does not end the sentence; capped at 300
  characters with an ellipsis.
- English-only stop lists and capitalisation rules for now: German/Dutch capitalised nouns
  flood the Term list and CJK text (no case) yields no entities. Non-English books get the LLM
  pass or nothing useful.

Quality on Pride and Prejudice (Gutenberg edition with a long preface), no network:
62 people, 88 terms; the top 25 are all real (Elizabeth, Mr. Darcy [Darcy], Jane, Mr. Bingley,
Mr. Wickham, Lydia, Mrs. Bennet, Mr. Collins, Lady Catherine, Longbourn, Netherfield, Meryton,
London, Pemberley, Rosings, Hertfordshire …); noise remains in the long tail (preface names,
"Ladyship", Gutenberg boilerplate). The LLM pass is what cleans the tail.

## LLM pass (design §7)

`deckle_xray::llm::describe_all` sends the candidates (label, count, first-mention sentence; at
most 120) in batches of 25, one request each, and asks for a JSON array
`[{name, kind, description, aliases}]`: spoiler-free 1–2 sentence descriptions in the book's
language, `person`/`term`, aliases among the listed names, and an empty description to drop
noise. Anthropic Messages (`/v1/messages`, `x-api-key`, `anthropic-version 2023-06-01`) or any
OpenAI-compatible chat endpoint (`<base>/chat/completions`, `Authorization: Bearer` when a key
is set; none for local servers). The JSON array is extracted from a possibly fenced or chatty
answer. A failed batch keeps the basic descriptions for its entities and adds a warning; the
other batches still apply. The prompt names the book's author (a first-person narrator is
usually the author), says any individual is a person, and lists sentence-start words and
interjections ("Yeah", "Hey") as noise. It does **not** send the heuristic's person/term guess:
qwen3:8b copied it even when told it was often wrong (brothers, a bodyguard and the author all
came back as terms), and without it the same batch was classified correctly, and faster.

**Local models (decision 2026-10-19, measured with Ollama + qwen3:8b on an M4).** One request
for the whole book failed in practice: Ollama loads models with a 4,096-token context by default
(its OpenAI endpoint cannot raise it), so the 120-entity prompt was truncated and only 46
entities were answered; the full answer would also need ~15 min at 16 tok/s against the 180 s
timeout. Batches of 25 fit a small context and take ~55 s each; the 180 s timeout applies per
batch. OpenAI-compatible requests to a non-default endpoint also send
`reasoning_effort: "none"` (qwen3 otherwise "thinks" first, roughly doubling the output); a
server that rejects it with a 4xx gets the batch again without it. OpenAI's own endpoint and
Anthropic never get the field.

Keys: `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`, or the API key field in the desktop settings
(stored in Deckle's settings file; the keychain is a follow-up). Defaults: `claude-sonnet-5-5`,
`gpt-5-mini`. `max_tokens` scales with the candidate count (1024 + 120 per entity, max 16384);
a `max_tokens` / `length` stop is reported as "answer truncated" rather than "not JSON".

**Deferred from design §7 (decision 2026-10-19):** no per-book token budget and no cost
estimate before the first run yet. A book sends at most 120 candidates (the rest keep their
basic quote), a few thousand input tokens in all and well under a cent on current models; the
estimate and budget come back when longer books make the cost visible. (Chunking, deferred in
the same decision, is now the batching above.)

## Wikipedia (opt-in)

`https://<lang>.wikipedia.org/api/rest_v1/page/summary/<Title_with_underscores>`; the `extract`
of a `type: standard` page replaces the quote for **Terms** whose description is still the
basic one; disambiguation pages and misses are ignored. Result line says `wikipedia` when it
contributed and no LLM ran.

## Result line

`X-Ray: <people> people, <terms> terms (basic | wikipedia | llm)` on the CLI and in the GUI
row. A book with no recurring names gets no sidecar and the warning
`X-Ray: no recurring names found in this book; no sidecar written`.

## Still to verify on hardware (owner)

- Paperwhite (mass storage): after `deckle run book.epub --to device --cde ebok`, open the book,
  long-press a name → the X-Ray card shows Deckle's description; the X-Ray menu entry lists our
  people and terms. Record the result in `docs/devices.md`.
- Scribe (MTP): same, after spike 2 (real ASIN) passes, with and without a real ASIN.
