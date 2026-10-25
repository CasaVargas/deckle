# One Mac, many papers — design

**Status:** approved by the owner on 2026-10-25, with call 6 revised to reuse #37's outbox
ownership (Kindle profiles design) ·
**Date:** 2026-10-22 · **Owner:** Jonathan (CasaVargas LLC) · **Builds on:**
`2026-10-20-daily-digest-design.md` (the paper, `digest.toml`, the schedule),
`2026-10-20-auto-sync-design.md` (sync ids, push and prune), `2026-10-21-send-to-kindle-email-design.md`
(email), `2026-10-22-readership-report-design.md` (`readership.json`).

One Mac prints a separate paper for each Kindle in the family. Each paper has its own name,
reader, sections, time and days, delivery (USB to one Kindle, or email to one address) and type.
The Daily tab gets **Your papers**: one card per paper, with **Edit**, **Preview** and **New
paper**.

## Calls (approved 2026-10-25)

1. **Config shape.** `digest.toml` stays the first paper, untouched and with the same path.
   Each extra paper is `papers/<slug>.toml` beside it. There is no migration: a one-paper user
   has no `papers/` folder and nothing changes.
2. **Household settings stay in `digest.toml`.** `[sync]` (it's about Kindles, not papers),
   the SMTP account in `[email]`, and `[reading]` (the inbox) are set there only. A paper file
   that sets them is an error that says so. Papers inherit them.
3. **Saved articles go to the main paper only.** The inbox, the bookmarklet and the app's
   "Save" all feed one queue (`reading.jsonl`), and only `digest.toml`'s paper prints Your
   Reading. A paper file that lists `reading` gets a note and the section is skipped.
4. **Per-paper state.** Readership, the puzzle (#108) and the Morgue (#109), all built since
   this was written, are kept per paper. The main paper keeps today's files; an extra paper's live in `papers/<slug>/`.
   Highlights (`highlights.json`) stay shared: they are read from a Kindle, not a paper.
5. **Edition names.** The main paper keeps `deckle-daily-YYYY-MM-DD`. An extra paper is
   `deckle-daily-<slug>-YYYY-MM-DD`. Prune and the outbox match the prefix **plus a date**, so
   the main paper's prefix never matches another paper's files on a shared Kindle.
6. **Delivery** (revised 2026-10-25). `kindle = "<name or sync id>"` sends a paper to one
   Kindle only; unset means any Kindle, as today. The name is resolved through
   `[sync.devices]` when the edition is queued. The edition waits in the shared outbox
   carrying #37's owner tag (`<entry>.for`), so the routing, the matching across MTP and mass
   storage, and the "waits for Mom's Kindle" report are the ones books already use. A paper
   bound to a Kindle goes only there: not to a Kobo, PocketBook or reMarkable outbox. Email is
   per paper (`[email] enabled`, `to`), on the household SMTP account.
7. **Schedule.** One job per paper (`[schedule] at`, `days`), plus the one watcher. The main
   paper keeps its job label, so its installed job isn't touched until its time is changed.
8. **Type size is "large print", not a point size.** A Kindle sets text size itself (Aa), for
   every book on that Kindle, and Kindle Previewer normalizes body text to that setting. So
   Deckle can't print "18 pt". `large_print = true` gives a layout made for big type (§6). The
   point size on the card is set on the Kindle. Whether body text can also be scaled is checked
   on a device in the plan, and stays off unless that check passes.
9. **CLI parity** through `--paper <slug>` on every `digest` command. `main` names
   `digest.toml`'s paper.
10. **Runs take a lock** (approved, MSRV 1.89). Two papers due at the same minute convert one after the other, since
    two Kindle Previewer runs at once are unsafe. This needs `std::fs::File::lock`, so the
    workspace's `rust-version` goes from 1.85 to 1.89.
11. **New section types are out.** "A note from you", scores, the church bulletin and the kids'
    sections (animal fact, comics, word of the day, story chapter) are non-goals. Feeds cover
    the bulletin and scores where they have RSS.

## 1. Intent and why

The family has four Kindles and one Mac. Dad reads in Ohio with large type and wants Toledo
news; Maya is nine and wants no news at all, on Saturdays. Today Deckle prints one paper, and
whichever Kindle plugs in first takes it. Every paper is the same machinery with a different
`digest.toml`, so the work is mostly naming, routing and scheduling, not new content.

**Success**, on Saturday 2026-10-24, with three papers set up:

- **05:30:** *The Morning Deckle* (`digest.toml`, `kindle` = the Scribe's serial) is built and
  waits for the Scribe.
- **06:00:** *The Lakeside Gazette* (`papers/lakeside.toml`, large print, Toledo) is emailed to
  Dad's `…@kindle.com` address and not queued for USB.
- **07:00:** *The Kid Courier* (`papers/kid-courier.toml`, `days = ["sat"]`) is built for the
  Kindle Kids. It isn't built Sunday to Friday.
- Maya plugs in her Kindle and gets `deckle-daily-kid-courier-2026-10-24.kfx` only. Her Kindle
  keeps its newest 2 Couriers, and the Morning Deckle never goes to it.
- The owner plugs in the Scribe and gets the Morning Deckle only. Its prune leaves no Courier
  behind and removes none.
- The Daily tab shows three cards as in the mockup.
- `deckle digest papers` lists the same three; `deckle digest run --paper kid-courier` builds
  the Courier now.
- A one-paper user who upgrades sees nothing change: same files, same names, same job.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **Papers on disk:** `digest.toml` plus any `papers/<slug>.toml` (§3). Each is a plain,
   commented TOML file the owner can copy, edit or delete.
2. **Per-paper settings:** name (`title`), reader (`reader`, e.g. "Dad, in Ohio"), place and
   location, units, sections, feeds, local news, Your Day, `keep`, photos, `kindle`, email,
   schedule, `large_print`.
3. **Delivery to the right Kindle** (§4), with prune per paper on that Kindle only.
4. **Schedule per paper:** a time and, optionally, days of the week (§5).
5. **Large print** (§6).
6. **CLI parity:** `--paper <slug>` on `build`, `run`, `schedule` and `readership`, plus
   `deckle digest papers` (list) and `deckle digest papers new <slug> --title … [--for …]`.
   `push` and `watch` always work over every paper.
7. **The Daily tab: Your papers.** One card per paper, showing its name, "for <reader>", its
   sections, Kindle, delivery and type, with **Edit** and **Preview**. **New paper** asks for
   a name and a reader. **Edit** points the rest of the Daily tab (the paper, Every morning,
   Email, How far you read) at that paper.

**Non-goals (this increment)**

- New section types: a note from you, scores, church bulletin, kids' content.
- Per-paper masthead styles. The mockup's three faces are app decoration only; the EPUB
  masthead stays as it is.
- Setting a Kindle's text size, or a point size in the EPUB (call 8).
- Removing a paper from the app. Delete its file and run `deckle digest schedule`; the stale
  job is removed (§5).
- Two Kindles plugged in at once. The watcher serves the first it finds, as today.
- Per-paper saved-article queues, and a per-paper highlights file.
- Weekly or monthly papers beyond days of the week.

**Anti-goals**

- A one-paper setup changes in any way: a file, a name, a job, an email subject.
- A paper reaches the wrong Kindle, or one paper's prune removes another paper's edition.
- Two papers' editions overwrite each other anywhere: work folder, outbox, readership.
- A paper file silently overrides a household setting (`[sync]`, SMTP, the inbox).

## 3. Config

```text
~/Library/Application Support/deckle/
  digest.toml               the main paper (slug "main"), as today
  papers/
    lakeside.toml           an extra paper
    kid-courier.toml
    lakeside/               its state: readership.json (later puzzles.json, morgue.json)
    kid-courier/
```

- **Slug:** the file stem. It matches `^[a-z][a-z0-9-]{0,31}$`; `main` is reserved. Other file
  names in `papers/` are skipped with a note.
- **New keys,** valid in every paper file including `digest.toml`. All are optional and left
  out of a save while they're at their defaults:

  ```toml
  reader = "Dad, in Ohio"          # shown on the card: "for Dad, in Ohio"
  kindle = "G0000000000"           # sync id (`deckle devices`); unset = any Kindle
  large_print = true

  [schedule]
  at = "06:00"                     # 24-hour
  days = ["sat"]                   # mon … sun; empty or absent = every day

  [email]                          # in a paper file: only these three keys
  enabled = true
  to = "dad_ABC123@kindle.com"
  usb_too = false
  ```

- **Household keys:** `[sync]`, `[reading]`, and `[email]`'s `from`, `smtp_host`, `smtp_port`
  and `username` are read from `digest.toml` only. In a paper file they're an error: "`[sync]`
  is set in digest.toml for every paper". After loading, a paper takes them from `digest.toml`.
- **The starter** (`STARTER`) gets the new keys commented. A paper starter (`PAPER_STARTER`) is
  used by `papers new`: title, reader, the main paper's place and location, `sections =
  ["weather", "local", "feeds"]`, and commented `kindle`, `large_print`, `[schedule]` and
  `[email]`.
- **Saving** a paper from the app goes through the existing `config::save` (comments kept), on
  that paper's file.

## 4. Editions and delivery

- **Names:**
  - main: `deckle-daily-YYYY-MM-DD`;
  - extra: `deckle-daily-<slug>-YYYY-MM-DD`.

  The same stem is used for the EPUB, the KFX on the Kindle, the email attachment
  (`<stem>.epub`) and the readership record. The KFX title stays "<title> — <long date>".
- **Matching:** prune and the outbox's "a new edition replaces yesterday's" both match
  `<prefix>YYYY-MM-DD.kfx` exactly. So `deckle-daily-` never matches
  `deckle-daily-kid-courier-…`. A slug starts with a letter, so it never looks like a date.
- **Work folder:** main keeps `<cache>/digest`; an extra paper uses `<cache>/digest/papers/<slug>`.
- **USB:**
  - `kindle` unset: the paper goes in the shared outbox, as today, for any Kindle.
  - `kindle` set: it goes in the shared outbox owned by that Kindle (`<entry>.for`, Kindle
    profiles design §2). Any other Kindle leaves it queued, and `push` says it's waiting.
  - When a Kindle connects, `push` and `watch` deliver what's anyone's or its own over one
    connection. Then they prune each paper that goes to this Kindle (unset, or its `kindle`
    is this Kindle), each by its own prefix and `keep`.
- **Other outputs** of an unbound paper follow it per paper:
  - export folders and the edition bundle use its own stem;
  - the reMarkable PDF and the Kobo/PocketBook EPUB replace only that paper's earlier edition
    (exact prefix-and-date match, as above);
  - `[export]` and `[remarkable]` are read from each paper's own file and are not inherited.

  A bound paper goes to none of these.
- **Email:** a paper with `[email] enabled` is emailed to its own `to` on the household account
  and password. `usb_too` queues it as above. The main paper's subject stays "Deckle Daily —
  <date>"; an extra paper's is "<title> — <date>".
- **Readership:** each paper records layouts and reads in its own `readership.json` (main: the
  existing file). At a connection, Deckle reads the `.yjf` for every paper's stems and records
  each in its own file.

## 5. Schedule

- **One job per paper with `[schedule] at`:**
  - macOS: a LaunchAgent. The main paper keeps `com.casavargas.deckle.digest`; an extra paper
    is `com.casavargas.deckle.digest.<slug>`.
  - Windows: a task, `\Deckle\Digest` or `\Deckle\Digest-<slug>`.
  - Each runs `deckle digest [--config …] run --paper <slug> --to device`.
- **Days:** macOS gets a `StartCalendarInterval` array with one dict per day (`Weekday`, 0 =
  Sunday). Windows gets a `ScheduleByWeek` trigger. No days means daily, as today.
- **Installing reconciles.** `deckle digest schedule` writes `[schedule]` into the paper's
  file, then installs every paper's job and removes `…digest.<slug>` jobs whose paper is gone.
  The watcher job is unchanged.
- **The main paper without `[schedule] at`** (every existing user) keeps whatever job is
  installed. Reconciling never removes it.
- **Status** lists each paper's job: time, days, and whether it's installed.
- **One run at a time:** `digest run` holds an exclusive lock on `<cache>/digest/run.lock`, and
  a second run waits for it.

## 6. Large print

Kindles set text size on the device, for all books, and Kindle Previewer's Enhanced Typesetting
treats body text as the reader's chosen size. So a paper can't set a point size. Instead,
`large_print = true` makes the edition easy to read at a big size:

- No text smaller than body text. Every style below 1em in `render::STYLE` (`.ears`, `.meta`,
  `.kicker`, `.link`, `.credit`, `.label`, `.small`, `.caption`, `.source`, the forecast strip)
  is lifted to 1em.
- Headlines and section heads are 1.1× their regular size, and rules are heavier.
- The five-day strip stacks as a list, one day per line, instead of a five-column table.
- `max_photos` is capped at 1.

This is a second stylesheet block appended after `STYLE`, so regular papers render exactly as
today. Builder's check, on a device: does `body { font-size: 1.25em }` survive conversion? If
it does on the Paperwhite and the Scribe, the plan adds it to large print. If not, it isn't
added. The card shows "Large print", and the editor says: "Set the text size on the Kindle
itself (Aa). It applies to every book on that Kindle."

## 7. The Daily tab

- **Your papers** sits at the top of the Daily tab once `digest.toml` exists. It has one card
  per paper, main first, then by title:
  - the paper's name, and "for <reader>" when set;
  - its section names, in order;
  - **Kindle**: the model name of the Kindle whose sync id matches (from the app's device
    list), or "Any Kindle";
  - **Delivered**: "USB at 05:30", "USB, Saturdays at 07:00", "Email at 06:00", or "Not
    scheduled";
  - **Type**: "Regular" or "Large print";
  - **Edit** and **Preview**.
- **The selected paper** is outlined. Every card below on the tab works on it.
  - **The paper** card gets: Reader, Kindle (a picker of known Kindles plus "Any Kindle"),
    Large print, Days.
  - **Every morning** sets that paper's time and days.
  - **Email** shows only "Send to" and "Also by USB" for an extra paper. The SMTP account
    stays on the main paper's card.
  - **Your Reading** shows only on the main paper. Elsewhere it reads: "Saved articles go to
    <main title>."
- **Hint:** with two or more papers and any of them on "Any Kindle", one line says: "<title>
  goes to whichever Kindle connects first."
- **New paper** asks for a name and a reader, makes the slug from the name, creates the file
  and selects it.

## 8. Testing

- **Config:**
  - With no `papers/` folder, loading gives one paper equal to today's `Config::load`.
  - Extra papers load in slug order. Bad slugs and `main.toml` are skipped with a note.
  - A paper file setting `[sync]`, `[reading]` or SMTP keys is an error naming the key.
  - Household settings are inherited.
  - The new keys round-trip and are left out at their defaults. A starter saved untouched
    gains no new lines.
- **Names:**
  - The stem and prefix for main and extra papers.
  - `prunable("deckle-daily-", …)` ignores `deckle-daily-kid-courier-2026-10-24.kfx` and
    `deckle-daily-email-test.kfx`.
  - The outbox replaces only the same paper's undelivered edition.
- **Routing:**
  - A targeted paper is queued in its Kindle's folder. Another Kindle's connection doesn't
    deliver it; its own does.
  - Prune runs per paper with its own `keep`, only for papers bound to this Kindle.
  - The shared outbox is delivered to every Kindle, as today.
- **Email:** each paper goes to its own `to`, on the main account. The main paper's subject and
  attachment are unchanged.
- **Reading:** an extra paper never prints or commits saved articles; the main one does.
- **Readership:** two papers on the same date record into separate files, and the `.yjf` read
  covers both papers' stems.
- **Schedule:**
  - The plist and task XML for daily and for `["sat"]`.
  - Labels per slug.
  - Reconcile removes a gone paper's job and keeps the main job when the main paper has no `at`.
  - Two `run`s hold the lock one after the other.
- **Large print:** the stylesheet has no size under 1em, the strip renders as a list, and a
  regular paper's XHTML is byte-identical to before (golden).
- **CLI:** `papers`, `papers new`, and `--paper` on `build` and `run` against a temp
  `--config`.
- **Desktop:** a vitest for the card's summary lines ("USB, Saturdays at 07:00", "Any Kindle")
  and the slug maker. Check the cards visually in the running app.
- **On the owner's Kindles:** a large-print edition on the Paperwhite and the Scribe (and the
  body-size check in §6). Two papers bound to two Kindles, each plugged in.

## 9. Docs

- `docs/daily.md`: "Many papers" (files, slugs, household keys, delivery, days, large print,
  removing a paper).
- CHANGELOG.
- The starter config.

## 10. Left to the builder

- Module placement (for example a `papers.rs` in `deckle-digest`).
- Tauri command names.
- The card CSS, and the three decorative masthead faces on the cards.
- How the app maps a sync id to a Kindle's name.

## 11. Delivery in increments (2026-10-25)

The build ships as four PRs, each usable alone and merged before the next:

1. **Papers core.** The config and loader, names and per-paper state, exact prune and replace,
   bound delivery through outbox ownership, `--paper` and `digest papers`, the run lock, and
   per-paper exports, reMarkable and EPUB outbox.
2. **Schedules.** One job per paper, with days, and reconciling stale jobs.
3. **Large print.** With the device check.
4. **The desktop.** Your papers.

Until 2 lands, an extra paper is built by hand (`digest run --paper <slug>`) or by the user's
own scheduler. Until 4 lands, papers are managed in their files and the CLI.


## 12. Implementation notes, PR 1 (2026-10-25)

These are the calls made where the spec left room, so the code and this document agree:

- **`kindle =` must name a Kindle Deckle knows**, by its `[sync.devices]` name or id. Otherwise
  `run` fails before any work and points at `deckle devices name`, so an edition never waits
  for a Kindle that doesn't exist.
- **The outbox's replace** keeps prefix matching, but a prefix never supersedes a name that goes
  on with a letter. `deckle-daily-` replaces `deckle-daily-2026-10-24.kfx` but not
  `deckle-daily-kid-courier-…`, and the long reads' date-prefixed replaces still work.
  `prunable` matches `<prefix>YYYY-MM-DD.kfx` exactly.
- **The Morgue** keeps another paper's editions in `<folder>/<slug>/`, and its first-run import
  is the main paper's. The `digest morgue` commands, and sending an edition again, are the main
  paper's in PR 1.
- **The puzzle's answer check** from the Kindle's notes runs for the main paper. An extra
  paper's puzzles are kept in its own `puzzles.json` but not checked yet.
- **`digest schedule --paper`** is refused until PR 2.
- **Saving an extra paper** (readership "apply", later the app) never writes the household
  settings into its file.

## 13. Implementation notes, PR 2 (2026-10-25)

- **`schedule --at`** writes `[schedule]` into the paper's own file, then installs every paper
  that has `at`, plus the watcher. Jobs for papers that are gone are removed. The main
  paper's job is replaced only when `digest.toml` has `at`, so an existing user's job stays
  until they set a time again.
- **`schedule --off --paper <slug>`** removes that paper's job and its `at`. The watcher is
  removed only when no edition job is left. Plain `--off` removes everything, as before. The
  app's "off" switch for the main paper uses the per-paper removal.
- **Linux** prints one cron line per paper, with days as cron weekdays.
- **`DECKLE_SCHEDULE_DRY_RUN`** makes install and remove report what they would do, for tests:
  a real install loads jobs into the login session whatever `HOME` is.
- **Saving an extra paper** adds a key the file lacks only when it isn't the default, so a
  hand-written paper file stays short.

## 14. Implementation notes, PR 3 (2026-10-25)

- **`render::LARGE_STYLE`** follows `STYLE` in `style.css` only when `Edition.large_print`;
  a regular paper's stylesheet and pages are byte-identical to before (the weather golden
  still matches). Later rules win, and a test checks that no selector is under 1em in effect.
  The one exception is the crossword's clue numbers (`table.grid span.n`), which have to fit
  inside their squares.
- **Heads** are 1.1× (`.paper`, `.section-head`, `h2.headline`, `h2.lead`, `h3.subhead`,
  `.big`). **Rules** go from 1px to 2px, and the section head's top rule from 3px to 4px.
- **The forecast strip** is `<ul class="strip-list">` with a `<li class="day">` per day when
  `cfg.large_print`, in `sources::weather`. The table stays for regular papers.
- **Photos:** gather prints with `max_photos` capped at 1 (`print_config`).
- **`body { font-size: 1.25em }`** is not added: it waits on the device check on the
  Paperwhite and the Scribe (§6).

## 15. Implementation notes, PR 4 (2026-10-25)

The Daily tab has grown rooms since §7 was written (Today, Editions, Reading, Delivery, The
paper). Your papers fits them like this:

- **Your papers** is a row of cards above the rooms, shown once `digest.toml` exists. Picking a
  card (Edit) selects that paper, and every room then works on it. Preview builds that paper.
- **The paper** room gets Reader, Kindle (Any Kindle, or a named Kindle from the device list),
  Large print (with the Aa line) and Days. A Kindle is saved by its name when it has one, else
  by its sync id.
- **Today:** the press bar's time and on/off are the selected paper's. Saving a time writes
  `[schedule] at` (and its `days`) to that paper's file, then installs its job, as the CLI
  does. Stop the presses stays household-wide (one `press.json`, as in the CLI).
- **Reading:** an extra paper shows "Saved articles go to <main title>." instead of Your
  Reading and the Shelf.
- **Editions:** How far you read uses the paper's own readership file. Every edition (the
  Morgue) stays on the main paper in the app for now, as `digest morgue` does in the CLI; an
  extra paper says so.
- **Delivery → Email:** an extra paper shows only on/off, Send to and Also by USB; the account
  stays on the main paper. The password is the household's.
- **Commands** take `paper: Option<String>` (`None` is the main paper). `papers_list` returns
  the cards and `paper_create(title, reader)` the new slug. Tomorrow's paper reads the paper's
  work folder, so "sent" days in This week are that paper's.
