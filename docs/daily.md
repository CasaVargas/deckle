# The Deckle Daily

A morning paper built on your machine and delivered to your Kindle. This page holds the full
reference; the README has the overview.

In the app: the **Daily** tab sets up the paper (name, place, sections, feeds), saves links to
Your Reading, copies the bookmarklet, turns on the morning schedule, and builds or sends an edition
on demand. Everything below is the same thing from the command line; both edit the same
`digest.toml`, and your own comments in it are kept.

A morning paper built on your machine: weather (Open-Meteo), local news (a Google News search
plus any local feeds), your RSS/Atom feeds, Powerball and Mega Millions, a horoscope Deckle
writes itself, and "On this day" from Wikipedia. Its cover is a generated front page, so the
Kindle library tile is today's paper. Every story comes with its full text and photos (grayscale
for e-ink), pulled Reader-View style from the publisher, so nothing sends you to a browser; a
story behind a paywall stays a headline. No keys or accounts; a source that fails prints
"unavailable today" and the paper still ships.

```sh
deckle digest init                  # starter config: ~/Library/Application Support/deckle/digest.toml
deckle digest build -o ./out        # today's edition as an EPUB
deckle digest run --to device       # build, convert, send (cover first), keep the newest 3
deckle digest push                  # deliver a queued edition to the Kindle now connected
deckle digest watch                 # keep running; deliver whenever a Kindle connects
deckle digest schedule --at 05:30   # both, automatically: the paper daily, the watcher from login
deckle digest schedule --off        # and gone again
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
