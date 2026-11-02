# A welcome, the bookmarklets installed by hand, and whole stories only — design

**Status:** built from the owner's one-line brief, calls marked · **Date:** 2026-11-01 · **Owner:**
Jonathan (CasaVargas LLC) · **Builds on:** `2026-10-20-save-to-paper-design.md` (the bookmarklet
and its listener), `2026-10-20-daily-digest-design.md` §5a (full text and the fallback),
`2026-10-26-cookbook-design.md` §8.3 (Save recipe), `2026-10-26-daily-tab-redesign-design.md`
(rooms, sheets).

The brief: *"I want an onboarding setup for this, to help with the bookmarklets. I also don't want
headlines that say unavailable offline."* Calls marked *(builder's call)* are mine, not the
owner's; review them.

## 1. Intent and why

Two things stand between a new Deckle and a paper that reads well:

- **Installing the bookmarklets is a chore.** Today the app says *copy it, then add a bookmark in
  your browser and paste it as the address*. That is four steps in two apps, the pasted address
  is a wall of JavaScript, and nothing tells you whether it worked. The command line says *drag
  these to your bookmarks bar*, which is the right way, but a terminal can't be dragged from.
- **Headlines that say "Full story unavailable offline."** A paper is for reading whole. A
  headline with the feed's two sentences and that line is a reminder of what you didn't get; the
  owner doesn't want them in the paper at all.

**Success:** a first launch walks you to a paper that prints on schedule and a browser with
**Save to paper** and **Save recipe** on its bookmarks bar, and the first click on one is
confirmed inside Deckle. A paper has no story that stops at its headline for want of the text.

## 2. Goals, non-goals, anti-goals

**Goals (the owner's)**

1. An onboarding flow in the app.
2. It gets the bookmarklets installed with as little fuss as the browser allows.
3. No story printed with "Full story unavailable offline."

**Goals (mine, *builder's call*)**

4. The flow shows once and can be shown again from Settings.
5. Installing a bookmarklet is a drag from a page Deckle opens, not a paste.
6. The first save is confirmed where you made the bookmarklet: the app watches for it.
7. The same component serves the Daily tab's Reading room and the Cookbook's Add page, so the
   three places you meet a bookmarklet look and work the same.
8. Everything works in the web app (the container) too, with the browser doing what the desktop
   needs a page for.

**Non-goals**

- Setting the town, feeds, sections or delivery inside the welcome. The Daily tab does each of
  those well already; the welcome points at them.
- A tour of the Books or Kindle tabs. One paragraph on the last page.
- A browser extension, or the bookmarklet carrying the page's text. Out of scope and out of
  character (the listener is on purpose a small thing).

**Anti-goals**

- A welcome that nags. It shows once. Skip is always one click away. It never shows to a build
  that already had its settings file say so.
- Dropping a story that *could* be read. Only the stories the pipeline already marks as fallen
  back go; subscriber stories with their reason stay as designed.

## 3. Whole stories only (goal 3)

Today `section_of` in `Yes` mode fetches every story in parallel and partitions by whether the
body carries the `UNAVAILABLE` note: whole stories lead, the fallen-back ones follow as briefs.
`full_article` falls back when the Google News interstitial can't be resolved, when the page
can't be fetched, or when extraction gives nothing (and the page isn't a known subscriber wall).

**Change:** in `Yes` mode the fallen-back stories are **dropped**. They are not printed and not
recorded as printed, so a story that failed for a passing reason (a timeout, an interstitial
that didn't resolve) gets another try tomorrow.

- A section where *every* story fell back prints its note line, as an empty section does today:
  `No story could be read in full today.` *(builder's call: a note over a silent empty section,
  so the reader knows the feed was reached.)*
- `Lead` mode is unchanged: one story whole, the rest briefs by design, none with the note.
- `No` mode is unchanged: the feed's text, never fetched, never noted.
- Subscriber stories that the site withholds keep their reason note (publisher subscriptions
  design §9). That is a different line and the owner didn't ask about it.
- No new config key *(builder's call)*. The owner asked for the line to be gone; a knob to bring
  it back can be added if someone wants briefs.
- README, `docs/daily.md` and the daily-digest spec's §5a say "a story that can't be read in full
  is left out" where they said "stays a headline".

## 4. The welcome

A full-window overlay on the app's first launch: a centered card, four pages, a step indicator,
**Skip** at every page, **Next** and **Back**. It shows when the settings file's `welcome_seen` is
false, and sets it true on Finish or Skip. **Settings → About** gets *Show the welcome again*.

**Where the flag lives:** `welcome_seen: bool` in `settings.json` (`Settings` in
`deckle-app`), default false. An existing install sees the welcome once after the update
*(builder's call: it's the page that tells them about the bookmarklets, which is the point; it's
one Skip to dismiss)*.

### 4.1 The pages

1. **Welcome.** "A morning paper for your e-reader, set on your own computer. And the books you
   own, finished like the store's." Three lines on what's coming: your paper, the bookmarklet,
   where things are.
2. **Your paper.** If `digest.toml` doesn't exist: *Set up your paper* (the Daily tab's
   `daily_setup`). Then the morning time (`<input type="time">`, default 05:30) and *Turn on the
   morning schedule*, which calls `schedule_set` for the main paper: the edition job and the
   watcher. Once on, the page says so: *Every morning at 5:30. The watcher that delivers it,
   and takes what you save, runs from login.* On the web the container is the schedule, so the
   page says that and has no button.
3. **Save to paper.** What the two bookmarklets do, then the install (§4.2), then a live check
   (§4.3). If the schedule is off on the desktop, a line says the bookmarklets work while it's
   on, with *Back* to page 2.
4. **Where things are.** The Daily tab for the town, feeds, sections and delivery (USB, email,
   Kobo, reMarkable, a folder); the Books tab for EPUBs and documents; the Cookbook. *Finish*
   lands on the Daily tab.

### 4.2 Installing the bookmarklets

**Desktop:** a button, *Open the install page*, writes `bookmarklets.html` next to
`bookmarklet.token` (the config folder) and opens it in the default browser. The page carries
both bookmarklets as real links styled as buttons, says *Drag each to your bookmarks bar*, and
shows how to reveal the bar in Safari, Chrome and Edge, and Firefox, with the line for the
browser it's open in picked out by its user agent. Each link also has *Copy* for a browser that
won't take a drag (Safari on iPad, say) and a line that the bookmarklets work while Deckle's
morning schedule is on. The page is generated in `deckle-digest::bookmarklet` so the command line
could print it too, and it borrows the popup's look (Georgia, warm paper).

**Web:** the app is already in a browser, so the links render inline, draggable, with the same
drag line and *Copy*. No file, no page. (React replaces a `javascript:` `href` prop with one
that throws, so the component sets the attribute on the mounted element instead.)

**Why a file, not the listener** *(builder's call)*: serving the page from the listener would
need the watcher running at install time, and a connection-refused page is the worst first
impression. A file opens whether or not the schedule is on; the welcome's page 2 is where the
schedule gets turned on, and page 3 says so if it isn't.

**The Reading room and the Cookbook's Add page** use the same component (`BookmarkletInstall`),
one bookmarklet or both, in place of today's *Copy bookmarklet* row.

### 4.3 The first save, confirmed

While page 3 is open the app polls `reading_list` every 3 s. A bookmarklet entry saved after the
page opened turns the check line from *Try it: open any article in your browser and click Save
to paper* into *"<title>" is in tomorrow's paper.* Save recipe lands in the cookbook, not the
reading queue, so the check watches the paper one only *(builder's call: one check, the one
whose result the page can read; a cookbook save shows on the Cookbook tab)*.

## 5. Constraints

- Nothing new goes online. The install page is a local file; the links point at the listener as
  before.
- The token stays where it is and is only ever written into the two bookmarklets and the page
  that holds them. The page lives in the config folder, as the token does.
- No new dependency. The page is a `format!` in Rust, the overlay is React with the app's own
  CSS variables.
- Every command added to `api.ts` is in the dispatch table (its test enforces it); the desktop-
  only one answers the web with the error the other desktop-only commands use.

## 6. Left to the builder

- Exact copy on the four pages, within the voice of the rest of the app.
- The overlay's styling (`.welcome*`), following `.sheet` and `.setup`.
- Pure logic in `onboarding.ts` with tests: the step list, the first-save check, the schedule
  line.
- Rust tests: the install page carries both bookmarklets; `section_of` in `Yes` mode drops
  fallen-back stories and notes an all-fallen section; `Settings` defaults `welcome_seen` false.
