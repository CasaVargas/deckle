# Tomorrow's paper, so far — design

**Status:** direction approved by owner (2026-10-22, "Deckle Daily ideas" canvas, board 1,
"Tomorrow's paper, so far"); document pending owner review (see §11, *Calls I made*) ·
**Date:** 2026-10-22 · **Owner:** Jonathan (CasaVargas LLC) · **Builds on:**
`2026-10-20-daily-digest-design.md` (editions, the schedule, the Daily tab),
`2026-10-20-save-to-paper-design.md` (the reading queue), `2026-10-22-readership-report-design.md`
(layouts, reads, section shares), `2026-10-22-almanac-design.md` (the weather page).

The Daily tab is rebuilt around the next edition. At the top: when it goes to press. Under it, a
live sketch of tomorrow's front page that fills in as you save articles and as your feeds post,
with one line saying why the lead leads. Beside it, the **page plan**: the sections in order,
each with a budget, and about how many Kindle pages that makes. Under that, a strip of how far
you read each of the last six papers.

## 1. Intent and why

Today the Daily tab opens on the *last* paper: a cover thumbnail and "Last built". Everything
about the *next* one is spread over a settings form. You save an article and nothing visible
happens until 05:30. A real newsroom works the other way: all day the front page is a dummy that
fills in. Showing the dummy makes saving an article feel like it did something, makes the order
of sections a thing you see rather than a list of checkboxes, and gives one obvious place to say
"not tomorrow" (Stop the presses).

Nothing here needs new sources or a server. The queue, the config, the schedule and readership
already hold almost all of it. The one new network use is reading your feeds' RSS a few times a
day, politely (§5.1).

**Success:** on Thursday 2026-10-22 at 15:08, with the press at 05:30 and Linden Falls set as
the place, the Daily tab shows the following:

- "Goes to press at 05:30 · in 14 h 22 m".
- A front page headed *The Morning Deckle*, "No. 296", "LINDEN FALLS, NC", "FRIDAY, OCTOBER 23,
  2026", and "Weather · fills in at press time".
- The article saved at 9:14 leads: "YOUR READING · THEATLANTIC.COM", its title, and "Saved at
  9:14 this morning. Leads the paper because nothing local outranks it yet."
- Tiles below: a second saved article, "Local · 3 slots" (dashed), "SCIENCE · 2 NEW SINCE 05:30"
  with the newest headline.
- **Send now**, **Preview**, **Stop the presses**.
- The page plan: Your Reading (all) · Weather (1 page) · Local (3 stories) · Science (2 stories)
  · Horoscope (12 signs) · On This Day (back page), "about 34 Kindle pages". Dragging Science
  above Local saves `digest.toml` and the front page reorders.
- "This week": the six papers before Friday's, Sat "read 90%" … Thu "read 85%".
- Clicking **Stop the presses** turns the bar into "Presses stopped: no paper Friday · next at
  05:30 Saturday", and at 05:30 Friday no edition is built or sent.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **The press clock.** The next press time from the installed schedule, with a live countdown.
   The **Every morning** card's switch and time move into this bar (§4.1).
2. **Stop the presses.** Skip the next scheduled edition once. It undoes itself after that press
   time, and **Restart the presses** undoes it before (§4.2).
3. **Tomorrow's front page, live.** The masthead of the next edition, a weather placeholder, a
   lead and a grid of tiles in page-plan order. Saved articles show at once. Feed headlines show
   once fetched (§5).
4. **One lead rule, used by the press and the preview.** The preview never promises a lead the
   press won't print (§5.2).
5. **Reason lines.** A fixed, small set of sentences, each tied to a rule (§5.3).
6. **The page plan.** Every section in order with a detail line and a budget. Drag (or the
   arrow buttons) to reorder; story budgets are editable; sections switch on and off. All of it
   writes the existing `digest.toml` keys (§6).
7. **Page estimate.** "about N Kindle pages", from the sizes of your own past editions when
   known, otherwise from defaults (§6.3).
8. **This week.** The last six editions' read percentages from Readership, and an **All
   editions** link that opens the Morgue once it exists (§7).
9. **CLI parity:** `deckle digest next [--fetch] [--json]` and `deckle digest stop [--undo]`;
   `deckle digest run` honours a stop unless `--force` (§9).

**Non-goals (this increment)**

- **Word counts, photo counts and long-read detection before press.** The queue doesn't know
  them until the article is fetched at press time. The mockup's "2,180 words, 3 photos" and "1
  long read rides along as its own book" wait for a later change that fetches saved articles
  when they're saved.
- **A live weather line.** It fills in at press time, as the mockup says.
- **Fetching article pages, photos, lottery, On This Day or the forecast for the preview.**
  Feeds' RSS only.
- **A page cap that trims the paper to fit** (readership's non-goal stays). The estimate informs;
  budgets are the lever.
- **New budget kinds** ("1 page" for a feed, time-boxed sections). Budgets are item counts where
  items exist and fixed labels elsewhere.
- **Grouping sections onto one page** (the mockup's "Horoscope · On this day" row). One row per
  section.
- **The Serial** (parked) and **From Your Shelf** (#6, being specced). Not listed until built.
- **Rendering the real EPUB in the tab.** **Preview** still builds the file.

**Anti-goals**

- The preview shows a lead, a section order or a budget the press won't use.
- The tab polls sources: a feed fetched more than twice an hour, or anything fetched while the
  tab isn't on screen.
- Stop the presses that silently stops more than one morning, or that survives past its date.
- A drag that loses `digest.toml` comments (the `[[feeds]]` merge from readership §3 applies).
- A countdown that's wrong across a clock change or after midnight.

## 3. Constraints

- Rust in `crates/deckle-digest` for everything that doesn't need readership state (new
  modules `press`, `front`, `next`). `deckle-readership` depends on `deckle-digest`, so the
  readership-derived inputs (sizes, the week) are computed there and passed in as plain structs
  defined in `deckle_digest::next`.
- The desktop reaches it through Tauri commands; config edits go through the existing
  `daily_save` (`config::save`, comments kept).
- **The press time lives in the installed job** (LaunchAgent plist or Task Scheduler task),
  read by `schedule::status().at`. There's no press time in `digest.toml`, and this design
  doesn't add one.
- **The edition date is the local date the press runs** (`digest run` uses `Local::now()`).
  The next edition is the next occurrence of `at`: today if now is before it, else tomorrow.
  Its number is that date's ordinal (`Edition::number`).
- **Nothing new in `digest.toml`.** Stop state goes in `press.json` beside `digest.toml`
  (`Config::press_state()`, with a `#[serde(skip)] press_path` override for tests, like
  `almanac_path`). Fetched headlines go in `next.json` in the digest cache folder
  (`~/Library/Caches/deckle/digest/next.json`): disposable.
- Plain JSON, temp file and rename on every write. A missing or corrupt file means "no stop" and
  "nothing fetched".
- Times are local. Copy follows `units`: 12-hour without AM/PM where the words carry it ("9:14
  this morning") for imperial, 24-hour for metric. Press times are always shown `HH:MM`, as the
  schedule sets them.

## 4. The press

### 4.1 The press bar

One line at the top of the tab:

| State | Copy | Controls |
| --- | --- | --- |
| Scheduled | "Goes to press at 05:30 · in 14 h 22 m" ("in 38 m" under an hour) | the time (click to edit), a switch |
| Stopped | "Presses stopped: no paper {Weekday} · next at 05:30 {Weekday}" | **Restart the presses** |
| Off | "No press time set" | a time field and **Turn on** |

The countdown ticks each minute in the app. It's computed from an absolute next-press instant,
so a clock change night is right (Europe/London 2026-10-25: 23:00 Saturday to 05:30 Sunday
reads "in 7 h 30 m"). The switch and time call the existing `schedule_set`. The **Every
morning** card is removed; its explanatory line ("A Mac asleep at 05:30 builds it when it
wakes.") becomes the bar's tooltip.

### 4.2 Stop the presses

- **What it does:** writes `press.json` = `{"stopped": "2026-10-23"}`, the date of the next
  scheduled edition. `deckle digest run` for that edition date (no `--date`, or `--date` equal
  to it) exits 0 without building, converting, emailing, queueing or touching the reading queue,
  and prints "Presses stopped for Friday, October 23: no paper today (deckle digest stop --undo
  restarts them)". Saved articles wait for the next paper.
- **Once only.** It names one date. A stop whose date is past is ignored and removed on the next
  write. The schedule itself is untouched; turning the schedule off is the switch in the bar.
- **Undo:** **Restart the presses** (or `deckle digest stop --undo`) removes it.
- **Explicit runs win.** `--force` on `digest run` ignores the stop. The app's **Send now**
  always passes `--force`: pressing it is an explicit ask.
- The watcher still delivers anything already queued.
- **Shown only when scheduled.** With the schedule off there's no press to stop.
- **While stopped,** the front page previews the edition after the stopped one (its date and
  number), and the bar says which morning is skipped.

## 5. Tomorrow's front page

### 5.1 What "live" means

- **Local data, always fresh.** The view is recomputed from `digest.toml`, `reading.jsonl`,
  `press.json`, `readership.json`, `next.json` and the schedule status in these cases: when the
  tab opens, when the window regains focus, every 60 s while the tab is visible, and right after
  a save, a removal, a config save or a run. This costs file reads only.
- **Feeds, politely.** A separate refresh reads the RSS/Atom of each `[[feeds]]` URL and the
  Local sources (the Google News search and `[local] feeds`) through `Net::get`. No article
  pages and no photos.
  - **Throttle:** at most once per URL per 30 minutes. The last fetch time and result (items or
    error) are kept per URL in `next.json`, so restarting the app doesn't refetch.
  - **When:** only while the Daily tab is visible and the app is in front. It's triggered on
    open and focus; a URL still within its 30 minutes is skipped.
  - A failed fetch waits the same 30 minutes. Never more than one request in flight per host.
  - Items go through the same `feeds::parse` (age window), `as_briefs` (Google News) and
    `max_items` cut as the press, so a tile's headline is one the press would consider.
- **Never live:** weather, lottery, On This Day, horoscope, and saved articles' contents.
- The card's corner reads "Live", with "Headlines checked 14:52" under it (or "Headlines not
  checked yet").

### 5.2 Layout and the lead rule

- **Masthead,** as the cover: "NO. {ordinal}", "All the news that fits", the place uppercased,
  the paper's name, and the long date uppercased. Then "Weather · fills in at press time" when
  the weather section is on.
- **The lead,** chosen by `front::lead` (below).
- **Then one tile per item, in page-plan order:**
  - each other saved article that fits `[reading] max_items`;
  - one tile per story section (Local, each feed): its newest fetched headline, or a dashed
    "{Section} · {n} slots" (n = its budget) when there's nothing fetched yet;
  - one dashed tile per fixed section: "{Section} · fills at press time".
- Under the tiles: "The page lays itself out as the day goes on. Each link you save drops into a
  slot right away, and dashed slots fill at press time."

**The lead rule** (`front::lead`). The press's cover uses it too, replacing today's rule. Today
the lead is the first news headline (Local and feeds interleaved), and saved articles never lead.

1. **Local leads** when the Local section has a story: its first with an opening paragraph,
   else its first.
2. **Otherwise,** the first section in paper order that has a story, among Your Reading and the
   feeds: its first story with an opening paragraph, else its first.
3. Otherwise there's no lead.

The cover keeps up to two saved articles at the top of its grid, skipping the one that leads.
The preview can't know opening paragraphs (saved articles aren't fetched yet, briefs have none),
so it counts every story as having one. **The preview predicts the leading section; the story
within it can change at press.**

### 5.3 Reason lines

Each line is exactly one of these:

| When | Line |
| --- | --- |
| A saved article (always first) | "Saved at {time} {this morning / this afternoon / this evening}." Or "Saved yesterday at {time}." Or "Saved {Weekday}." within 6 days, else "Saved {Mon D}." Afternoon starts at 12:00, evening at 17:00. |
| Lead from Your Reading | "Leads the paper because nothing local outranks it yet." |
| Lead from Local | "Leads the paper: local news always leads." |
| Lead from a feed | "Leads the paper: nothing local yet, and {Section} comes first in your page plan." |
| Feed tile kicker | "{SECTION} · {n} NEW SINCE {last press HH:MM}", or "{SECTION}" when n = 0 |

- **"New since":** items whose date is after the last press instant (the next press minus one
  day, or local midnight with no schedule). Undated items aren't counted as new, but can still
  be the tile's headline.
- **Saved kickers:** "YOUR READING · {host}", with the host taken from the URL, `www.` dropped,
  uppercased.

## 6. The page plan

### 6.1 Rows

One row per section the paper will print, in order, as `gather` expands them: each `[[feeds]]`
entry is its own row, at the `feeds` position. Sections that are off are listed under the on
ones, muted, with a switch.

| Row | Detail line | Budget |
| --- | --- | --- |
| Your Reading | "{n} saved today" when all pending were saved today, else "{n} waiting". "Nothing saved yet" at 0. When n > max: "{max} of {n} fit; the rest wait". | **editable**, `[reading] max_items`. Shows "all" when n ≤ max, else "{max}". |
| Weather | "{place} · 5-day box and almanac", plus " · Your Day" when `[day] calendars` isn't empty | "1 page" |
| Local | the local feeds' hosts, then "+ Google News" when the search is set | **editable**, `[local] max_items`, "{n} stories" |
| a feed | the feed's channel title from `next.json`, else the URL's host | **editable**, that entry's `max_items`, "{n} stories" |
| Lottery | game names | "1 page" |
| Horoscope | "{n} signs" ("all 12 signs" when empty) | "{n} signs" |
| On This Day | "From Wikipedia" | "1 page" |

- **Back page:** the last row, when it's a fixed section, shows "back page" as its budget.
- **Readership note:** when Readership has a share for the row's key at or under its `RARELY`
  (20%), the detail gains " · you get to it about {share}% of days".
- Budgets offer 1–20. A change writes the key at once (like the Your Day card), and the front
  page and estimate refresh.

### 6.2 Reordering

Rows are dragged with native HTML drag and drop (no new dependency). The existing up/down arrow
buttons stay on each row for the keyboard. A move becomes `digest.toml` edits:

- A non-feed row moves its `SectionKind` within `sections`. The feed rows are one block in that
  list: a row dropped inside the block goes to the block's nearest end.
- A feed row moves within `[[feeds]]`. Dropped outside the block, it goes to the block's nearest
  end. A feed can't leave the block; to put Local after your feeds, drag Local.
- The save goes through `daily_save` (`config::save`, the `[[feeds]]` merge keeps comments).

### 6.3 The page estimate

"about N Kindle pages", where N = round(Σ row sizes / `POSITIONS_PER_PAGE`), and N ≥ 1.

- **Sizes come from your own editions** when Readership has them. Each recorded layout gives
  every section a length: the next start minus this start, or the end for the last.
  - **Story rows** (Your Reading, Local, feeds) cost the median length per item over the last 14
    layouts, times the items planned. The per-item figure needs the section's item count, which
    layouts gain (`SectionStart.items`, recorded from the build report).
  - **Fixed rows and the front page** cost their median length.
- **Items planned:** for Your Reading, min(pending, max). For Local and feeds, min(budget,
  fetched fresh items) when fetched, else the budget.
- **Defaults without history** (pages): front 2; Your Reading 3 per article; a story 1.5 per
  item with `full_text`, else 0.3; weather 2; lottery 1; horoscope 0.25 per sign; On This Day 2.
- `POSITIONS_PER_PAGE = 1_800`: KFX positions per Paperwhite page at the default font. The
  builder checks it once on the Paperwhite against a real edition and adjusts.

## 7. This week

- The six local dates before the next edition, oldest first: "Fri" … "Wed".
- **Each day reads one of:**
  - "read N%": the furthest position over the end, the best across Kindles;
  - "not opened": a layout, but no device reported it opened;
  - "sent": an edition was built, but there's no layout (email-only, or before Readership);
  - "no paper".
- Read from `readership.json` through a new `deckle_readership::recent(state, before, days)`.
  "Sent" needs the build to be known without a layout; the builder may infer it from the digest
  cache's `deckle-daily-*.epub` files, or show "no paper".
- **All editions** scrolls to the Morgue's **Every edition** card (#109). It's rendered only
  when an element with id `every-edition` is on the page, so it appears when the Morgue lands
  and never links nowhere.

## 8. The Daily tab, before and after

| Today | After |
| --- | --- |
| Paper card: cover, "Last built", Send now, Preview, Show file, run result | **Tomorrow's paper, so far** card. The buttons and run result stay; "Last built {date} · Show file" becomes one muted line under them. The cover thumbnail goes. |
| Every morning card | Merged into the press bar (§4.1). |
| The paper → "Sections, in order" | **Page plan** card (§6). |
| The paper → feeds' name and URL | Stays in The paper. `max_items` moves to the page plan. |
| How far you read | Stays, under **This week**. Applying a suggestion refreshes the page plan. |
| Your Reading card | Stays: saving, the list with remove, inbox, bookmarklet. Moves under the page plan. |
| Email to Kindle, The paper, Your Day | Unchanged. |

**Order:** the press bar; then Tomorrow's paper beside the Page plan and This week (two columns
on a wide window, stacked under about 900 px); then How far you read, Your Reading, Email to
Kindle, The paper and Your Day. The first-run setup card and the broken-config card are
unchanged.

## 9. CLI

- `deckle digest next [--fetch] [--json]`: the same view as the tab.
  - In text: the press line, the masthead line, the lead with its reason, the tiles, the page
    plan with budgets, "about N Kindle pages", and the week.
  - `--fetch` refreshes headlines first, under the same 30-minute throttle. Without it, only
    `next.json` is used.
- `deckle digest stop [--undo] [--json]`: sets or clears the stop for the next scheduled
  edition. With no schedule it's an error: "nothing is scheduled; there's no press to stop".
- `deckle digest run [--force]`: honours the stop (§4.2).

## 10. Testing

- **Press clock:** next press before and after `at`; across midnight; across the 2026 US and
  UK clock changes (fixed `Tz`); off gives `None`. Countdown copy at 14 h 22 m, 38 m and 0 m.
- **Stop:** `run` for the stopped date builds nothing, leaves the queue untouched and exits 0.
  `--force` builds. A past stop is ignored and cleaned. `stop` with no schedule errors. `--undo`
  clears it.
- **Lead rule:** Local beats a saved article; with no Local, plan order decides between Your
  Reading and a feed; the opening paragraph is preferred within the section. The cover golden
  or unit test is updated, and the old "saved never leads" case is ledgered as an intended
  change.
- **Reason lines:** each row of §5.3, including the boundaries at 12:00 and 17:00, yesterday,
  6 days back and 7 days back, and both unit systems.
- **Throttle:** with a counting fake `Net`, two refreshes 10 minutes apart make one request per
  URL, and a third at 31 minutes makes another. A failure waits too. Only feed and Local URLs
  are requested.
- **Page plan:** rows for a config with two feeds; budgets map to the right keys; back page;
  the readership note at 20% and not at 21%. TS reorder: a section into the feed block snaps;
  a feed outside snaps; the resulting config round-trips through `config::save` with comments.
- **Estimate:** history medians per item and per section; defaults without history; planned
  items from fetched counts.
- **Week:** read %, not opened, no paper; two Kindles take the best.
- **CLI:** `digest next --json` parses and matches the view; `stop` then `run` skips.
- **Desktop:** vitest for countdown, reorder and budget labels. Check the tab visually in the
  running app: save a link and watch it drop in, drag a section, change a budget, stop and
  restart the presses, and look at a narrow window.

## 11. Calls I made (for owner review)

1. **"Live" is local files every minute, plus feed RSS at most every 30 minutes per URL, only
   while the tab is on screen.** No article pages, photos, weather or other sources until press
   time.
2. **Budgets are the existing `max_items`** (`[reading]`, `[local]`, each `[[feeds]]`). Fixed
   sections get read-only labels. **Nothing new in `digest.toml`.**
3. **Stop the presses skips the next scheduled edition once,** stored as a date in
   `press.json`, which expires by itself. It doesn't pause the schedule. **Send now** overrides
   it (`--force`).
4. **The page estimate uses your own editions' sizes** (KFX positions from Readership layouts,
   per item and per section) with defaults until there's history, at 1,800 positions per page.
   Layouts gain an `items` count.
5. **The lead rule changes in the press too:** Local leads; otherwise the first section in paper
   order with a story, Your Reading included. That's what the mockup's reason line implies.
   Today a saved article never leads.
6. **Reason lines are a fixed set of five** (§5.3). No word counts: the queue doesn't know them
   before press.
7. **CLI parity:** `digest next` and `digest stop`, and `digest run --force`.
8. **Removed or merged:** the cover thumbnail, the Every morning card (into the bar), and the
   section checklist (into the page plan). Feed `max_items` moves to the plan. The rest stays,
   reordered.
9. **The masthead shows the next edition's date.** The mockup pairs "in 14 h 22 m" with today's
   date and No. 295. At 15:08 on the 22nd, the next press is Friday the 23rd, No. 296.
10. **"Preview" is unchanged:** it builds today's edition now. Building with tomorrow's date
    would misalign the forecast.
11. **The Serial, From Your Shelf and the Puzzle** appear in the page plan only once their
    `SectionKind` exists. "All editions" appears only once the Morgue's card does.

## 12. Left to the builder

- Module boundaries inside `next`, Tauri command names, the tile and column CSS, and the drag
  feedback.
- How "sent" is inferred for the week (or dropped to "no paper").
- Tuning `POSITIONS_PER_PAGE` once on the Paperwhite.

The builder also updates the docs: "Tomorrow's paper" in `docs/daily.md` (live, page plan, stop
the presses), the CLI lines in the README, and the CHANGELOG.
