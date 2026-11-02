# A welcome, the bookmarklets installed by hand, and no "unavailable offline" line — design

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
confirmed inside Deckle. No story in the paper carries the line "Full story unavailable offline."

## 2. Goals, non-goals, anti-goals

**Goals (the owner's)**

1. An onboarding flow in the app.
2. It gets the bookmarklets installed with as little fuss as the browser allows.
3. No story printed with the line "Full story unavailable offline."

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
- Dropping stories. A story that couldn't be read whole still runs as a brief; only the line
  goes. Subscriber stories keep their reason note as designed.

## 3. No "unavailable offline" line (goal 3)

Today `section_of` in `Yes` mode fetches every story in parallel and partitions by whether the
body carries the `UNAVAILABLE` note: whole stories lead, the fallen-back ones follow as briefs
that end with "Full story unavailable offline." `full_article` falls back when the Google News
interstitial can't be resolved, when the page can't be fetched, or when extraction gives nothing
(and the page isn't a known subscriber wall).

**Change:** the fallen-back stories still run after the whole ones, as briefs (the feed's own
text, the source line), but **without the note**. That is what a `lead` section's briefs have
always looked like, so the paper reads the same way throughout.

- *Rejected (first cut, 2026-11-01):* dropping fallen-back stories altogether, with a one-line
  "No story could be read in full today." for a section where all of them fell back. The
  integration suite, whose fake network serves feeds but no story pages, showed what that does
  when article fetches fail wholesale (the Google News resolver is undocumented and does break):
  every section turns into that one line. A brief is a worse story than a whole one but a
  better paper than an empty section.
- `Lead` and `No` modes are unchanged.
- Subscriber stories that the site withholds keep their reason note (publisher subscriptions
  design §9). That is a different line and the owner didn't ask about it.
- `UNAVAILABLE` stays as the internal marker `full_article` hands back; `section_of` strips it
  from the briefs, and the interests section goes on using it to skip a failing lead.
- No new config key *(builder's call)*.
- README, `docs/daily.md` and the daily-digest spec's §5a say "runs as a brief" where they
  said "stays a headline".

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

### 4.4 The miniature (added 2026-11-02)

The card is two panels: the pages on the right, and on the left tomorrow's front page in
miniature, on a tinted desk, tilted a little and cropped at the card's edge. The miniature is
paper-coloured in both themes (it's paper). Its real parts are the paper's name in its masthead
face (the Daily tab's choice; *Your paper* in the default face until there is a paper), the
day's date, and the press time, which follows the time field on page 2 and the installed job
after. Everything else is grey bars standing for stories, a photo and the weather box. On page 3
a dashed clipping says *The story you save goes here*; when the first save lands it fills with
the story's title and drops onto the page. Page marks (four dots, the current one a short bar)
replace the step titles. One entrance: the card rises and the masthead's ink comes in; a page
change fades. All of it is still under `prefers-reduced-motion`. The miniature is `aria-hidden`;
the marks carry the page names for a screen reader.

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
- Rust tests: the install page carries both bookmarklets; `section_of` in `Yes` mode runs
  fallen-back stories as note-free briefs after the whole ones; `Settings` defaults
  `welcome_seen` false.
