# Save to tomorrow's paper — design

**Status:** approved by owner ("merge it and build it", 2026-10-20); built in #61's PR · **Date:** 2026-10-20 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-20-daily-digest-design.md` (the Deckle Daily, §5a full text) ·
**Companion:** #60 (highlights back)

Save an article from anywhere during the day; it is in tomorrow morning's Deckle Daily,
whole, with its photos, in a section called **Your Reading**. Long pieces become their own
book. No account, no service: the queue is a plain file on your disk.

## 1. Intent and why

Read-later tools are how people move reading from the phone and browser to a calm screen,
and the field just got emptier: Pocket shut down in 2025, and Instapaper's Kindle digest is
a subscription. Deckle already builds a daily paper with full-text extraction and delivers
it when the Kindle connects. The missing piece is a way to put *your own* reading into it.

Success: you save three things on Tuesday, and on Wednesday morning they are in the paper,
readable end to end on the Kindle, and you never opened a browser tab to read them.

## 2. Goals, non-goals, anti-goals

**Goals (v1)**

1. Three ways to save, all local:
   - `deckle save <url>…` on the command line.
   - **The Inbox folder:** drop a `.webloc`, `.url`, `.txt` (one URL per line) or a saved
     `.html` page into it. The folder is configurable, so you can point it at iCloud Drive
     and save from an iPhone with a Shortcut ("Save URL to file").
   - **A bookmarklet:** it talks to `deckle digest watch` over `127.0.0.1` (§4).
2. Saved articles appear in the next edition as the **Your Reading** section. They get the
   same extraction, photos, fallbacks and styling as the news sections.
3. Long reads (over 4,000 words) become a separate book in the outbox, named after the
   article and sent with the paper. The paper carries a one-line pointer to it with a
   reading time.
4. A saved `.html` page is extracted from the file and never fetched. This is how you get
   an article from a site you're logged into (your own subscription) onto the Kindle.
5. The queue is a plain, human-readable file; `deckle save --list` and `--remove` manage it.

**Non-goals (v1)**

- Email-in (IMAP) capture. It's the obvious v2 channel; the queue format must not preclude
  it.
- A browser extension, mobile app or share-sheet extension (the Inbox folder plus a
  Shortcut covers iOS; a native share extension can come with the desktop app later).
- Tags, folders, archive search: this is a reading queue, not a library.
- PDFs and DOCX (#18, #17).

**Anti-goals**

- Any server, account or sync service of ours.
- Losing a saved item silently. Every saved URL either prints, or stays visibly queued with
  the reason.
- Fetching around a paywall. A saved URL is fetched like any reader would; a saved page you
  could already read is how logged-in content gets in.

## 3. The queue

`<data dir>/deckle/reading.jsonl`, one JSON object per line, appended to. A file per user,
never written by two processes at once (an exclusive lock file beside it).

```json
{"id":"r-20261020-1f3a","url":"https://…","file":null,"title":null,"saved_at":"2026-10-20T14:03:11Z","source":"cli","state":"pending","tries":0,"printed_in":null,"note":null}
```

- `file`: the path of a saved `.html` page, copied into `<data dir>/deckle/reading/<id>/` at
  save time, with its photos. A browser saves those in one of three ways, and all are
  used:
  - the sibling `<name>_files/` folder, copied along;
  - `data:` images embedded in the page;
  - remote image URLs, fetched normally. If they refuse without cookies, the article runs
    without those photos.

  When `file` is set, `url` is the page's canonical URL, if it declares one.
- `source`: `cli`, `inbox` or `bookmarklet`.
- `state`:
  - `pending` → `printed` (in an edition);
  - `pending` → `book` (became its own book);
  - `pending` → `failed` (after 3 tries).
- `tries` counts failed extractions.
- `printed_in` is the edition date.
- `note` holds the last failure reason.
- **Duplicates:** saving a URL that is already `pending` does nothing. Matching uses the URL
  without its fragment and without `utm_*` parameters.
- **Rewriting the file:** state changes rewrite it atomically (write a temp file, then
  rename). Lines that aren't valid JSON are kept as they are and reported once.

## 4. Capture

- **CLI:** `deckle save <url>… [--title T]` appends to the queue. Its other forms:
  - `deckle save --list` shows the queue: pending first, then the last 7 days.
  - `deckle save --remove <id|url>` removes an item.
  - `deckle save <file.html>` saves a page from a file.
- **Inbox folder.** `reading.inbox` in `digest.toml`, default `~/Deckle/Inbox`.
  - Scanned at the start of every `digest build`/`run`, and every tick of `digest watch`.
  - Each file found is turned into queue entries, then moved to `Inbox/Saved/` (or to
    `Inbox/Unreadable/`, with a `.txt` saying why).
  - Files still being written (modified less than 5 s ago) wait for the next scan.
- **Bookmarklet.** `deckle digest watch` also listens on `127.0.0.1:47219`, localhost only.
  - `GET /save?url=…&title=…` queues the URL and answers with a tiny HTML page: "Saved to
    tomorrow's paper" and a close button.
  - The bookmarklet opens that page in a small popup window, so the browser's
    mixed-content and CORS rules don't apply.
  - `deckle save --bookmarklet` prints the bookmarklet code to paste.
  - A random token generated at first run is part of the URL, so other local web pages
    can't add to your queue blindly.
  - With no `watch` running, the bookmarklet fails visibly ("Deckle isn't running") because
    the popup can't load.

## 5. The edition

- `reading` becomes a section kind in `sections`. The starter config puts it first, right
  after the front page: your own reading leads, the news follows.
  - Existing configs have no `reading` entry. When there are pending saves and `reading`
    isn't listed, the section is inserted first anyway, because saving is an explicit ask.
    `digest build`/`run` print one line saying so, and `deckle save` warns once.
  - So saves never wait forever, and nobody's config file is rewritten.
- At build time:
  1. Take every `pending` entry, oldest first, up to `reading.max_items` (default 10).
  2. Extract each one with the §5a pipeline: fetch and extract, or extract from the saved
     file.
  3. Fewer than 120 words counts as a failure:
     - `tries` goes up and the item stays pending for tomorrow;
     - on the 3rd failure it prints as a headline with the reason and becomes `failed`.
- Over `reading.long_read_words` (default 4,000), the article becomes its own book: an EPUB
  with a generated typographic cover ("Deckle Reads"), the article's title and byline, and
  the same XHTML and photos. The paper gets a pointer article: "In your library: <title>,
  about 18 min."
  - **No length cap.** The §5a 6,000-word cap applies only to stories printed inside the
    paper. Extraction takes a `max_words` argument, and long reads pass none.
  - **Where long reads go.** Wherever the paper goes. With `run --to device` they're
    converted and queued in the outbox; with `run --to <folder>` they're delivered there;
    with `build` they're written as EPUBs next to the paper.
  - **How they're converted.** They go through the normal book pipeline with the cover rule,
    like any book Deckle sends. On a Kindle that allows it they ship as EBOK with Word
    Wise; unlike the paper, they are not forced to PDOC.
  - **File name.** `deckle-reads-YYYY-MM-DD-<slug>.kfx`. The `deckle-reads-` prefix keeps
    them out of the paper's replace-and-prune. Long reads are never pruned automatically:
    they are books you chose to keep.
- Each article's meta line reads: "Saved Tue 2:03 PM · <outlet> · about 7 min".
- **Who uses up the queue.** Only `digest run` commits queue state. It does so after the
  edition (and any long reads) converted successfully and went to the target or the outbox.
  - `digest build` is a preview: it includes pending items but changes nothing.
  - Rebuilding the same date is safe. A `run` for date D takes the `pending` items *plus* the
    ones whose `printed_in` (or book date) is D. The replacement edition, which supersedes
    the queued one in the outbox under the same name, therefore carries everything the
    first one did.
  - "Printed" means "in the edition that `run` shipped or queued", not necessarily read on
    the Kindle.
- The front page and cover show "Your Reading: 3 saved" in the index.
- **Cover lead:** the news keeps the lead, so the cover stays today's front page. Saved
  articles compete for grid spots after the news lead, labelled YOUR READING.

## 6. Config (`digest.toml`)

```toml
sections = ["reading", "weather", "local", "feeds", "lottery", "horoscope", "onthisday"]

[reading]
inbox = "~/Deckle/Inbox"         # point at iCloud Drive to save from an iPhone
max_items = 10                   # per edition; the rest wait
long_read_words = 4000           # longer pieces become their own book
```

## 7. Code layout

- **`deckle-digest`**
  - `reading.rs`: queue file, lock, dedupe, inbox scan, state transitions.
  - `sources/reading.rs`: the section.
  - A small `longread.rs` that reuses `render`, `cover` and `epub` for a one-article book.
- **`deckle-cli`**: `save` (with `--list`, `--remove`, `--bookmarklet`), and the inbox scan
  inside `digest build|run|watch`.
- **The bookmarklet listener:** a minimal HTTP/1.1 handler on `std::net::TcpListener` inside
  `watch`. No server framework.

## 8. What's left to the builder

- **Long-read cover:** its exact typography, within the Deckle Daily visual language.
- **Inbox parsing:** how `.webloc` (plist XML) and `.url` (INI) files are read.
- **The token:** where it's stored (beside the queue) and its length.

## 9. Calls made in this draft (owner to check)

- **Capture channels:** CLI, Inbox folder and bookmarklet for v1. Email-in is v2.
- **iPhone:** saving from the phone goes through an iCloud Drive Inbox folder and a
  Shortcut. No iOS app.
- **Placement:** Your Reading leads the paper.
- **Long reads:**
  - over 4,000 words they become separate books;
  - no length cap;
  - converted with the normal cover rule (EBOK with Word Wise where allowed);
  - named `deckle-reads-`, never pruned.
- **When an item counts as printed:** only `digest run` uses up the queue, and a same-day
  rebuild reprints the same items. `build` is a preview.
- **Failures:** an item gets 3 tries over 3 days, then prints as a headline.
- **Saved pages:** their photos come from the `_files` folder, embedded `data:` images, or
  plain fetches.
- **Missing `reading` section:** when saves are pending it's inserted first, without
  rewriting your config.
- **Cover lead:** news keeps the lead; saved articles get grid spots.
