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
  section already printed (Local, other places, and earlier feed sections). Local and other places only
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
- **Every edition, on the Daily tab.** A month of front pages, each with the paper's name and its
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

## Export folders: an iPad, a Boox, any folder

The paper doesn't have to go to a Kindle. Each `[[export]]` folder gets every morning's edition
as an EPUB, plugged in or not, and keeps the newest few. An iCloud Drive folder puts it in an
iPad's Files app (tap it to open it in Books); a Dropbox or Google Drive folder reaches a Boox.
(`docs/superpowers/specs/2026-10-23-other-readers-design.md`)

```toml
[[export]]
name = "iPad"
dir = "~/Library/Mobile Documents/com~apple~CloudDocs/Deckle Daily"
format = "epub"                  # only EPUB for now; Kobo's KEPUB and a PDF edition come later
photos = "color"                 # or "gray" for an e-ink reader
keep = 7                         # editions kept in the folder; 0 keeps all
books = false                    # also the long reads, not only the paper
```

Or use the **Export folders** card on the Daily tab: **Add folder…** opens on iCloud Drive on a
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
- **Pruning** only removes `deckle-daily-YYYY-MM-DD.epub` files beyond `keep`; long reads and
  anything else in the folder stay.
- **A folder that's missing** is made when its parent exists. When the parent is missing too
  (iCloud Drive turned off), the run's report says so and the paper still ships everywhere else.
