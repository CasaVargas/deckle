# Word Wise notes (M4, 2026-10-19)

Working notes for `crates/deckle-wordwise`, the `data/wordwise` builder, and the Word Wise part of
the `enrich` stage. Authority: WordDumb and Proficiency (both GPL-3, by xxyzz), which have shipped
device-accepted Word Wise files for years. Confidence tags: **device** / **reference** /
**inferred**. Nothing here is **device** yet: see the runbook in `docs/devices.md`.

## The sidecar (reference)

`documents/<book>.sdr/LanguageLayer.en.<ASIN>.kll`, SQLite, next to the X-Ray sidecar. The book
must be **EBOK** and the ASIN must match the KFX metadata, exactly like X-Ray.

| table | columns | notes |
|---|---|---|
| `metadata` | `key, value` | `acr` = the KFX `asset_id`, `targetLanguages en`, `sidecarRevision 9`, `bookRevision` (empty for KFX), `sourceLanguage en`, `enDictionaryVersion 2016-09-14`, `enDictionaryRevision 57`, `enDictionaryId kll.en.en`, `sidecarFormat 1.0` |
| `glosses` | `start INTEGER PRIMARY KEY, end, difficulty, sense_id, low_confidence` | one row per occurrence; `start` = KFX position, `end` NULL for one word and the end position of a phrase; `low_confidence` 0 |

Positions are the X-Ray model (`deckle_kfx::text::content_runs`): run pid + code-point offset.

The `.kll` holds **no text**. `sense_id` is a row in a dictionary on the Kindle,
`system/kll/kll.<book lang>.<gloss lang>.klld`, picked by the Kindle's *Word Wise → Language*
setting, not by the `enDictionary*` rows (WordDumb writes Amazon's 2016 values in every mode;
Deckle does too).

## Two dictionaries (decision, M4 plan)

- **The Kindle's** `kll.en.en.klld` is Amazon's, downloaded by a registered Kindle the first time
  Word Wise opens. Store books point into it, so it is never written. When present, Deckle reads
  it at send time (mass storage: a file read; MTP: GetObject), maps each match's lemma + part of
  speech to Amazon's sense id (`senses.display_lemma_id → lemmas.lemma`, `pos_types.label`, lowest
  `sense_number`), and drops words Amazon's dictionary lacks. The file is read into a temp dir and
  never kept or shipped. Join columns are **inferred** from Proficiency's mirror of the schema; a
  schema probe falls back to Deckle's dictionary if a column is missing.
- **Deckle's** `.klld` (same layout as Amazon's, base64 definitions, metadata
  `deckleDictionary = <version>+<build>`) goes in the **Chinese slot** `kll.en.zh.klld`, as
  WordDumb's Wiktionary mode does. The reader sets *Word Wise → Language* to 中文 once; the hints
  are English. Deckle writes the slot only when it is empty or holds an older Deckle build, and
  never replaces Amazon's Chinese dictionary (Word Wise is then withheld with the reason). About
  40 MB, installed once per device; folder exports mirror it at `<out>/system/kll/`.

### Language must be exactly `en` (verified on a Scribe 2024, 2026-10-19)

The Kindle shows Word Wise (the widened line spacing) for an `en-US` book but draws no hints:
every gap stays blank. With the KFX `language` metadata set to exactly `en` the same sidecar
works. WordDumb forces `en` for the same reason. Deckle writes `en` for EBOK books that get Word
Wise and whose language is any English variant (`ConvertOptions::exact_language`); kfxlib would
otherwise keep `en-US`.

Senses: Amazon's `kll.en.en.klld` has ~7,000 definitionless placeholder senses (negative ids,
`sense_number` 0.0, `pos_type` 0); only senses with a `short_def` are used.

### MTP Kindles that hide `system/` (Scribe 2024, verified 2026-10-19)

Over MTP the Scribe shows `documents/`, `fonts/` and others but no `system/`; creating it fails
with SendObjectInfo → General Error (0x2002). Neither dictionary above is reachable there. Deckle
then looks for a **staged copy** of the Kindle's dictionary at `documents/deckle/kll.en.en.klld`,
which the reader copies once from `system/kll/` with an on-device file manager (KOReader on a
jailbroken Kindle). It is read like the real one (GetObject, temp dir, never kept). Without it,
Word Wise is withheld with a reason that says how to stage it; Deckle's own dictionary is not an
option because the Chinese slot can't be written. The copy goes stale only when Amazon updates
its dictionary, which shows as hints pointing at wrong senses: copy it again.

## Deckle's database (`deckle-wordwise-en-v1.sqlite`)

Built by `data/wordwise` (`deckle-wordwise-build <kaikki.jsonl.gz> <large_en.msgpack.gz> <out>
<version>`) from kaikki.org's English Wiktionary extract and wordfreq's `large_en` list in about
15 s. Published gzipped (`gzip -9 -n`) as a release asset of this repo (`wordwise-v1`), pinned by
SHA-256 in `deckle_core::wordwise::DB_SHA256`, downloaded on first use into the cache dir
(`~/Library/Caches/deckle`, `%LOCALAPPDATA%\deckle`, `~/.cache/deckle`).
`--wordwise-db` / `DECKLE_WORDWISE_DB` use a local file instead.

Schema: `metadata(format 1, version, build, …)`, `senses(id, lemma, pos, short_def, full_def,
example, difficulty)` with one sense per lemma + part of speech, and `forms(form, sense_id)`
mapping every lowercase surface form to one sense, so the app needs no lemmatizer or tagger.
`build` is a fingerprint of the sense rows: sense ids are valid for one build only, so the cached
and installed `.klld` are keyed on `version+build`.

2026-10-19 build (kaikki 2026-10-03, wordfreq master): 173,853 senses, 320,013 forms, 32.5 MB
raw, **12.1 MB** gzipped (budget was < 60 MB).

### Selection rules (inferred, tuned on Pride and Prejudice)

- Entries: `lang_code en`, POS noun / verb / adj / adv / phrase, lowercase lemma of 1–3 words and
  3+ letters. A word whose **first** entry is a function word (pron, prep, conj, det, …) is never
  hinted ("thee" is not "to address using the pronoun thee").
- Difficulty from wordfreq Zipf (phrases combine as 1/f = Σ 1/fᵢ): ≥ 4.25 too common (not
  hinted, and a surface form that common is never hinted either); 3.75 → 5, 3.25 → 4, 2.75 → 3,
  2.0 → 2, below → 1. 1 is the rarest (shown even at "fewer hints").
- One entry per lemma + POS: the etymology with the most usable senses ("bear" = carry, not the
  stock-market verb). A surface form maps to the richest entry that lists it, then to its own
  lemma ("ladies" → lady, not the poker hand; "bore" → bear).
- Sense: the first without archaic/dated/rare/historical/dialect/slang tags or a specialist topic
  (grammar, physics, law …), else the first. Senses that are only a spelling variant ("British
  standard spelling of behavior") are dropped; archaic, dated, literary and contracted spellings
  are hinted with the modern word ("connexion" → connection, "shew" → show).
- Short gloss: no parentheticals; the first `;` part (or the shortest meaningful one when the
  first is over 50 characters); the first `,` part unless it dangles ("An open, horse-drawn
  carriage", "Any well-bred, well-mannered, kind-hearted man"). Examples are not stored (Word
  Wise does not show them, and they doubled the dictionary).

### Matching (inferred)

Longest phrase first (up to 3 words, whitespace-joined only), case-insensitive, curly apostrophes
straightened. Skipped: all-caps tokens, capitalized words mid-sentence, and words the book uses
as names (capitalized mid-sentence more often than lowercase: "Darcy", "Bennet", "Mrs"). The
period of "Mr." / "Mrs." / "Dr." … does not start a sentence. Hint level *L* keeps difficulty ≤ *L*;
the Kindle's slider filters further, so the default is 5.

Pride and Prejudice (Gutenberg #1342, no images), level 5: **11,477 hints** (~9 % of words;
5,250 / 3,025 / 1,676 / 1,097 / 429 at difficulty 5 → 1). Top hinted words: aunt, assure,
gentleman, affection, acquaintance, civility, carriage, agreeable, scarcely, countenance,
amiable, disposition, recollect. Known weak spots: Wiktionary's first everyday sense is
sometimes the historical one ("carriage" → "the act of conveying"), and Gutenberg boilerplate
("copyright") gets hints.

## Out of scope for M4

Non-English books (the builder is language-parameterized, only `en` is built), POS tagging,
per-word overrides, Vocabulary Builder / Anki import.
