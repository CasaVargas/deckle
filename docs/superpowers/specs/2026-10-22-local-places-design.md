# Local news for more places — design

**Status:** direction approved by owner (2026-10-22, issue #102, "we don't need weather for
multiple places, just local news") · **Date:** 2026-10-22 · **Owner:** Jonathan (CasaVargas LLC)
· **Builds on:** `2026-10-20-daily-digest-design.md` (Local), `2026-10-22-readership-report-design.md`
(section keys), `2026-10-22-tomorrows-paper-design.md` (page plan).

Keep up with a relative's town or a vacation spot: each extra place gets its own local news
section, right after your own Local. Weather, the almanac, the cover and the front page's
weather line stay on your main place.

## 1. Config

An optional list under `[local]`, additive: a `digest.toml` without it reads and builds exactly
as before, and a save leaves it out while it's empty.

```toml
[[local.places]]
name = "Asheville, NC"          # the section's title in the paper
google_news = "Asheville NC"    # a Google News search; "" searches the name
feeds = []                      # local paper RSS feeds, merged in
# max_items = 6                 # stories; default: [local] max_items
```

An empty `google_news` falls back to the name, as typed: a place with only a name ("Asheville,
NC") searches Google News for it. The name isn't quoted, as Local's search isn't: "Asheville, NC"
in quotes would miss "Asheville, N.C.". A custom search wins when there is one (the Daily
tab's second box asked for both and a name alone printed nothing). Only a place with no name and
no feeds prints "Unavailable today (give this place a name or local feeds)".

## 2. The paper

- Where `local` sits in `sections`, the paper prints Local, then one section per place, in file
  order. Turning Local off turns the places off with it.
- Each place's section is built exactly as Local is: its search and feeds merged, deduplicated,
  newest first, cut to its `max_items`, the age window `[local] max_age_hours`, whole stories
  when `full_text` is on. Kicker: "News from around <name>".
- A place whose fetch fails shows its own "Unavailable today"; the others are unaffected.
- **Keys:** `local:<name>` (Readership, the page plan), like `feeds:<name>`.
- **The lead:** only your own Local leads by rule; a place's stories count like a feed's.

## 3. Elsewhere

- **Readership:** a place's row is named after it. A Drop suggestion for it removes that place.
- **Tomorrow's paper:** one page-plan row and one tile per place, after Local. Its headlines are
  fetched like Local's (the same twice-an-hour limit). The row's budget edits its `max_items`.
  Place rows follow Local: they aren't dragged on their own.
- **Daily tab → The paper:** "Other places" with a name and an optional custom search per place
  ("Custom search (optional)"; the hint says the name is searched by default), add and remove.
- **Docs:** `docs/daily.md`, the starter's commented example, the CHANGELOG.

## 4. Testing

- A config without places builds the same sections as before (existing tests unchanged).
- A place with only a name searches its name; one with its own search searches that, not the
  name.
- Two places → two extra sections after Local, keys `local:<name>`, each requesting its own
  search URL; one failing leaves the other.
- Config round-trip: an untouched config saves no `places`; one with places keeps them.
- Readership: a place's name in the report; Drop removes only it.
- Page plan: rows and tiles per place; budget lands on the place; dragging a place row is a no-op.
