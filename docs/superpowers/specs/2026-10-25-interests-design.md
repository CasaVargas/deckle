# Your interests: sections that hunt, and a paper that notices — design

**Status:** approved by the owner on 2026-10-25 ("Spec approved. Write the implementation plan and start on increment 1.") · **Date:** 2026-10-25 · **Owner:** Jonathan (CasaVargas LLC)
· **Builds on:** `2026-10-22-newspaper-sections-design.md` (merged sources, `"lead"` full text,
section keys), `2026-10-22-local-places-design.md` (Google News search with `when:`),
`2026-10-22-readership-report-design.md` (`.yjf` read-back, suggestions applied with one click),
`2026-10-22-from-your-shelf-design.md` (clippings and shelf read on the same connection),
`2026-10-24-daily-llm-editor-design.md` (one bounded completion, what may enter a prompt),
`2026-10-22-many-papers-design.md` (per-paper files and state).

Today a section is a bucket: a Google News topic (SPORTS, TECHNOLOGY) or one RSS feed. An
**interest** is a thing you actually care about, in your words: "Carolina Panthers", "Rust",
"e-ink readers", "Moore County Schools". Each interest becomes its own section, and every
morning the paper goes hunting for it across the wires. And because Deckle already reads back
what you tapped and highlighted, the paper **notices** what you keep reading about and offers it
as a new interest, one click to accept.

## 1. Intent and why

- **Why:** the owner said it exactly: "I care about the Panthers, Rust, e-ink, and Moore County
  schools, and the paper goes and finds those stories across every source, every morning." A
  topic feed gives you everyone's sports; an interest gives you yours.
- **Why both halves:** typed interests are the floor; the learning half is what makes it feel
  like a paper that knows you ("magically both", owner, 2026-10-25). Learning stays passive
  and local, and never changes the paper without a yes.
- **What's unique:** nothing leaves the Mac. The signals are the Kindle's own reading position
  and clippings, read over the cable Deckle already uses. No account, no tracking pixel, no
  recommendation server.

## 2. Goals, non-goals, anti-goals

**Goals**

1. `[[interests]]` entries in a paper's TOML, one line each, each **its own section** (owner's
   decision, 2026-10-25), in the paper where `"interests"` sits in `sections`.
2. Each interest draws on a Google News search built from its name (plus optional extra phrases
   and feeds), merged, deduplicated, filtered for relevance, newest first, cut to a budget. One
   source failing leaves the others.
3. Interest sections are news sections: the top story in full, the rest briefs, section fronts
   and back links, a lead that can head the front page, readership per section, a row in the
   page plan.
4. **Learning:** from what you tapped into, what you highlighted in the paper, and what's on
   your shelf, Deckle keeps **candidates**; one that recurs becomes a **suggestion** on the Daily
   tab ("You read 4 stories about Bryce Young in two weeks"), accepted or dismissed in one click.
   Accepting writes the `[[interests]]` entry, comments kept.
5. With the optional editor on, relevance is judged, not matched: the day's catch for each
   interest is scored and ordered by how much it's about the interest, and candidate subjects
   get a proper name and search query.
6. **Nudges (stretch, off by default):** the paper itself carries the suggestions, each a tiny
   page; tapping one on the Kindle counts as "yes" at the next connection.
7. CLI parity: `deckle digest interests` to list, add, remove, see and accept suggestions, with
   `--paper`.

**Non-goals**

- Learning from books: shelf *titles and authors* can seed a candidate; the text of your books
  never does. Highlights in books are not read for subjects (only the paper's own highlights).
- Ranking stories *across* the whole paper by interest; interests add sections, they don't
  reorder Sports.
- Near-duplicate detection (same story, different headline) beyond what the editor already does.
- Social or shared interests, trending topics, anything fetched about *other* readers.
- Auto-accepting a suggestion, ever (nudges excepted, because a tap is a yes).

**Anti-goals**

- A thicker paper by default: eight interests must not mean a 100-page edition.
- A section full of near-misses ("Panthers" the hockey team, "Rust" the fungus).
- Anything about your reading leaving the Mac without the editor explicitly on, and even then
  only headlines and excerpts of *public news*, never highlights, shelf contents or clippings.
- Breaking an existing `digest.toml` or paper file.

## 3. Config (all additive)

```toml
sections = ["reading", "weather", "local", "interests", "feeds", "shelf", ...]

[[interests]]
name = "Carolina Panthers"
# query = "\"Carolina Panthers\" OR \"Panthers\" NFL"   # the Google News search; default: the name, quoted
# also = ["Bryce Young", "Dave Canales"]      # more searches, merged; a story matching one is relevant
# not = ["Florida Panthers", "hockey"]        # a story whose title or excerpt has one is dropped
# feeds = ["https://example.com/panthers.rss"] # feeds merged in
# max_items = 5                                # stories; default 5
# full_text = "lead"                           # "lead" (default), true or false
# max_age_hours = 72                           # default 72: interests move slower than the wires

[[interests]]
name = "Rust"
query = "\"Rust\" programming OR \"Rust language\" -game"
not = ["corrosion", "Rust Belt"]

[learning]                  # what the paper notices; all local
# enabled = true            # default true; false: no candidates, no suggestions, no nudges
# window_days = 14          # the window a subject must recur in
# min_stories = 3           # distinct stories about it, across at least 2 editions
# nudges = false            # §7: the suggestions as tappable pages in the paper itself
```

- **Section kind:** `SectionKind::Interests`, `"interests"` in `sections`. Like `Feeds`, it stands
  for every `[[interests]]` entry in file order. Missing from `sections` with entries present:
  the entries are not printed and the build notes "You have interests but \"interests\" isn't in
  sections". **My call:** the starter puts `"interests"` after `"local"` and before `"feeds"`, so
  a Panthers story goes to your Panthers section and not to Sports (the existing rule: the first
  section to print a title keeps it; §4).
- **Key:** `interests:<name>`, next to `feeds:<name>` and `local:<name>`. Readership, the page
  plan and layouts need only the new prefix.
- **`name`** is required and unique within a paper (case-insensitive); a duplicate is a config
  error naming both. It's the section title in the paper.
- **`query`** defaults to the name in quotes. It's passed through `google_news_url`, which appends
  `when:<days>` from `max_age_hours`. Google's own operators (`OR`, `-word`) are allowed as-is.
- **`also`** are extra searches, each run through `google_news_url` too, and extra *relevance
  phrases* (§4).
- **`not`** drops a story when its title (or a feed item's description, or the fetched lead's first
  600 characters of text) contains a phrase, case-insensitive, at word boundaries.
- **`feeds`** merge in like a `[[feeds]]` entry's `urls`.
- `[learning]` is one table per paper, left out of a save at its defaults, like `[puzzle]`.
- **Per paper:** `[[interests]]` and `[learning]` are valid in `digest.toml` and in every paper
  file; a paper's interests are its own and its learning state lives in `papers/<slug>/`.
- **Saving** keeps comments and the new fields through `config::save`, with round-trip tests like
  `[[feeds]]`' and `[[local.places]]`'.

## 4. Gathering an interest section

For each `[[interests]]` entry, in `gather_with_editor`, where `SectionKind::Interests` falls:

1. **Fetch** every source: the `query` search, each `also` search, each feed. Each is one
   request through `Net`. This is a new gatherer, `interest_section`, a hybrid of the two that
   exist: `local_news` (search + feeds merged, but no `"lead"` mode and no `printed` handling)
   and `feed_section` (`"lead"` and `taken`, but no search). **My call:** lift the shared
   merge-and-dedupe into one helper the three call, rather than copying it a third time. One
   source failing is noted; all failing makes the section "Unavailable today" with the usual
   note.
2. **Merge and deduplicate** by title (the `feed_section` rule), then drop anything already in
   `printed` (a title printed by Local, a place or an earlier section).
3. **Relevance, no editor:** keep a story only if its **title** (plus the feed description, when a
   `feeds` item has one; Google's search items carry none, `as_briefs` clears it) contains the
   `name` or an `also` phrase (case-insensitive, word boundaries; a phrase of one word must
   match a whole word, so "Rust" doesn't match "trust"), and none of the `not` phrases. Google's
   search is loose; this is what keeps the Florida Panthers out. **My call, not fetching every
   candidate first:** that would be one page fetch per search hit per interest, eight times
   Local's cost for eight interests. The lead is the exception (step 5).
4. **Order** newest first, cut to `max_items`.
5. **Full text** follows `full_text`, default `"lead"`: the top story whole, the rest `as_briefs`.
   The fetched lead is checked again against `not` and relevance on its first 600 characters of
   text; if it fails, it's dropped and the next story is fetched instead, at most three fetches
   per section. Photos as for any feed section.
6. **A quiet day.** Zero relevant stories and no source failure: the section is **left out of the
   edition** (not "Unavailable today"; nothing broke, there was just no news), with a runner note
   "Nothing new about Rust today." Readership's `seen_in` already handles a section that skips
   an edition. **My call;** the alternative, printing an empty section, is exactly the thin
   paper the anti-goals forbid.
7. **Section shape:** `Section { key: "interests:<name>", title: name, kicker: Some("Your
   interests") , .. }`; pushed into `news`, so the lead can head the front page and the editor
   may deduplicate across sections as it does for `feeds:`.

**With the editor on** (§6), step 3 still runs (it's cheap and the `not` list is the reader's
word), and the editor re-scores what's left.

**Budget.** Default `max_items` 5 and `"lead"` full text: eight interests fetch eight whole
stories and 32 briefs. The page plan shows each interest as a row with its budget, like a feed.

## 5. Learning: candidates and suggestions

All of this runs on the Mac, at connect time (the `watch`/sync pass that already reads `.yjf`
files) and at build time (for the shelf). Today `My Clippings.txt` is read only when highlights
export or the shelf section asks for it (`Plan::clippings` in `deckle-core`'s sync); **learning
adds its own reason**: when `[learning] enabled` is true for a paper that goes to this Kindle,
the plan reads clippings too. It's one more `||` in the plan, honoured by the per-device
overrides like the others. It writes `interests.json` beside
`readership.json` (per paper, in `papers/<slug>/` for extra papers).

### 5.1 Signals

| Signal | Where it comes from | What it says |
|---|---|---|
| **Taps** | the edition's `.yjf` `page.history.store` positions, which are the **origins** of jumps (readership design §1: they landed on front-page items), mapped through the layout to the story the tapped tile or headline links to | you chose this story from a front |
| **Read to the end** | a history origin inside a story's body: the only link there is "back to the front/section" at its end | you finished it and went back |
| **Paper highlights** | `My Clippings.txt` entries whose book title is an edition's full title, matched to the story whose body contains the clipped text | you cared enough to mark it |
| **Shelf** | the shelf snapshot: books in progress, their author names | a person you're reading |

Not a signal: `fpr` alone (reading straight through reaches every story), words read,
highlights in books, dictionary look-ups.

### 5.2 What has to exist for taps

- **`Positions` gains `history: Vec<i64>`**, parsed from `page.history.store` (readership design
  §1 shows the field; the parser today reads `fpr`, `lpr`, `book.info.store` and the pace
  samples). A `.yjf` without it: empty.
- **The history holds origins.** The readership spec's hardware note (2026-10-22, Scribe) found
  the positions "land on front-page items", i.e. the Kindle records where a jump started. The
  design assumes that; the builder confirms it on hardware **before increment 2**, with one
  edition, three deliberate taps and a look at the `.yjf`. If the Kindle turns out to record
  destinations, the mapping below flips to story starts (the layout records both anyway) and
  nothing else changes.
- **`Layout` gains link origins:** `SectionStart.links: Vec<LinkStart { start, end, target }>`
  for the front page (each tile) and each section front (each headline): the KFX position range
  of the link text and the `interests:<name>`/`feeds:<name>` key plus story index it leads to.
  Each story also records its own range (`stories: Vec<StoryStart { title, start, end }>`), so a
  history origin inside a story body is "read to the end". KFX sections are paper sections (one
  XHTML file per section), so `section_first_runs` can't see these; the builder scans
  `content_runs` within each section's range and matches the runs to the rendered link and
  heading texts. Layouts recorded before this have no links and contribute no taps.
- **A tap** is a history origin inside a tile's or headline's range. The target story is the hit.
  Page-through never enters the history, so plain reading can't look like a tap.

### 5.3 From stories to subjects

Offline, deterministic, per paper:

1. Take the titles (and, for paper highlights, the clipped sentence) of every tapped or
   highlighted story in the window.
2. Extract **phrases**: runs of capitalized words (allowing "of", "the", "and", "de" inside;
   "Moore County Schools", "Bryce Young"), and single capitalized words of 4+ letters. Lowercase
   the key for counting; keep the most common spelling for display.
3. **Stoplist:** weekday and month names, the paper's title and place, outlet names seen in
   `meta`/`outlet`, every section title, every existing interest's `name`/`also`/`not`, phrases
   with a dismissed candidate's key, and a short built-in list (Breaking, Opinion, Live, Watch,
   Photos, Video, Update, Report).
4. A **candidate** records the phrase, its hits `(date, section key, story title)`, first and
   last seen. Hits older than `window_days` fall off at the next connection.
5. A candidate with `min_stories` distinct stories across at least 2 editions becomes a
   **suggestion**. Top 5 by hit count, ties by recency.

**Shelf candidates:** each author of a book in progress is a candidate with a single synthetic
hit per edition the book was in progress, so an author you've been reading for two weeks
qualifies; the suggestion reads "You've been reading Brandon Sanderson for two weeks."

### 5.4 Suggestions in the app

On the Daily tab, in the Readership card beside today's "Drop it" and "Move up" rows, a
**"Deckle noticed"** list:

> **Bryce Young** · You tapped into 4 stories about Bryce Young in the last two weeks.
> [Add as interest] [Not interested]

- **Add as interest** writes `[[interests]] name = "Bryce Young"` to the paper's file through
  `config::save`, adds `"interests"` to `sections` before `"feeds"` (or at the end if no
  `"feeds"`) when it's missing, marks the candidate accepted in `interests.json`, and refreshes
  the card and the page plan. A shelf candidate's entry gets `query = "\"Brandon Sanderson\"
  author OR book"` (my call; the plain name would pull in anyone with that name).
- **Not interested** records `dismissed[key] = today`; the phrase stays on the stoplist for 90
  days (my call), then may return if it recurs.
- Nothing here is automatic. No suggestion changes the paper until clicked.

**Which paper:** the desktop is the main paper's today (many papers PR 4, "Your papers", hasn't
landed). Increments 1 and 2 put the Interests card and "Deckle noticed" on the main paper; extra
papers get interests and suggestions through the CLI (`--paper`), as the many-papers spec did
for its own features. When PR 4 lands, the cards follow the selected paper. The Tauri commands
take an optional slug from day one so that's a UI change only.

An **Interests card** on the Daily tab, next to Topics and feeds: each interest as a chip with
its name; a text field "Add an interest…" (Enter writes the entry with defaults); a chip opens a
small editor for `also`, `not`, `query`, the budget, and the name; remove deletes the entry. Order follows
the file; drag reorders through the existing page-plan move. **Renaming** changes the key
(`interests:<name>`), so the app migrates the old key to the new one in `readership.json` and
`interests.json` in the same save (my call: keep the history rather than forbid the rename).

### 5.5 CLI

```text
deckle digest interests [--paper <slug>]            list interests and pending suggestions
deckle digest interests add "<name>" [--query Q] [--also P]... [--not P]...
deckle digest interests remove "<name>"
deckle digest interests accept "<phrase>" | dismiss "<phrase>"
deckle digest interests why "<name>"                 the stories behind a candidate or interest
```

## 6. With the editor on

The one bounded completion per edition gains two operations, same request, same validation:

- **Relevance:** for each interest section (`interests:` key, in `news`), each story's id, title
  and excerpt plus the interest's `name` and `also` phrases. The model returns `{id, score}` with
  score 0, 1 or 2 (not about it / mentions it / about it). Stories scoring 0 are dropped; the
  rest are ordered by score, then recency. Only ids that exist count; a missing id keeps its
  keyword order; any invalid answer leaves the section exactly as §4 built it. The lead (full
  text) is re-chosen after ordering only if the new first story already has full text; otherwise
  the order changes and the full story stays where it is (fetching more is out of bounds for the
  editor).
- **Naming:** pending **paper** candidates (phrase, up to 5 example headlines each, at most 10
  candidates) go in; the model returns `{key, name, query}`: a clean display name ("Bryce Young"
  for "Young", "Carolina Panthers' quarterback" for a cluster) and a search query. Stored on the
  candidate; the suggestion shows the clean name and **Add as interest** uses the query. Invalid
  or missing: the phrase and the quoted phrase, as today.
- **Never in the prompt:** shelf candidates (an author's name came from your Kindle), clipped
  text, anything from `[reading]`, calendars, highlights. The test that proves private sections
  never enter prompts extends to these.
- Limits join the existing ones: 10 candidates × 5 headlines; interest stories count toward the
  80-story cap. Over the cap: the editor skips relevance for the paper (keyword order stands)
  rather than scoring half of it.

## 7. Nudges in the paper (stretch, `[learning] nudges = true`)

A short page **"Deckle noticed"** after the last interest section (or after Local when there are
no interests yet): up to 3 pending suggestions, each a line:

> You keep reading about **Bryce Young**. *Tap here* to make it a section.

Because the history records origins (§5.2), each *Tap here* is its own link with its own range
in the layout (`nudge:<key>`), all leading to one small confirmation page: "Noted. It becomes a
section the next time your Kindle connects. Change your mind on the Daily tab." At the next
connection, a history origin inside a nudge link's range is an **accept**: the entry is written
exactly as the app's button would write it, and the runner notes "Added Bryce Young to your
interests (you tapped it in Thursday's paper)."

- Page-through can't accept: only jumps enter the history.
- Off by default until the §5.2 hardware check has passed and a nudge tap has been seen in a
  `.yjf`. If the Kindle records destinations instead, each nudge gets its own tiny page (the
  destination) and the rule becomes "a history position inside a nudge page".
- Never shown in another paper's edition than the one whose candidates they are.

## 8. Where things show up

- **The paper:** each interest a section with its front, stories, back links; a lead among the
  front-page headlines; kicker "Your interests". The nudge page when enabled.
- **The cover and masthead:** unchanged (an interest's lead competes for the lead like any news
  section's).
- **Daily tab:** the Interests card; "Deckle noticed" rows in Readership; interest rows in the
  page plan; the `[learning]` switch in The paper, with one line: "Deckle keeps track of what you
  tap and highlight in the paper, on this Mac, to suggest new interests." Editor opt-in text
  grows one clause: "…and which stories are about your interests".
- **Edition bundle and PDF:** interest sections are ordinary news sections; nothing new.
- **docs/daily.md:** "### Your interests" after "Topics and feeds"; "### Deckle noticed" after
  "How far you read".

## 9. Constraints and risks

- **Google News search quality.** Loose relevance is why §4.3 exists. The query default (quoted
  name) is conservative; `also` widens, `not` narrows. Verify live on day one that quoted
  phrases and `OR` survive URL encoding in `google_news_url`.
- **Interstitial resolution** (free-sources note): search results need the `batchexecute` hop
  for full text; already in place for Local and topics.
- **KFX link and heading matching** (§5.2) is text matching against `content_runs` inside a
  section's range. A tile whose text isn't a distinct run gets no range and no taps; the layout
  must stay valid with partial link lists.
- **`page.history.store` depth.** The Kindle keeps a handful of jumps. Deckle reads at every
  connection, so a daily reader loses little; a weekly plug-in loses taps. Noted in docs.
- **Clippings matching:** a highlight's text may span a paragraph break; match on the first 80
  characters of the clip against the plain text of each story in that edition.
- **Phrase extraction** is English-capitalization-based; a lowercase subject ("e-ink") only
  arrives as a typed interest. Acceptable for v1; say so in the doc.
- **Per-paper Kindles:** taps are attributed to the paper whose edition they came from, so Dad's
  taps feed Dad's suggestions even on a shared Mac.

## 10. Testing

Fixtures: RSS bodies for searches and feeds (`tests/fixtures`), a `.yjf` with
`page.history.store` captured from the Scribe (builder captures one; the parser test uses it), a
`My Clippings.txt` with a paper highlight, a built edition KFX.

- **Config:** parse the starter with the commented block uncommented; duplicate names error;
  `query` default; round-trip keeps comments and fields; `[learning]` left out at defaults;
  interests in a paper file load and save on that file.
- **Gathering:** merge + dedupe + `not` + relevance (word boundaries: "trust" ≠ "Rust"); newest
  first and budget; `"lead"` fetches one whole story; a failing source leaves the others; all
  failing → Unavailable; zero relevant stories → section absent + note; a title printed by Local
  isn't reprinted; key and kicker; the section is in `news` and its lead can head the front page.
- **Layout and taps:** story starts from a built KFX; a history position in story 3 of a section
  is a tap, one in story 1 isn't; layouts without story starts yield no taps.
- **Candidates:** extraction on a fixed set of titles (phrases, stoplist, dismissed); window
  roll-off; `min_stories` across 2 editions; shelf author candidates; top-5 ordering; accept
  writes the entry, adds `"interests"` to `sections` before `"feeds"`, keeps comments; dismiss
  hides for 90 days.
- **Editor:** disabled never calls; relevance drops 0s and reorders; invalid ids keep keyword
  order; naming updates the candidate; shelf candidates and clipped text never appear in the
  request body (assert on the serialized prompt); over-cap skips relevance.
- **Nudges:** page rendered only when enabled and suggestions exist; a history position in a
  nudge page accepts exactly once; the next edition no longer shows it.
- **App:** `dailyRooms`/`readershipView` tests for the new rows and card state; Tauri commands
  `interests_add/remove` return the refreshed `DailyState` (they edit config) and
  `interests_accept/dismiss` return the refreshed `ReadershipView` plus `DailyState` (accept edits
  config too), following the existing `readership_apply` pattern.
- **CLI:** each subcommand against a temp paper, `--paper` picks the file.

## 11. Delivery in increments

1. **Interest sections** (§3, §4, Interests card, page plan rows, CLI add/remove/list, docs).
   Useful on its own; this is the floor.
2. **Learning** (§5): the origins-or-destinations hardware check first, then
   `page.history.store`, link and story ranges, candidates, suggestions in the app (main paper),
   CLI accept/dismiss/why.
3. **Editor relevance and naming** (§6).
4. **Nudges** (§7), after the hardware check, behind the flag.

Each lands as its own PR against `main` with `Closes #<issue>` for its own issue.

## 12. Calls I made (for the owner)

1. Interests sit before Feeds in the starter, so your interest wins a story over the topic
   section.
2. A quiet day leaves the section out rather than printing an empty one.
3. Default budget 5 stories, `"lead"` full text, 72-hour window.
4. Learning is **on** by default (it's local and does nothing without a click); nudges **off**.
5. `min_stories` 3 across 2 editions in 14 days; dismissed phrases rest 90 days.
6. Shelf authors are candidates; book text and book highlights are not.
7. Shelf candidates never go to the editor; paper candidates (headlines only) may.
8. Relevance runs on titles (and the fetched lead), not on a fetch of every search hit.
9. The app's interest UI is main-paper-only until many-papers PR 4; extra papers use the CLI.
10. Learning turns on the clippings read by itself; it doesn't wait for highlights export.
11. Renaming an interest migrates its readership and candidate history to the new key.

## 13. Left to the builder

- The `page.history.store` wire format inside the `.yjf` (KRDS); the origins check (§5.2).
- How link and heading runs are found in the KFX, and the fallback when one isn't.
- Exact phrase-extraction rules and the built-in stoplist.
- The JSON shape of `interests.json`.
- Whether `interests` rows in the page plan reuse the feed row component or get their own.

## 14. Implementation notes, PR 1 (2026-10-25)

Increment 1 (§11 item 1) landed from `docs/superpowers/plans/2026-10-25-interests-sections.md`.

- **Config** (§3) as written: `InterestConfig`, `LearningConfig` (parsed and saved now, used by
  increment 2), `SectionKind::Interests`, key `interests:<name>`, duplicate names an error,
  defaults filled by `normalize` (5 stories, 72 hours, `"lead"`). `Config::ensure_section` puts
  `"interests"` before `"feeds"` for the CLI and the app. The serializer writes a new
  `interests` key inline (`interests = [{ … }]`), as it does every new key; a hand-written
  `[[interests]]` block keeps its shape and comments.
- **Gathering** (§4): `sources/interests.rs` with `contains_phrase`, `relevant` and
  `interest_section`; the search-plus-feeds merge was lifted out of `local_news` into
  `fetch_merged`, which Local and the places now share. The lead is re-checked on its first 600
  characters of text and a failing one is dropped (not demoted). Quiet days return `None` and a
  note; interest sections arrive keyed, and the gather loop leaves keyed sections alone.
- **Elsewhere:** the editor's section filter includes `interests:`; readership rows, Move up and
  Drop; the page plan's rows (with `interest` index), `taken` pass and lead candidates; the live
  cache fetches each interest's searches and feeds.
- **CLI:** `deckle digest interests [add|remove]`, `--paper` aware, saving to the paper's own
  file.
- **Desktop:** main paper only (call 9). "Your interests" card with name, also, not and an
  add field; interest rows in the page plan with budgets and block reordering.
- **Not yet:** the `[learning]` switch in the app (increment 2), the editor's relevance scoring
  (increment 3), nudges (increment 4), and the rename migration of call 11 (renaming an interest
  in the card changes its key; its readership history stays under the old key until increment 2,
  which adds `interests.json` and migrates both files in one save).
- **After review (2026-10-25):** Move up applies to interests; the card has a search (`query`)
  field; blank names are not duplicates; names are trimmed in `normalize` so the key, the page
  plan and readership agree. `fetch_merged` changed two Local details in passing: a failing feed
  is reported by host rather than full URL, and of a repeated story the newest copy is kept.
- **Live check (2026-10-25):** one real build with `name = "Carolina Panthers", also =
  ["Bryce Young"]` and `name = "Rust", not = ["Rust Belt"]`. The Panthers section filled its
  five slots with stories naming the team or the quarterback; the quoted query and `when:3d`
  survived URL encoding. The Rust section filled with the other Rusts (a surname, corrosion, the
  film): a one-word name with everyday meanings needs its own `query` (`"Rust" programming`),
  which the starter and docs say. Increment 3's editor scoring is the better answer.
