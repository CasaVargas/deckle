# Changelog

## Unreleased

- **Adding a book again under a name that differs only in case keeps it.** On a Mac (or any
  case-insensitive disk), adding `mini.epub` when the library held `Mini.epub` deleted the copy
  just written, and a library scan then removed the loose file too, leaving no copy of the
  book. The library now keeps the one file.
- **Two different files with the same name are two books.** A text, Word, RTF or comic file
  without its own identifier is known by its title and author, and a text file's title is its
  name, so adding `home/notes.txt` replaced the book from `work/notes.txt`. Such a file now
  updates a book only when it's the same file or the library's own copy; anything else is filed
  as a new book. An EPUB with the publisher's identifier still updates its book in place.
- **A second paper no longer deletes the first one's editions on a Kobo or PocketBook.** Its
  files (`deckle-daily-kid-courier-…`) were counted as the main paper's editions and took their
  places, so every one of the main paper's editions was removed, the newest too. Now only a
  paper's own dated editions count toward its `keep`, and each paper on the reader is pruned
  by its own, as on a Kindle.
- **A send no longer removes a different book from the Kindle.** Two books can get the same
  Deckle ASIN, and sending one removed the other, with its notes and highlights. Deckle now
  removes an older copy only when its title and author match too; another book with the same
  ASIN stays, and the send says so.
- **Long older Kindle books in other alphabets import.** A MOBI 6 book with no page breaks,
  more than about 200 KB of text and letters outside ASCII (Cyrillic, Greek, accented Latin)
  failed to import; it's now cut into parts between paragraphs as English
  books are.
- **News in your language.** Local, each other place, each interest and each Google News topic
  can read another country's edition of Google News: pick Deutschland, España, France, Italia or
  Brasil under the section in the Daily tab, or set `hl`, `gl` and `ceid` by hand. On this day
  can come from another language's Wikipedia (`[onthisday] language = "it"`). Leaving them
  alone keeps the US English edition and English Wikipedia
  ([docs/daily.md](docs/daily.md#news-in-another-language)). Thanks to [@antoblu99](https://github.com/antoblu99).
- **The window opens faster.** At launch Deckle was starting Kindle Previewer twice to read its
  version, asking the system about each plugged-in reader, and counting the library, all before
  the window could draw. These now run in the background, and Previewer is started once per
  install instead of at every check. The device check every few seconds no longer stutters the
  window either.
- **Eleven fixes from a review of the whole project.** Thanks to [@jloutsch](https://github.com/jloutsch),
  who found and reproduced each one.
  - Removing a book from a Kobo or PocketBook no longer deletes the highlights of another
    format of the same name, and takes a KEPUB's own sidecar; a backup now copies a KEPUB's.
  - A reMarkable backup keeps documents that share a name (`Notebook.tar`,
    `Notebook (2).tar`) instead of writing one over the other and reporting both copied.
  - "Make again" on a puzzle book makes the same book: its seed lost its last digits on the
    way to the window. A different book never takes the name of one already there, and the
    same book keeps the iPad bundles it was sent as.
  - KOReader over Wi-Fi: today's paper always downloads, even when newer editions of other
    papers fill the recent list (it answered 404).
  - A Kitchen Book recipe saved from a page takes a picture only from the page's own folder,
    as the paper's stories do, and a recipe file's name is its id, whatever the file says
    inside.
  - No more crashes on a footnote with `&amp;` before a curly quote, a KOReader backup with a
    date out of range, or a clippings line that is a bare "- ".
  - Highlights: a byte-order mark before any title no longer splits one book in two, a title
    like `1984` or `True` is quoted in the note's frontmatter, and a book with tens of
    thousands of highlights groups in a moment.
  - X-Ray: "Drake" no longer loses its "Dr" before it's matched to "Francis Drake", names are
    merged the same way on every run, and a book with thousands of names is read in seconds
    instead of a minute.
  - The Books tab opens quickly with a large calibre library (20,000 books took two minutes).

## 0.14.0 — the paper in color on a Colorsoft, and two-column PDFs read in order

- **The paper in color on a Colorsoft.** Once Deckle has seen a Kindle with a color screen (a
  Colorsoft or a Scribe Colorsoft), the paper keeps its photos in color, as the Kindle book and as
  the Scribe PDF; they were always gray before. A gray Kindle gets the same paper and shows them
  in gray. `photos = "color"` or `"gray"` under `[sync]` settles it by hand
  ([docs/daily.md](docs/daily.md#photos-on-a-color-kindle)).
- **Two-column PDFs read one column, then the other.** A journal's tight gutter (about 1.3 em,
  as in *Nature Communications*) is now found, so a left-column line is no longer glued to the
  right-column line beside it. Citations set as superscripts ("generations⁵³⁻⁵⁵") stay in
  their sentence instead of breaking it into short paragraphs, a full stop before one still
  ends the sentence, and an equation's tall bracket no longer shuffles the lines beside it
  together.
- **A paper's first page reads in order.** Author names stay on one line past their ORCID
  icons, a sidebar of dates beside the authors and abstract is read on its own instead of
  between their lines, and an abstract or author list set in larger type is a paragraph, not a
  heading. A figure caption set smaller than the text no longer runs on into it.
- **More displayed equations become pictures.** An equation whose symbols are in a math font
  Deckle didn't know (Advent's, MathTime's) and whose letters are in the text font was
  scrambled into its pieces ("heat = X T Txpct 2 ð Þ cum day"); it's now cut from the page whole.
- **A shorter README.** A section for each of the app's five tabs (the Cookbook and Puzzles had
  none of their own), shorter tables, and the long parts moved to pages of their own:
  [docs/books.md](docs/books.md), [docs/readers.md](docs/readers.md),
  [docs/cli.md](docs/cli.md) and [docs/how-it-works.md](docs/how-it-works.md), with the AI
  editor's setup in [docs/daily.md](docs/daily.md#the-ai-editor). **What goes online** now lists
  the Kitchen Book's two: a recipe's page when you save it, and the optional engravings.
- **Deckle in Unraid's Apps tab.** The container is listed in Community Applications: search for
  **Deckle** in the Apps tab. The template now also offers the ports for KOReader over Wi-Fi
  (47220) and KOReader's calibre connection (9090), and the reader server's address, under
  **Show more settings**. It's filed under Books and Productivity, with screenshots. It's edited
  in `docker/unraid/deckle.xml` and copied to
  [CasaVargas/deckle-unraid](https://github.com/CasaVargas/deckle-unraid), which Community
  Applications reads.

## 0.13.0 — the Devices tab, the paper as a PDF on a Scribe, and the app in six languages

- **The Devices tab: Kobo, PocketBook and reMarkable libraries.** The Kindle tab is now
  **Devices**, with a chip for each Kindle and each other reader connected: a Kobo or
  PocketBook as a USB drive, a reMarkable over its USB web interface or SSH. Each gets a page
  with every book (title, author, folder, format, size), how far it's read and when (a Kobo's
  database and KOReader's sidecars; the reMarkable's own page count), the space left, search
  and sort, **Remove** for the ones you tick, and **Back up** (now, or to a folder) with the
  last backup's date.
  - A Kobo's `KoboReader.sqlite` is read from a copy and copied aside into
    `reader-backups/<reader>/databases/` before anything is removed; the Kobo drops a removed
    book from its library when it's next ejected. Books bought from Kobo are listed and never
    removed. KOReader's `.sdr` notes go with a removed book and come along in a backup.
  - A reMarkable over its web interface backs up each document as a PDF with its handwriting,
    and can't delete. Over SSH, a removed document goes to the tablet's trash (and the tablet's
    reader restarts to show it), and a backup keeps each document as a `.tar` of the tablet's
    own files, handwriting included.
  - Backups go to `reader-backups/<reader>/<time>/` beside `digest.toml`, with a
    `backup.json`. New commands `readers_list`, `readers_library`, `readers_remove`,
    `readers_backup` and `readers_last_backup` (also in the web app); the logic is
    `deckle_core::reader_library`. Signing in to a reMarkable over SSH moved to
    `deckle_digest::remarkable_login`, shared by the command line and the app.
- **Deckle in six languages, corrected by readers.** A new **Language** row under Settings →
  About offers English, Deutsch, Español, Français, Italiano and Português (Brasil), each named
  in itself; switching is live, and the choice is remembered. So far it covers the welcome (with
  its schedule and bookmarklet lines) and the Language row; the rest of the app is still in
  English and moves over one area at a time. Each language began as an AI draft for its readers
  to correct, and a French reader has already corrected the French. The Language row says so
  and links **Help translate** to [docs/translating.md](docs/translating.md), and a
  **Translation fix** issue form takes a correction without touching a file. With no choice
  made, Deckle follows the browser's languages (Mexican Spanish gets Spanish, European
  Portuguese the Brazilian), and the page's `lang` follows the choice so a screen reader speaks
  the right language. A new language is one file in `apps/desktop/src/locales/`: a half-done one
  can be merged, and shows up once it's complete and listed in `LOCALES`.
- **Edit a book's details anywhere it is.** Title, authors and series:
  - **Before it's sent:** **Change title & author for this send…** on a queued book's menu
    writes them into a copy of the book for that send (a DOCX, TXT or other import is made an
    EPUB first); the file you added is never changed, and the library keeps the edited copy.
    `RunRequest.details` (`BookEdit`).
  - **On a Kindle:** **Edit details…** in a book's drawer finds what it can be rebuilt from
    (`device_edit_source`): the library's copy (fixed for good first), its original in
    calibre or a folder (never changed), or the Kindle's own DRM-free AZW3/MOBI made an EPUB.
    It goes on the Books tab as an upgrade, replacing the Kindle's copy on send; the old copy
    and its notes are kept, and **Put back** undoes it. The new copy starts without a reading
    position or highlights on the Kindle (written out first when a highlights folder is set).
    Store books, DRM and KFX with no original say why they can't be changed.
  - **On a Kobo or PocketBook:** **Edit details…** on the Devices tab rewrites an EPUB's or
    KEPUB's own details in place (written beside it, then swapped), and on a Kobo its library
    entry (`Title`, `Attribution`, and `Series`/`SeriesNumber` where the firmware has them), the
    database copied aside first (`kobo_shelves::set_book`). **On a reMarkable over SSH:**
    **Rename…** sets the document's name.
- **Removing a book Amazon delivered says what happens.** Removing a book bought from the
  Kindle Store or sent to Kindle by email takes only the download: Amazon keeps it in the
  account and the Kindle can still list it under All. The Kindle drawer's and This Kindle's
  confirmations now say so and point to Remove from library or Manage Your Content and
  Devices.
- **Puzzles tab, easier to read.** A theme that's greyed out now says why under the chips, not
  only on hover, with more exact reasons (the paper has no Puzzle page to keep headline words,
  none kept in the last 7 days, no interests followed, the Scribe not sharing looked-up words
  over USB, the paper's settings not loading). Making a book reports each step (gathering the
  words, making each puzzle, the AI's clues, laying out the pages) with a whole-book bar, a
  percentage and the time so far (`puzzles-progress` now carries `stage`). The preview stays
  beside the form and scrolls on its own, so a long crossword's clues no longer push Your
  books off the page. The Daily, Puzzles and other wide tabs use up to 1480 px instead of
  1120 px on big screens.
- **The paper as a PDF on a Kindle Scribe.** Each Kindle can take the paper as the Kindle book
  or as a PDF laid out for a Scribe's screen, which its pen writes straight onto: **Send the
  paper as** on This Kindle in the Devices tab, `deckle devices paper pdf|book [--kindle …]`, or
  `paper = "pdf"` under `[sync.devices."<id>"]`. While any Kindle asks for it, the build also
  makes a `kindle-scribe` PDF edition and queues it beside the Kindle book; each push sends the
  one that Kindle takes and drops the other. Pruning counts `.pdf` papers with `.kfx` ones.
  `[email] pdf = true` (**Send it as a PDF to write on** on the email card) emails the same PDF
  instead of the EPUB, as `application/pdf` and never with "convert" in the subject, so
  Amazon keeps it a PDF.
- **The puzzle page no longer looks like tables on a Kindle.** The mini's grid and the word
  search, and yesterday's answers to both, are drawn as square PNG pictures in the EPUB instead
  of HTML tables, which a Kindle (Enhanced Typesetting) stretched to the column's width with a
  zoom button under them. Yesterday's word search loops each word in gray. The search's words
  print as one line-wrapped paragraph, theme words in bold. The PDF edition and the iPad
  bundle are unchanged.
- **Puzzle books for the iPad, as data.** A puzzle book can be written as a bundle the way an
  edition is: a `deckle-puzzles-<title>-<date>-<seed>.decklepuzzles` folder with `book.json`
  (every puzzle of every kind with its full solution, so Check and Reveal work offline) and a
  cover, written through the same hidden-folder-then-rename staging as the edition bundle, the
  manifest last. The same options, seed and date give the same book as the PDF: one `make`, two
  writers. `deckle puzzles <kind> … --as bundle` writes one to `-o` or `--to <folder>`; a bundle
  already there is never overwritten without `--replace`. The format is documented in
  `docs/superpowers/specs/2026-11-03-puzzle-book-bundle-design.md` with an MIT schema,
  `docs/formats/puzzle-book.schema.json`, and a sampler of every kind under `fixtures/puzzles/`.
- **Send to iPad.** The Puzzles tab's **Your books** gets **Send to <name>** for every export
  folder whose format is bundle (an iCloud Drive folder the iPad's reader watches), beside Send
  to the Kindle and Save a copy. The book goes as a bundle, made again from its saved request
  with the same seed and date; a bundle already there asks before it's replaced, since the
  reader may hold ink on it. The row says where it went.
- **Every Sunday, with the paper.** A paper's new `[puzzles]` table (`sunday = true` and the
  recipe: kind, count, size, difficulty, theme, clues; a switch and the recipe at the foot of
  the Puzzles tab) has the Sunday morning run make the book after the edition is in place and
  write it beside the edition in every bundle export, marked with the paper and the Sunday it
  belongs to, so the reader shelves them together. `keep` prunes editions, never books, and a
  Sunday run again leaves the book as it is.
- **What the iPad solved comes back.** The reader writes a return per book under
  `.deckle-returns/<reader>/puzzles/<book-id>.json` (`docs/formats/deckle-puzzle-returns.schema.json`,
  MIT, with a fixture): per puzzle, done, seconds and its own ink by kind. Your books shows
  "n of N solved on the iPad" beside the Kindle's count, merged across readers by the latest
  per puzzle; `deckle puzzles solved <bundle>` prints it. The daily word search's found words
  come back too (returns 1.2, `puzzle.search`), and tomorrow's "yesterday's answers" ticks them.

## 0.12.1 — the AI on this Mac asks for the model it installed

- **Fixed: the AI on this Mac asked for a model it hadn't installed.** With Settings → AI on
  **On this Mac** and no model named, the checklist installed the model for this machine's
  memory (`qwen3:8b` on 16 GB or more) while X-Ray, recaps, the editor and puzzle clues asked
  Ollama for the smallest one, `qwen3:4b`, and got "model not found"; X-Ray fell back to basic
  descriptions with only a warning. Every request now resolves an empty model the way the
  checklist and `deckle llm pull` do. (#308)

## 0.12.0 — a local model that sets itself up, and the Puzzles tab

Everything since 0.11.0 was tagged: an AI that runs on your own computer with nothing to
configure, one setting for it across the books and every paper, and a fifth tab that makes
puzzle books to solve in ink, themed on what you read.

- **Ollama, on this computer, as its own choice.** `--llm ollama`, `[llm] provider = "ollama"`
  and "Ollama, on this computer" in the app talk to Ollama's own API instead of its
  OpenAI-compatible shim, so Deckle sets the context size itself (the Daily editor's one
  request no longer silently overflows Ollama's 4k default), turns thinking off, holds the
  model to a JSON schema for X-Ray and the editor, and needs no key. The default model is
  `qwen3:4b`, or `qwen3:8b` on a machine with 16 GB or more. New: `deckle llm check` (is
  Ollama running, is the model installed, does it answer, and what to do about whichever
  isn't) and `deckle llm pull` (download the model, with progress). A local model gets
  shorter story excerpts in the editor's request, and a paper too long for one request says
  so in its note.
- **Settings → AI.** One page for where the AI runs, for X-Ray, chapter recaps and the Daily
  editor alike: **Off**, **On this Mac** or **A service online**. On this Mac is a checklist
  that fills itself in: Ollama running (with **Get Ollama**, and a check every few seconds
  until it answers), the model installed (**Download**, with a progress bar and a Stop) and
  **Try it**, then a line on what to expect. Advanced holds every model Ollama has and an
  address for Ollama on another computer. The service card keeps the provider, model and key
  fields with a price table beside them. Reading aids and Online & privacy point there, and
  Online & privacy says "nowhere online" when the model runs on this Mac.
- **The Puzzles tab, and four new kinds of book.** A fifth tab makes a write-on puzzle book:
  crossword, word search, sudoku, and now word ladders (three a page, the rungs by
  difficulty), cryptograms (public-domain quotes or your own, letters given by difficulty, a
  key to work in), nonograms (Deckle's pictures and shapes, every one solvable by line logic,
  or your own photo as puzzle 1) and mazes (6 to 40 a side, the path drawn in the answers).
  The first page is previewed as it will print while you change the options, a progress bar
  follows the making, and every book is kept in `~/Deckle/Puzzles` with the options that made
  it, so **Make again** gives the same book and **Send** copies it to the plugged-in Kindle or
  **Save a copy** to a folder. The same four kinds on the command line: `deckle puzzles
  ladder|cryptogram|nonogram|maze`.
- **Crosswords and word searches about what you follow.** A theme for the book: your interest
  sections (pick which) or the whole week's paper, with the words from the headlines the Daily's
  puzzle store keeps and the headline as the clue; the words you looked up on the Kindle, clued
  with the definition you tapped; your cookbook's dishes and ingredients; or a subject the AI
  writes about, words and clues both. With the AI on (Settings → AI), any crossword's clues can
  be rewritten as trivia, cryptic, or for a kid, one request a puzzle, every clue checked so it
  never names its answer. `deckle puzzles crossword --theme interests --interest Rust --clues
  cryptic` does the same. Design: `docs/superpowers/specs/2026-11-02-puzzles-tab-design.md`.
- **The welcome shows the back pages.** A fourth page in the first-launch tour: the Mini with
  yesterday's headlines as its answers, the word search, the word ladder, the almanac, the
  horoscope and On this day, how a Kindle's notes mark the answers, the puzzle books to write
  on, From Your Shelf, and the AI model with a link straight to Settings → AI. The miniature
  paper beside it shows a crossword grid on that page.
- **One setting for the paper too.** A paper's AI editor now runs where Settings → AI says,
  with the key saved there when the environment has none, so there's nothing to copy between
  Books and the Daily. A `[llm]` table that names a provider, model or endpoint stays that
  paper's own (the command line's and Docker's way, and **Advanced** on the paper's sheet);
  one from before this, with only an endpoint, keeps meaning an OpenAI-compatible server.
  Design: `docs/superpowers/specs/2026-11-02-local-model-design.md`.

## 0.11.0 — every reader, comics, puzzle books and KOReader

About sixty pull requests since 0.10.0, in ten themes.

**The reMarkable, wireless and with books.** Until now a reMarkable only got the paper, only
over the USB cable.

- **Books to a reMarkable.** `deckle run --to remarkable` and **Send to reMarkable** on the
  app's Inbox send books the way the paper goes: a comic (CBZ, CBR, a folder of pages, a scan)
  as an image-only PDF, one page per image fitted to the `[remarkable] screen` and grey unless
  the panel is colour; any other book as an EPUB, with its Who's who page and recaps if asked.
  They wait in the reMarkable's outbox until it answers, then `deckle digest push` or `watch`
  sends them, with or without the paper set up. A publisher's fixed-layout EPUB is refused for
  now, since the tablet's reader would reflow it.
- **A reMarkable over SSH.** `[remarkable] transport = "ssh"` writes the paper and books
  straight into the tablet's document folder, over Wi-Fi as well as the cable, in a Deckle
  folder, and prunes old papers there, which the web interface can't do. The password (from
  the tablet's Settings → Help → About) is kept in the system keychain by `deckle digest
  remarkable password`; the host key is pinned on first use. The tablet's reader has to
  restart to show new files, so Deckle restarts it only when the tablet has been idle for ten
  minutes, and says so otherwise. Deckle never changes a tablet setting: SSH over Wi-Fi, and
  Developer Mode on a Paper Pro (a factory reset), are yours to turn on, and the docs say so.
- **Easier to reach.** `[remarkable] address` (**Address** on the card) points Deckle at a
  tablet whose web interface you've put on your Wi-Fi, instead of the USB link. `deckle
  devices` lists the tablet when it answers. And `docs/daily.md` says how to pull the paper
  from Dropbox, Google Drive or OneDrive with the tablet's own integration, no cloud account
  with Deckle.
- **The Paper Pro Move gets colour.** `[remarkable] screen = "remarkable-paper-pro-move"` now
  sends the paper's photos and a comic's pages in colour, as the Paper Pro does; it had been
  treated as a grey screen (#297).

**Comics, manga and scanned books, on every reader** (#20). A page of pictures goes to each
reader as pictures, fitted to its screen.

- **Bring in a comic any way you have it.** A `.cbz`, a `.cbr` (through the system's
  `bsdtar`), a folder of page images, a fixed-layout EPUB or AZW3, or a scanned PDF, with or
  without an invisible text layer. Drop it on the window, keep it in the library, or queue it
  from the Inbox. Two-page spreads are split at import; `ComicInfo.xml` gives the title and
  series.
- **Kindle.** A comic arrives as a fixed-layout AZW3, which the Kindle opens as it would a
  bought comic, with its pages fitted to that Kindle's own screen: nineteen models known by
  their serial number, the Colorsoft in colour. Checked on a Kindle Scribe over MTP (#278).
- **Kobo and PocketBook.** A fixed-layout EPUB without the KEPUB markup Deckle normally adds,
  which would break picture pages. Thirty Kobo models are recognised and fitted; a colour Kobo
  keeps colour.
- **Reads right to left.** A checkbox on the comic's row in the app, remembered per book, for
  manga, and `deckle run --direction rtl` on the command line. A comic that says so in its
  own metadata gets it without asking.
- **Send a PDF as it is.** `deckle send` copies a PDF, TXT, AZW3 or MOBI to a Kindle or a
  folder untouched, under its own name. On a Scribe 2024 that's the way to put a document on
  the tablet that takes pen ink.

**Puzzle books you solve in ink on a Kindle Scribe** (`deckle puzzles`). A device test
settled that only a PDF takes pen ink on the Scribe, so these are PDFs: a title page, one
puzzle per page, the answers at the back, and a `--screen` for a reMarkable, an iPad or A5
paper. See [docs/puzzle-books.md](docs/puzzle-books.md).

- **Word search books.** Pick the size (8 to 20) and the directions, or give your own words.
  Every puzzle is checked against a blocklist so nothing rude appears by accident.
- **Crossword books.** Any size from 5×5 to 15×15, symmetric, filled from a 45,000-word
  clue list drawn from Wiktionary. A grid that won't fill takes rarer words and says so.
  `--words` with `word: clue` lines seeds each puzzle with as many of your own as will fit.
- **Sudoku books.** Easy, medium and hard, each with exactly one solution. A hard puzzle is
  always solvable by logic, never a guess.
- The same options, seed and date make the same book, byte for byte, and each run prints the
  command that makes it again. Books go to a folder or straight to a plugged-in Kindle.

**AI recaps for books, without spoilers** (#29). Optional, with the model you already set up
for X-Ray (Anthropic, OpenAI, or a local server). Nothing is bundled and nothing is sent
unless you turn it on.

- **Chapter recaps.** A short, labelled recap at the end of each chapter, written from that
  chapter alone so it can't spoil what's ahead. For Kindle, Kobo, PocketBook, a reMarkable
  and any EPUB reader. Results are cached, so a book is never summarised twice.
- **A Summary page** at the back of the book, the whole story including the ending, built
  from the chapter recaps rather than sending the book's text again.
- **A Previously page** for a later book in a series: what happened in the earlier ones,
  placed before chapter one and written only from those books' recaps, so it can't spoil the
  one you're starting.
- **Settings → Reading aids** has the switch and the app a per-book toggle; `deckle run
  --recaps` on the command line, which now stops up front if no model is set rather than
  after the whole conversion.

**KOReader as a first-class reader.** KOReader runs on jailbroken Kindles, Kobos,
PocketBooks, Boox tablets and reMarkables. Deckle now reads from it, serves it and manages it.

- **Highlights and progress from any reader running it.** Deckle reads KOReader's files
  beside each book, over USB or from a folder the reader syncs back, and never writes them.
  Highlights come out as Markdown like a Kindle's; books in progress reach From Your Shelf.
- **Today's paper over Wi-Fi, no cable.** Deckle's watcher serves an OPDS catalog on your
  home network, behind one account with a password of your choosing. KOReader browses it and
  downloads the paper and your books, a KEPUB for a Kobo. **Daily → Delivery** has the card
  and the two addresses; `deckle reader-server password|status` on the command line.
- **Progress sync, served by Deckle.** Point KOReader's own sync at the same address and
  every reader reports how far you read, down to the section of the paper, so the readership
  report is exact.
- **A Deckle plugin for KOReader** (`deckle koreader plugins install deckle`). It fetches the
  morning paper on its own when the reader wakes or joins Wi-Fi, checks the download, tidies
  its own old papers, and sets up progress sync with one tap.
- **Plugins, managed from Deckle.** List, install, update, remove and roll back KOReader
  plugins from a zip, a folder or a GitHub release, over USB. Switch them and user patches on
  and off. **Backup and restore** of KOReader's settings and reading data now carries plugins.
- **A plugin store.** A reviewed, pinned list of recommended plugins, each install checked
  byte for byte against the reviewed file, and a Discover list beyond it.
- **Settings → KOReader** in the app: the plugged-in reader, the Deckle plugin, your plugins
  with switches, and the store.
- **Papers open as designed.** Each paper Deckle sends to a reader carries KOReader settings
  for its fonts, styles and hyphenation, written once and never over your own.

**Deckle as the bridge to your calibre library.** calibre stays the library of record;
Deckle moves books out and reading data back, over Wi-Fi, and never edits calibre's database.

- **KOReader's calibre connection, answered by Deckle.** Tap *Connect* in KOReader and
  Deckle appears as calibre would. Your books, calibre's books and the paper go to the reader
  over Wi-Fi, with no plugin and no calibre running. The app shows every reader that has
  connected, its books and free space, and a queue; `deckle wireless` on the command line.
- **Reading data back into calibre.** Progress, finished, last read and rating into calibre
  columns you choose, through calibre's own tools. Off by default, with a preview, and a
  value you changed in calibre is left alone.
- **Highlights into calibre.** Every highlight and note as Markdown in a column, and
  optionally into calibre's own annotations.
- **Shelves from calibre rules.** A tag, series, author, column or saved search becomes a
  shelf on a Kobo or a collection in KOReader, holding the books that are actually on the
  reader. Only shelves Deckle made are ever changed.

**Kobo and PocketBook catch up with the Kindle.**

- **Who's who.** The people, places and terms Deckle finds for a Kindle's X-Ray now go inside
  the EPUB as a Who's who page at the back, on a Kobo, a PocketBook, a reMarkable and any
  EPUB reader. The first mention of each name in a chapter links to its entry, and back.
- **The paper to a Kobo over Dropbox.** Point an export folder at Dropbox's *Rakuten Kobo*
  folder as KEPUB and the Kobo fetches the paper each morning when you tap Sync. Deckle still
  signs in to nothing.

**The paper.**

- **A daily word search.** A 9×9 grid with eight to ten hidden words beside the mini
  crossword, with yesterday's answers, in every edition: EPUB, PDF and the bundle.
- **No repeat crosswords.** The mini once printed the same grid two days running, with the
  answers above it. A grid from the last sixty days is never reused.
- **Google News stories read in full again.** For a week, every story linked through Google
  News arrived as a stub. Dropped connections are retried and the reply is parsed properly.
- **Teasers stop at a sentence.** One teaser was a whole 37,000-character article. Teasers
  now end at a whole sentence within about 300 characters, and spaces around italics are kept.
- **No more "Full story unavailable offline."** A story the paper couldn't read in full (a
  paywall, a video page, a fetch that failed) still runs after the whole stories as a brief,
  but without that line, the way a `lead` section's briefs always have.
- **Kindle readership works again.** The print layout's section folios had hidden every
  section from the readership report.

**The app and the importers.**

- **A welcome on first launch.** Four pages: your paper and its morning schedule, the
  bookmarklets, and where things are. The Save to paper page watches for your first save and
  says when it landed. Beside the pages sits tomorrow's front page in miniature, in your
  paper's name and masthead face, with the day's date and the press time as you set it; the
  first story you save lands on it as a clipping. **Settings → About** shows it again.
- **Bookmarklets you drag, not paste.** *Open the install page* (on the Reading room, the
  Cookbook's Add page and the welcome) opens a page in your browser with **Save to paper** and
  **Save recipe** as buttons to drag to the bookmarks bar, and how to show the bar in each
  browser. The web app shows the buttons in place. Copy is still there.
- **Messy Word and PDF files get their structure back** (#268). A document with no real
  headings gets chapters recognised from size, weight and "Chapter" lines, with no model and
  no network. Documents that already have structure convert as before.

**Fixes you'd otherwise hit.**

- The watcher restarts after an update. It used to keep running the old build until the next
  login, so a new bookmarklet hit an old watcher and got *Not found*.
- Kindle Previewer runs one conversion at a time. Two at once shared a scratch folder and
  both failed; jobs now wait their turn.
- Found on real readers: KOReader's own SSH plugin no longer gets Update and Remove buttons;
  a reader's free space shows correctly instead of 0 MB; the paper goes over the calibre
  connection once, not on every connect.
- A comic with a wide title banner no longer stretches every page to the banner's shape on a
  Kindle.
- A scan with an invisible OCR layer is treated as a scan, not as text to reflow.

## 0.10.0 — Deckle on Linux

- **Deckle in a container.** A Linux image, `ghcr.io/casavargas/deckle`, runs the Daily
  headless: it prints each paper on schedule, catches up a morning it missed, and emails the
  paper to your Kindle. Passwords come from environment variables, since a container has no
  keychain. There's a Compose file, an Unraid template, and Linux command-line tarballs (x86_64
  and aarch64) with each release. Kindle book conversion still needs Mac or Windows. See
  [docs/docker.md](docs/docker.md).
- **The app in your browser.** The container now serves the Deckle app on port 8080: the Daily,
  the Cookbook and your library, from any browser on your network. Set `DECKLE_WEB_PASSWORD` to
  turn it on; there's one password and no accounts. Files you pick come from the server's
  folders, and uploads and saved files go under `/data/Deckle`. Features that need a Mac or PC
  (Kindle book conversion, a plugged-in Kindle, the keychain, Show in folder, in-app updates)
  stay visible and say why they're off. A container set up without the web UI keeps working
  with no changes: with no password, the web UI stays off and the papers still print. See
  [The web app](docs/docker.md#the-web-app).
- **Email-only papers without Kindle Previewer.** A paper whose only way to a Kindle is
  email now builds and sends on a Mac or PC with no Kindle Previewer, instead of stopping with
  a Previewer error.
- **A new icon.** The morning paper on a warm board: a deckle-edged sheet, a Fraktur "D"
  masthead, columns of type and the mini crossword. On macOS 26 it's a Liquid Glass icon.

## 0.9.0 — no more black windows on Windows

- **No more console windows on Windows.** Opening the Daily tab popped a burst of black
  command-prompt windows (a Reddit reader spotted them on first launch). They were Deckle
  asking Task Scheduler about its edition task; the same thing could happen when a Kindle was
  plugged in or Kindle Previewer ran. Every such check now runs with no window at all.
- **Save recipe works on sites that block apps.** The Save recipe bookmarklet now takes the
  recipe from the page you're looking at, so sites like Food Network that refuse Deckle's own
  fetch work too. Copy the new bookmarklet from the Cookbook tab (or `deckle save
  --bookmarklet`) and replace the old one.
- **A site that refuses Deckle says so.** Pasting the address of such a site on the Cookbook
  tab now says the site doesn't let apps read its pages and what to do instead, not a raw
  HTTP error.

## 0.8.0 — the Kitchen Book, and a paper that looks printed

- **The Kitchen Book.** A cookbook that grows on your Kindle. Save a recipe from a recipe site
  and Deckle files it in a chapter (Breakfast, Mains, Desserts, …), with the site's photo made
  for e-ink. The book has its own cover and an index by ingredient, and every Kindle gets it as
  one book, *Deckle Kitchen Book*, brought up to date each time you plug in after adding a recipe.
- **A Cookbook tab.** Paste a recipe's address anywhere on the tab, or drop a link file, and see
  it before it goes in the book. **Recipes** lists the book by chapter, with search; open a
  recipe to change its chapter, picture or note. **Delivery** shows each Kindle and lets you
  leave one out. **Book settings** has the title, subtitle and chapters (rename, reorder, add).
- **Save recipes from anywhere.** A *Save recipe* bookmarklet sends the page you're on to the
  cookbook (`deckle save --bookmarklet` prints it). Links and saved pages dropped in the
  Inbox's new `Recipes` folder go there too, so with the Inbox in iCloud Drive an iPhone
  Shortcut can save recipes from your phone. Offline? Deckle keeps trying for a day.
- **Engravings of your dishes (optional, off by default).** Deckle can draw each recipe as an
  old-fashioned engraving, using an OpenAI-compatible image service and your own API key. Only
  the title, the chapter and up to eight ingredients are sent; the site's photo is kept.
- **The Daily looks printed.** Page labels like A1 and B3, page numbers, the day's top stories
  teased across the top of the front page, a lead story that continues inside, and pull
  quotes taken word for word from the story.
- **Newspaper columns in the PDF edition.** Stories are set in justified, hyphenated columns:
  two on a reMarkable, Paper Pro, iPad or A5, one on a Paper Pro Move or in large print.
  Columns balance on their last page, with a hairline between them, and never leave a lone
  line at the top or bottom.
- **Newsletter sign-ups no longer print inside stories.** The "Sign up for the … newsletter"
  boxes some sites put in the middle of a story are left out.
- **Who makes Deckle.** Settings → About now shows Casa Vargas, the studio behind Deckle, and
  its other apps.
- **Fixes.**
  - Books your Kindle filed under `Downloads` can now be upgraded.
  - *A paper you subscribe to* opens its setup row again, and signing in works again on BLOX
    papers that moved their login form.
  - Typing in a paper's settings no longer loses focus every few seconds.

## 0.7.0 — many papers, interests, and a redesigned app

- **A redesigned app.** Every tab was rebuilt, one at a time.
  - **Books:** one Add menu, Send at the foot of To send, search in the top bar, a selection
    bar, one-line statuses, and a drawer for each book with a details editor (description,
    tags, publisher, dates, ISBN, sort names) where Open Library suggests what a book is missing.
  - **Kindle:** the Kindle in the top bar and a Shelf that reads first, a drawer for each book
    on the Kindle, Upgrades as pick-then-one-button, This Kindle in two columns, Make room by
    why, and, unplugged, the shelf as Deckle sent it.
  - **Settings:** an Overview with search and a sidebar that says what's to do; Sending as a
    choice you can see; Word Wise and X-Ray on one Reading aids page; everything that goes
    online in one Online & privacy page; Library, Kindles, and Highlights & reading regrouped.
  - **The Daily:** tabs, a paper switcher and Today; Sections as one list, saved as you go;
    Add a section; the paper's own settings; Delivery in two groups.
- **Interests.** A section that hunts: a topic ("Rust", a team) searched every morning and
  kept only when a headline names it. The paper notices what you read and suggests new
  interests, the AI editor (when on) scores each story's relevance, and the paper can nudge
  you toward one.
- **The cover's top line reads "Printed this morning".** It used to borrow a magazine's old motto.
- **Other places need only a name.** The Daily searches Google News for a place's name; the second box is now an optional custom search. A place with just a name no longer prints "Unavailable today".
- **Ten masthead faces, and a new default.** The Daily's title can be drawn in Manufacturing Consent (the new default), Grenze Gotisch, Pirata One, Texturina, UnifrakturMaguntia (the old default; `masthead = "blackletter"` still picks it), Playfair, Bodoni, Caslon, Old Standard or Inter, all under the SIL Open Font License. A face without a letter of your title falls back to Manufacturing Consent, then Old Standard.
- **Local no longer repeats a story.** The Daily's Local section skips a story an earlier section already printed and refills with its next one.
- **The Daily: a paper you subscribe to prints in full.** BLOX (TownNews) papers: sign in once, pick its sections, and they print as their own section group. Stories your plan doesn't cover print as a stranger sees them, with the reason.
- **Papers you subscribe to.** The Daily: accounts for the papers you subscribe to (`deckle digest
  subscribe`), with the password in the keychain. A story the site withholds prints as a headline
  saying why.
- **Many papers.** One Mac can print a paper for each Kindle: `papers/<slug>.toml` beside
  `digest.toml`, with its own title, sections, keep and email address. `kindle = "Mom's Kindle"`
  sends a paper only to that Kindle. `deckle digest papers` lists them, and `--paper <slug>`
  works on one. Each paper is pruned by its own `keep` and keeps its own readership, puzzles
  and Morgue folder.
- **Large print.** `large_print = true` in a paper's file lays it out for big type: no small
  print, bigger headlines, heavier rules, the forecast one day a line, and one photo a story.
  The Kindle's own text size (Aa) still sets how big.
- **A schedule per paper, with days.** `deckle digest --paper kid-courier schedule --at 07:00
  --days sat` builds that paper on Saturdays only. Every paper gets its own job, and a removed
  paper's job goes the next time the schedule is set.
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
