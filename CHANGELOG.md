# Changelog

## Unreleased

- **Several Kindles in one home.** Name each Kindle (`deckle devices name`, or This Kindle in
  the app) and send to it by name with `deckle run --kindle`, or drop books in an Inbox folder
  named after it. A book for a Kindle that isn't plugged in waits in the outbox, and no other
  Kindle takes it; `push` and `watch` say what's waiting and for whom. The app's Kindle picker
  shows the names.
- **DRM-free Kindle books become real KFX.** Import `.mobi`, `.azw`, `.azw3` or `.prc`. AZW3
  books are rebuilt from their KF8 part. Older MOBI files have their Mobipocket HTML cleaned
  up: links, pictures and the contents come through, and HUFF/CDIC-compressed text is read.
  Either way the book gets KFX typesetting, page numbers, X-Ray and Word Wise. DRM-protected
  files are refused, never decrypted.
- **RTF becomes a book.** Import `.rtf` from TextEdit, WordPad, Scrivener or older Word, the same
  way as Word documents: headings become the contents, and formatting, lists, tables, pictures,
  links and pop-up footnotes come through. Text in older code pages (Cyrillic, Japanese and
  others) decodes correctly.
- **OpenDocument text becomes a book.** Import `.odt`, `.ott` or flat `.fodt` from LibreOffice
  and friends, the same way as Word documents: headings become the contents, styles become bold,
  italics and the rest, and lists, tables, pictures and pop-up footnotes come through.
- **FictionBook (FB2) becomes a book.** Import `.fb2` or `.fb2.zip` with its chapters and
  contents, pop-up footnotes, poems, tables, embedded cover and illustrations, and the authors,
  series and language it declares. Files in windows-1251 and other declared encodings work.

## 0.6.0 — notes become books, and a more personal paper

- **Markdown becomes a book.** Import a Markdown file or a folder of notes, including local
  images, tables, YAML metadata and links between notes. The library keeps a self-contained
  EPUB snapshot. Unsupported or missing links and images produce warnings.
- **Search across your library and Kindles.** Find titles, authors and series across library
  copies, a scanned connected Kindle and recorded sends. Historical sends are identified as
  records, not proof that a book is still on a device.
- **Book styles.** Choose Academic, Novel or Manual per queued book, or use `--preset` in the
  CLI. Presets set typography and enrichment defaults; fixed-layout content is left alone.
- **Language repairs and image accessibility checks.** Normalize language tags and fill missing
  document language declarations. Report images missing alt text without inventing descriptions
  or overwriting existing alt text. These checks work offline.
- **An optional Daily AI editor.** When enabled, it groups duplicate stories across sections,
  adds short section briefs and writes entertainment horoscopes. AI text is labeled; article
  bodies are preserved. Provider failures leave the original edition intact. Off by default,
  with provider credentials read from environment variables.

## 0.5.2 — the title's typeface, and a fix for Upgrades

- **Upgrades no longer offers a book it can't rebuild.** An older MOBI with no KF8 part (and no
  original on this computer) was listed as "can be upgraded" and then failed with "couldn't
  rebuild it cleanly". It is now left as it is and the Upgrades room says so ("1 older MOBI book
  Deckle can't rebuild"). A book whose original is in calibre or a named folder still upgrades.
- **The title's typeface, your choice.** `masthead` in `digest.toml` (or **The paper → Title font**
  in the app) sets the face the paper's name is drawn in on the cover and the PDF edition:
  `"blackletter"` (as before), `"serif"`, `"sans"`, or the path to a `.ttf` of your own. A
  file that can't be used falls back to the blackletter and the run says why. Inside the book the
  title is still your Kindle's own font.

## 0.5.0 — your library, a smarter Kindle tab, and other readers

- **The Books tab, a library as well as a queue** (Books and Kindle design, phase 5).
  - **Your library** sits beside **To send**: covers from Deckle's library, calibre and the
    folders you named. You can search it, and group it by series with the gaps called out
    ("The Expanse, 1–4 · 5 isn't in your library"). **Not on this Kindle** shows what that
    Kindle doesn't have yet. Pick books and add them to To send. With no library, the queue is
    the whole tab, as before.
  - **Before you send**: click a queued book's title for:
    - what Deckle will do: typography fixes counted, pop-up notes made, the cover, the mode and
      why, X-Ray and Word Wise, contents, pictures and warnings, worked out by running the
      stages before Kindle Previewer;
    - the first page of chapter one as a Kindle sets it, **Before** and **After** the
      typography pass;
    - **Cover**: its own, Deckle's typographic cover, or one of Open Library's (with Open
      Library on);
    - **Fix details**: title, authors, series and number, written to the library's copy and
      never to the original or calibre.
  - **Watch a folder** (Settings → General, off by default): EPUBs that land in it are added to
    the library.
- **The Kindle tab, in rooms** (Books and Kindle design, phase 4). Like the Daily tab, a rail
  down the left: **Shelf**, every book with its cover, a progress bar and when it was last
  opened (from the Kindle's own position files, now read for AZW3 as well as KFX), filtered by
  Reading, Not started, Finished, Amazon's own, Sideloaded or Deckle, sorted by recently read,
  title, author or size, and searchable; **Upgrades**, grouped by where each new copy comes from;
  **Highlights**; **Words**; and **This Kindle**: the space split by kind, **Make room** (finished
  books over 10 MB, second copies of a book, papers past `keep`) and **Back up**, which copies
  every DRM-free book and its `.sdr` (reading position, highlights) to this computer, Amazon's own
  left out. **Restore** puts a backup on any Kindle, leaving the books already there alone. CLI:
  `deckle kindle backup [--into DIR]` and `deckle kindle restore DIR`.
- **Upgrade from the Kindle's own copy** (Books and Kindle design, phase 3). A DRM-free AZW3
  (or a MOBI with a KF8 part) with no original on this computer is rebuilt as an EPUB from the
  Kindle's copy: its text, pictures, fonts, styles, links and contents come back, and a quality
  gate refuses a book that doesn't come back whole. The rebuild follows KindleUnpack's KF8
  path; two Project Gutenberg AZW3s rebuilt this way pass EPUBCheck with no errors. Older
  MOBI-only files, HUFF/CDIC compression and fixed-layout books are left as they are.
- **Upgrade books Deckle didn't send** (Books and Kindle design, phase 1). The Kindle tab now
  reads AZW3 and MOBI files too (title, author, cover, DRM) and lists the books whose originals
  are on this computer: in your calibre library (read-only, matched by calibre's id, ASIN, ISBN,
  then title and author) or in folders you name in Settings → General. **Upgrade** sends each
  through the Books tab with Enhanced Typesetting, page numbers, X-Ray and Word Wise in place of
  the Kindle's copy, which is kept with its `.sdr` (highlights, reading data); **Put back**
  undoes it. Books you're partway through aren't ticked at first. Store books and anything with
  DRM are never touched. CLI: `deckle kindle upgrade [--dry-run]` and `deckle kindle put-back`;
  `deckle kindle books` names each book's class.
- **The paper as data, version 1.1: fit for a 12.9-inch iPad** (Books and Kindle design,
  phase 2). Bundles now carry each photo as a full-size color master (up to 2400 px, JPEG 85)
  with its caption, credit, alt text, focus point and average color, and each story's byline,
  publish time, reading time, prominence, a paywall flag and a pull quote. Moving pictures
  stay the reader's job: parallax from the still. Every fetched story also loses boilerplate
  (newsletter sign-ups, "Advertisement", share prompts) and gets curly quotes and dashes;
  `deckle digest lint` shows what a page keeps and drops.
- **The Daily tab, in rooms.** Its fourteen cards are grouped into five rooms in a rail down the
  left: Today (the press bar, tomorrow's paper, the page plan and the week), Editions, Reading,
  Delivery and The paper, each with a line on what's in it ("Press at 05:30", "Kindle and 2
  more"). Delivery lists every place the paper goes, the Kindle, Email to Kindle, reMarkable,
  export folders, Kobo and PocketBook, one row each with its state, and opens a row's settings
  beneath it. This week's **All editions** now opens the Editions room.
- **The edition bundle, for reader apps** (edition bundle design, phase 1). `format = "bundle"`
  on an export folder writes each edition as data: `edition.json` with the front page plan, every
  story's text, link and photos, the weather as numbers and the crossword with its answers, beside
  the photos, cover and EPUB. It's a documented, versioned format (CC-BY-4.0 spec, MIT JSON
  Schema) that a native app reads; `digest build --bundle` writes one by hand.
- **The PDF edition and reMarkable** (other readers design, phase 4). The paper can be a PDF
  laid out for one screen (reMarkable 2, Paper Pro, Paper Pro Move, iPad, A5), with the masthead,
  weather strip, photos and crossword grid in place, an outline, front-page links and searchable
  text. `format = "pdf"` puts it in an export folder; `[remarkable]` (or the Daily tab's
  reMarkable card) uploads it to a reMarkable over its USB web interface when it's plugged in, no
  account needed. A reMarkable can't be pruned. Not yet checked on a real tablet.
- **PocketBook** (other readers design, phase 3). Plugged in once, a PocketBook gets the paper,
  long reads and Inbox books as EPUBs in a `Deckle/` folder, pruned to `keep`, and `deckle run`
  sends to it when no Kindle is plugged in. No Previewer needed. Its highlights and shelf aren't
  read yet, and it hasn't been checked on a real PocketBook.
- **Kobo** (other readers design, phase 2). Plug a Kobo in once and every morning's paper is
  also queued for it, and goes over as a KEPUB (page counts and reading statistics) in a `Deckle/`
  folder the next time it's connected, pruned to `keep`. Long reads and Inbox books go too, and
  `deckle run book.epub` sends to a Kobo when no Kindle is plugged in. No Kindle Previewer is
  needed for any of it. `[sync] clippings` exports a Kobo's highlights as Markdown, and From Your
  Shelf prints the books you're reading on it. Not yet checked on a real Kobo.
- **The paper in a folder, for an iPad or any reader** (other readers design, phase 1). Add an
  `[[export]]` folder to `digest.toml`, or **Add folder…** on the Daily tab's **Export folders**
  card, and every morning's edition is also put there as an EPUB, plugged in or not, keeping the
  newest `keep`. An iCloud Drive folder reaches an iPad's Files app. With `photos = "color"` the
  stories' photos stay in color for that folder (the Kindle's paper is still grayscale), and
  `books = true` brings the long reads along. Kindle Previewer isn't needed for it: without
  Previewer, `digest run` sends the paper to the export folders only, instead of failing.
- **Bind several files into one book** (#22). `deckle bind a.epub b.docx c.pdf https://… --title
  "Course Packet"` makes one EPUB of them, in that order: each part keeps its own styles and
  pictures, and the contents list every part with its chapters under it. EPUBs, Word documents,
  text files, PDFs and web articles all work; a finished `.kfx` doesn't. Send the result with
  `deckle run`. In the app, drop the files, then **Bind into one book…** in the Send button's
  menu: tick and order the parts, name the book, and it takes their place in the list.
- **Looked-up words to Anki, in the app** (#135). The Kindle tab has a **Looked-up words** card:
  **Export to Anki…** saves every English word you looked up on that Kindle as flashcards (the
  word, a short definition, the sentence and the book), the same CSV as `deckle words anki`.
- **PDFs in the app** (#133). Drop a PDF on the window (or a folder of them, or pick one with
  **Choose files…**) and it's reflowed, built and sent like any book; its row shows the title and
  author it reflows with, and a scan says why it can't be read. The app now carries PDFium, so
  nothing extra needs installing. `deckle inspect` reads PDFs, Word documents and text files too.
- **Word documents and text files in the app** (#134). Drop a `.docx` or `.txt` on the window,
  or pick one with **Choose files…**, and it's built and sent like an EPUB, its row showing the
  title and author it imports with. A folder drop takes Word documents too, but leaves `.txt`
  files alone (in a folder they're usually notes); name a text file directly to send it.
- **PDFs, reflowed** (#18). `deckle run paper.pdf` turns a text PDF into a Kindle book that
  reflows: two-column pages read column by column, running heads and page numbers are dropped,
  words broken at line ends are joined again (compounds like "well-known" keep their hyphen), and
  tables, displayed equations and figures come through as pictures cut from the page. Chapters
  and contents come from the headings. Every PDF run carries a note that the layout was rebuilt.
  Reading PDFs needs PDFium (free, from bblanchon/pdfium-binaries): `tools/fetch-pdfium.sh`
  fetches it for a source build, or set `DECKLE_PDFIUM`. Scans (no text layer) are refused; OCR
  is #19. The desktop app doesn't take PDFs yet.
- **Looked-up words to Anki** (#60). `deckle words anki --to words.csv` writes every English word
  you looked up on the Kindle, once each (its newest lookup), as a CSV Anki imports: word, short
  definition (from a Word Wise database already on this machine, never downloaded), the sentence
  you met it in, the book and the day. It reads `vocab.db` over the same single connection as a
  send, writes nothing to the Kindle, and overwrites the file each time.
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
