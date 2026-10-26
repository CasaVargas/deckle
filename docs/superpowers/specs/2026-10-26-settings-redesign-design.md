# Settings redesign: organised by what you're trying to do

Mockups: the "Deckle Settings Redesign" canvas (Overview, Sending, Reading aids, Online &
privacy). Same look as the Books and Kindle tab redesigns. No setting is added or removed;
they move, and a few get explained.

## 1. Why

- **General** holds the Kindle Previewer path, the library, where originals are found, a
  watched folder, and "Default mode: Automatic / Cover first / Features first", which never
  says what you gain or lose.
- **X-Ray** and **Word Wise** are two small pages for one decision: help while reading.
- **Kindles** mixes a highlights folder, reading of the paper, From Your Shelf and per-Kindle
  options.
- What goes online is spread over four pages, and only About lists it, as a paragraph.
- Nothing says whether setup is complete; everything is a row of switches and native menus.

## 2. Pages

| Page | Holds | Was |
| --- | --- | --- |
| Overview | setup checklist, what goes online, a card per page | new |
| Sending | how books show up (the mode), Kindle Previewer | General |
| Library | library folder, add every book, originals (calibre, folders), watch a folder | General |
| Books | page numbers, covers, typography, pop-up footnotes; Open Library and store-cover switches stay here | Books |
| Reading aids | Word Wise, X-Ray, its AI model, Wikipedia | Word Wise, X-Ray |
| Kindles | one card per connected Kindle: mode, real-ASIN X-Ray, known issues | Kindles |
| Highlights & reading | highlights folder and on-connect writing, reading of the paper, From Your Shelf | Kindles |
| Online & privacy | every network feature with its switch, what it sends and to whom; what always goes online | About (prose) |
| About | version, updates, credits | About |

The gear opens the page last used, Overview the first time. "Kindle Previewer not found"
opens Sending.

## 3. Frame

- The sidebar: Back to Deckle, **Search settings**, then the pages. A page with something to
  do carries a short note ("1 to do"; Online & privacy: "3 on").
- **Search** matches a fixed index of every setting (its label, page and a few words people
  might type: "ASIN", "dictionary", "calibre", "API key"). While there's text, the content
  shows the matches, each opening its page. Esc clears it.
- Pages use cards (a heading, a line on what it's for, then its rows) instead of one long
  list.

## 4. Overview

- **Setup**, one row each, with a mark (✓ or !) and, when something's missing, the button
  that fixes it:
  - Kindle Previewer: found, or not found → Choose…;
  - Library: *n* books in *folder*, or not set up → Choose folder…;
  - Originals: where Deckle looks (calibre, *n* folders), or nowhere → Library;
  - Highlights: the folder, or none chosen → Choose folder…;
  - Kindles: the connected ones, or none connected (no button).
  The heading says how many need doing: "Everything Deckle needs is set up" or "1 thing
  needs doing".
- **Going online**: "*n* features go online: …" from the switches that are on, linking to
  Online & privacy; "Nothing optional goes online" when none are.
- **A card per page** with its current state in one line.

## 5. Sending

- **How books show up**: three cards, a radio each, with a small cover and what you get:
  - **Automatic** (recommended): Deckle chooses for each Kindle;
  - **Kindle book** (EBOK): X-Ray, Word Wise and real page numbers; some Kindles show a plain
    cover;
  - **Document** (PDOC): its own cover always shows; no X-Ray or Word Wise.
  Under them, which connected Kindles set their own mode, linking to Kindles.
- **Kindle Previewer**: found (with its version) or not, what it's for, its location and
  Choose….

## 6. Reading aids

- **Word Wise**: switch, then "How many hints" (the 1–5 slider, labelled at both ends, with
  "At every hint, the Kindle's own slider decides"), beside a small Kindle page showing hints
  above two hard words. The example is Deckle's own sentence.
- **X-Ray**: switch; "Who finds the people and places" as two cards: Deckle, on this Mac
  (nothing goes online) or An AI model (Online: sends text from the book to the provider).
  Provider, model, endpoint and key show only for the AI. **Wikipedia summaries** (Online)
  below.
- Both say they need a book sent as a Kindle book.

## 7. Online & privacy

- **You choose**: Fill missing details (Open Library), Match the store cover (Open Library),
  Wikipedia summaries, AI model for X-Ray. Each row: its switch (the same setting as on its
  own page), what it sends and to whom, and a link to where it's set.
- **Always**: the update check (nothing about you or your books, to GitHub) and the Word Wise
  dictionary (downloaded once).
- A line on what a Kindle itself may ask Amazon, and that the Daily's news comes from the
  sites chosen on the Daily tab.
- About keeps the version, updates and credits; its online paragraph moves here.

## 8. Pull requests

1. **Frame and Overview**: the sidebar with search and notes, cards, the Overview page; the
   other pages as they are, still reachable and searchable.
2. **Sending**: the new page; the mode and Previewer leave General.
3. **Reading aids**: Word Wise and X-Ray become one page.
4. **Online & privacy**: the new page; About loses its paragraph.
5. **The rest regrouped**: General becomes Library; Kindles splits into Kindles and Highlights &
   reading; the search index and Overview follow.

## 9. Verification

- Unit tests for the pure parts: the setup checklist, the online summary, the page notes,
  and the search over the index.
- Each PR checked in the running app (scratch `HOME`, as for the Kindle tab), and every
  setting confirmed reachable after each move.
