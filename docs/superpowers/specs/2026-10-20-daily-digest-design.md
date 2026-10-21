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

1. ~~Delivery (a): outbox, `push`, plug-in delivery~~ — done (#48), see §5 *Delivery*.
2. ~~Scheduling~~ — #49, see §5b.
3. Delivery (b): opt-in Send-to-Kindle email over the user's own SMTP. This reverses the v1
   non-goal "USB/MTP only" for the digest only, as an explicit, labelled option, because it
   sends the edition through Amazon.
4. ~~Full-text articles and images~~ — in progress (#51), see §5a.
5. ~~Desktop app: a Daily tab~~ — #52, see §5c.
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
place = "New York, NY"          # shown on the masthead and weather page
latitude = 40.71
longitude = -74.01
units = "imperial"              # or "metric"
keep = 3                        # editions kept on the device
max_age_hours = 36              # older feed items are left out
sections = ["weather", "local", "feeds", "lottery", "horoscope", "onthisday"]

[local]
# A Google News search, plus any local paper feeds.
google_news = "New York City"
feeds = []
max_items = 12
max_age_hours = 72              # local news moves slower

[[feeds]]
name = "Hacker News"
url = "https://hnrss.org/frontpage"
max_items = 10

[lottery]
games = ["powerball", "megamillions"]

[horoscope]
signs = []                      # empty: all twelve
```

## 4. Sources (all free, no key)

| Section | Source | Notes |
|---|---|---|
| weather | Open-Meteo forecast API | current, today, 4-day outlook, sunrise/sunset; moon phase computed locally |
| local | Google News RSS search + optional feeds | RSS/Atom via `feed-rs`. The search gets `when:<n>d` from `local.max_age_hours`; without it Google returns months-old results by relevance |
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
- **Cover:** 1264×1680 JPEG, black on white, laid out like a front page: ears (edition
  number, place), blackletter masthead, a black date band in large bold caps (sized to stay
  legible on a Kindle library tile, so you can tell editions apart), the weather line, the
  lead story (first headline with a prose opening paragraph) with that paragraph in italic,
  a two-column grid of further headlines labelled by section (Local items by outlet), and an
  "Inside" index. Fonts bundled under `crates/deckle-digest/fonts/` with their OFL licences:
  UnifrakturMaguntia (masthead) and Old Standard TT regular/bold/italic.
- **File name:** editions land as `deckle-daily-YYYY-MM-DD.kfx`, so they sort by name.
  The Kindle shows the title from the metadata, not the file name.
- **Delivery:** `run` builds and converts into a work folder. With `--to <folder>` the
  edition goes straight there. With `--to device` it goes into the outbox
  (`<cache>/deckle/outbox`), replacing any undelivered older edition. It is then pushed at
  once if a Kindle is connected, else left queued. `deckle digest push` delivers the queue
  to a connected Kindle and succeeds quietly when none is connected, so it is safe on a
  timer. `deckle digest watch [--interval 20]` polls and pushes when a Kindle appears. It
  tries once per connection: a failed push waits for a reconnect or a new edition. A
  launch-at-login agent for `watch` is part of scheduling (#49).
- **Pruning:** after a push, over the *same* connection (a second MTP session would reset
  the Kindle again), `deckle-daily-*.kfx` files beyond the newest `keep` by name are
  removed with their `.sdr` folders. This applies to folders, mass storage and MTP.

## 5a. Full text and photos (#51)

The owner's bar: it should feel like a real newspaper, with nothing that sends you to a web
browser. Every story carries its full text and photos inside the edition.

- **Finding the article.**
  - A feed item's link is the article.
  - Google News links are not: they land on an interstitial page. The page carries
    `data-n-a-id`, `data-n-a-ts` and `data-n-a-sg`, and a POST to
    `news.google.com/_/DotsSplashUi/data/batchexecute` (rpc `Fbv4je`, `garturlreq`) returns
    the publisher URL.
  - This is undocumented and may break. When it fails, the item stays a brief (headline,
    outlet, time) and the edition still ships.
- **Extraction:** `dom_smoothie` (a port of Mozilla Readability, the code behind Firefox
  Reader View) gives title, byline, lead image and cleaned content.
  - The content is rewritten into a small XHTML whitelist: `p`, `h3`, `blockquote`,
    `ul`/`ol`/`li`, `em`/`strong`, `br`, and `figure` with `img` and `figcaption`.
  - Everything else is unwrapped to its text, and links become plain text.
- **Thresholds:** a story under 120 words after extraction counts as a failure (paywall,
  consent wall, video page). The item then falls back to the feed's own summary, with the
  line "Full story unavailable offline." Text is capped at 6,000 words.
- **Photos:**
  - At most 4 per story: the lead image (`og:image`) first, then inline images in order,
    deduplicated by URL without its query, then by a 64-bit difference hash of the picture
    (CDNs serve one photo under several URLs; repeats measured 0–10 bits apart, distinct
    photos 20+, cutoff 14).
  - Images whose decoded shorter side is under 200 px are dropped (icons, trackers).
  - Each is converted to 8-bit grayscale (e-ink), fit to 1000 px wide, and saved as JPEG
    quality 70, as `images/<n>.jpg`.
  - Formats: JPEG, PNG, GIF, WebP. Anything else is skipped.
- **Requests:**
  - Article pages are fetched with a desktop Safari user agent. Many publishers refuse
    unknown agents, and the edition is read only by the person who built it.
  - APIs keep the honest `Deckle/<version>` agent.
  - Up to 8 fetches run in parallel: 15 s per API request, 25 s per publisher page.
- **Order:** within a section, stories with full text come first; the ones that fell back
  follow, like briefs.
- **No outbound links:** the "Read online" link is gone. Each story ends with a small
  "Source: <outlet>" line in plain text.
- **Config:** `full_text = true` and `photos = true` at the top level, both default on.
  `max_photos = 4`.
- **Tests:** extraction and XHTML rewriting run against saved article pages, and the decoder
  against a saved interstitial page plus a saved batchexecute reply. No network in tests.

## 5b. Scheduling (#49)

`deckle digest schedule --at 05:30` makes the paper arrive by itself:

- **Two jobs.**
  - *The edition*, daily at the given time: `deckle digest run --to device`. It delivers
    if a Kindle is connected and otherwise queues in the outbox.
  - *The watcher*, from login on, kept alive: `deckle digest watch`. It delivers the queue
    when a Kindle connects, scans the Inbox, and serves the bookmarklet.
- **macOS:** two LaunchAgents in `~/Library/LaunchAgents`, loaded into the user's GUI
  session so Kindle Previewer can run.
  - `com.casavargas.deckle.digest` uses `StartCalendarInterval`; launchd runs a missed
    time when the Mac wakes.
  - `com.casavargas.deckle.watch` uses `RunAtLoad` and `KeepAlive`.
  - Both log to `~/Library/Logs/Deckle/`.
- **Windows:** two Task Scheduler tasks under `\Deckle\`. "Digest" runs daily at the time,
  with "run as soon as possible after a missed start". "Watch" runs at logon. Both are
  created with `schtasks`.
- **Linux:** not scheduled by Deckle; `--at` prints the crontab line and the systemd user
  unit to use.
- **Other forms.**
  - `--off` removes both jobs.
  - `--status` (the default) says what's installed and at what time.
  - Re-running `--at` with a new time replaces the jobs.
  - `--config <path>` is carried into both jobs.
- **Which binary.** The jobs run the `deckle` binary that created them (`current_exe`),
  by absolute path.
  - When that path is inside a Cargo `target/` folder, `schedule` warns. Such a binary
    disappears on `cargo clean`; `cargo install --path crates/deckle-cli` gives a stable one.
  - The desktop app ships only `deckle-desktop` today. Its Daily tab (#52) must bundle the
    CLI as a sidecar before it can offer this toggle.
- **Code:** `deckle_digest::schedule`, with pure generators (plist XML, `schtasks`
  arguments) under unit tests, plus `install`, `remove` and `status`, which call
  `launchctl` and `schtasks`.

## 5c. The Daily tab in the app (#52)

Everything the CLI does, without the CLI. The window gets a **Books | Daily** switch in the
top bar; Books is today's view, unchanged.

- **One binary.** `deckle-cli` becomes a library plus a thin `deckle` binary. `deckle-desktop`
  starts with the same code when its first argument is `digest` or `save`, and exits
  without opening a window. The schedule (§5b) therefore runs the app itself
  (`/Applications/Deckle.app/Contents/MacOS/deckle-desktop digest run --to device`), and
  nothing new needs signing.
- **The Daily screen**, four cards top to bottom:
  1. **Today's paper.** The newest edition's cover (from the build cache), its date, the
     number of editions waiting in the outbox, and two buttons:
     - **Preview** runs `digest build` without using up saved articles;
     - **Send now** runs `digest run --to device`, which queues when no Kindle is in.

     Both run as a child process of the app with `--json`, so the window never blocks. The
     card then shows the result's section counts and notes.
  2. **Your Reading.**
     - A "Save a link" field.
     - The queue: pending first, then the last 7 days, with state and a remove button.
     - The Inbox folder, with a Change… picker and Reveal.
     - **Copy bookmarklet**, with one line on how to add it. A webview can't drag a link
       into a browser's bookmarks bar.
  3. **Every morning.** A switch and a time. On: `schedule::install` with the app's own
     binary. Off: `schedule::remove`. Shows what is installed.
  4. **The paper.** The settings in `digest.toml`:
     - **Name**, **place** and units. Place is typed and found with Open-Meteo geocoding;
       you choose from the matches, which fills latitude and longitude.
     - **Sections**: checkboxes in order, with up and down arrows.
     - **Local news**: the search query.
     - **Feeds**: name and URL rows, add and remove.
     - **Lottery**: games. **Horoscope**: signs.
     - **Full text** and **photos**, and how many editions to keep.
     - A **Save** button.
- **Saving keeps your file yours.** The settings are written into the existing
  `digest.toml` with `toml_edit`. Comments and key order survive, keys you didn't change
  stay byte for byte, and a missing file is created from the starter.
- **First run.** With no `digest.toml`, the Daily screen shows one "Set up your paper"
  button. It writes the starter and asks for the place.

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
