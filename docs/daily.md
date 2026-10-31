# The Deckle Daily

A morning paper built on your machine and delivered to your Kindle. This page holds the full
reference; the README has the overview.

In the app: the **Daily** tab shows tomorrow's paper as it stands (below), sets up the paper
(name, place, feeds), saves links to Your Reading, copies the bookmarklet, sets the press time, and
builds or sends an edition on demand. Everything below is the same thing from the command line; both edit the same
`digest.toml`, and your own comments in it are kept.

A morning paper built on your machine: weather (Open-Meteo), local news (a Google News search
plus any local feeds), your RSS/Atom feeds, Powerball and Mega Millions, a horoscope Deckle
writes itself, a puzzle page, and "On this day" from Wikipedia. Its cover is a generated front page, so the
Kindle library tile is today's paper. Every story comes with its full text and photos (grayscale
for e-ink), pulled Reader-View style from the publisher, so nothing sends you to a browser; a
story behind a paywall stays a headline. No keys or accounts; a source that fails prints
"unavailable today" and the paper still ships.

```sh
deckle digest init                  # starter config: ~/Library/Application Support/deckle/digest.toml
deckle digest build -o ./out        # today's edition as an EPUB
deckle digest run --to device       # build, convert, send (cover first), keep the newest 3
deckle digest push                  # deliver a queued edition to the Kindle now connected
deckle digest puzzle                # today's puzzle, and yesterday's answers
deckle digest shelf                 # what From Your Shelf would print now (--forget deletes it)
deckle digest morgue search mill    # every kept edition: list, search, show, send again
deckle digest watch                 # keep running; deliver whenever a Kindle connects
deckle digest schedule --at 05:30   # both, automatically: the paper daily, the watcher from login
deckle digest schedule --off        # and gone again
deckle digest next [--fetch]        # tomorrow's paper so far (--fetch checks the feeds first)
deckle digest stop [--undo]         # skip the next scheduled paper, once (or restart the presses)
```

`schedule` installs two LaunchAgents on macOS (or two Task Scheduler tasks on Windows) that run the
`deckle` you scheduled from, so schedule from a stable one: `cargo install --path crates/deckle-cli`.
A Mac asleep at 5:30 builds the paper when it wakes. Logs: `~/Library/Logs/Deckle/`.

**Save to tomorrow's paper.** Anything you save during the day prints in the next edition as
*Your Reading*, whole and with its photos; pieces over 4,000 words become their own book that rides
along with the paper.

```sh
deckle save https://example.com/story     # or a saved .html page (sites you're signed into)
deckle save --list                        # what's waiting, and what printed
deckle save --bookmarklet                 # bookmarklets for your browser (needs `digest watch`)
```

Or drop links (`.webloc`, `.url`, `.txt`) and saved pages into `~/Deckle/Inbox`. Point
`[reading] inbox` in `digest.toml` at an iCloud Drive folder and an iPhone Shortcut ("Save File")
saves from your phone. The queue is a plain file, `reading.jsonl`, next to `digest.toml`.

`deckle save --bookmarklet` prints two bookmarklets: **Save to paper** (above) and **Save recipe**,
which sends the page to the cookbook instead (see below). Both send only the page's address and
title, never its contents, and both need `digest watch` running.

The same Inbox takes books: drop an `.epub`, `.docx` or `.kfx` there and it's sent later. The
file's extension decides: `.epub`, `.docx` and `.kfx` are books, and `.webloc`, `.url`, `.txt`
and `.html` are articles. Each book is converted like `deckle run` would (a KFX is copied as it
is) and waits in the outbox with the paper; see below for how it's finished on push. It moves to `Inbox/Sent`, or to `Inbox/Unreadable`
with a `.txt` saying why it couldn't be converted. Books need Kindle Previewer; without it they
stay in the Inbox until it's installed.

Recipes have their own folder, `Inbox/Recipes` (made on the first scan): links (`.webloc`, `.url`,
`.txt`) and saved pages (`.html`) dropped there go to the cookbook instead of Your Reading, and
move to `Inbox/Recipes/Sent`, or to `Inbox/Recipes/Unreadable` with a `.txt` saying why. Only
`watch` adds them (not `build`, `run` or `push`). A link it can't fetch yet, such as one that
syncs in before Wi-Fi is up, waits and is retried for a day; a page that's gone (404) or isn't a
recipe goes to `Unreadable` at once. Deckle doesn't guess which kind a link is: the folder you drop it in decides. An iPhone Shortcut pointed
at the iCloud Inbox's `Recipes` folder saves recipes from your phone. `Recipes` is reserved, so a
Kindle named "Recipes" gets no Inbox folder of its own (`watch` says so once). The Save recipe
bookmarklet drops its link there too. See [the cookbook](cookbook.md).

No Kindle plugged in at build time? The edition waits in the outbox and goes over on the next
`push`, or as soon as you plug in while `watch` is running.

Long reads and Inbox books are finished for the Kindle that takes them: the slow Previewer step
runs when they're queued, and the push picks PDOC or EBOK for that Kindle (and the per-Kindle
mode you set in the app), adding X-Ray and Word Wise where it ships as EBOK. That step stays
offline: basic X-Ray, no Amazon match, and Word Wise only once its database has been downloaded
by a `deckle run`. If finishing fails, the book still goes over as a personal document with its
cover. The paper itself is always PDOC.

What `watch` does on plug-in is set per Kindle in `digest.toml`: `[sync]` holds the defaults
(`push = true`, `prune = true`, `clippings = false`), and `[sync.devices."<sync id>"]` overrides
them for one device, using the id `deckle devices` shows. Push, prune and clippings share one
connection. `clippings = true` writes the Kindle's highlights as Markdown into the highlights
folder (set it once with `deckle highlights export --to <dir>`, or in **Settings → Kindles**). It
runs once per plug-in, in the same connection as a push or a prune when there is one.

### Tomorrow's paper, so far

The Daily tab is in rooms, listed down its left side, each with a line on what's in it: **Today**
(tonight's paper), **Editions** (every edition and how far you read), **Reading** (Your Reading and
From Your Shelf), **Delivery** (where the paper goes) and **The paper** (its settings and Your
Day). It opens on Today, the **next** edition, not the last one:

- **The press bar** says when the paper goes to press ("Goes to press at 05:30 · in 14 h 22 m"),
  with the time and the switch for the morning schedule.
- **Stop the presses** skips the next scheduled paper, once (`deckle digest stop`). The stop is a
  date in `press.json` next to `digest.toml`: it lapses by itself after that morning, and
  **Restart the presses** (`deckle digest stop --undo`) lifts it before. A Mac asleep through
  press time still skips it. **Send now** always sends (`deckle digest run --force`).
- **Tomorrow's paper, so far** is a sketch of the front page: the masthead with the next
  edition's number and date, the lead with one line saying why it leads, then a tile per saved
  article and section in page-plan order. Saved articles appear at once. Your feeds' headlines
  appear once fetched: their RSS only, each feed at most twice an hour, and only while the tab is
  on screen. Weather, the lottery, the horoscope and On This Day fill in at press time.
- **The lead rule** is the press's own: local news leads when there is some; otherwise the first
  section in your page plan with a story, saved articles included.
- **The page plan** lists the sections in order with their budgets. Drag a row (or use the
  arrows) to reorder; pick how many stories Your Reading, Local and each feed print. Each edit
  writes `digest.toml` at once, keeping your comments. "About N Kindle pages" comes from the sizes
  of your own past editions (Readership), or from rough defaults until there are some.
- **This week**: how far you read each of the last six papers, from Readership.

`deckle digest next` prints the same view (`--json` for data). It only reads files; `--fetch`
checks the feeds first, under the same twice-an-hour limit.

### Topics and feeds

Each `[[feeds]]` entry is its own section, in order. Besides a plain RSS/Atom `url`, an entry
can take a Google News **topic** (`WORLD`, `NATION`, `BUSINESS`, `TECHNOLOGY`, `ENTERTAINMENT`,
`SPORTS`, `SCIENCE`, `HEALTH`), more feeds in `urls`, and a `full_text` setting. Tick topics in
the Daily tab (**The paper → Topics**) or write them in `digest.toml`:

```toml
[[feeds]]
name = "Sports"
topic = "SPORTS"
urls = ["https://example.com/more.rss"]   # optional extra feeds, merged in
full_text = "lead"              # true, false or "lead"
# max_items = 5                 # stories; default 5 for a topic, 10 for a plain feed
```

- Every source of an entry (the topic, `url`, `urls`) is merged, newest first, and a story that
  appears in several is printed once (matched by title).
- `full_text = "lead"` is the default for a topic: the top story prints whole, the rest as short
  briefs. `true` prints every story whole, `false` none. A plain `url` entry follows the global
  full-text setting unless you set it.
- **No repeats across sections:** Local, a `[[feeds]]` section and a subscribed paper's section drop
  a story whose title an earlier news section already printed, then refill from their next-newest
  stories. Other places keep their own stories and only record their titles; Your Reading is never
  trimmed or recorded.

### Your interests

A topic section gives you everyone's sports. An **interest** gives you yours: "Carolina
Panthers", "Rust", "e-ink readers", "Moore County Schools". Each `[[interests]]` entry is its own
section, and every morning Deckle searches Google News for it (the name in quotes, or your own
`query`), merges in any `also` searches and `feeds`, keeps only the stories whose headline names
the interest, and prints the top story whole with the rest as briefs. Add them in the Daily tab
(**The paper → Your interests**), from the terminal, or in the file:

```toml
sections = ["reading", "weather", "local", "interests", "feeds", ...]

[[interests]]
name = "Carolina Panthers"      # the section's title; the search is the name in quotes
# query = "\"Carolina Panthers\" NFL"   # your own search instead; Google's OR and -word work
# also = ["Bryce Young"]        # more searches, merged; a story naming one is relevant
# not = ["Florida Panthers"]    # a story whose headline has one of these is dropped
# feeds = []                    # RSS feeds merged in
# max_items = 5
# full_text = "lead"            # top story whole, the rest briefs; true or false
# max_age_hours = 72            # interests move slower than the wires
```

- **Relevance:** Google's search is loose, so a story stays only if its headline (or a feed
  item's description) names the interest or an `also` phrase, as whole words: "Rust" never
  matches "trust". The top story's own text is checked again after it's fetched; one that turns
  out to be about the wrong Rust is dropped and the next is tried.
- **`not`** is the reader's word: "Florida Panthers" keeps the hockey team out. A one-word name
  with everyday meanings ("Rust" is also a surname, a film and what happens to iron) wants its own
  `query`: the card's search field, or `--query` from the terminal.
- **A quiet day** leaves the section out of the paper rather than printing an empty one; the
  build notes "Nothing new about Rust today." A source that failed prints "Unavailable today"
  as any section does.
- Interests sit before your feeds in the starter, so your interest wins a story over Sports.
  Each is a row in the page plan (budget editable there) and in How far you read.
- **With the AI editor on** (`[llm]`), the editor also scores each interest story: about the
  interest, merely mentioning it, or about something else that shares the words. The last kind
  is dropped and the rest are ordered by score; a section with nothing left prints "Nothing new
  about Rust today." Only headlines and excerpts go to the provider, never your `not` list.
- Interests live in each paper's own file, so Dad's paper has Dad's.

```bash
deckle digest interests                       # list them
deckle digest interests add "Rust" --query '"Rust" programming' --also "Cargo"
deckle digest interests remove "Rust"
deckle digest --paper kid-courier interests add "Dinosaurs"
```

The paper noticing what you read and suggesting new interests, and accepting one with a tap on
the Kindle, come in later increments (`docs/superpowers/specs/2026-10-25-interests-design.md`).

### The front page and section fronts

The first page is laid out like a paper's front: the lead story, then a two-column table of
headlines from the sections, then **Inside today** linking to each section. A section with two or
more stories opens with **In this section**, a list of its stories. Every story ends with
**↩ <Section> · ↩ Front page** links back to the section's front and the paper's front page.

Caveats: the ↩ glyph may not render in every Kindle font (it needs a fallback; check on yours).
The look on e-ink hasn't been checked on a device yet (see `docs/devices.md`). And since the
Kindle records the furthest position you reached, jumping from the front page to a late section
counts every earlier section as reached in *How far you read* (see below).

### Other places

Keep up with a relative's town or a trip: each extra place gets its own local news section,
printed right after your own Local and built the same way (its Google News search and any local
feeds, merged, newest first). The search is the place's name unless you give your own. Weather, the almanac and the cover stay on your place. Add places in
the Daily tab (**The paper → Other places**), or in `digest.toml`:

```toml
[[local.places]]
name = "Asheville, NC"          # the section's title
google_news = "Asheville NC"    # optional; empty searches the name
feeds = []                      # local paper RSS feeds, merged in
# max_items = 6                 # stories; default: [local] max_items
```

Each place is its own row in the page plan (its story budget is editable there) and in How far
you read, where a Drop suggestion removes just that place. Turning Local off turns the places off
with it.

### How far you read

Each time a Kindle connects (`watch`, `push`, or the Kindle tab), Deckle reads the reading
position the Kindle keeps for every edition still on it: the `.yjf` file in the edition's `.sdr`
folder. It does this first in the connection, before prune removes old editions. When an edition
is built, Deckle notes where each of its sections starts. Put together, the Daily tab's
**Editions → How far you read** panel and `deckle digest readership` show, over the last 14
editions, how many you opened, how far in you usually get, and per section how often you got to
it. With 7 opened editions it suggests up to three edits: **Drop it** for a section near the end
you rarely reach, **Move up** for a feed you rarely reach. One click (or `deckle digest readership apply
<n>`) changes `digest.toml`, keeping its comments; **Dismiss** (`… dismiss <n>`) hides one for
14 editions.

- It's on by default (`[sync] readership = true`, per device under `[sync.devices."<sync id>"]`,
  or **Settings → Kindles**). Everything stays in `readership.json` next to `digest.toml`.
- "Got to" means got to or past: the Kindle moves its furthest position on a contents jump too,
  so jumping to *On This Day* counts every section before it as reached. That can hide a Drop
  suggestion, never invent one.
- Editions that went by email only aren't tracked: no KFX is built for them on this machine, so
  there's nothing to compare the position against (`usb_too = true` editions are).

### Deckle noticed

The paper notices what you read and suggests new interests, on this Mac only. Each time your
Kindle connects, Deckle looks at three things: the stories you jumped back to the front from
(the Kindle keeps the last few page jumps in its reading data), the passages you highlighted
in the paper itself (from `My Clippings.txt`), and the authors of the books in progress on your
shelf. A subject that comes up in three stories on two different days within two weeks becomes
a suggestion under **Deckle noticed** in the Editions room, beside How far you read:

> **Bryce Young** · You read 4 stories about Bryce Young in the last 14 days.
> [Add as interest] [Not interested]

- **Add as interest** writes the `[[interests]]` entry (an author gets a narrower search,
  `"Brandon Sanderson" author OR book`) and puts `interests` in `sections` if it wasn't there.
- **Not interested** sets the subject aside for 90 days.
- Nothing changes the paper on its own; a suggestion waits for a click.
- What isn't a signal: how far you read (reading straight through reaches every story),
  highlights in books, and words you looked up.

It's on by default and lives in `interests.json` beside `readership.json` (per paper). Turn it
off in **The paper → Your interests** or with `[learning] enabled = false`. Taps need
`[sync] readership`; highlights are read from the Kindle whenever learning is on.

```bash
deckle digest interests                      # interests, then what Deckle noticed
deckle digest interests accept "Bryce Young"
deckle digest interests dismiss "Bryce Young"
deckle digest interests why "Bryce Young"    # the stories behind it
```

Renaming an interest in the app keeps its history: its readership rows and what Deckle noticed
move to the new name.

**Nudges in the paper** (`[learning] nudges = true`, off by default, or the second checkbox
under Your interests): the paper itself carries what Deckle noticed, as its last pages, one per
suggestion. Each says why and has one link, "Tap here to make Bryce Young a section.", which
opens a confirmation page with one link back to the front. The Kindle keeps the page a jump
was made from, so at the next connection those two jumps add the interest, exactly as the
app's button would, and the run notes "Added Bryce Young to your interests (you tapped it in
the paper)." Turning pages can't accept, and neither can leaving the page by Go To. Kindle
only: other outputs get the paper without these pages. It ships off until a tap has been
confirmed in a Kindle's reading data (see the design spec).

With the AI editor on, the morning build also asks it to name what Deckle noticed, from the
headlines only ("Young" becomes "Bryce Young", with a search that finds him and not every
Young), so the suggestion reads properly and **Add as interest** uses that search. Highlighted
text and your shelf never go to the provider.

### The weather page

The **Weather & Almanac** page has a five-day strip (a picture, the high and low, and a word or
two such as "Showers, pm" or "Frost at dawn"), what it's like right now, and an almanac:

- Sunrise, sunset and daylight, with the change from yesterday.
- The usual first (or last) frost, in the 45 days before it.
- The moon: phase, how much is lit, and the next full or new moon with its traditional name
  (Harvest, Hunter's, Blue Moon; northern hemisphere only).
- An equinox, solstice or clock change in the next 7 days.
- A meteor shower paragraph when one is active.

Only the forecast comes from the network (Open-Meteo, the same single request as before).
**The sun, moon, seasons and showers are worked out on your Mac**, so the almanac still prints
when the forecast is down.

The frost dates come from a 30-year climate record (1991–2020) for your place: one request to
Open-Meteo's archive, the first time, cached in `almanac.json` next to `digest.toml`. A place
where frost is rare (under 15 years in 30) gets no frost line.

**Your Day** adds today's events from your calendars, plus one line when rain or snow is likely
during one of them ("Rain likely around 4 PM, during School pickup."). Private and confidential
events print as "Busy". Add calendars in the Daily tab's **The paper → Your Day**, or in `digest.toml`:

```toml
[day]
calendars = ["~/Calendars/Home.ics", "webcal://p01-caldav.icloud.com/published/2/…"]
max_items = 8
weather_note = true
```

A calendar is an `.ics` file or a private address (`https://` or `webcal://`). To find yours:

- **Google Calendar:** Settings → the calendar → *Integrate calendar* → *Secret address in
  iCal format*.
- **iCloud:** in Calendar, the calendar's share button → *Public Calendar* → copy the link.
- **Outlook:** Settings → *Calendar* → *Shared calendars* → *Publish a calendar* → the ICS link.

A private address lets anyone holding it read that calendar, so Deckle never writes it into its
messages or logs; it shows only the host (`https://calendar.google.com/…`). Calendars are read when
the paper is built and the events go into that edition only. **One exception:** with email delivery
on, the paper (agenda included) passes through Amazon on its way to your Kindle.

### The puzzle page

Near the back of the paper, after the horoscope: **The Mini**, a 5×5 crossword, and a **word
ladder** (one 4-letter word into another, a letter a rung). Up to two of the mini's answers come
from yesterday's headlines, clued as the headline with the word blanked: "Leaf ____ peaks on the
ridge this weekend (Local, yesterday)". The rest are clued from a short dictionary definition.
The next paper prints **Yesterday's answers**: the filled grid and the ladder.

- **Check your answers on a Kindle that keeps notes.** Highlight a clue, add a note with your
  answer (case, spaces and punctuation don't matter; for the ladder, highlight its line and write
  the whole path). The next time that Kindle connects (`watch`, `push`, or the Kindle tab), Deckle
  reads the notes from `My Clippings.txt`, and the next paper says "You solved 4 of 10". This
  runs whenever the puzzle is in `sections`, even with `[sync] clippings = false` (no highlights
  are exported then). A Kindle Scribe (2024) keeps its notes where they can't be read over USB:
  you get the answers without a score, and the hint line goes away.
- **The same puzzle all day.** Each day's puzzle is made once and kept in `puzzles.json` next to
  `digest.toml` (the last 60 days), so every rebuild, the email copy and tomorrow's answers
  match what you saw. `deckle digest puzzle` prints today's (and yesterday's answers);
  `--json` gives the same as data.
- **Offline.** The words come from a list built into Deckle from Wiktionary and wordfreq (CC BY-SA
  4.0); nothing about your paper leaves your Mac for the puzzle. If a mini can't be made, the
  ladder still prints; if nothing can, the page says "Unavailable today".

It's in the starter config. An existing `digest.toml` doesn't get it on its own: add `"puzzle"`
to `sections`, or switch it on in the Daily tab. Optional settings:

```toml
[puzzle]
ladder = true
theme = true      # clues from yesterday's headlines
```

### The Morgue

Every edition you've had, kept on purpose. A newspaper's morgue is its archive of back issues;
Deckle's is a plain folder, `~/Deckle/Morgue` (next to the Inbox), that you can see, back up or
point at iCloud Drive. The Kindle keeps only the newest three, and the cache folder is the kind
of place a cleaner empties.

- **What's kept.** Each build (`digest build`, `run`, the schedule, the app's Build) copies its
  EPUB into `YYYY/YYYY-MM-DD.epub` with a small `YYYY-MM-DD.json` beside it holding the
  headlines, teasers and each story's text. After conversion the KFX is kept too, for fast
  re-sending, for two weeks. The first time it runs, Deckle also brings in the editions still in
  the cache folder.
- **A year by default.** `[morgue] keep_days = 365` (`0` keeps everything). Only files named
  exactly `YYYY/YYYY-MM-DD.epub`, `.json` or `.kfx` are ever deleted; notes or renamed copies you
  put in the folder are left alone and never listed. A folder that can't be written (iCloud
  offline, a full disk) doesn't fail the paper: the run says "Morgue: couldn't save the edition
  (…)".
- **What's in the files.** The sidecars and EPUBs hold the full text of the "Your Reading"
  articles you saved, not just headlines. If you point the folder at iCloud Drive or another
  synced folder, that text syncs with it.
- **Every edition, in the Daily tab's Editions room.** A month of front pages, each with the paper's name and its
  lead headline; days with no paper are blank, and the next one says "Tomorrow, 05:30". Type a
  word and the days with a match light up, with the matching stories listed. Click a day for its
  weather and lead, how far you read ("You read about 91%, to Local"), how many passages you
  highlighted, and whether it's still on the Kindle, with **Send again**, **Open** and **Show in
  Finder**. The card's footer shows the folder and its size and lets you change the folder.
- **Search.** Every word has to match, ignoring case and accents, in a story's headline, teaser or
  full text: `mill brewery` finds the stories with both, `cafe` finds "Café". From the terminal:
  `deckle digest morgue search mill brewery`, `deckle digest morgue list --month 2026-10`,
  `deckle digest morgue show 2026-10-21`. Add `--json` for data.
- **Send again.** `deckle digest morgue send 2026-10-21` (or the button) queues the kept KFX, or
  converts the kept EPUB first, as `deckle-daily-2026-10-21.kfx`. It goes over the next time a
  Kindle connects and then stays for a week, even though it's older than the newest three; after
  that it's pruned like any other.
- **What the counts are.** "Passages highlighted" counts that edition's highlights in the last
  `My Clippings.txt` read (so it needs the highlights, puzzle or shelf reads on, and a Kindle that
  keeps the file; a Scribe doesn't). "On the Kindle" comes from the reading positions read at the
  last connection. Both live in `morgue.json` next to `digest.toml`, and an edition with no
  record shows no such line.

```toml
[morgue]
folder = "~/Deckle/Morgue"
keep_days = 365
```

### From Your Shelf

A page drawn from your own Kindle, after your feeds: **On your nightstand** (the books you're in the
middle of, how far in, how long since you opened them, and a nudge back into the newest one),
**You highlighted this … ago** (a passage from at least six months back, with your note under it; when nothing that old
qualifies, one at least 30 days old is used instead)
and **Words you looked up this week**.

- **It's as of the last plug-in.** The page says which Kindle and when ("Taken from your Kindle
  (Scribe) when it was last plugged in, Wednesday"). Deckle reads the Kindle when it connects
  (`watch`, `push`, or the Kindle tab) and keeps one snapshot per Kindle in `shelf.json` next to
  `digest.toml`. A snapshot older than 30 days isn't printed.
- **KFX, MOBI and AZW3 books, up to three folders down.** A book Deckle can't open (over MTP,
  or a MOBI) shows a title read from its file name, and a percent from the Kindle's own
  progress ("about 15%"). PDFs and text files have no reading position and don't appear.
- **The nightstand works on every Kindle, the Scribe included.** It comes from the reading
  positions the Kindle keeps beside each book. The highlight and the words appear only on Kindles
  that share them: a Paperwhite over USB does; the Scribe 2024 doesn't over USB (#99), and the
  Daily tab says "Highlights and words aren't shared by this Kindle." instead of failing.
- **Percent.** Exact for books Deckle sent and for any KFX on a USB-storage Kindle; a book with
  no known end shows its last-read day without a percent.
- **More than one Kindle.** Each is told apart by its USB serial number, so two Paperwhites keep
  separate snapshots (`deckle devices` shows each one's sync id, `<model>/<volume>#<serial>`).
  If the serial can't be read (an unusual OS setup), a Kindle is known by its model and volume
  name, as before, and two of one model would share a snapshot. Settings written for the older
  id (`[sync.devices."Kindle/Kindle"]`) still apply to every serial-id Kindle of that model.
  Give each Kindle the name the page should use with `[sync.devices."<id>"] name = "Mom's Kindle"`;
  without one the page says "Kindle", or the Scribe's model.
- **What it shares.** The page reads only books, never the Kindle's account. Turn it off for
  one Kindle with `[sync.devices."<id>"] shelf = false`, for all of them under Settings →
  Kindles, or leave `"shelf"` out of `sections`. **Forget what my Kindles shared** on the Daily
  tab (or `deckle digest shelf --forget`) deletes every snapshot.
- **Email.** With email delivery on, the paper (this page included) passes through Amazon.
- **Where the words' definitions come from.** A short definition only from a Word Wise database
  already on this Mac; Deckle never downloads one for this.

It's in the starter config. An existing `digest.toml` doesn't get it on its own: add `"shelf"` to
`sections`, or switch it on in the Daily tab. Optional settings:

```toml
[shelf]
nightstand = true
highlights = true
words = true
hide = []         # titles containing any of these stay off the nightstand

[sync]
shelf = true      # read the books (and what the Kindle shares) when it connects
```

Unlike books, the digest goes online by design: each edition fetches the sources in your config.

## Many papers: one for each Kindle

One Mac can print a separate paper for each Kindle in the house. `digest.toml` stays the main
paper. Each other paper is a file of its own in `papers/` beside it:

```bash
deckle digest papers new --title "The Kid Courier" --for "Maya"   # writes papers/kid-courier.toml
deckle digest papers                                              # lists every paper
deckle digest --paper kid-courier run                             # builds that one now
deckle digest --paper kid-courier schedule --at 07:00 --days sat  # Saturdays at 7
deckle digest schedule                                            # every paper's job, and the watcher
```

A paper's file name, its *slug*, is lowercase letters, digits and dashes (`kid-courier`). Each
paper has its own title, sections, feeds, place, `keep`, export folders, reMarkable and email
`to`, plus three new keys:

```toml
reader = "Maya"                   # who it's for
kindle = "Mom's Kindle"           # only this Kindle gets it (its name or sync id); leave out for any
large_print = true                # a layout for big type (below)
```

- **Shared settings.** `[sync]`, `[reading]` (the Inbox and saved articles) and the email
  account (`from`, `smtp_host`, `smtp_port`, `username`) are household settings. They're set
  in `digest.toml` only, and every paper uses them. A paper file that sets one is refused,
  with a message saying which.
- **Saved articles** print in the main paper only.
- **Bound papers.** A paper with `kindle` set waits in the outbox until that Kindle connects,
  and no other Kindle takes it. `push` says "1 book waits for Mom's Kindle". Name a Kindle
  first with `deckle devices name`. A bound paper doesn't go to a Kobo, PocketBook, reMarkable
  or export folder.
- **Pruning.** Each paper's editions are named `deckle-daily-<slug>-YYYY-MM-DD`. Each paper is
  pruned by its own `keep`, so one paper never removes another's.
- **Per-paper state.** Each paper keeps its own readership and puzzles in `papers/<slug>/`, and
  its Morgue editions in a `<slug>/` folder inside the Morgue.
- **Schedules.** Each paper has its own job. `schedule --at` saves `[schedule] at` (and
  `days`) in that paper's file and installs every paper's job, plus the one watcher. On macOS
  that's a LaunchAgent per paper (`com.casavargas.deckle.digest.<slug>`); on Windows, a task
  (`\Deckle\Digest-<slug>`). A main paper you scheduled before many papers keeps its job
  until you set its time again. `schedule --off` removes everything; with `--paper`, only that
  paper's job.
- **Large print.** `large_print = true` lays the paper out for big type: nothing smaller than
  the body text, bigger headlines, heavier rules, the five-day forecast as a list (one day a
  line), and one photo a story. Set the text size on the Kindle itself (Aa). It applies to
  every book on that Kindle.
- **For now:** the `digest morgue` commands work on the main paper only. The app's Your papers
  is coming.
- **Removing a paper:** delete its file, then run `deckle digest schedule --at …` for any
  paper (or `--off`) so its job goes too.

## The title's typeface

The paper's name is drawn on the cover, and at the top of the PDF edition, in one of ten faces
that come with Deckle. The app shows each with your paper's name in it, under **Paper settings →
Masthead** (the paper menu on the Daily tab). In `digest.toml` it's `masthead`:

```toml
masthead = "consent"   # the default: Manufacturing Consent
# Blackletter: "consent", "grenze" (Grenze Gotisch), "pirata" (Pirata One), "texturina",
#              "unifraktur" (UnifrakturMaguntia, the old default; "blackletter" still means it)
# No blackletter: "playfair", "bodoni", "caslon" (Libre Caslon Display),
#                 "serif" (Old Standard Bold), "sans" (Inter Bold)
# Or the path to a .ttf file, e.g. "~/Fonts/Fraktur.ttf"
```

- Every face is under the SIL Open Font License; each licence is beside its font in
  `crates/deckle-digest/fonts`.
- Only the title changes: the date band, the ears and the headlines keep their own faces.
- A file must be **TrueType (`.ttf`)**; `.otf` and `.ttc` aren't read.
- A missing file, one that isn't TrueType, or a face without a letter of your title falls back
  to Manufacturing Consent, then to Old Standard when that lacks a letter too, and the run says
  why ("Masthead font: …"). The paper still comes out. The app marks the faces that can't draw
  your title.
- **Inside the book the title is your Kindle's own font**, because a Kindle sets the text in the
  font you picked on it (Aa → Font). This setting can't reach that.
- A long read's "Deckle Reads" cover uses Manufacturing Consent.

## Email to Kindle (opt-in)

With no cable, the paper can arrive over Wi-Fi: `digest run` emails its EPUB to your Kindle's
Send-to-Kindle address (`…@kindle.com`) through your own mail account. This one is labelled
because the paper then goes through Amazon, which converts it with its own typesetting. Long reads
and Inbox books stay on USB, where they get X-Ray and Word Wise.
(`docs/superpowers/specs/2026-10-21-send-to-kindle-email-design.md`)

1. Uncomment `[email]` in `digest.toml` (or fill in the **Delivery → Email to Kindle** on the Daily tab):
   `to`, `from`, `smtp_host`, `smtp_port` (587 STARTTLS or 465 TLS; plain-text SMTP is refused),
   `username`. `to` must end in `@kindle.com` or `@free.kindle.com`.
2. Save the password in the system keychain: `deckle digest email password` (asks without echo;
   `--stdin` for scripts), or the card's password field. It's never written to `digest.toml`.
   Gmail and iCloud need an app password, made with 2-step verification on:
   [Google](https://myaccount.google.com/apppasswords),
   [Apple](https://support.apple.com/en-us/102654).
3. Add the `from` address to Amazon's Approved Personal Document E-mail List (*Manage Your Content
   and Devices → Preferences → Personal Document Settings*), or Amazon drops the email.
4. `deckle digest email test` (or **Send a test**) sends a one-page EPUB. Then set
   `enabled = true`.

```sh
deckle digest email password [--stdin]   # save the SMTP password in the keychain
deckle digest email test                 # a one-page test EPUB to your Kindle
deckle digest email forget               # delete the keychain entry
```

When the email goes out, the paper skips the USB outbox (`usb_too = true` sends both, so two
copies). When it fails (no password, login refused, no network, over 45 MB), the paper is queued
for USB as before and the run's report says why. `--to <folder>` and `digest push` never email.

- **Emailed papers pile up in the Kindle's Docs.** Amazon gives Deckle no way to delete them, so
  `keep` only prunes USB editions; clear old ones in
  [Manage Your Content and Devices](https://www.amazon.com/mycd).
- **The keychain entry** is `com.casavargas.deckle.smtp`, account `<username>@<smtp_host>`. With no
  keychain at all (Linux without Secret Service, CI), `DECKLE_SMTP_PASSWORD` is read instead and
  the report says so.
- **macOS dev builds:** the scheduled run uses the app's own signed binary, which reads the entry
  the app wrote with no prompt. A separately built and signed `deckle` (e.g. `cargo install`)
  makes macOS ask once whether "deckle wants to use your confidential information": choose
  **Always Allow**, or the 5:30 run waits on that dialog.

## Papers you subscribe to

If you pay for a newspaper's website, the Daily can sign in as you and read its full articles.
Most small-town US papers run on BLOX (TownNews): sign in once
(`deckle digest subscribe`) and the paper's stories print in full. A story your plan doesn't
cover, or one the site withholds, prints as a stranger sees it, with a note saying why. Any other
site's stories print as a stranger would see them: a free story in full, and a story the site
withholds as a headline with a note saying why. A saved article the site withholds waits in Your
Reading without using up its tries.
(`docs/superpowers/specs/2026-10-25-publisher-subscriptions-design.md`)

Add the account to `digest.toml` (one block per site; the password is not stored here):

```toml
# A newspaper you subscribe to: the paper signs in as you to read its full articles. The
# password is not stored here; it goes in the system keychain. One block per site.
# [[subscriptions]]
# site = "yourpaper.com"          # just the host, no https:// or path
# username = "you@example.com"
```

Print the paper's own sections as a group in your paper with `[[subscribed]]` (in `digest.toml`
or a paper's file under `papers/`; the site should also have a `[[subscriptions]]` account):

```toml
# Print a subscribed newspaper's own sections in full, as a group in the paper. The site
# should also be in [[subscriptions]] above. Sections for a BLOX site: news, sports, opinion,
# obituaries, business, lifestyles.
# [[subscribed]]
# site = "yourpaper.com"
# name = "Your Paper"             # optional heading; the site is used without it
# sections = ["news", "sports"]   # in print order
# max_items = 8                   # per section; defaults to [local] max_items
# max_age_hours = 48              # defaults to [local] max_age_hours
```

The group prints as its own section, in file order, with each story's kicker carrying the
paper's name. `max_items` is stories per section and `max_age_hours` the age window; both
default to `[local]`'s. The `subscribed` section sits in `sections` right after `local` (add or
move it there like any other), and the **Print this paper** switch on the Subscriptions card adds
the block and lets you tick its sections and set the stories per section. A story already printed
by an earlier section is skipped (other places excepted: they keep their own stories), so the paper
doesn't repeat itself; each section asks the site for five stories beyond its budget to refill from.

Removing a subscription on the card drops this paper's group only. Another paper's file under
`papers/` keeps its `[[subscribed]]` block, and that paper then prints the group as a stranger
sees it, with the note below.

If the site has a `[[subscribed]]` block but no account in `[[subscriptions]]`, the edition says so
("No subscription for <site> in digest.toml") and the stories print as a stranger sees them.

```sh
deckle digest subscribe                     # list your sites and their state
deckle digest subscribe yourpaper.com --username you@example.com
                                            # add or update the account, ask for the password
                                            # without echo, save it, sign in once
deckle digest subscribe yourpaper.com --stdin   # read the password from one line of stdin
deckle digest subscribe yourpaper.com --test    # sign in only, to check the password
deckle digest subscribe yourpaper.com --forget  # remove the account, keychain entry and session
```

The same fields are in **Daily → The paper → Subscriptions**: site, username, a write-only
password, **Save password**, **Test sign-in** and **Remove**. `deckle digest lint <url>` prints
`signed in as <user> at <site>` (or why it couldn't) before its usual report.

- **The password** lives in the system keychain: service `com.casavargas.deckle.site`, account
  `<username>@<site>`. With no keychain at all (Linux without Secret Service, CI),
  `DECKLE_SITE_PASSWORD_<SITE>` is read instead: the site uppercased, anything that isn't a letter
  or digit as `_` (`DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE`).
- **The session** (the cookies from signing in) is kept in `sessions/<site>.json` beside
  `digest.toml`, one file per site, readable only by you, and only ever sent to the site over
  HTTPS (a plain `http://` link is fetched signed out). Delete it to sign out; the next run signs
  in again. `--forget` deletes it too, and works without a keychain (there is just no password to
  delete).
- **The site** must be a bare host: no user name, port or trailing dot (`yourpaper.com`).

## Export folders: an iPad, a Kobo, a Boox, any folder

The paper doesn't have to go to a Kindle. Each `[[export]]` folder gets every morning's edition
as an EPUB, plugged in or not, and keeps the newest few. An iCloud Drive folder puts it in an
iPad's Files app (tap it to open it in Books); a Dropbox or Google Drive folder reaches a Boox.
(`docs/superpowers/specs/2026-10-23-other-readers-design.md`)

**A Kobo without the cable.** A Kobo linked to Dropbox
([Kobo's steps](https://help.kobo.com/hc/articles/360033830114)) fetches what's in Dropbox's
`Apps/Rakuten Kobo` folder when you tap **Sync**. Point an export there with `format = "kepub"`
and the paper is waiting each morning after one tap. Picking that folder in the app sets the
KEPUB and gray photos for you. Pruning only cleans the Dropbox folder: an edition the Kobo already
fetched stays on the Kobo until you remove it there. Not yet checked on a Kobo
(`docs/devices.md`).

```toml
[[export]]
name = "iPad"
dir = "~/Library/Mobile Documents/com~apple~CloudDocs/Deckle Daily"
format = "epub"                  # or "kepub" for a Kobo; "pdf": laid out for one screen (below)
photos = "color"                 # or "gray" for an e-ink reader
keep = 7                         # editions kept in the folder; 0 keeps all
books = false                    # also the long reads, not only the paper
```

Or use **Delivery → Export folders** on the Daily tab: **Add folder…** opens on iCloud Drive on a
Mac.

- **Color.** With any `photos = "color"` export, the stories' photos are kept in color and the
  run also writes a color edition beside the paper (`color/` in the build folder). The Kindle's
  paper, the Morgue and email still get the grayscale one.
- **No Kindle Previewer needed.** Without Previewer, `digest run` still builds the paper and puts
  it in the export folders, says "no Kindle copy" in its notes, and queues nothing for a Kindle.
  Long reads then print inside the paper, unless an export takes them (`books = true`). With
  Previewer installed, the Kindle gets its copy as before.
- **Only the morning run exports.** `digest run` does, like email; `digest build` and
  `--to <folder>` don't.
- **Pruning** only removes `deckle-daily-YYYY-MM-DD.epub` files beyond `keep` (`.kepub.epub` in a
  KEPUB folder, `.pdf` in a PDF one); long reads and anything else in the folder stay.
- **A folder that's missing** is made when its parent exists. When the parent is missing too
  (iCloud Drive turned off), the run's report says so and the paper still ships everywhere else.

## KOReader over Wi-Fi

KOReader runs on Kobos, PocketBooks, Boox tablets, reMarkables and jailbroken Kindles. Deckle
can serve it two things over your home network (KOReader hub design,
`docs/superpowers/specs/2026-10-30-koreader-hub-design.md`):

- **A catalog (OPDS):** today's paper, the last two weeks of papers, and the books waiting for
  your e-readers, each as an EPUB or, for a Kobo, a KEPUB. In KOReader: Search → OPDS catalog → +,
  with the catalog URL, the username and the password.
- **Progress sync:** KOReader's own sync, pointed at this computer instead of the internet.

Turn it on in Delivery → **KOReader over Wi-Fi**, choose a password, and copy the two URLs the
card shows. Or from the command line:

```toml
[reader_server]
enabled = true
port = 47220                     # the default
username = "deckle"
```

```sh
deckle reader-server password    # asked for without echo; only its MD5 is kept
deckle reader-server status      # on or off, and the URLs to type into KOReader
```

- The watcher serves it, so the app (or `deckle digest watch`) has to be running. Changes apply
  within a tick, no restart.
- The password's MD5 is kept in `reader-server.json` beside `digest.toml`, readable only by you.
  KOReader's sync protocol sends exactly that MD5, so treat the file like a password.
- It's plain HTTP on every network interface. Use it at home or over Tailscale, never open to the
  internet. `DECKLE_READER_SERVER_BIND` limits the address it listens on.
- **Progress sync:** in KOReader, Tools → Progress sync → Custom sync server, the sync URL; then
  Log in with the same username and password (registering is closed: the account is Deckle's).
  Each position lands in the paper's Readership, as `koreader/<device id>`, to the section, or
  for any other book on From Your Shelf under the reader's name. KOReader names a book by the
  MD5 of its file; Deckle remembers that for every file it hands over (the catalog, a Kobo's or
  PocketBook's drive, an export folder) in `koreader-documents.json`. For a book that came from
  elsewhere, turn on KOReader's "Send document metadata" so Deckle learns its title. Positions
  are kept in `koreader-sync.json`.
- Not yet checked with KOReader on a real device (`docs/devices.md`).

## KOReader plugins

With a Kobo, PocketBook or jailbroken Kindle plugged in (or `--from <folder>`, a copy of its
drive), Deckle manages KOReader's plugins, in **Settings → KOReader** or from the command line
(`docs/superpowers/specs/2026-10-31-koreader-plugins-design.md`):

```sh
deckle koreader plugins list                      # built in and added, on or off, and patches
deckle koreader plugins install owner/repo        # newest release, else the default branch
deckle koreader plugins install ./thing.koplugin.zip
deckle koreader plugins update appstore           # again from where it came from
deckle koreader plugins remove appstore
deckle koreader plugins rollback appstore         # the copy the last change kept
```

- Install and update show what they'll write and ask first (`--yes` for scripts). Plugins run
  code on your reader: install what you trust.
- An archive must hold exactly one `<name>.koplugin` folder (or be a repository whose top is the
  plugin) with a `main.lua`, nothing outside it, and under 20 MB unpacked.
- KOReader's built-in plugins are never replaced or removed. On a reader they share the
  `plugins/` folder with yours, so Deckle tells them apart by KOReader's own list of built-in
  names.
- What a change replaces or removes is kept in `deckle-backup/plugins/` in KOReader's folder,
  and where each plugin came from in `deckle-plugins.json` beside it.
- `deckle koreader backup` now includes your added plugins and `patches/`, and
  `restore --settings` puts them back.
- **On and off:** `deckle koreader plugins enable|disable <name>`. KOReader rewrites its settings
  when it closes, so Deckle doesn't edit them over USB: it leaves the change in
  `deckle-pending.lua`, the Deckle plugin applies it when KOReader starts and asks for a restart,
  and `list` shows it as waiting until then. This needs the Deckle plugin on the reader.
  `deckle koreader plugins patches on|off` switches every patch at once, directly.
- KOReader loads new plugins at its next start.

### The Deckle plugin: the paper by itself

```sh
deckle koreader plugins install deckle
```

installs Deckle's own plugin, which fetches today's paper from **KOReader over Wi-Fi** (turn that
on first) with no taps:

- when the reader wakes with Wi-Fi on, when it joins Wi-Fi, and when the file browser opens, at
  most every ten minutes and never while a book is open;
- or from **Tools → Deckle → Get today's paper**.

It asks Deckle what today's paper is, skips it if it's already there, downloads it into
`Deckle/` in KOReader's home folder, checks its size and SHA-256, says "Today's paper is here",
and keeps the last three papers it fetched (Tools → Deckle → Papers kept). A Kobo gets KEPUB.

- No reader can wake itself on a timer, so the paper arrives when you pick the reader up. **Turn
  Wi-Fi on when waking, to fetch** (off by default; it costs battery) makes waking enough.
- Deckle writes the plugin's settings to `settings/deckle.lua`: the server's address, the
  username and the password's MD5, never the password. Installing again updates those and keeps
  the rest. `DECKLE_READER_SERVER_URL` sets the address it's given.
- With several papers, it fetches the household's main paper (`digest.toml`'s).
- **Tools → Deckle → Use Deckle for progress sync** points KOReader's progress sync at Deckle
  (server, username and key), after asking. Automatic sync stays as you set it.

## Kobo

A Kobo gets the paper too, with no Kindle Previewer: plug it in once (or keep it plugged in for
`digest run`, `digest push` or `digest watch`) and Deckle remembers it. From then on each
morning's edition is also queued as an EPUB, and goes over the next time the Kobo is connected,
as a KEPUB in a `Deckle/` folder of its own, so the Kobo shows page counts and reading
statistics for it. Eject the Kobo to see the new paper: it only looks for books after an eject.
(`docs/superpowers/specs/2026-10-23-other-readers-design.md` §8)

- **Long reads and Inbox books** go along as KEPUBs too. An Inbox book is converted for the Kobo
  first; when there is no Previewer for the Kindle's copy, the Kobo's is enough for it to leave
  the Inbox.
- **`keep`** prunes the Kobo the same way: only `Deckle/deckle-daily-*.kepub.epub` beyond the
  newest `keep`; nothing outside `Deckle/` is touched.
- **Highlights and From Your Shelf.** `[sync] clippings = true` exports the Kobo's highlights and
  notes as Markdown, as for a Kindle (`deckle highlights export --to <dir>` works with a Kobo
  plugged in and no Kindle). From Your Shelf prints the books you're reading on it with the
  percent the Kobo keeps, and a past highlight. A Kobo has no looked-up words list, and no
  readership report yet.
- **The database** (`.kobo/KoboReader.sqlite`) is copied out and the copy read; the Kobo's own
  file is never opened.
- **Books by hand:** `deckle run book.epub` (or `.docx`, `.pdf`, `.txt`, a URL) with a Kobo and no
  Kindle plugged in sends it to the Kobo, cleaned and as a KEPUB.
- **Per device:** a Kobo's sync id is `kobo/<serial>` (`[sync.devices."kobo/N418…"]`).
- **Other mount points:** `DECKLE_MOUNTS` (a path list, as `PATH` is written) replaces the places
  Deckle looks for a drive.

The Kobo support is built against Kobo's files as the e-reader community documents them and
tested on a simulated drive; it hasn't been checked on a Kobo yet (`docs/devices.md`).

## PocketBook

A PocketBook works like a Kobo, with plain EPUB: plug it in once and each morning's edition, the
long reads and Inbox books go over as EPUBs into its `Deckle/` folder, pruned to `keep`
(`Deckle/deckle-daily-*.epub` only). `deckle run book.epub` with a PocketBook and no Kindle
plugged in sends it there. No Kindle Previewer is needed. Its sync id is `pocketbook/<volume>`.

Highlights and From Your Shelf don't read a PocketBook yet. Deckle knows one by its `system/`
folders (`system/explorer-3/`, or `system/config/` with `system/profiles/`, and no `documents/`);
like the Kobo, it hasn't been checked on a real one (`docs/devices.md`).

Deckle remembers every Kobo and PocketBook it has seen in `readers.json` beside the app's
`devices.json`; while that names one, `digest run` queues the paper for it, plugged in or not.

## The PDF edition and a reMarkable

The paper can also be a PDF laid out for one screen: the page is the screen's size, so nothing is
zoomed or panned, and the masthead, the weather strip, the photos and the crossword grid keep
their places. It has an outline (front page, sections, stories) and the front page links to each
section. The text is the paper's own fonts, so it can be searched and copied.
(`docs/superpowers/specs/2026-10-23-pdf-edition-design.md`)

Screens: `remarkable-2` (the default), `remarkable-paper-pro`, `remarkable-paper-pro-move`,
`ipad`, `a5`.

- **In an export folder:** `format = "pdf"` and `screen = "…"` in an `[[export]]` block, or **As:
  PDF** on the Daily tab's Delivery → Export folders. Pruning counts `deckle-daily-*.pdf` there.
- **To a reMarkable**, over its USB web interface (no account, no cloud): turn on *Settings →
  Storage → USB web interface* on the tablet, then

  ```toml
  [remarkable]
  enabled = true
  screen = "remarkable-2"          # or remarkable-paper-pro, remarkable-paper-pro-move
  books = false                    # also the long reads, as EPUBs
  ```

  or **Delivery → reMarkable** on the Daily tab. Each morning's PDF waits in its own outbox and is
  uploaded the next time the tablet is plugged in and answers (`digest run`, `push` or `watch`).
  The photos are gray on a reMarkable 2 and in color on a Paper Pro (`photos = "gray"` or
  `"color"` overrides it). A file the tablet already shows is not sent twice.
- **No pruning on a reMarkable:** its USB web interface can't delete, so older papers stay on the
  tablet until you remove them there.
- **No Kindle Previewer needed** for either.

The reMarkable side is built from how its USB web interface is known to work (`GET
/documents/`, `POST /upload` at `10.11.99.1`) and tested against a stand-in; it hasn't been
checked on a tablet yet (`docs/devices.md`). The page sizes for each screen are checked there
too.

## The edition bundle, for reader apps

`format = "bundle"` in an `[[export]]` block (or **As: Bundle** on the Daily tab) puts each
morning's edition in the folder as data rather than a finished file: a
`deckle-daily-YYYY-MM-DD.deckle` folder with `edition.json`, the photos (in color) and icons,
the cover and the EPUB. A native reader app draws the paper from it, with the front page plan,
each story's text and link, the weather as numbers and the crossword with its answers.
`books = true` adds the long reads' EPUBs. `keep` prunes old bundles.

From version 1.2 a story carries `access`: `open` for a free page, `subscription` when the paper
signed in as a subscriber and read it whole, and `walled` when the site withheld it (a reader can
show the note saying why).

From version 1.1 the bundle's photos are full size for a tablet: in color, up to 2400 px on the
long side (the EPUB keeps its 1000-px e-ink copies). Each photo comes with its caption, credit
and alt text from the story's page, and where its subject is, so a reader can crop around it or
move it in parallax. Each story says who wrote it, when it was published, how long it takes to
read, where the front page put it, whether a paywall cut it short, and a sentence to pull out.

Every story the paper fetches, in any output, loses the page's furniture first: newsletter
sign-ups, "Advertisement", share prompts, related-link lists and cookie notices. Then it gets the
same typography pass as books (curly quotes, dashes). `deckle digest lint <url or saved page>`
shows what a page keeps and what it drops.

The format is documented in `docs/superpowers/specs/2026-10-23-edition-bundle-design.md` and
`docs/formats/edition-bundle.schema.json`, offered under CC-BY-4.0 and MIT, so any reader app,
open or not, can read it. `deckle digest build --bundle -o <dir>` writes one by hand.

A bundle is written into a hidden `.….deckle.tmp` folder and renamed into place, and its
`edition.json` lists every file with its size and SHA-256: a reader waits until every listed
file has arrived, so a half-synced iCloud Drive folder is never read as a whole paper.


## Fresh-paper requests

A compatible client can ask this computer to build a fresh full paper through one shared bundle
folder. Requests are **off by default**. They use the existing Deckle watcher; setup does not
add another scheduling service or change your morning schedule.

1. Update Deckle and the compatible client to versions supporting the fresh-paper protocol.
   In **Daily → Delivery**, add the existing synced folder and choose **Bundle** as its format.
2. Turn on **Fresh-paper requests** for that folder. Deckle saves pending edits, checks that this
   paper owns the folder, and installs or starts the existing watcher. **Ready** means this setup
   succeeded now, not a continuing online-status or delivery guarantee.
3. In the compatible client, select the same synced folder and wait for its request capability
   to arrive before requesting a fresh paper. Keep this computer awake, signed in, and able to
   reach the folder. The watcher and cloud-sync software must be running. Requests cannot wake
   the computer, and either direction of sync can take time.

One paper and one designated producing computer own each enabled folder; multiple compatible
clients may use it. A fresh edition goes only to the folder that requested it. It is not sent by
email, queued for USB, copied to other exports, or saved as a scheduled Morgue edition. It does
not consume saved articles or advance scheduled puzzles. Stopped presses reject requests;
scheduled weekdays do not restrict explicit requests. Existing optional model features use your
configured providers and may incur their usual costs. There is no new hosted account or service.

A request expires after 24 hours. The producer starts at most one fresh job per paper per five
minutes, with one queued next job and one active fresh job across papers. Builds have a bounded
15-minute deadline and at most three attempts. The folder retains seven fresh editions, keeping
each for at least 48 hours and protecting pending results. Cloud delivery has no fixed deadline.

### Turn off or change a folder

Turn the switch off before moving a folder to another producing computer. Changing a bundle
folder's format or removing it also disconnects the old advertisement before saving that edit.
If the old root is unavailable, requests turn off locally and the edit reports a failure; reconnect
the old root and retry so other clients can see the disabled notice. Do not assume the shared
notice disappeared just because the local switch is off. Ordinary autosave cannot enable requests
by copying `press_enabled = true`; activation requires the setup transaction.

### A folder belongs to another producer

Deckle never silently takes ownership. To deliberately reconnect an existing folder:

1. On the old producing computer, turn off requests for that paper and folder. Keep that computer
   disabled. Wait for the shared `.deckle-press/producer.json` to sync with `enabled: false`.
2. Explicitly remove **only** that folder's `.deckle-press/producer.json`, and wait for the removal
   to sync. Keep request, status, edition, return, and other files. The ordinary off switch keeps
   the ownership record, so turning it off alone does not transfer ownership.
3. On the new producing computer, check the selected paper and folder, then retry setup. Reopen
   or refresh the compatible client's selected folder after the new advertisement syncs.

If the old computer is unavailable, first ensure it cannot resume its watcher or write to this
folder. Do not remove another active producer's record. A duplicate folder mapping to another
paper on this computer must also be removed or corrected before setup can succeed.

### Watcher and sync troubleshooting

- **Needs attention / setup failure:** check that the actual synced folder is mounted, downloaded,
  writable, and supports atomic file publication. Deckle does not create a replacement folder
  when a sync root is missing. Use **Retry setup** after fixing access. A failed watcher start
  never reports Ready.
- **Enabled · not checked:** the saved opt-in exists, but this window has not checked its service
  and shared binding. **Check setup** verifies the binding and starts the existing watcher.
  Startup never turns a missing or disabled advertisement back on or rewrites your saved settings.
  After an incomplete sync, missing advertisement, or watcher-start failure, use explicit setup
  again. Automatic repair and automatic refresh of the advertised paper name are not promised.
- **Watcher missing or stopped:** `deckle digest schedule` shows existing jobs without changing
  them. Use **Check setup** or **Retry setup** to repair only the watcher. For foreground diagnostics,
  `deckle digest watch` runs the existing watcher in a terminal; avoid running extra watcher
  instances alongside its installed service. `deckle digest schedule --off` removes all edition
  jobs and the watcher, so it is not the per-folder opt-out command.
- **No response:** check both devices' clocks, the stopped-presses setting, that the same folder
  is selected, and that the cloud provider has uploaded and downloaded hidden control files.
  A visible request file is not proof it reached the producer. Let the client report expiry or
  offer its explicit retry; do not delete control files to force another build.
- **Identity unavailable:** restore this installation's private request state before reconnecting.
  It lives under Deckle's persistent application configuration directory in `press/`, separate
  from synced folders and disposable caches. Do not copy a producer identity between computers
  or manually edit it. After reinstalling without that state, use the deliberate disconnect
  procedure above before creating a new ownership record.
