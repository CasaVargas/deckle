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
deckle save --bookmarklet                 # a bookmarklet for your browser (needs `digest watch`)
```

Or drop links (`.webloc`, `.url`, `.txt`) and saved pages into `~/Deckle/Inbox`. Point
`[reading] inbox` in `digest.toml` at an iCloud Drive folder and an iPhone Shortcut ("Save File")
saves from your phone. The queue is a plain file, `reading.jsonl`, next to `digest.toml`.

The same Inbox takes books: drop an `.epub`, `.docx` or `.kfx` there and it's sent later. The
file's extension decides: `.epub`, `.docx` and `.kfx` are books, and `.webloc`, `.url`, `.txt`
and `.html` are articles. Each book is converted like `deckle run` would (a KFX is copied as it
is) and waits in the outbox with the paper; see below for how it's finished on push. It moves to `Inbox/Sent`, or to `Inbox/Unreadable`
with a `.txt` saying why it couldn't be converted. Books need Kindle Previewer; without it they
stay in the Inbox until it's installed.

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

The Daily tab opens on the **next** edition, not the last one:

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
the Daily tab (**Topics**) or write them in `digest.toml`:

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
- **No repeats across sections:** a `[[feeds]]` section drops a story whose title an earlier news
  section already printed (Local, other places, and earlier topics). Local and other places only
  record their titles and are never trimmed themselves; Your Reading is never trimmed or recorded.

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
feeds, merged, newest first). Weather, the almanac and the cover stay on your place. Add places in
the Daily tab (**The paper → Other places**), or in `digest.toml`:

```toml
[[local.places]]
name = "Asheville, NC"          # the section's title
google_news = "Asheville NC"
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
is built, Deckle notes where each of its sections starts. Put together, the Daily tab's **How
far you read** panel and `deckle digest readership` show, over the last 14 editions, how many
you opened, how far in you usually get, and per section how often you got to it. With 7 opened
editions it suggests up to three edits: **Drop it** for a section near the end you rarely
reach, **Move up** for a feed you rarely reach. One click (or `deckle digest readership apply
<n>`) changes `digest.toml`, keeping its comments; **Dismiss** (`… dismiss <n>`) hides one for
14 editions.

- It's on by default (`[sync] readership = true`, per device under `[sync.devices."<sync id>"]`,
  or **Settings → Kindles**). Everything stays in `readership.json` next to `digest.toml`.
- "Got to" means got to or past: the Kindle moves its furthest position on a contents jump too,
  so jumping to *On This Day* counts every section before it as reached. That can hide a Drop
  suggestion, never invent one.
- Editions that went by email only aren't tracked: no KFX is built for them on this machine, so
  there's nothing to compare the position against (`usb_too = true` editions are).

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
events print as "Busy". Add calendars in the Daily tab's **Your Day** card, or in `digest.toml`:

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

Unlike books, the digest goes online by design: each edition fetches the sources in your config.

## Email to Kindle (opt-in)

With no cable, the paper can arrive over Wi-Fi: `digest run` emails its EPUB to your Kindle's
Send-to-Kindle address (`…@kindle.com`) through your own mail account. This one is labelled
because the paper then goes through Amazon, which converts it with its own typesetting. Long reads
and Inbox books stay on USB, where they get X-Ray and Word Wise.
(`docs/superpowers/specs/2026-10-21-send-to-kindle-email-design.md`)

1. Uncomment `[email]` in `digest.toml` (or fill in the **Email to Kindle** card on the Daily tab):
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
