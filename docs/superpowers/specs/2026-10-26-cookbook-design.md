# The Kitchen Book: a cookbook that grows on your Kindle — design

**Status:** approved by owner ("write the plan", 2026-10-26) · **Date:** 2026-10-26 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-20-save-to-paper-design.md` (the extractor, the Inbox, the bookmarklet,
saved pages for signed-in sites), `2026-10-21-web-article-design.md` (one web page → one book
with a stable identifier), `2026-10-20-daily-digest-design.md` (outbox and push on plug-in),
`2026-10-24-daily-llm-editor-design.md` (opt-in model calls, provider conventions).
**Mockups:** the two widgets in the session that produced this note (four e-ink pages: cover,
contents, recipe, index; and the desktop Cookbook tab). The descriptions in §6 and §8 are the
source of truth. The mockups only illustrate them.

Save a recipe link and it lands in **one** Kindle book, a cookbook of your own, filed into the
right chapter, with a picture, its ingredients and method laid out cleanly and the life story
cut. Each new recipe rebuilds the book, and the next time a Kindle is plugged in the new copy
replaces the old one. No account, no service: the recipes are plain files on your disk.

## 1. Intent and why

Recipe sites are the worst reading on the web: pop-ups, video players, and a thousand words
before the ingredients. Cooking from a phone means a screen that sleeps and gets floury.
A Kindle on the counter is good at this: it stays on, it's calm, and nobody minds it getting
dusty. What's missing is a way to get recipes onto it as something better than a pile of
one-off documents.

The idea is a **living book** rather than one document per recipe. A cookbook you can browse
by chapter, look up by ingredient and keep for years is worth more than forty loose files in
the Kindle library. Deckle already has nearly all the plumbing: page fetching and extraction,
the Inbox and bookmarklet, generated covers, the outbox and push-on-plug-in, and stable
identifiers so a re-sent book replaces itself. The new parts are reading the recipe as data,
the cookbook store, and the book layout.

**Success**, on a Sunday:

- On Saturday you saved three recipes: one with the bookmarklet, one from the phone through
  the iCloud Inbox, and one pasted into the desktop app.
- On Sunday morning you plug in the Kindle. The library still has **one** *Vargas Kitchen
  Book*, not three new items. Its cover now says *Fifty recipes, gathered* and names the
  newest dish.
- The contents show the three new recipes with a small star, each in its chapter (the soup
  under *Soups and stews*, the focaccia under *Baking*).
- The recipe page shows the dish's picture, one line of facts (*Serves 4 · Prep 15 min ·
  Cook 50 min*), ingredients, numbered steps, your note if you wrote one, and the source at
  the foot. There's no story about the author's grandmother.
- *Index by ingredient → Leeks* lists every recipe with leeks, each a link.
- The book opens where you left off. (This depends on §9, phase 0.)
- The site that published the soup has since gone offline. The recipe is still in the
  book, because Deckle kept it when it was saved.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **Read the recipe as data.** Most recipe sites publish the recipe as structured data in
   the page: schema.org `Recipe`, in JSON-LD. Parse that first. Fall back to the article
   extractor only when the page has none (§4).
2. **Keep every recipe locally**, as a plain file plus its picture, from the moment it's saved.
   A rebuild never fetches anything (§5).
3. **One book, chapters, a contents page and an index by ingredient**, rebuilt from the store
   whenever a recipe is added, moved or removed (§6).
4. **A picture per recipe**, chosen per recipe from three options with a book-wide default
   (§7):
   - **Site photo:** the default. The recipe's own photo, cropped and turned grayscale for
     e-ink.
   - **Illustration:** opt-in. An engraving-style drawing from an image model the owner
     configures.
   - **None.**
5. **Chapters file themselves** from the recipe's own category, and the owner can move any
   recipe and rename, add, remove or reorder chapters (§6.2).
6. **Four ways in**, all local: the CLI, a desktop Cookbook tab (paste a link), a *Recipes*
   folder in the Inbox, and a *Save recipe* bookmarklet (§8).
7. **Delivered like an Inbox book**: queued in the outbox and pushed on the next plug-in.
   It replaces the copy already on the Kindle and never adds a second one (§9).

**Non-goals (this increment)**

- **More than one cookbook.** One book per Deckle install. The store is laid out so a second
  book can come later (§5), but there's no UI or CLI for it.
- **Scaling, unit conversion and shopping lists.** Kindle books run no code, and parsing
  quantities reliably is its own project. Quantities print as the site wrote them.
- **Editing a recipe's ingredients or steps.** The owner can change the title, the chapter,
  the picture choice and a free-text note. Everything else is as the site published it.
- **Signing in to recipe sites** (NYT Cooking, America's Test Kitchen…). A saved `.html` page
  from a signed-in browser already works (save-to-paper goal 4). The publisher-subscription
  adapters are not extended to recipe sites.
- **Nutrition panels, ratings, reviews, comments and videos.** None of it goes into the book.
- **Deckle Reader (iPhone/iPad) support.** The cookbook is a Kindle (and EPUB) book only.
- **Sharing or exporting the cookbook to other people.** It's a personal copy, like the Daily.

**Anti-goals (a build that does these has failed)**

- **The life story leaks in.** If the structured data is present, the book shows only what
  the data says (§4.1). The fallback path is labelled in the app as "read from the page text"
  so the owner knows it may be messy.
- **Two cookbooks on the Kindle**, or a new library item per recipe.
- **A saved recipe disappears or changes** because the site went away or edited the page.
  Rebuilds read the store only.
- **Anything goes to a model provider without the owner turning illustrations on.** When it's
  on, only the recipe title, the chapter and ingredient names are sent: no URL, no notes,
  no photo (§7.2).
- **The book needs the network to build.** With illustrations off, `deckle cookbook build`
  works fully offline.

## 3. Where it sits

```
link / .webloc / saved .html / bookmarklet / paste
        │
        ▼
 fetch (deckle-digest Net)  ──▶  recipe parse (JSON-LD → Recipe)  ──▶  store
                                  └─ fallback: article extractor ─┘     │
                                                                        ▼
                                  picture (site photo │ illustration │ none)
                                                                        │
                                                                        ▼
                         build: cover + contents + chapters + index → cookbook.epub
                                                                        │
                                                                        ▼
                         outbox → (Previewer / cover rule) → push on plug-in, to each Kindle
```

The recipe parser, store and book builder go in a new module, `deckle_digest::cookbook`
(files under `crates/deckle-digest/src/cookbook/`). It uses the crate's fetching
(`fetch::Net`), the extractor (`article::extract`), photo handling (`article::eink_photo`), the
link-file parsing (`inbox::urls_in`), `reading::fnv` / `normalize_url` and `render::STYLE`
directly. Several of those are `pub(crate)`, and the Inbox scan and the bookmarklet listener
that feed recipes in also live in `deckle-digest`, so a separate crate would need either a
dependency cycle or a callback layer. `deckle-core` doesn't depend on it. As with web
articles (web-article §3), the cookbook writes an EPUB and hands it to the existing pipeline.
The CLI commands go in `deckle-cli` and the desktop commands in `apps/desktop/`.

## 4. Reading a recipe

### 4.1 Structured data first

Find every `<script type="application/ld+json">` in the page and look for a node whose
`@type` is or contains `Recipe`. It may be top-level, in an array, or inside `@graph`.
If there are several Recipe nodes, take the first one with both `recipeIngredient` and
`recipeInstructions`.

Map the node to Deckle's `Recipe`:

| `Recipe` field | From | Notes |
|---|---|---|
| `title` | `name` | Required. HTML entities decoded, tags stripped. |
| `description` | `description` | Optional. At most 300 characters, cut at a sentence end. It's printed in italics under the title. |
| `yield` | `recipeYield` | A string or an array: take the first entry that has a digit, or else the first entry. Printed as given ("4 servings", "1 loaf"). |
| `prep`, `cook`, `total` | `prepTime`, `cookTime`, `totalTime` | ISO 8601 durations → minutes. Unparseable values are dropped silently. If `total` is missing it's `prep + cook` when both exist. |
| `ingredients` | `recipeIngredient` (or legacy `ingredients`) | A list of strings, each decoded and stripped. Empty entries are dropped. |
| `steps` | `recipeInstructions` | Accepts a string (split into paragraphs on newlines), an array of strings, `HowToStep`s (`text`, or else `name`), and `HowToSection`s (`name` becomes a sub-heading, and its `itemListElement` steps go under it). |
| `category` | `recipeCategory` | A string or an array. Used only for filing (§6.2). |
| `cuisine` | `recipeCuisine` | Printed in the facts line if present ("Thai"). |
| `author` | `author.name` or `author` string | Printed at the foot with the source. |
| `image_url` | `image` | A string, an array (take the largest by `width`, else the first), or an `ImageObject.url`. |
| `source_url` | the page's canonical URL | Else the URL that was fetched. |
| `site_name` | `og:site_name` | Else the host. |

A recipe needs a `title`, at least one ingredient and at least one step. Anything less falls
through to §4.2.

### 4.2 Fallback: page text

With no usable structured data, run the existing article extractor (`article::extract`).
Store the result as an **unstructured** recipe: the title, the extracted body as the method,
no ingredient list, no facts line, and the extractor's lead photo as the picture. Pages under
120 words fail with the web-article reason. The desktop preview and `deckle cookbook add` both
say "Read from the page text, not recipe data". The index skips unstructured recipes, because
it has no ingredient list to read.

### 4.3 Sources

- `http(s)://` address → fetched with the shared `Net` (same user agent, timeouts and Google
  News link resolution as web articles).
- `.webloc`, `.url` → the link inside, then fetched.
- Saved `.html` / `.htm` page → parsed from the file and never fetched. The image loads from
  beside it if it's local, otherwise from its URL. This is how the owner gets a recipe from a
  site they're signed into.

## 5. The store

Plain files under Deckle's application-support folder, next to `digest.toml`:

```
cookbook/
  cookbook.toml          # the book: title, chapters, defaults (below). Owner-editable; comments kept.
  recipes/<id>.json      # one Recipe each (§4.1 fields + the owner's fields below)
  images/<id>.jpg        # the picture as printed (grayscale, cropped)
  images/<id>.orig.jpg   # the site photo as fetched, kept so the crop can be redone
  images/<id>.ill.jpg    # the illustration, if one was generated (never regenerated on rebuild)
```

`<id>` is the first 12 hex digits of the FNV hash of the normalized source URL
(`reading::normalize_url` and `reading::fnv`, as the reading queue does), or of the file's
absolute path for a saved page. **Saving the same recipe again updates it in place**: the
parsed fields are refreshed and the owner's fields are kept. The app says "Updated" rather
than "Added".

Owner fields in each recipe file: `chapter` (a chapter id), `picture` (`site` | `illustration`
| `none` | `default`), `note` (free text, at most 1,000 characters), `title_override`, and
`added` (an RFC 3339 timestamp).

`cookbook.toml`:

```toml
title = "The Vargas Kitchen Book"     # default: "The Kitchen Book"
subtitle = "Recipes, gathered"        # cover line under the title; the count is added
picture = "site"                      # book-wide default: site | illustration | none

[[chapters]]                          # order here is order in the book
id = "breakfast"
name = "Breakfast"
match = ["breakfast", "brunch"]       # recipeCategory keywords (§6.2)
# ... the defaults in §6.2

[illustrations]                       # §7.2; absent or enabled = false means off
enabled = false
```

There's no database and no hidden state. Deleting `recipes/<id>.json` removes that recipe on
the next build. The desktop app and the CLI read and write the same files, and the owner's
own comments in `cookbook.toml` are kept when it's rewritten (same TOML-editing approach as
`digest.toml`).

## 6. The book

### 6.1 Structure

One EPUB, identifier `urn:deckle:cookbook:default`. The identifier never changes, so the
generated ASIN never changes either (`generate_asin(asin_seed(...))`, the same mechanism
that makes a re-sent article replace itself). **The file name on the Kindle is fixed too**,
`Deckle Kitchen Book.kfx` (`.azw3`/`.epub` for other readers), whatever the book's title.
`deliver::device_filename` normally derives the name from title and author, so renaming the
book in Book settings would otherwise leave two cookbooks on the Kindle. Add a file-name override to
the delivery path for this one book. A title change still shows in the Kindle library,
because the title comes from the metadata, not the file name. The book has, in order:

1. **Cover**, generated each build (§6.3).
2. **Contents**: each chapter with its recipe count, then *Index by ingredient*. Under each
   chapter, its recipes alphabetically by title, each a link. Recipes added in the last 7
   days get a small star (★) after the title, and a line at the foot says
   "New this week: N".
3. **Chapters** in `cookbook.toml` order. An empty chapter is left out of the book (but kept
   in the config). Each chapter has a title page, followed by its recipes alphabetically.
4. **One recipe per section**, each starting a new page:
   - the picture, full text width, landscape 3:2;
   - the title;
   - the description in italics, if there is one;
   - the facts line: yield · prep · cook (or total) · cuisine, leaving out whatever is
     missing;
   - **Ingredients** as a plain list, with `HowToSection`-style groupings kept if the data
     had them;
   - **Method** as a numbered list, with section sub-headings;
   - **Your note**, set off in a ruled box, if there is one;
   - the foot: "From *site_name* · by *author* · saved *date*" with the source URL as text.
     The URL is not a link: on a Kindle a link only opens the experimental browser.
5. **Index by ingredient** (§6.4).

Typography follows the Daily's book style (Georgia-like serif via the existing `render::STYLE`
conventions, real page numbers through the existing page-list support). On a wide screen
(Scribe) the ingredients and method sit side by side. On narrower screens they stack. Use a
CSS media query only if Enhanced Typesetting keeps it, and otherwise always stack.
*(Builder's call, checked in Kindle Previewer.)*

### 6.2 Chapters and filing

Default chapters, in order, with their `match` keywords (matched case-insensitively as whole
words against each `recipeCategory` value, then against the title):

| id | name | match |
|---|---|---|
| breakfast | Breakfast | breakfast, brunch |
| soups | Soups and stews | soup, stew, chili, chowder, bisque |
| salads | Salads | salad |
| mains | Mains | main, dinner, entree, entrée, supper |
| sides | Sides | side, vegetable |
| baking | Baking | bread, baking, loaf, muffin, scone, biscuit |
| desserts | Desserts | dessert, cake, cookie, pie, tart, pudding |
| drinks | Drinks | drink, cocktail, beverage, smoothie |
| sauces | Sauces and basics | sauce, dressing, condiment, stock, spice |
| other | Everything else | (none: the catch-all) |

Rules:

- The first chapter in book order whose keywords match a category value wins. If none match
  a category, try the title the same way. If nothing matches, the recipe goes in *Everything
  else*.
- Filing happens **once, when the recipe is first saved**, and is written to the recipe's
  `chapter`. Re-saving or editing chapter keywords later never moves an existing recipe.
  The owner's choice is permanent.
- A recipe whose `chapter` no longer exists (the chapter was deleted) goes in *Everything
  else* at build time. Its file is left alone.
- *Everything else* can be renamed but not deleted.

### 6.3 Cover

Built with the same machinery as the "Deckle Reads" and Daily covers (`deckle-cover`,
`digest::cover`), 1600×2560 grayscale:

- a small engraving-style ornament (a fixed vector drawing shipped with Deckle, not
  generated);
- the book title;
- a subtitle line with the count spelled out ("Forty-seven recipes, gathered");
- "Last added · *title*" at the foot.

The cover changes with every build, so the Kindle's library tile always shows the current
count.

### 6.4 Index by ingredient

For each structured recipe, reduce each ingredient line to a **key ingredient** with a small
deterministic normalizer:

1. lowercase the line and drop anything in parentheses;
2. drop leading quantities, fractions (including Unicode ones) and ranges;
3. drop units from a fixed list (cup, tbsp, tsp, g, kg, oz, lb, ml, l, clove, can, pinch,
   bunch, …);
4. drop preparation words from a fixed list (chopped, diced, minced, fresh, large, …) and
   anything after the first comma;
5. singularize the last word with a few simple rules;
6. look the result up in a shipped alias table (`scallion` → `green onion`, `aubergine` →
   `eggplant`, …).

The index lists keys alphabetically, with the recipes that use each key under it as links.
Keys used by only one recipe are still listed. Keys made of stopwords only (`salt`, `pepper`,
`water`, `oil`, `salt and pepper`) are left out. The fixed lists ship as plain text files in
`src/cookbook/` so they can be tuned without code. *(Builder's call: the exact word lists. The bar
is that the 20 fixture recipes in §10 produce an index with no obviously junk keys.)*

## 7. Pictures

### 7.1 Site photo (the default)

At save time, fetch `image_url` (or the extractor's lead photo for unstructured recipes) and
keep it as `images/<id>.orig.jpg`. Produce the printed picture from it: center-crop to 3:2,
then pass it through the Daily's e-ink treatment (`article::eink_photo`, which caps width
at 1,000 px and makes it grayscale), giving 1000×667. Keep that as `images/<id>.jpg`. A recipe with no image, or an image
that fails to load or decode, falls back to **none** without failing the save. The app says
"No picture on the page".

### 7.2 Illustration (opt-in)

An engraving-style drawing of the dish, generated once per recipe and kept.

- **Off by default.** It's turned on in `cookbook.toml` `[illustrations]` or in the Cookbook
  tab's Book settings, and the desktop switch explains what is sent where. Turning it on doesn't
  generate anything for existing recipes until the owner chooses **Illustrate all** or
  changes a recipe's picture to Illustration.
- **Provider:** an OpenAI-compatible image generation endpoint
  (`POST {endpoint}/images/generations`), configured like X-Ray's LLM: `endpoint` and
  `model`. The key is found the same way X-Ray and the Daily editor find theirs:
  `OPENAI_API_KEY` from the environment, or the desktop's existing `llm_api_key` setting.
  (The desktop app launched from Finder doesn't see shell variables, so the setting is the
  desktop path.) The key never goes in `cookbook.toml`. A local compatible server is fine.
- **Prompt:** a fixed template with the recipe title, the chapter name and up to eight key
  ingredients (§6.4). For example: "A single plated dish, *Roasted squash soup*, drawn as a
  19th-century copperplate engraving: black ink line work and cross-hatching on white, no
  color, no text, no hands, no background scene. Visible ingredients: butternut squash,
  onion, cream." Nothing else is sent: no URL, no description, no note, no photo.
- **Result:** requested at 1536×1024 (or the nearest size the endpoint offers), grayscale,
  stored as `images/<id>.ill.jpg`, and used for the printed picture when the recipe's picture
  is Illustration. It's never regenerated on rebuild. **Redraw** in the app regenerates one
  on request.
- **Labelling:** the recipe's foot adds "Illustration generated" when an illustration is
  shown.
- **Failure:** missing key, HTTP error, timeout or an undecodable image keeps the recipe's
  current picture (site photo, or else none). The app shows one generic line ("Couldn't draw
  this one; kept the photo"). Raw provider errors and replies never go in the book, the app
  or the logs, as in the Daily editor.
- **Limits:** one request per recipe per action, the existing request timeout, and no retry
  loop. **Illustrate all** runs one recipe at a time and can be stopped.

## 8. Ways in

### 8.1 CLI

```sh
deckle cookbook add <url|file>...       # parse, keep, file into a chapter; prints chapter and picture
deckle cookbook add <url> --chapter baking --note "Half the sugar works"
deckle cookbook list [--chapter <id>]   # title, chapter, added, structured or text, picture
deckle cookbook move <id|url> <chapter> # refile
deckle cookbook remove <id|url>         # deletes the recipe file and its images
deckle cookbook note <id|url> "text"    # set or clear (empty string) the note
deckle cookbook picture <id|url> site|illustration|none|default
deckle cookbook illustrate <id|url>|--all  # draw (or redraw) illustrations; needs [illustrations] on
deckle cookbook build -o <dir>          # write cookbook.epub, offline
deckle cookbook send [--to device]      # build and queue in the outbox (or send now if a Kindle is connected)
deckle cookbook chapters                # list; add/rename/remove/reorder are done by editing cookbook.toml
```

`add`, `move`, `remove`, `note` and `picture` **queue a rebuild** (§9). They don't build on
the spot, so adding five recipes in a row costs one build.

### 8.2 Desktop: a Cookbook tab

A fourth top-level tab, **Cookbook**, built on the Daily tab redesign's frame
(`2026-10-26-daily-tab-redesign-design.md`, PRs #206–#208) with the same look as the Books,
Kindle, Settings and Daily tabs. It can't be built until that redesign is merged (§11).

**The frame.** Tabs under the app bar: **Add · Recipes · Delivery**. To the left of the tabs,
where the Daily has its paper switcher, a **book menu**: the book's title and its recipe
count, as a menu button. The menu holds **Book settings…** for now. When more cookbooks
arrive (§12), they're listed here the way papers are.

**Add** (two columns, stacked when narrow, like the Daily's Today):

- **Add a recipe**: a link box (with ⌘V / Ctrl+V paste anywhere on the tab) and **Fetch**.
  Dropping `.webloc`, `.url` or `.html` files on the tab does the same for each one. A fetched
  recipe shows as a preview card:
  - the picture, with Site photo / Illustration / None chips;
  - the title, and a facts line with ingredient and step counts;
  - a chapter menu, preset by §6.2;
  - a note field;
  - a line saying whether it was read from recipe data or from the page text;
  - **Add to the book**, the one action that commits. Before that, nothing is written.

  After adding, the card says "Added to *Soups and stews*", with a link to the recipe's row on
  Recipes. A recipe already in the book says "Updated" instead.
- **The book card**: a small cover thumbnail (the cover as it will print), "*n* recipes in
  *m* chapters", "Last added · *title*", and one delivery line from Delivery ("Both Kindles
  up to date", "3 changes go to the Scribe on next plug-in"), linking to Delivery. Buttons:
  **Send now** (builds and sends to every connected Kindle) and **Preview** (builds and opens
  the EPUB, as the Daily's Preview does). The last build's result shows under the buttons.

**Recipes**: "What's in the book", one list in book order (§6.1): a heading row per chapter
with its count, then its recipes alphabetically. Empty chapters show as a heading with
"Nothing yet" and are left out of the book.

- A recipe row: the picture thumbnail, the title (★ while new), a one-line detail
  (site · added date · "from the page text" when unstructured), and a chevron that opens it.
- A row opens beneath itself, one at a time, with:
  - title (an override; empty shows the site's title);
  - chapter (a menu);
  - picture (Site photo / Illustration / None / Book default chips; Illustration offers
    **Redraw** while illustrations are on);
  - note;
  - the source address with **Open** (in the browser);
  - **Remove** (asks once).
- Moving a recipe between chapters is the chapter menu, not dragging. Recipes within a chapter
  are always alphabetical.
- **Every change saves at once**, using the Daily redesign's save rules and its save queue
  (`saveQueue.ts` from #207): menus and chips straight away, text after about half a second
  without typing and on leaving the field. Saves run one at a time against the latest saved
  file, and a failed save shows its reason on the row and keeps the typed text. There's no
  Save button anywhere on the tab. Each save marks the book dirty (§9).
- A search box above the list filters by title and ingredient. *(Builder's call whether it
  ships in this PR or after.)*

**Delivery**: "Where it goes", one row per Kindle Deckle knows:

- each row has the Kindle's name, its state ("Up to date", "3 changes behind", "Never sent"),
  when it was last delivered, and a switch (on by default) that writes `cookbook = false`
  for that Kindle (§9);
- any phase 0 note for that kind of Kindle shows on its row ("Opens the book at the start
  after each update");
- while the watcher isn't installed, a note above the rows: "Updates go over when you press
  Send now, or turn on delivery when a Kindle connects", linking to the Daily's Delivery tab
  where that switch lives.

**Book settings** opens as a sheet from the right, like the Daily's Paper settings (Esc or ×
closes it), and saves as it changes:

- **Title**, **Subtitle** (the count is added on the cover), **Default picture**.
- **Chapters**: one list in book order. Each row has a drag handle (or Alt+↑/↓ on a focused
  row, as on Sections), the name (editable in place), its recipe count, and Remove (shut on
  *Everything else*). **+ Add a chapter** goes below. Removing a chapter that has recipes
  says they'll print under *Everything else* until moved (§6.2). Match keywords are edited
  in `cookbook.toml` only.
- **Illustrations**: its switch, a line on exactly what is sent (title, chapter, ingredient
  names) and where, the endpoint, model and key fields while on (the key goes to the
  desktop's existing `llm_api_key` setting, as with the AI editor), and **Illustrate all**
  with progress and Stop.
- `cookbook.toml`'s path and **Show the file** at the foot.

The Books tab's **Add ▾** menu gets **Recipe…**, which switches to Cookbook → Add with the
link box focused.

### 8.3 Inbox and bookmarklet

- **Inbox:** a `Recipes/` subfolder of the existing Inbox (created on first run of the
  watcher). Link files and saved pages dropped there go to the cookbook instead of Your
  Reading, and move to `Inbox/Recipes/Sent` or `Inbox/Recipes/Unreadable` (with a `.txt`
  reason), using the same folder names as the Inbox. `Recipes` becomes a reserved Inbox
  subfolder name: today any other subfolder is read as a Kindle's folder when it matches a
  Kindle's name, so a Kindle actually named "Recipes" has to be skipped with a one-line
  warning. An iPhone Shortcut pointed at the iCloud Inbox's
  `Recipes/` folder is the phone path.
- **Bookmarklet:** a second bookmarklet, **Save recipe**, beside the existing one. It works
  exactly like the existing one (a GET popup to the same `127.0.0.1` listener with `url`,
  `title` and the token), adding `kind=recipe`. Like the existing bookmarklet, it needs
  `deckle digest watch` running. It sends **the address only**, never the page's HTML: posting
  page bodies to `127.0.0.1` from HTTPS pages runs into local-network permission prompts and
  site CSPs, and it would need a body parser on a listener that today reads one request line.
  Recipes from signed-in sites go in as saved `.html` pages (Inbox or desktop drop).
  *(My call.)*

Recipes never land in the Daily's Your Reading, and articles never land in the cookbook.
The owner picks by the way they save. There's no content sniffing to guess which one a link
is. *(My call: food blogs put a recipe at the bottom of essays, so guessing would misroute
both ways.)*

## 9. Delivery

- Any change to the store marks the cookbook **dirty**. The watcher (`digest watch`) and
  `deckle cookbook send` rebuild a dirty cookbook. Each build has a **build id** (a hash of
  the EPUB's content). Only the newest build is kept, under `cookbook/build/`: the EPUB, plus
  one converted file per content type a Kindle needs, made on demand and cached by build id.
- **Every Kindle gets it** (owner's ruling, 2026-10-26). The cookbook is a standing delivery,
  not a one-shot outbox entry: an ordinary outbox entry is deleted after the first Kindle
  takes it, so a second Kindle would never see that build. `cookbook/delivered.json` records,
  per Kindle sync id, the build id last delivered. On plug-in, if that Kindle's recorded build
  id differs from the newest (or is missing), push the newest build and record it. It shares
  the plug-in connection with the paper and Inbox books. A Kindle can be left out with
  `cookbook = false` in its `[sync.devices."<sync id>"]` table in `digest.toml`, or with the
  per-Kindle switch on the Cookbook tab's Delivery (default on). A Kindle Deckle has never seen
  gets the cookbook the first time it's plugged in after the first build. The Cookbook tab's
  Delivery lists each Kindle with "Up to date" or "N changes behind", and `deckle cookbook list`
  prints the same at its foot.
- The push treats it like an Inbox book: the Previewer step runs when it's queued, and the
  push picks PDOC or EBOK by the existing cover rule and per-Kindle mode. **The cover rule
  wins** over reading position (Deckle's own rule: covers are non-negotiable, and the cover
  carries the recipe count). The identifier and the device file name are fixed (§6.1), so the
  generated ASIN and file name are the same every time. The push overwrites the copy on the
  Kindle (`deliver` already replaces a same-named file).
- **No X-Ray or Word Wise on the cookbook** in this increment. Its Inbox-book preparation sets
  X-Ray off and Word Wise off, which keeps each rebuild to one Previewer run. Cooking X-Ray
  is §12.
- **Who rebuilds:** the watcher (`digest watch`, installed by the Daily's schedule or run by
  hand) rebuilds a dirty cookbook and pushes it on plug-in, the same as Inbox books. The
  desktop's **Send now** builds and sends immediately when a Kindle is connected. If the
  watcher isn't installed, the Cookbook tab's Delivery says so ("Updates go over when you press Send
  now, or turn on delivery when a Kindle connects") and links to the Daily's delivery switch.
  The desktop app doesn't run its own watcher.
- **Phase 0 (hardware spike, run by the owner; it blocks nothing):** on the Scribe (MTP) and the
  Paperwhite (mass storage), send a 10-recipe cookbook, read into recipe 6, highlight a line,
  then send a 12-recipe build with two recipes inserted *before* recipe 6. Record whether:
  1. the library shows one item, with the updated cover;
  2. the book reopens at recipe 6 rather than at an earlier position or the start;
  3. the highlight survives, and where it lands;
  4. "collections" membership survives.

  Do this for both PDOC and EBOK. Write the results in `docs/devices.md`.

  Use whichever type the cover rule picks for each device. Phase 0 doesn't change the type.
  It decides only what the app tells the owner. If the position doesn't survive on a device,
  that Kindle's row on the Cookbook tab's Delivery says "This Kindle opens the book at the start after each update". If the
  library shows two items, that's a bug to fix before step 3 ships.
- The cookbook is **never pruned** by the Daily's keep-newest-N rule.

## 10. Verification

- **Parser fixtures** (`crates/deckle-digest/tests/fixtures/cookbook/`): 20 saved recipe pages trimmed to
  their markup. They cover:
  - JSON-LD top-level, in an array, and in `@graph`;
  - `@type` as an array;
  - `HowToSection`s;
  - instructions as a single string;
  - `recipeYield` as an array;
  - ISO durations with hours and days;
  - `image` as a string, an array and an `ImageObject`;
  - HTML entities in names;
  - two Recipe nodes on one page;
  - no JSON-LD (falls back to text);
  - a stub page (fails).

  Each asserts the mapped `Recipe`. Use fictional or self-written recipe content in the
  fixtures, not copied text from real sites.
- **Store:** add → file written with `chapter` filed by §6.2. Re-add the same URL → parsed
  fields refreshed, owner fields kept, no second file. Remove deletes the file and images.
  `cookbook.toml` round-trips with the owner's comments kept.
- **Filing:** a table test of category arrays and titles against the default chapters,
  including no match → Everything else, and a deleted chapter → Everything else at build.
- **Index:** the normalizer on a table of ingredient lines; the 20 fixtures' index snapshot.
- **Build:** a fixture store builds a valid EPUB (epubcheck as other book writers do) with
  the identifier `urn:deckle:cookbook:default` every time, chapters in config order, empty
  chapters omitted, stars on recipes newer than 7 days (with an injected clock), and the
  index links resolving. Building makes no network calls (a `Net` that panics if used).
- **Pictures:** an image that fails to fetch saves with no picture. Crop and grayscale give
  a 1000×667 JPEG. With illustrations off, no image request is ever made (an injected
  transport that fails the test if called). With them on, the request body contains only the
  title, chapter and ingredient keys. Failures keep the old picture and log no provider text.
- **Delivery:** with the simulated mass-storage Kindle, two builds in a row leave one file
  in `documents/` with the second build's content, and the generated ASIN is identical.
  Renaming the book between builds still leaves one file. With two simulated Kindles, a build
  reaches both on their next plug-ins, a Kindle already up to date gets no push, and a Kindle
  with `cookbook = false` gets nothing.
  The phase 0 results go in `docs/devices.md`.
- **Desktop:** visually verified in the running app (Add with its preview, Recipes with a row open, Delivery, Book settings), with the
  cover and two recipe pages checked in Kindle Previewer.

## 11. Build order

0. Hardware spike (§9) on Scribe and Paperwhite. The owner runs it with a hand-built book
   from step 2. Its results only change the wording on the Cookbook tab's Delivery.
1. `deckle_digest::cookbook`: the parser (§4), the store (§5), filing (§6.2); `cookbook add / list /
   move / remove / note`.
2. The book builder (§6) with site photos (§7.1): `cookbook build`.
3. Delivery (§9): dirty flag, build id, per-Kindle standing delivery, `cookbook send`.
4. The Inbox `Recipes/` folder and the Save recipe bookmarklet (§8.3).
5. Illustrations (§7.2), backend and CLI (`cookbook picture … illustration`,
   `cookbook illustrate`).
6. The desktop Cookbook tab (§8.2), including the Books tab's **Recipe…** entry. **Starts only
   after the Daily tab redesign (#206, #207, #208) has merged**, because it reuses that
   frame's tabs, sheet and save queue. Steps 1–5 are Rust only and don't wait for it.

Each step is its own PR and works on its own: after step 2 the owner can already build and
sideload the book by hand.

## 12. Later (not in this increment)

- **X-Ray for cooking techniques:** a shipped glossary (blanch, fold, deglaze, julienne…) fed
  to the existing offline X-Ray so terms are explained in place. Needs EBOK, so it depends on
  phase 0.
- **Metric alongside US units**, once the ingredient normalizer has proven itself.
- **More than one cookbook** (a baking book, a holiday book) via a `[[books]]` list.
- **A printed PDF** of the cookbook through the existing PDF edition path.
