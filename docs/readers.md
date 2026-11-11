# What each reader gets

The README has the short table. This is the whole one, and how the paper reaches each reader.

## How the paper gets there

| Reader | How the paper gets there |
|---|---|
| **Kindle** | Over USB when it's plugged in, or by email over Wi-Fi through your own mail account. A Kindle Scribe can take it as a PDF its pen writes on: **Send the paper as** on its page in Devices, or **Send it as a PDF to write on** on the email card |
| **Kobo** | As a KEPUB, in a Deckle folder, the first time it's plugged in. Long reads and Inbox books too |
| **PocketBook** | The same, as EPUB |
| **reMarkable 2, Paper Pro, Paper Pro Move** | A PDF laid out for its screen, crossword and all, in colour on a Paper Pro. Over its USB web interface, or over SSH and Wi-Fi into a Deckle folder that Deckle keeps tidy. No account. `deckle run --to remarkable` and **Send to reMarkable** send books too: a comic as an image PDF fitted to its screen, anything else as EPUB |
| **Anything running KOReader** | Over Wi-Fi, no cable: KOReader browses Deckle's catalog on your network, or the Deckle plugin fetches the paper by itself when the reader wakes. Progress sync, served by Deckle, tells the paper how far you read |
| **Boox, iPad, anything** | An export folder (iCloud Drive, Dropbox, a USB stick) gets each edition as EPUB or PDF, photos in color if you like |

Only Kindle books need Kindle Previewer. Every setting is in [daily.md](daily.md).

## Reader by reader

| | Kindle (USB) | Kindle (email) | Kindle Scribe | Kobo | PocketBook | reMarkable | KOReader (Wi-Fi) | A folder |
|---|---|---|---|---|---|---|---|---|
| The paper | KFX | EPUB, Amazon sets it, or the Scribe PDF | KFX over MTP, or a PDF to write on | KEPUB | EPUB | PDF, colour on a Paper Pro | EPUB or KEPUB | EPUB, KEPUB, PDF or bundle |
| Crossword and word search | yes, scored from your notes | yes | yes, no score | yes | yes | yes | yes | yes |
| How far you read | yes | – | yes | with KOReader | with KOReader | – | yes, by sync | – |
| Highlights read back | yes | – | – ([#99](https://github.com/CasaVargas/deckle/issues/99)) | yes | with KOReader | – | over USB or a synced folder | KOReader's |
| Books, finished | KFX, cover, pages | – | KFX, cover first | clean KEPUB | clean EPUB | EPUB | EPUB over the calibre connection | KFX |
| X-Ray and Word Wise | yes | – | with a real ASIN | Who's who page | Who's who page | Who's who page | – | – |
| AI recaps, Summary, Previously | yes | – | yes | yes | yes | yes | – | yes |
| Comics, manga, scans | AZW3, fitted per model | – | AZW3, checked on device | fixed-layout EPUB | fixed-layout EPUB | image PDF, colour on a Paper Pro | – | AZW3 |
| Puzzle books | PDF, no ink | – | PDF, write on it | – | – | PDF, sized for it | – | PDF, or a bundle the iPad solves, with the solved count back |
| Its books on the Devices tab: progress, remove, back up | yes | – | yes | yes | yes, progress with KOReader | yes, removing over SSH | – | – |
| calibre connection, reading data back, shelves | with KOReader | – | with KOReader | yes | with KOReader | – | yes | – |
| The Kitchen Book | yes | – | yes | – | – | – | – | EPUB |

Every cell is a feature of the free app; Kindle books and the Kindle's paper need Kindle Previewer,
which runs on Mac and Windows. A Kobo, PocketBook or reMarkable needs no Previewer. The Linux
container prints the paper for email and export folders and serves KOReader; Kindle books aren't
converted there.

What has been checked on real hardware is in [devices.md](devices.md).

## The Devices tab

- **A shelf you can read.** Every book on the Kindle with its cover, progress and when you last
  opened it, filtered and searchable. Unplugged, it shows what Deckle sent there.
- **Upgrade what's already there.** Books calibre or anything else put on the Kindle are matched to
  their originals on your computer (your calibre library, read-only, or folders you name) and
  rebuilt with Enhanced Typesetting, page numbers, X-Ray and Word Wise. With no original, a DRM-free
  AZW3 is rebuilt from the Kindle's own copy and checked before it replaces anything. The Kindle's
  copy is kept with its highlights and reading data, so **Put back** undoes it. Store books and
  anything with DRM are left alone.
- **Make room, back up, restore.** Space split by why: finished books, second copies, old papers.
  Removing a book Amazon delivered (bought, or sent by email) takes only the download, and Deckle
  says so: Amazon keeps it, and the Kindle can still list it under All. Copy every DRM-free book
  with its highlights and reading position to your computer, and put a backup on any Kindle later.
- **Kobo, PocketBook and reMarkable.** Each gets a page with every book, how far you've read it
  and when, the space left, **Remove** for the ones you tick, and **Back up** to your computer
  (book files with KOReader's notes, a Kobo's database, a reMarkable's documents as PDFs with
  handwriting, or over SSH as the tablet keeps them). A Kobo's database is copied aside before
  anything is removed, and books bought from Kobo are listed but never deleted. The reMarkable's
  web interface can't delete, so removing there needs SSH.
- **Edit a book's details where it is.** Title, authors and series. On a Kindle, **Edit
  details…** rebuilds the book from your library copy, its original or the Kindle's own DRM-free
  copy and swaps it in, with **Put back** to undo; the new copy starts without its reading
  position or highlights (saved first, if you keep a highlights folder). On a Kobo or PocketBook
  it rewrites them inside the EPUB, and in the Kobo's library too. On a reMarkable over SSH,
  **Rename…** sets a document's name.
- **Highlights out.** Highlights and notes from `My Clippings.txt`, one Markdown file per book in a
  folder you choose (a notes vault, iCloud Drive). It covers sideloaded books, which Amazon's own
  export leaves out, and later exports only add what's new. The 2024 Scribe doesn't keep that file
  ([#99](https://github.com/CasaVargas/deckle/issues/99)). If you read in KOReader, on any reader
  Deckle can see over USB or in a folder your reader syncs back, its highlights and notes come
  too, and on a Kobo or PocketBook its books in progress reach From Your Shelf. The paper opens
  in KOReader as designed: Deckle sends it with settings for its own fonts, styles and
  hyphenation, once, and never over KOReader's own.
- **Words to Anki.** The words you looked up, as flashcards with a short definition, the sentence
  you met it in and the book.
- **Several Kindles in one home.** Name each Kindle, then send a book to *Mom's Kindle* even when
  it's in her bag: it waits in the outbox until that Kindle connects, and no other Kindle takes it.
