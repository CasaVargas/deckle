# Newspaper sections and a front page that reads like one — design

**Status:** draft for owner review · **Date:** 2026-10-22 · **Owner:** Jonathan (CasaVargas LLC)
· **Builds on:** `2026-10-22-local-places-design.md` (section keys, merged sources),
`2026-10-22-tomorrows-paper-design.md` (page plan, lead rule), `2026-10-22-readership-report-design.md`
(section keys).

The Daily should read like a real newspaper: topic sections (World, Sports, Technology,
Entertainment, …), a front page laid out like the cover with a tappable lead and headlines, and
a way to tap into a story and get back to the page you were on.

## 1. Intent

- **Why:** today a feed is one URL, so a "Sports" section is whatever one RSS feed gives, and the
  front page is a plain list of links. Neither feels like a paper.
- **Goals**
  - Topic sections that are one line of config (or one click in the Daily tab).
  - A section can draw on several sources, merged and deduplicated.
  - A front page shaped like the cover: lead with its opening paragraph, two columns of
    headlines (the top story of each section), then the Inside index.
  - Each section opens with its own small front: a headline list linking to its stories.
  - Every story ends with links back to the section front and to the front page tile it was
    reached from.
- **Non-goals:** raster or image-based front pages (a possible later level 2); fixed-layout
  pages; CSS multi-column; an LLM editor (#53); keyword filtering; per-user topic learning;
  near-duplicate detection (same story, different headline).
- **Anti-goals:** a thicker paper by default (more sections must not mean a 100-page edition);
  anything that breaks an existing `digest.toml`; losing Kindle reading features (selectable
  text, dictionary, font size) on the front page.

## 2. Config (all additive)

A `[[feeds]]` entry gains two ways to say where its stories come from, next to `url`:

```toml
[[feeds]]
name = "Sports"
topic = "SPORTS"                 # a Google News topic, see below
# urls = ["https://example.com/rss"]   # more feeds, merged with the topic and url
# max_items = 5
# full_text = false              # brief or whole stories for this section
```

- `url` stays and still works alone. `urls` is a list of extra feeds. `topic` is one of
  `WORLD`, `NATION`, `BUSINESS`, `TECHNOLOGY`, `ENTERTAINMENT`, `SPORTS`, `SCIENCE`, `HEALTH`
  (case-insensitive) and builds the Google News topic feed
  (`https://news.google.com/rss/headlines/section/topic/<TOPIC>?hl=en-US&gl=US&ceid=US:en`,
  to be confirmed against a live fetch).
- A feed needs at least one of `url`, `urls`, `topic`; none fails config validation with the
  entry's name. `url` stops being required by serde; nothing else about existing files changes.
- All of an entry's sources are fetched, merged, deduplicated by title, newest first and cut to
  `max_items`, the way Local merges its search and feeds (`local_news`). One source failing
  leaves the others; all failing shows the section's usual "Unavailable today".
- **Defaults (my call):** `max_items` is 5 for an entry with a `topic` and 10 otherwise (as now).
  `full_text` defaults to the global setting for entries without a `topic`, and to `false` (brief
  stories, `as_briefs`) for topic entries, so a starter set of eight sections doesn't fetch eighty
  full articles. The setting per entry overrides either.
- **Brief stories** (`full_text = false`) still get an entry in the section page with its `#a<N>`
  anchor: headline, source line, and the "Source:" host line, but no body and no `teaser` (the
  feed gives only a title). Tiles and back links work the same for them. Because they have no
  teaser, they only lead the front page when nothing else can (the existing lead rule).
- **Keys** stay `feeds:<name>`: readership and the page plan need no change.
- The config writer keeps comments and the new fields through a save, like the rest of
  `[[feeds]]` (existing round-trip tests extend to them).

## 3. Across sections

Stories in `[[feeds]]` sections are dropped when the same title (the existing normalized title
key in `feeds.rs`) already appears in an earlier section of the paper: Local, other places, or an
earlier feed, in page-plan order. Your Reading is never deduplicated. The first section wins, so
reordering the page plan changes where a shared story prints. A section emptied by this prints
"No new stories today." like any empty section.

*My call:* dedupe happens at assembly, before each section is cut to `max_items`, so a section
whose stories were taken by an earlier one refills from its own next-newest items. It runs
before the lead and the tiles are chosen, and the tomorrow's-paper sketch calls the same
function.

## 4. The front page

`front_page()` in `render.rs` becomes, top to bottom, reflowable XHTML:

1. Masthead and the weather line (as now).
2. **The lead:** headline (linked to its story), source line, and its opening paragraph
   (`Article.teaser`), under a thin rule. The lead rule is unchanged (`front.rs`: Local leads when
   it has a story, otherwise the first section with one, preferring a story with a teaser).
3. **Headlines in two columns:** a two-cell-wide `<table class="cols">` (Kindle's renderer does
   not reliably support CSS columns; tables are already used for the weather strip and puzzle
   grids). Each cell is one story: the section name in small caps, the linked headline, the
   source line. The stories are `Edition::headlines`' round-robin (the top story of each news
   section, then the seconds), minus the lead, up to 10, filling cells left-to-right in rows.
4. **Inside today:** the existing index of sections with item counts, linking to each section
   front.

Each cell and the lead carry an anchor, `id="t<section>-<article>"`.

The cover image (`cover.rs`) is unchanged; it already follows the same lead rule and the front
page now matches its structure, not its pixels.

## 5. Section fronts and the way back

- **Section front:** at the top of each section page, after the heading and kicker, an "In this
  section" list of all its headlines linked to `#a<N>`, when the section has two or more
  stories. A one-story section skips the list. Sections with `unavailable` show it as now.
- **Back links:** after each story, a small line: "↩ <Section> · ↩ Front page".
  - "<Section>" links to the section's top (`#top`).
  - "Front page" links to that story's tile on the front page (`front.xhtml#t<s>-<a>`) when the
    story is on the front page, else to `front.xhtml`.
- A story not among the front-page tiles gets a plain "Front page" link to `front.xhtml`.
- The Kindle's own back arrow after following a link is assumed to work and is not relied on.
  It is not yet checked on the Scribe with a KFX edition; the in-page links are the guarantee.
- **Readership caveat (existing, unchanged):** jumping from the front page counts every section
  before the target as reached (`docs/daily.md`). Back links don't change that.

## 6. Elsewhere

- **Daily tab → The paper:** a "Topics" picker listing the eight topics; ticking one adds a
  `[[feeds]]` entry (`name` = the topic's display name, e.g. "Technology") and unticking removes
  it, keeping comments. *My call:* the picker only reads and removes entries that have a `topic`,
  matched by the `topic` field, never by name, so a hand-written feed is never touched. If the
  display name is already taken by another feed, the new entry is named "<Topic> (topic)". Its
  page-plan row and budget work as for any feed.
- **Tomorrow's paper:** topic entries fetch their headlines like other feeds (the same
  twice-an-hour limit); the sketch's tile and the lead follow the same rule as the press.
- **`deckle digest next`** and the Daily tab need no new concepts.
- **Docs:** `docs/daily.md` (topics, `urls`, dedupe, back links), the commented starter config in
  `config.rs`, the CHANGELOG.

## 7. Constraints and risks

- **Starter config:** topics ship as commented examples only. A new install's paper does not
  change, so the paper-length guardrail is untouched.
- **Topic URL:** the plan's first task confirms the Google News topic URL with a live fetch. If it
  has changed, the builder finds the current form; there is no alternative host, and a failing
  fetch just shows "Unavailable today" for that section.
- **Merge gate:** the code merges when its tests pass. The device test edition and the CSS
  tuning it causes are a follow-up change, recorded in `docs/devices.md`.

- **KFX conversion:** the EPUB becomes KFX (`deckle-kfx`). Fragment anchors (`#t3-1`, `#a2`,
  `#top`) and `<table>` cells with links must survive that conversion; links to `#aN` already do.
  The builder verifies anchors and tables in the produced KFX before relying on them.
- **Device look:** nothing here can be judged on e-ink without a device. The plan ends with a
  test edition sent to the Scribe (and a Paperwhite if available); table widths, rule weights
  and the two-column cells are tuned there. Expect one round of CSS adjustments.
- **Paper length:** eight topic sections at 5 briefs each is about 40 short stories. The page
  plan's "about N Kindle pages" is the guardrail; the builder checks the estimate with a full set.
- **Google News limits:** many topic fetches per run add up; sources are fetched in parallel
  (`par_map`) as Local's are.

## 8. Testing

- Config: `topic` alone, `urls` alone, `url` alone, all three, none (error naming the entry);
  an old config parses unchanged; round-trip keeps comments and new fields.
- Topic URL: each of the eight builds the expected URL, case-insensitive; an unknown topic is a
  validation error listing the valid ones.
- Merge: two sources with a shared title keep one; one failing keeps the other; all failing
  gives "Unavailable today".
- Cross-section dedupe: a title in Local and in a feed prints only in Local; reversing the page
  plan moves it; Your Reading is untouched.
- Front page: lead and teaser; the two-column table has the expected cells in order and no
  cell for the lead; anchors exist for every cell; with one section only, no empty cells break
  the table.
- Section page: the headline list appears for 2+ stories and not for 1; back links point at
  `#top` and the right tile or `front.xhtml`.
- KFX: a built edition's anchors and tables survive conversion (builder to find the cheapest
  check in `deckle-kfx`).
- Device: the test edition above, recorded in `docs/devices.md`.

## 9. Left to the builder

- The exact CSS for the two-column table, rules and small caps.
- The display names and default order of the eight topics in the Daily tab.
- Whether the back-link line is one paragraph or two, and its exact glyphs, if "↩" doesn't
  render in the Kindle fonts.
- How `Edition::headlines` is parameterized for "minus the lead, up to 10".
