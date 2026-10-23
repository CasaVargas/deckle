# Changelog

## Unreleased

- **Morgue and From Your Shelf hardening** (#127, #126). Two builds of one date no longer share
  a temp file, the Morgue's EPUB and sidecar are staged before either is replaced, and
  `morgue.json` is saved only when the first import ran. Shelf read errors name the file, not the
  book's title; a back-dated build no longer prints "-1 days ago"; "From Your Shelf" sits after
  Feeds in the off-sections list; Windows **Open** uses Explorer.
- **From Your Shelf, smaller fixes** (#126). A snapshot's freshness and the "plugged in" line now
  count days the same way (in the paper's time zone). The same book on two Kindles is one line
  even when only one copy has an ASIN (it is matched by title and author). Two Kindles with the
  same name are told apart by the end of their serial, e.g. "Kindle (0001)".
- **The Morgue, smaller fixes** (#127). Editions brought in from the cache keep their weather
  line and lead on the calendar; an "unavailable" note no longer ends up in a story's text. The
  Every edition card reloads when a build finishes or a send is queued, drops a slow earlier
  search, says where the earlier editions stayed after you change the folder, and shows a
  `digest.toml` that won't parse instead of quietly using the default folder.
- **The Morgue** (#109): every edition is kept in a folder you can see, `~/Deckle/Morgue` (change
  it under `[morgue]` or on the Daily tab), for a year, with the stories' text so you can search
  it. **Every edition** on the Daily tab shows a month of front pages, search with the matching
  days lit up, and a day's detail: how far you read, passages highlighted, whether it's still on
  the Kindle. **Send again** puts an old edition back on the Kindle for a week, past the newest
  three. `deckle digest morgue list|search|show|send` does the same from the terminal. The
  editions already in the cache folder are brought in the first time.
- **Two Kindles of one model keep their own send records and options** (#128). The library's
  "on this Kindle" and update checks, and the per-Kindle choices (Cover first / Features first,
  real-ASIN X-Ray), are now keyed by model and USB serial. What was recorded before still applies,
  and a Kindle's first sighting inherits the model's old choices.

## 0.4.0 — a real front page, topic sections, From Your Shelf

- A site's default author picture (the same grey avatar in several stories, or any small square
  image) is no longer printed inside stories.
- **Two Kindles of one model no longer share a snapshot.** A USB-storage Kindle is now told apart
  by its USB serial number (macOS, Windows and Linux), so a second Paperwhite keeps its own shelf
  snapshot and per-Kindle settings. Settings written before keep working. `[sync.devices."<id>"]
  name = "..."` names a Kindle on the From Your Shelf page, which no longer prints the firmware
  line as the Kindle's name.
- **A real front page, and topic sections**: the Daily opens on a front page (the lead, a
  two-column table of headlines, "Inside today"), each section starts with "In this section" and
  each story links back to its section and the front page. Tick Google News topics (World,
  Sports, Science and more) in the Daily tab, or give a `[[feeds]]` entry `topic = "SPORTS"`,
  several `urls` and `full_text = "lead"` (top story whole, the rest briefs). A story already
  printed in an earlier news section is dropped from later ones. Tomorrow's paper sketch shows it
  all.
- **From Your Shelf**: a page in the Daily drawn from your own Kindle. **On your nightstand**
  lists the books you're in the middle of, how far in and how long since you opened them, with a
  nudge back into the newest one; it works on every Kindle, the Scribe included. A past highlight
  (with your note) and **words you looked up this week** appear on Kindles that share them. It's
  as of the Kindle's last plug-in, stays on this Mac, and **Forget what my Kindles shared** wipes
  it. In the starter config; add `"shelf"` to `sections` for an existing paper. `deckle digest
  shelf` prints it. Emailed papers include the page, so it passes through Amazon.
- **Local news for more places** (#102): add a relative's town or a vacation spot under **The
  paper → Other places** (or `[[local.places]]` in `digest.toml`), and each gets its own local
  news section after yours. Weather stays on your place.
- **Tomorrow's paper, so far**: the Daily tab now opens on the next edition. A press bar with
  the countdown and **Stop the presses** (skip one morning; `deckle digest stop`), a live sketch
  of the front page that fills in as you save articles and your feeds post (with a line saying
  why the lead leads), a **page plan** you drag to reorder with story budgets and "about N Kindle
  pages", and **This week**'s reads. `deckle digest next` prints the same. The cover's lead now
  follows one rule everywhere: local news first, else the first section in your plan with a
  story, so a saved article can lead the paper.
- A **puzzle page** in the Daily: a 5×5 mini crossword whose theme answers are yesterday's
  headlines with the word blanked, a word ladder, and yesterday's answers. Answer a clue as a
  Kindle note and the next paper says "You solved N of M" (Kindles that keep `My Clippings.txt`).
  Each day's puzzle is kept, so rebuilds print the same one. In the starter config; add
  `"puzzle"` to `sections` (or use the Daily tab) for an existing paper. `deckle digest puzzle`
  prints it; `deckle digest push --to <folder>` pushes to a folder standing in for a Kindle.
- The Daily's weather section is now a **Weather & Almanac** page:
  - a five-day strip with pictures and short words;
  - sunrise, sunset and daylight, computed on your Mac;
  - the usual first frost for your place;
  - the moon with its next full moon and name (the Hunter's Moon);
  - equinoxes, solstices, clock changes, and the meteor shower of the night.
- **Your Day** puts today's events from your `.ics` calendars or private calendar addresses on
  the page, with a line when rain is likely during one (Daily tab → **Your Day**, or `[day]` in
  `digest.toml`). If the forecast is down, the rest of the page still prints.
- No more Linux builds (`.deb`, `.AppImage`). Kindle Previewer doesn't run on Linux, so the
  main thing Deckle does, converting a book, never worked there. Existing Linux installs keep
  working but get no further updates. Linux is no longer tested in CI and is unsupported.
- Highlights and notes out as Markdown: one file per book in a folder you choose, sideloaded
  books included. From the Kindle tab (**Write**), with `deckle highlights export`, or on every
  plug-in with the morning schedule (`[sync] clippings = true`, or **Settings → Kindles**). Later
  exports only append what's new, so edits you make to the files stay (#60). Works on Kindles
  that keep `My Clippings.txt`; the Kindle Scribe (2024) keeps its highlights where they can't be
  read over USB (#99).
- How far you read the paper: each time the Kindle connects, Deckle reads its reading position
  for every edition and shows, on the Daily tab and with `deckle digest readership`, how often you
  got to each section, with one-click suggestions (drop a section you rarely reach, move a feed
  up). Local only, on by default (`[sync] readership`).

## 0.3.1 — full-window settings

- Settings open full-window with a sidebar (General, Books, X-Ray, Word Wise, Kindles, About)
  instead of a crowded slide-over; each option is one short row with a switch.
- Tidier book rows: one line of plain-word chips ("Keeps cover", "X-Ray + Word Wise",
  "Sent · 496 pages"); the counts, mode and paths moved to **Details** in the ⋯ menu.
- A visible **Add article** button. Pasting a web address on the Books tab (⌘V / Ctrl+V) now
  works on the Mac, where it silently did nothing before.
- Real icons in place of text symbols.

## 0.3.0 — in-app updates

- In-app updates: Deckle checks GitHub for a newer version when it starts. The footer says when
  one is out, and **Settings → About → Update** downloads, verifies and installs it; nothing
  installs or restarts without a click. Install this version by hand once; later ones update
  themselves.
- Windows ships only the per-user `-setup.exe`; the `.msi` is gone (uninstall a 0.2.x `.msi`
  before installing).
- Releases are normal releases, no longer marked pre-release.

## 0.2.1 — library, more inputs, the Deckle Daily (pre-alpha)

0.2.0 was built but never published; everything listed under it ships in this release.

- Windows installers are code-signed by Casa Vargas LLC with Azure Artifact Signing (the same
  account and profile as Beltr and DebridDownloader), checked after every release build.
- More inputs: Word documents (`.docx`: headings become the contents, footnotes pop-up notes),
  plain text (`.txt`), and web articles (an address, a `.webloc`/`.url` link or a saved `.html`
  page) become their own books. A finished `.kfx` is sent as is.
- Library: a plain folder, one `Author/Title/` per book with a `deckle.json` sidecar.
  `deckle library init|add|list|scan|series|reprocess`; `run` adds what it sends. Series are
  grouped, in reading order, with gaps shown.
- Kindle tab (and `deckle kindle books|remove`): what's on the Kindle, matched to the library,
  with free space. It says which books Deckle could now build better; **Update all** re-sends
  them. Sending a book again replaces Deckle's older copy.
- Opt-in cleanups: fill missing metadata from Open Library, tidy typography, turn footnotes into
  Kindle pop-up notes.
- Compatibility warnings: Deckle reads the Kindle's firmware (also over MTP) and warns on known
  issues.
- The Deckle Daily (preview): a morning paper built locally from weather, local news and your
  feeds, with full article text and photos, a front-page cover, Your Reading (`deckle save`, an
  Inbox folder, a bookmarklet), a schedule, delivery whenever a Kindle connects, and optional
  Send-to-Kindle email through your own SMTP account (password in the OS keychain). The Daily
  tab in the app sets it all up.
- TLS is verified against the OS certificate store.

## 0.2.0 — real Kindles: Scribe 2024 over MTP, Paperwhite (pre-alpha)

Verified on a Kindle Scribe 2024 (MTP): cover, X-Ray and Word Wise. Also delivered to a
mass-storage Kindle on firmware 5.19.2.

- X-Ray LLM descriptions work with local models (Ollama): candidates go out in batches of 25,
  self-hosted endpoints are asked not to "think", the prompt names the author and no longer
  sends Deckle's person/term guess, and a failed batch only affects its own names.
- X-Ray: contractions ("I'd", "I’ve", "We’re") are no longer people or terms, and a sentence
  opener is no longer glued to the name after it ("Since Dad" is "Dad").
- MTP: Word Wise no longer breaks delivery. Reading the Kindle's dictionary and sending the book
  now share one MTP connection; closing the first one reset the Kindle, which dropped it to the
  home screen and off the bus ("device is no longer connected"). Reopening also falls back to the
  serial number when the bus address is stale.
- MTP: no USB reset on close. On a Scribe 2024 the reset dropped it off the bus until replugged.
- MTP: Word Wise is skipped with a warning when the Kindle hides `system/` over MTP (Scribe 2024),
  where its dictionaries live; the book and X-Ray still ship. Delivery errors name the file.
- Word Wise on MTP Kindles reads a copy of the Kindle's dictionary staged at
  `documents/deckle/kll.en.en.klld`.
- Manual Amazon ASIN per book (app: ⋯ → Set Amazon ASIN…; CLI: `run --asin`). With EBOK the
  Kindle fetches the store cover, so MTP Kindles get covers with X-Ray and Word Wise.
- Word Wise hints show on the Kindle: English books that get Word Wise are tagged `en` (not
  `en-US`), without which every hint was a blank gap; hints never point at Amazon's
  definitionless placeholder senses.

## 0.1.0 — first release (pre-alpha)

- EPUB → KFX with Enhanced Typesetting through Kindle Previewer 4, repackaged by a Rust port of
  kfxlib (byte-exact against its output); approximate page numbers.
- Cleans EPUBs for Previewer: JPEG cover marked `cover-image`, large images downscaled, scripts
  and remote resources removed, a TOC when the book has none; font-obfuscated (DRM-free) EPUBs
  are accepted, DRM is refused.
- The cover rule: EBOK plus a Deckle-written thumbnail on mass-storage Kindles (restored after
  Amazon's "no image" overwrite), PDOC on MTP-only Kindles and folder exports; per-book and
  per-Kindle overrides; optional Amazon edition matching through Open Library.
- Delivery to mass-storage Kindles and, through Deckle's own PTP/MTP client, to MTP-only
  Kindles (Scribe 2024, Colorsoft, Paperwhite 2024); export to a folder.
- X-Ray sidecars: heuristic people and terms, optional LLM descriptions (Anthropic or any
  OpenAI-compatible endpoint), optional Wikipedia summaries.
- Word Wise sidecars from Deckle's Wiktionary-derived dictionary (downloaded once, 12 MB),
  pointing at the Kindle's own dictionary when present.
- Desktop app for macOS 14+ and Windows 11 (Tauri); `deckle` command line for everything.
- macOS builds are signed with a Developer ID and notarized; Windows builds are unsigned in v1.
- Known gaps: not yet verified on real Kindles (see `docs/devices.md`).
