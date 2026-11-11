# Command line

Everything the app does, from a terminal. `deckle --help` and `deckle <command> --help` list
everything else. The Daily's commands are in [daily.md](daily.md), the Kitchen Book's in
[cookbook.md](cookbook.md) and puzzle books' in [puzzle-books.md](puzzle-books.md).

```sh
cargo build --release -p deckle-cli               # binary at target/release/deckle

# The paper
deckle digest init                                # a digest.toml to start from
deckle digest run --to device                     # build today's paper and send it
deckle digest schedule --at 05:30                 # every morning
deckle digest interests add "Rust" --query '"Rust" programming' --also "Cargo"
deckle digest --paper kid-courier schedule --at 07:00 --days sat
deckle digest papers                              # every paper this computer prints
deckle digest next                                # tomorrow's paper so far: lead, page plan, this week
deckle digest stop                                # stop the presses: skip the next scheduled paper
deckle digest readership                          # how far you read the paper, per section
deckle digest readership apply 1                  # make suggested edit 1 to digest.toml
deckle digest shelf                               # what From Your Shelf would print now
deckle digest morgue search mill                  # every kept edition: list, search, show, send again
deckle digest subscribe                           # sign in to a paper you pay for

# Books
deckle devices                                    # connected Kindles (mass storage and MTP)
deckle devices name "Mom's Kindle"                # name the connected Kindle
deckle devices paper pdf                          # send it the paper as a PDF its pen writes on (Scribe)
deckle run book.epub --kindle "Mom's Kindle"      # to that Kindle; queued until it connects
deckle run book.epub --to device                  # convert, enrich, send
deckle run book.epub --to ./out --cde pdoc        # export to a folder, cover-first
deckle run book.epub --to device --cde ebok --asin B0XXXXXXXX   # real ASIN: store cover on MTP Kindles
deckle run book.epub --to device --cde ebok --llm anthropic     # X-Ray descriptions (ANTHROPIC_API_KEY)
deckle run book.epub --to device --cde ebok --llm openai \
  --llm-endpoint http://localhost:11434/v1 --llm-model llama3   # or a local model
deckle run book.epub --to device --word-wise-level 3            # hints for rarer words only
deckle run book.epub --preset academic            # or novel, manual
deckle run ./notes --to device                    # a flat Markdown folder, one note per chapter
deckle run notes.docx --to device                 # also .odt, .rtf, .mobi/.azw3, .txt, .fb2, .pdf, .cbz/.cbr, a URL, .webloc/.url/.html
deckle run manga.cbz --to remarkable              # a comic as an image PDF for the reMarkable's screen; other books as EPUB
deckle run manga.cbz --to device --direction rtl  # a comic that doesn't say it reads right to left
deckle run book.epub --to device --llm anthropic --recaps --book-summary --series-recap  # AI recaps, off unless asked
deckle send notes.pdf --to device                 # a PDF as it is: on a Scribe, a document you can write on
deckle puzzles crossword --to device --size 13    # a book of crosswords to solve in ink (also search, sudoku, ladder, cryptogram, nonogram, maze)
deckle puzzles crossword --as bundle --to ~/iCloud/Puzzles   # the same book as data, for the iPad's reader
deckle puzzles solved ~/iCloud/Puzzles/deckle-puzzles-….decklepuzzles  # how many the iPad solved
deckle bind a.pdf b.docx https://… --title "Pack"  # several files into one book, then `deckle run` it
deckle inspect book.epub --json                   # metadata, spine, TOC, warnings
deckle convert book.kpf -o book.kfx               # repackage an existing Previewer KPF

# The library and the Kindle
deckle library init ~/Books/Deckle                # keep a copy of everything you send
deckle library search le guin --json              # library, connected Kindles and past sends
deckle library list --series                      # grouped by series
deckle library reprocess --dry-run                # books Deckle could build better now
deckle kindle books                               # what's on the Kindle, and free space
deckle kindle upgrade --dry-run                   # books with an original here (calibre, folders)
deckle kindle put-back "Middlemarch.azw3"         # undo an upgrade: the kept copy goes back
deckle kindle backup                              # DRM-free books and their .sdr, to this computer
deckle kindle restore "…/kindle-backups/Kindle Scribe 2026-10-23T22-30"  # onto any Kindle
deckle highlights export --to ~/Notes/Reading     # one Markdown file per book (folder remembered)
deckle words anki --to ~/Desktop/words.csv        # looked-up words as Anki cards (File → Import)

# KOReader, the reMarkable and calibre
deckle reader-server password                     # the paper and your books over Wi-Fi, in KOReader's catalog and sync
deckle koreader plugins install deckle            # the Deckle plugin: the paper fetches itself
deckle koreader store list                        # reviewed plugins, pinned by hash
deckle koreader backup                            # KOReader's reading state, settings and plugins, to this computer
deckle digest remarkable password                 # SSH to the tablet: a Deckle folder, over Wi-Fi, pruned
deckle wireless status                            # calibre's wireless connection, answered by Deckle
deckle calibre preview                            # reading data back into calibre, through calibredb
deckle shelves add "Space opera" tag:space-opera  # a Kobo shelf or KOReader collection from a calibre rule
```
