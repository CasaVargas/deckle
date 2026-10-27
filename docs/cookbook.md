# The Kitchen Book (`deckle cookbook`)

Recipes you keep become one book, rebuilt when it changes and sent to every Kindle on its next
plug-in. Design: `docs/superpowers/specs/2026-10-26-cookbook-design.md`. The folder is
`cookbook/` beside the other Deckle data (`DECKLE_COOKBOOK_DIR` moves it).

## Commands

Add `--json` (before or after the subcommand) for machine-readable output.

| Command | What it does |
|---|---|
| `deckle cookbook add <url\|file>... [--chapter <id>] [--note <text>]` | Keep recipes from web addresses, `.webloc`/`.url` link files or saved `.html` pages and file each into a chapter. Prints the chapter and the picture. |
| `deckle cookbook list [--chapter <id>]` | The recipes: id, title, chapter, added, recipe data or page text, picture. Below them, a footer (see Delivery): `Changes not built yet: <why>` while the book has changes no build has picked up, then one `<Kindle>: <status>` line per known Kindle; nothing when there is nothing to say. `--json` gives `{recipes, kindles, pending_build, build_error}` (plus `kindles_error` when the delivery record can't be read), with `kindles` as `[{name, id, status, state, behind}]`. |
| `deckle cookbook move <id\|url> <chapter>` | File a recipe into another chapter. |
| `deckle cookbook remove <id\|url>` | Delete a recipe and its pictures. |
| `deckle cookbook note <id\|url> <text>` | Set a recipe's note; an empty string clears it. |
| `deckle cookbook picture <id\|url> site\|illustration\|none\|default` | Choose a recipe's picture. |
| `deckle cookbook illustrate <id\|url> \| --all [--redraw]` | Draw an engraving-style illustration (below). |
| `deckle cookbook chapters` | The chapters in book order, with recipe counts. Add, rename, remove and reorder them by editing `cookbook.toml`. |
| `deckle cookbook build -o <dir>` | Write `<dir>/cookbook.epub`, offline. An export: it sends nothing and leaves the "changed" flag alone. |
| `deckle cookbook send [--to device\|<folder>]` | Build if the book changed, then send it (below). |

`add`, `move`, `remove`, `note`, `picture` and `illustrate` only mark the book changed; they don't build, so
adding five recipes in a row costs one build.

### `send`

- Builds only when the book changed (a recipe saved, or `cookbook.toml`, a recipe file or a
  picture edited by hand) or has never been built. Otherwise the current build is sent as it is.
  No recipes yet is an error.
- A Kindle connected (`--to device`, the default): sent the way `deckle digest push` sends, so
  it also carries anything else waiting in the outbox. The Kindle is skipped, and the command
  says why, when it already has this build or is left out with `cookbook = false`.
- A Kindle that didn't take the book and doesn't have this build on record is an error (`not sent to <name>: <reason>`, exit 1), never "up to date".
- No Kindle connected: prints `Every Kindle already has this build` when every known Kindle (bar those left out) is up to date, else `Queued: goes to each Kindle on its next plug-in`; exits 0. `--json` `result` is `all_current` or `queued`.
- Only one build runs at a time (`cookbook/build/.lock`): `send` waits for a running build; `digest watch` skips the tick and tries again.
- `--to <folder>`: copies `cookbook.epub` there. A folder isn't a Kindle, so nothing is recorded.

## Illustrations (opt-in)

Off unless `cookbook.toml` says so:

```toml
[illustrations]
enabled = true
endpoint = ""    # an OpenAI-compatible base URL; empty is https://api.openai.com/v1
model = ""       # empty is gpt-image-1
```

The key is `OPENAI_API_KEY` in the environment (the same one X-Ray and the Daily editor use); it
never goes in `cookbook.toml`. Nothing is sent anywhere while `enabled` is false, and turning it
on draws nothing by itself.

**What is sent, and where:** one `POST {endpoint}/images/generations` per drawing, whose prompt
holds the recipe's title, its chapter's name and up to eight key ingredient names (the ones the
index uses), in a fixed engraving template. No address, description, note or photo is sent.

- `deckle cookbook illustrate <id|url>` draws one recipe; `--all` draws every recipe that has no
  illustration yet, one at a time, printing `<n>/<total> <title>: drawn` or `... kept the photo`.
  A recipe that already has one is skipped (and said so) unless `--redraw` is given.
- `deckle cookbook picture <id|url> illustration` chooses the drawing for that recipe, and draws
  it right away when there isn't one yet. With illustrations off it only saves the choice.
- The drawing is kept as `images/<id>.ill.jpg` (grayscale JPEG, at most 1000 px wide) beside
  the site photo, which is never overwritten. A recipe prints the drawing when its picture is
  `illustration` (or `default` and the book's `picture` is `illustration`); without a drawing it
  prints the site photo. Its foot then says "Illustration generated".
- A rebuild never draws: only `illustrate` (or `picture ... illustration`) talks to the endpoint.
- If the key is missing, the endpoint errors or its answer isn't a picture, the recipe keeps
  the picture it had and the command says `Couldn't draw this one; kept the photo`. What the
  provider replied is not printed, logged or stored.

## Saving from the Inbox

Drop recipe links (`.webloc`, `.url`, `.txt` with one address per line) or saved pages (`.html`,
`.htm`) into `Recipes/` inside the Inbox (`~/Deckle/Inbox/Recipes` unless `[reading] inbox` moves
it). Only `deckle digest watch` adds them (a one-shot `digest build`, `run` or `push` leaves the
folder alone, so fetching a backlog can't delay the paper). Each file is moved to `Recipes/Sent`
once added. Recipes saved this way never appear in the Daily's Your Reading, and articles
dropped in the main Inbox never reach the cookbook. A recipe from a site you're signed into goes
in as a saved `.html` page. An iPhone Shortcut ("Save File") pointed at the iCloud Inbox's
`Recipes` folder is the phone path.

A link that can't be fetched yet (no network when the Mac wakes, a timeout, a server error, a
file that hasn't finished downloading from iCloud) stays in `Recipes/` and is tried again, at
most every two minutes, for a day; `watch` says `waits for the next try`. After a day it moves
to `Recipes/Unreadable` with the reason `Couldn't fetch for a day: …`. A site that says no
(404, 403, 410…), a page with no recipe on it, or a file of another kind goes to
`Recipes/Unreadable` at once, with a `.txt` giving the reason. When a `.txt` with several
addresses fails part way, the reason lists the ones that were added; dropping the file back in
is safe. Drop an Unreadable file back in to try again.

### Save recipe bookmarklet

`deckle save --bookmarklet` prints a second bookmarklet beside the paper's, **Save recipe**. Drag it
to the bookmarks bar; on a recipe page, click it. A small popup says "Sent to your cookbook" and
`deckle digest watch` adds the recipe on its next scan (every few seconds). It works like the
paper's bookmarklet (a popup to `127.0.0.1:47219` with the same token, so it needs `watch`
running), but the listener only writes a link file, named after the page's title, into
`Inbox/Recipes`; the scan does the fetching. It sends the page's address only, never its HTML.
For a recipe behind a login, save the page as `.html` and drop it into `Inbox/Recipes`. If the
popup says the bookmarklet is out of date, run `deckle save --bookmarklet` again.

`Recipes` is reserved in the Inbox: a Kindle actually named "Recipes" can't have a folder there,
and `watch` says so once.

## Delivery

Every Kindle gets the newest build (not just the first one to plug in). `cookbook/delivered.json`
records, per Kindle sync id, the build it last took. `deckle digest watch` rebuilds a changed
book and, when a Kindle connects whose record differs from the newest build, pushes it on that
connection and records it; `digest push` and `cookbook send` do the same on demand. A Kindle
Deckle has never seen gets the book the first time it's plugged in after the first build. The
file on the Kindle has a fixed name and ASIN, so each build replaces the last (one library item).

Leave a Kindle out in `digest.toml`:

```toml
[sync.devices."<sync id>"]
name = "Attic Kindle"
cookbook = false
```

Status in the `list` footer: `Up to date`, `<n> changes behind` (builds since the one it
has), `Behind`, `Never sent`, or `Off` for a Kindle with `cookbook = false`. Kindles listed are
the ones named in `digest.toml` plus any with a delivery on record. In `--json`, `state` is
`up_to_date`, `behind`, `never_sent` or `off`, and `behind` is the number of builds behind (0 when
up to date; `null` when never sent, off, or the build it has is no longer in the history).

A failed build is kept in `cookbook/build/error` until a build succeeds; the footer's `Changes not
built yet:` line shows it, and `pending_build` / `build_error` carry it in `--json`.
