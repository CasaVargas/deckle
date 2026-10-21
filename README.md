# Deckle

Turn any EPUB you own into a Kindle book that reads like one from the store, and put it on
your Kindle. Enhanced Typesetting, covers that actually show, page numbers, X-Ray, Word Wise,
real metadata. One window: drop books, plug in Kindle, press go.

Mac and Windows. Open source (GPL-3.0-or-later). No account, no subscription, no telemetry,
no auto-updater.

**Status: 0.1.0, pre-alpha.** Every stage works from the desktop app and the command line and is
tested against fixtures and a simulated Kindle; acceptance on real Kindles is still being
recorded in `docs/devices.md`. Design: `docs/superpowers/specs/`; build plans:
`docs/superpowers/plans/`.

![A book shipped with X-Ray and Word Wise](docs/screenshots/gui-wordwise-done.png)

## Install

1. Install **[Kindle Previewer 4](https://kdp.amazon.com/en_US/help/topic/G202131170)** (free,
   from Amazon). Deckle drives it to produce Enhanced Typesetting and never bundles it.
2. Download Deckle from [Releases](https://github.com/prjoni99/deckle/releases):
   - **macOS 14+** (Apple Silicon: `aarch64`, Intel: `x64`): open the `.dmg`, drag Deckle to
     Applications, and open it. Releases are signed with a Developer ID and notarized by Apple,
     so macOS opens them like any other app.
   - **Windows 11**: run the `-setup.exe` (installs for your user, no admin) or the `.msi`.
     Releases are code-signed by Casa Vargas LLC (Azure Artifact Signing). A new certificate
     can still get a SmartScreen prompt until it builds reputation: **More info → Run anyway**.
   - Linux (`.deb`, `.AppImage`) builds and runs every stage except conversion, which needs
     Kindle Previewer (Mac/Windows only).
3. Plug in your Kindle, drop EPUBs on the window, press **Send to Kindle**.

Close calibre, Android File Transfer and Amazon's Send to Kindle first: only one program can talk
to an MTP Kindle (Scribe, Colorsoft, Paperwhite 2024) at a time.

## What goes over the network

Nothing about you, ever. Deckle goes online only for:

| What | When | Default |
|---|---|---|
| Word Wise dictionary (12 MB, from this repo's Releases) | once, the first time a book gets Word Wise | on |
| LLM descriptions for X-Ray (Anthropic or any OpenAI-compatible endpoint, incl. local Ollama) | each EBOK book | off |
| Wikipedia summaries for X-Ray terms | each EBOK book | off |
| Amazon edition lookup (Open Library) | books for MTP Kindles | off |

Your Kindle itself may ask Amazon about a book sent with features (EBOK) because it carries an
ASIN; that is the Kindle's behaviour, and *Settings → Default mode → Cover first (PDOC)* avoids it.

## Using it

Each row shows cover, title, author, series and a mode badge: **Cover** (PDOC: the cover always
shows, no X-Ray/Word Wise), **Features** (EBOK: X-Ray and Word Wise), **Everything** (EBOK plus a
cover thumbnail Deckle writes itself), **Ready** (a finished `.kfx`, sent as is without
conversion), or **Error** with the reason. Deckle picks the mode per
Kindle (see *How it works*); the row menu overrides it per book. **Send to Kindle** runs the
pipeline book by book with per-stage progress; the split button exports to a folder instead. The
result line says what shipped and why, e.g. `X-Ray: 62 people, 88 terms (basic) · Word Wise:
11,477 hints (level 5, Deckle's dictionary)`.

Settings hold the Kindle Previewer path, default mode, page numbers, Amazon matching, X-Ray (LLM
provider, model, endpoint, key; Wikipedia), Word Wise (on/off, hint level) and per-Kindle
options. *About* lists the version, a link to check for newer releases, and the credits.

### Word Wise on your Kindle

Hints point at the Kindle's own English Word Wise dictionary when it has one (it downloads it the
first time you use Word Wise on a store book), so they look exactly like a store book's. A Kindle
without it gets Deckle's dictionary, installed once: set **Word Wise → Language → 中文
(Chinese)** on the Kindle; the hints are English. Deckle never replaces Amazon's dictionaries.

### Covers on MTP Kindles

The Scribe 2024 and other MTP-only Kindles can't take Deckle's cover thumbnail, so an EBOK book
(needed for X-Ray and Word Wise) shows a grey cover unless the Kindle can fetch Amazon's. Give
the book its real ASIN: copy the Kindle edition's ASIN (`B0…`, under *Product details* on its
Amazon page), choose **⋯ → Set Amazon ASIN…** and **Prefer features (EBOK)**, and send. The
Kindle downloads the store cover over Wi-Fi. Command line: `deckle run book.epub --cde ebok
--asin B0XXXXXXXX`.

MTP Kindles such as the Scribe 2024 hide the folder that holds those dictionaries. On a
jailbroken one, copy `system/kll/kll.en.en.klld` to `documents/deckle/` once with KOReader's file
manager and Deckle uses it; otherwise Word Wise is skipped on that Kindle (the book and X-Ray
still ship).

## Command line

```sh
cargo build --release -p deckle-cli            # binary at target/release/deckle
deckle devices                                 # connected Kindles (mass storage and MTP)
deckle run book.epub --to device --pages auto  # inspect → normalize → Previewer → KFX → enrich → Kindle
deckle run book.epub --to ./out --cde pdoc     # export to a folder instead
deckle run notes.docx --to device              # a Word document: imported as an EPUB first
deckle run book.epub --to device --cde ebok --llm anthropic   # X-Ray with LLM descriptions (ANTHROPIC_API_KEY)
deckle run book.epub --to ./out --cde ebok --llm openai --llm-endpoint http://localhost:11434/v1 --llm-model llama3
deckle run book.epub --to ./out --cde ebok --wikipedia       # Wikipedia summaries for places and things
deckle run book.epub --to ./out --xray off                   # no X-Ray sidecar
deckle run book.epub --to device --word-wise-level 3         # Word Wise hints for rarer words only
deckle run book.epub --to device --word-wise off             # no Word Wise sidecar
deckle inspect book.epub --json                # metadata, spine, TOC, warnings
deckle convert book.kpf -o book.kfx --asin B0XXXXDECK   # repackage an existing Previewer KPF
```

A `.docx` goes through the same pipeline: Heading 1–3 become the contents, footnotes become
pop-up notes, and pictures, lists, tables and bold/italic come through
(`docs/superpowers/specs/2026-10-21-docx-import-design.md`). The app's drop zone still takes
EPUBs only.

`--cde auto` (default) applies the cover rule; `--cde pdoc|ebok` overrides it; `--match` looks up
an Amazon edition on Open Library when an MTP Kindle needs a real ASIN.

### The Deckle Daily (preview)

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

No Kindle plugged in at build time? The edition waits in the outbox and goes over on the next
`push`, or as soon as you plug in while `watch` is running.

What `watch` does on plug-in is set per Kindle in `digest.toml`: `[sync]` holds the defaults
(`push = true`, `prune = true`), and `[sync.devices."<sync id>"]` overrides them for one device,
using the id `deckle devices` shows. Push and prune share one connection.

Unlike books, the digest goes online by design: each edition fetches the sources in your config.
Opt-in Send-to-Kindle email comes next
(`docs/superpowers/specs/2026-10-20-daily-digest-design.md`).

## How it works: what a "store-quality" sideloaded book needs

Sideloaded EPUBs are second-class on a Kindle. Every fix exists somewhere as a calibre plugin or a
forum recipe, each with its own install story, and they fight each other: the one that gives you
X-Ray takes away your cover on a 2024 Scribe. This is what we learned building one tool that does
all of it (details in `docs/formats/`).

**Enhanced Typesetting needs KFX, and KFX needs Kindle Previewer.** Only Amazon's converter
produces Enhanced Typesetting. Its CLI turns an EPUB into a KPF (a zip around a SQLite database of
Ion fragments, with 1 KiB fingerprint records injected every MiB); a Kindle wants a *monolithic*
KFX. jhowell's kfxlib (GPL-3, inside the KFX Input plugin) contains the complete KPF → KFX path.
Deckle's KFX writer is a Rust port of it, tested byte for byte against kfxlib's output, so no
Python is involved. Approximate page numbers come from the same port (1,850 positions a page).

**Covers and features pull in opposite directions.** X-Ray and Word Wise only load for books
marked **EBOK** with an ASIN. An EBOK with an ASIN the store does not know shows a grey cover, and
Amazon replaces any cover thumbnail with a "no image" file the moment the Kindle goes online.
**PDOC** books always use their embedded cover but get no X-Ray or Word Wise. Deckle's rule:

| Kindle | What Deckle sends |
|---|---|
| Mass storage (`system/thumbnails` writable) | EBOK + Deckle's own 500 px thumbnail, restored on every send: covers *and* features |
| MTP-only (Scribe 2024, Colorsoft, Paperwhite 2024) | PDOC (covers win), unless the book matched a real Amazon edition and you confirmed that Kindle keeps sidecars for real ASINs |
| Folder export | PDOC |

Amazon reuses a print ISBN-10 as that edition's ASIN, so Open Library (no Amazon scraping) is
enough to find a candidate.

**MTP-only Kindles need their own transport.** macOS has no MTP. Deckle has a small PTP/MTP client
over USB: always the 32-bit ObjectInfo layout (libmtp switches layouts on Kindles' nameless root
folders and every write then fails with `0xA806`), zero-length packets after full-packet data
phases, TransactionID 0 for OpenSession, and it never invents a `documents` folder.

**X-Ray is a SQLite sidecar** (`<book>.sdr/XRAY.entities.<ASIN>.asc`) in the schema WordDumb
reverse-engineered, with occurrences at KFX positions (text-run position + character offset).
Deckle finds people and terms with a heuristic tuned on Pride and Prejudice (honorifics, speech
verbs, stop lists, alias merging), and an optional LLM writes spoiler-free descriptions.

**Word Wise is a sidecar too, but it holds no text.** `LanguageLayer.en.<ASIN>.kll` stores
positions and *sense ids* into a dictionary on the Kindle (`system/kll/*.klld`) chosen by the
Kindle's Word Wise language. Deckle matches the book against its own Wiktionary-derived lexicon
(174k senses, every inflection mapped to its lemma, difficulty from wordfreq), then points the
hints at the Kindle's own English dictionary, or installs Deckle's in the Chinese slot. Per-book
proper-noun detection keeps "Darcy" from being glossed as a unit of permeability.

## Build from source

Rust (stable), Node 22 and pnpm. macOS or Windows for conversion.

```sh
cargo test --workspace                               # all crates
cd apps/desktop && pnpm install && pnpm tauri dev     # the app, with hot reload
pnpm tauri build                                     # installers in target/release/bundle
```

Releases: `docs/release.md`. The Word Wise database builder: `data/wordwise/README.md`.

## Credits

Deckle stands on research and code from the MobileRead community: jhowell's KFX Input/Output and
kfxlib, jloutsch's kfxgen, xxyzz's WordDumb and Proficiency, seapy's kindle-cli, and calibre's
Kindle driver. Word Wise definitions come from Wiktionary via kaikki.org (CC BY-SA 4.0); word
frequencies from wordfreq (CC BY-SA 4.0). Kindle Previewer is Amazon's and is never bundled.
Kindle is a trademark of Amazon.com, Inc.; Deckle is not affiliated with Amazon.
