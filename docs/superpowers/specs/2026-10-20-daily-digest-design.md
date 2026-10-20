# Deckle Daily — design

**Status:** owner said "get something going now, then refine" (2026-10-20); this spec is the
v0 contract and the refine backlog · **Date:** 2026-10-20 · **Owner:** Jonathan (CasaVargas LLC)

A newspaper for your Kindle, built fresh every morning on your own machine from sources you
choose, and delivered like any other Deckle book: Enhanced Typesetting, a real cover in the
library view, chapter-and-article navigation.

## 1. Intent and why

The Kindle is the best screen in the house for reading and the worst for news: Amazon's
periodicals are a short list of paid subscriptions, and "send this article" tools give you
loose files with no front page. Deckle already knows how to make a file read like a store book.
Point that at a daily edition and the Kindle becomes the morning paper: weather, local news,
the lottery numbers, a horoscope, and your feeds, on paper-like e-ink, with no account.

What makes it unique, not just "RSS to EPUB":

- **A real front page and a live cover.** The cover in the Kindle library is a generated
  masthead with today's date, the weather, and the lead headlines, so the library tile itself
  is the paper on your doorstep.
- **It's a newspaper, not a feed dump.** Sections in a fixed order, an "Inside today" index,
  nested navigation (section → article), short items kept short.
- **Local first.** Built on your machine, from your config file, with free no-key sources.
  Nothing about the reader is sent anywhere beyond the source requests themselves.

## 2. Goals, non-goals, anti-goals

**Goals (v0, this increment)**

1. `deckle digest build` writes a valid EPUB for today from a plain TOML config.
2. `deckle digest run --to device|<folder>` builds it and runs the normal pipeline (forced
   PDOC so the cover always shows; X-Ray and Word Wise off), then keeps only the newest
   `keep` editions on the target (default 3).
3. Sections: front page, weather, local news, feeds, lottery, horoscope, on this day.
   Each is optional and ordered by the config. A source that fails becomes a one-line
   "unavailable today" note in its section; the edition still ships.
4. Generated masthead cover (PNG) with the paper's name, date, weather line and top
   headlines, using bundled OFL fonts.
5. Works offline-tolerant: with zero network the edition still builds (front page, horoscope,
   "unavailable" notes).

**Next increments (refine backlog, in order)**

1. Delivery (a): an outbox and `deckle digest push`, plus an "on plug-in" hook so a Kindle that
   was unplugged at build time gets the edition when it's next connected.
2. Scheduling: a generated launchd plist (macOS) / Task Scheduler entry (Windows) from
   `deckle digest schedule --at 05:30`; later a toggle in the desktop app.
3. Delivery (b): opt-in Send-to-Kindle email over the user's own SMTP. This reverses the v1
   non-goal "USB/MTP only" for the digest only, as an explicit, labelled option, because it
   sends the edition through Amazon.
4. Full-text articles (Readability-style extraction) when a feed only carries summaries;
   images in articles.
5. Desktop app: a Daily tab to edit sections, preview the front page, and build now.
6. Optional LLM editor (existing provider settings): dedupe stories across feeds, a three-line
   brief per section, an LLM-written horoscope.
7. Extras: Sudoku/word puzzles with answers in the next edition, word of the day from the
   Word Wise lexicon, NC lottery games if a stable source exists, weekly long-read edition.

**Non-goals**

- Paid or paywalled content, logins, cookies.
- A hosted service. The digest is built where Deckle runs.

**Anti-goals (would make it a failure even if it "works")**

- One broken source sinks the whole edition.
- Yesterday's paper piling up on the Kindle.
- The library tile is a grey placeholder.
- Any account, subscription or telemetry.

## 3. Config

Plain TOML, default path `<config dir>/deckle/digest.toml` (`--config` overrides).
`deckle digest init` writes a commented starter file. Section order in the edition is the
order of `sections`.

```toml
title = "The Deckle Daily"
place = "Carthage, NC"          # shown on the masthead and weather page
latitude = 35.35
longitude = -79.42
units = "imperial"              # or "metric"
keep = 3                        # editions kept on the device
sections = ["weather", "local", "feeds", "lottery", "horoscope", "onthisday"]

[local]
# A Google News search, plus any local paper feeds.
google_news = "Moore County NC"
feeds = []
max_items = 12

[[feeds]]
name = "Hacker News"
url = "https://hnrss.org/frontpage"
max_items = 10

[lottery]
games = ["powerball", "megamillions"]

[horoscope]
signs = ["leo"]
```

## 4. Sources (all free, no key)

| Section | Source | Notes |
|---|---|---|
| weather | Open-Meteo forecast API | current, today, 4-day outlook, sunrise/sunset; moon phase computed locally |
| local | Google News RSS search + optional feeds | RSS/Atom via `feed-rs` |
| feeds | any RSS/Atom URL | summaries / `content:encoded`, HTML reduced to text paragraphs (v0) |
| lottery | data.ny.gov open data (Powerball `d6yy-54nr`, Mega Millions `5xaw-6ayf`) | latest draw + date |
| horoscope | built in, deterministic per (date, sign) | for fun; labelled as such. LLM version later |
| onthisday | Wikimedia feed API `onthisday/selected` | top 5 |

Every fetch has a timeout (15 s) and a `User-Agent: Deckle/<version>`. Fetches run
sequentially in v0 (an edition is a handful of requests).

## 5. The edition

- **Metadata:** title `<title> — <Weekday, Month D, YYYY>`, author `Deckle`, identifier
  `urn:deckle:digest:<YYYY-MM-DD>`, language `en`, publisher `Deckle`.
- **Files:** EPUB 3 with `nav.xhtml` and `toc.ncx` (nested: section → article). One XHTML
  per section, articles as `<h2 id>` blocks inside it.
- **Front page:** masthead, date and edition number (day of year), the weather line,
  "Inside today" (each section with its item count, linked), lead headlines (the first item of
  local and feeds).
- **Cover:** 1264×1680 PNG (largest current Kindle screen ratio), black on white: blackletter
  masthead, rule lines, date line, weather line, up to five headlines. Fonts bundled under
  `crates/deckle-digest/fonts/` with their OFL licences.
- **Pruning:** after delivery, editions whose device filename starts with the digest's
  `Deckle - <title>` prefix are sorted by date and all but the newest `keep` are removed
  (mass storage and MTP).

## 6. Code layout

- New crate `crates/deckle-digest`: `config` (TOML), `fetch` (HTTP + per-source parsers into a
  plain model), `horoscope`, `moon`, `render` (model → XHTML), `epub` (writer), `cover`
  (PNG). `build(&Config, date, out_dir) -> BuildReport` never fails because of a source.
- `deckle-cli`: `digest init | build | run` subcommands. `run` calls
  `deckle_core::pipeline::run` with `cde_override = Pdoc`, X-Ray off, Word Wise off.
- Tests: parsers against saved fixtures (no network in tests), EPUB structure checks, cover
  renders at the right size, prune keeps the newest N.

## 7. Calls made by the builder (owner to check)

- TOML over JSON for the config: it's hand-edited and needs comments.
- Lat/lon in config instead of geocoding a place name (one less network call; `init` can
  geocode later).
- Horoscope generated locally rather than scraped from an unofficial API.
- PDOC forced for the digest: the cover-is-the-paper idea depends on the cover always showing.
