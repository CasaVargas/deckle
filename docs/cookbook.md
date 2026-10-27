# The Kitchen Book (`deckle cookbook`)

Recipes you keep become one book, rebuilt when it changes and sent to every Kindle on its next
plug-in. Design: `docs/superpowers/specs/2026-10-26-cookbook-design.md`. The folder is
`cookbook/` beside the other Deckle data (`DECKLE_COOKBOOK_DIR` moves it).

## Commands

Add `--json` (before or after the subcommand) for machine-readable output.

| Command | What it does |
|---|---|
| `deckle cookbook add <url\|file>... [--chapter <id>] [--note <text>]` | Keep recipes from web addresses, `.webloc`/`.url` link files or saved `.html` pages and file each into a chapter. Prints the chapter and the picture. |
| `deckle cookbook list [--chapter <id>]` | The recipes: id, title, chapter, added, recipe data or page text, picture. Below them, one `<Kindle>: <status>` line per known Kindle (see Delivery); none when no Kindle is known. `--json` gives `{recipes, kindles}`, with `kindles` as `[{name, id, status}]`. |
| `deckle cookbook move <id\|url> <chapter>` | File a recipe into another chapter. |
| `deckle cookbook remove <id\|url>` | Delete a recipe and its pictures. |
| `deckle cookbook note <id\|url> <text>` | Set a recipe's note; an empty string clears it. |
| `deckle cookbook picture <id\|url> site\|illustration\|none\|default` | Choose a recipe's picture. |
| `deckle cookbook chapters` | The chapters in book order, with recipe counts. Add, rename, remove and reorder them by editing `cookbook.toml`. |
| `deckle cookbook build -o <dir>` | Write `<dir>/cookbook.epub`, offline. An export: it sends nothing and leaves the "changed" flag alone. |
| `deckle cookbook send [--to device\|<folder>]` | Build if the book changed, then send it (below). |

`add`, `move`, `remove`, `note` and `picture` only mark the book changed; they don't build, so
adding five recipes in a row costs one build.

### `send`

- Builds only when the book changed (a recipe saved, or `cookbook.toml`, a recipe file or a
  picture edited by hand) or has never been built. Otherwise the current build is sent as it is.
  No recipes yet is an error.
- A Kindle connected (`--to device`, the default): sent the way `deckle digest push` sends, so
  it also carries anything else waiting in the outbox. The Kindle is skipped, and the command
  says why, when it already has this build or is left out with `cookbook = false`.
- No Kindle connected: prints `Queued: goes to each Kindle on its next plug-in` and exits 0.
- `--to <folder>`: copies `cookbook.epub` there. A folder isn't a Kindle, so nothing is recorded.

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
the ones named in `digest.toml` plus any with a delivery on record.
