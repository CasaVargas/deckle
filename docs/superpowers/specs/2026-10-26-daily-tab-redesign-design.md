# Daily tab redesign: one list of sections, everything else where you'd look

Mockups: the "Deckle Daily Redesign" canvas, Direction A (Today, Sections, the paper's own
settings, Delivery). Same look as the Books, Kindle and Settings redesigns. No setting is lost;
most move, and every change saves as it's made.

## 1. Why

- **The paper** room is one form of about twenty groups (name, Kindle, font, days, place, local
  search, other places, topics, interests, learning, feeds, lottery, stories, keep, AI editor,
  horoscope), then Your Day and Subscriptions as cards that save differently.
- A section is set up in two places: the page plan on Today turns it on, orders it and sets its
  stories; The paper holds where it comes from. Ticking a topic happens in one room and placing
  it in another.
- Things that belong together are apart: the press time is on Today and its days in The paper;
  "keep the last 3 editions" sits under Stories while Delivery shows the same number.
- Saving is mixed: The paper needs Save, its cards save at once, and the page plan locks until
  The paper is saved.
- Your papers are a strip of cards above every room, with Edit and Editing buttons.

## 2. The frame

- Tabs under the app bar, as on the Kindle tab: **Today · Sections · Reading · Editions ·
  Delivery**. The rail of rooms goes.
- Left of the tabs, the **paper switcher**: the selected paper's name and who it's for, as a
  menu button. The menu lists every paper (name, reader, its Kindle), then **Paper settings…**
  and **New paper…**. Picking a paper switches every tab to it. New paper asks for a name and
  who it's for, makes it, switches to it and opens its Sections.
- Papers that aren't the main one say, on Reading and Editions, that those follow the main paper
  (as today).
- The switcher's notes (a paper with no Kindle chosen while two are named, and the papers view's
  warnings) show under the tabs while they apply.

## 3. Today

Two columns; one under the other when narrow.

- **Tomorrow's paper, so far**: the front page as now, "headlines checked hh:mm", and a line
  pointing to Sections for what's in it.
- **The press card**: a dot and the press line ("Goes to press at 5:30 · in 11 h 4 m", "Presses
  stopped: no paper Tuesday…", "No press time set"), the on switch, the time, and the **days** as
  seven toggles (none on means every day). Days move here from The paper and save at once; an
  installed job is reinstalled with them, as Save does now. Under it, where the paper goes, in
  one line from Delivery ("To your Kindle over USB and by email"), linking to Delivery.
  Buttons: **Send now**, **Preview**, and **Skip the next paper** (today's Stop the presses);
  **Restart the presses** while stopped. The run's result and the last-built line stay under
  the buttons.
- **Save something for tomorrow**: the link box from Reading (main paper only; other papers say
  saved links go to the main paper), and "*n* saved", linking to Reading.
- **This week**: the six days with how far each was read, and All editions. When Readership
  knows where you usually stop, one line: "You usually stop in Sports", linking to Editions.

The page plan leaves Today: it becomes Sections.

## 4. Sections

"What's in the paper": one list, in reading order, with "about *n* Kindle pages", and
**+ Add a section**.

- One row per page-plan row (today's `plan`), then a "Not in the paper" divider and the rows that
  are off (`off`). A row: drag handle, name, a one-line detail, its stories (a menu where it has a
  budget, else "1 page"), an on switch, and a chevron that opens it. Moving (drag, or the
  keyboard: Alt+↑/↓ on a focused row) and the stories menu keep today's rules (`reorder`,
  `setBudget`, `step`): grouped kinds stay one block, other towns follow Local.
- A row opens beneath itself, one at a time, with what it comes from:
  - **Your Reading**: up to *n*, the long-read length, the inbox folder (Change…), the
    bookmarklet (Copy), and a link to Reading.
  - **Weather**: your town (Find, the place search), units, and **Your Day** (today's card:
    calendars, mention rain).
  - **Local**: your town (the same setting as Weather, said so), stories, and "Search Google News
    for something else" (`local.google_news`), shut while it matches the town.
  - **Another town**: its name, its search, stories, Remove.
  - **Something you follow** (an interest): its name, its own search, Also, Leave out,
    stories, Remove. The learning switches stay on the paper's own settings (§6).
  - **A feed**: its name, its address, stories, Remove. A topic feed: the topic, stories, Remove.
  - **A paper you subscribe to**: its sections and stories, from today's Subscriptions card; the
    sign-in (site, username, password in the keychain) for that site.
  - **From Your Shelf**: its status line, and Settings → Highlights & reading when the Mac isn't
    reading the Kindle for it.
  - **Lottery**: the games. **Horoscope**: the signs (none means all twelve). **Puzzles**,
    **On this day**: nothing to set beyond on and off.
- Kinds with several rows (feeds, interests, subscribed papers) switch per row where the config
  allows it, else the switch says it turns the whole group off, as today's tooltip does.
- Every change saves at once: switches, menus and moves straight away, text after a pause in
  typing (about half a second) and on leaving the field. Saves run one at a time against the
  latest saved file, so the page plan no longer locks; a failed save shows its reason on the row
  and keeps the typed text. There's no Save button anywhere on the tab.

## 5. Add a section

The menu, each with a line on what it is:

- **Something you follow**: a team, a person, a topic. Asks for its name; becomes an interest.
- **A news topic**: the topics not yet in the paper (today's ticked Topics).
- **A website or feed**: a name and an address.
- **Another town**: a name; its search starts as the name (as PR #194 makes the backend do).
- **A paper you subscribe to**: opens the subscription setup in its new row.
- **The back pages**: Weather, From Your Shelf, Puzzles, Lottery, Horoscope, On this day, as
  switches for the ones that are off.

The new row lands where the backend puts that kind and opens, focused on its first field.

## 6. The paper's own settings

A sheet from the right, opened by **Paper settings…** in the switcher; Esc or × closes it.

- **Name**, **For**, **Goes to** (Any Kindle or a named one; "Name a Kindle on the Kindle tab to
  choose it" while none are named), **Units**.
- **Masthead**: the ten faces of §7 as cards, each showing this paper's own name in that face,
  in two groups (Blackletter, No blackletter), then "Use a font file…" for a .ttf, which shows as
  an eleventh card while chosen. A face missing a letter of the name says so on its card.
- **Reading it**: Whole stories (with their photos under it), Large print with its line.
- **AI editor**: its switch, what it sends, and its provider fields while on, as today.
- **Learning what you like**: the two learning switches.
- Saved as it changes. The file's path and Show the file at the foot.

## 7. Mastheads

Ten bundled faces, all SIL Open Font License, each with its OFL text beside it in
`crates/deckle-digest/fonts`:

| `masthead` | Face | Group |
| --- | --- | --- |
| `consent` (default) | Manufacturing Consent | Blackletter |
| `grenze` | Grenze Gotisch (SemiBold) | Blackletter |
| `pirata` | Pirata One | Blackletter |
| `texturina` | Texturina (Bold) | Blackletter |
| `unifraktur` (also `blackletter`) | UnifrakturMaguntia | Blackletter |
| `playfair` | Playfair Display (Bold) | No blackletter |
| `bodoni` | Bodoni Moda (Bold) | No blackletter |
| `caslon` | Libre Caslon Display | No blackletter |
| `serif` | Old Standard TT (Bold) | No blackletter |
| `sans` | Inter (Bold) | No blackletter |

- Variable fonts are cut to the static weight above (fontTools' instancer), since the cover and
  the PDF edition embed TrueType outlines as they are.
- **The default changes** from UnifrakturMaguntia to Manufacturing Consent: a paper with no
  `masthead` gets the new face. `blackletter` keeps meaning UnifrakturMaguntia, so a file that
  names it keeps its look.
- A face missing a letter of the title falls back to the default, and when the default lacks it
  too, to Old Standard, with the reason in the run log (today's fallback, one step longer).
- The app shows the cards with the same files, loaded as web fonts.

## 8. Delivery

"Where it goes", in two groups:

- **Kindle**: **Over USB** (when it's plugged in; **keeps the last *n*** as a menu here, moved
  from Stories) and **By email** (today's Email to Kindle settings, opened from its row; Send a
  test and the Amazon notes inside).
- **Other readers**: reMarkable, each export folder as its own row, Kobo and PocketBook
  (automatic), and **+ Add a folder**.
- Each row: name, how it gets there, its state, and a switch where it can be off.

## 9. Reading and Editions

What they hold doesn't change: Reading is the saved list, the inbox and the bookmarklet (the
inbox and bookmarklet also appear in Your Reading's row); Editions is How far you read and Every
edition. They take the cards and spacing of the other tabs.

## 10. Pull requests

1. **Frame and Today** (with this spec): the tabs, the paper switcher, Today with the days and
   the save box. The Paper room's form shows on Sections, as it is, until part 2; its days go.
2. **Sections**: the list with its rows opening to their sources, saving as they change. What's
   left of the form (name, Kindle, font, units, stories, AI editor, learning) shows as a card
   under the list until part 4.
3. **Add a section**: the menu and the new rows.
4. **The paper's own settings and the mastheads**: the sheet, the ten faces in the digest crate
   and the app, the new default.
5. **Delivery**: the two groups, keep moved to Over USB, the email settings in their row.

## 11. Verification

- Unit tests for the pure parts: the press card's lines, the days' saving, each row's detail
  line, the add-menu's choices, the save queue, masthead names and the fallback chain.
- `cargo test` for the masthead parsing, the default, and that each face renders "The Deckle
  Daily" on a cover.
- Each PR checked in the running app with a scratch `HOME`, every setting confirmed reachable
  after each move, and a paper previewed after each part.
