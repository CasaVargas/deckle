# Product Hunt launch: Deckle

Copy and assets for the Product Hunt post. Limits are Product Hunt's: name 40, tagline 60,
description 260. Assets are in [product-hunt/](product-hunt/).

## Basics

| Field | Value |
|---|---|
| **Name** | Deckle |
| **Link** | https://casavargas.app/deckle/ |
| **Extra links** | https://github.com/CasaVargas/deckle (source) · https://github.com/CasaVargas/deckle/releases/latest (download) |
| **Pricing** | Free |
| **Open source** | Yes, GPL-3.0 |
| **Platforms** | Mac, Windows, Linux (CLI and Docker) |
| **Topics** | Open Source · Books · News |
| **Thumbnail** | `product-hunt/thumbnail-240.png` |

## Tagline (pick one, ≤ 60)

1. **A morning paper for your e-ink reader, set on your computer** (59) ← recommended
2. Your feeds and local news as a real paper on your Kindle (56)
3. A daily newspaper for your Kindle, Kobo or reMarkable (53)

## Description (≤ 260)

> Every morning Deckle builds a real newspaper from your town's news, weather, RSS feeds and
> saved articles, and has it on your Kindle, Kobo or reMarkable before you're up. It also
> turns the books you own into Kindle books that behave like store books.

(250 characters.)

## Gallery (1270×760, in this order)

1. `gallery-1-hero.jpg`: the Daily tab beside the front page it printed
2. `gallery-2-paper.jpg`: three pages of an edition: a story, the almanac, the crossword
3. `gallery-3-daily-sections.jpg`: every section of the paper in one list
4. `gallery-4-daily-delivery.jpg`: Kindle, reMarkable, iPad, Kobo, PocketBook
5. `gallery-5-books-library.jpg`: the library and the send queue
6. `gallery-6-books-drawer.jpg`: what Deckle does to a book, before and after
7. `gallery-7-kindle-shelf.jpg`: the Kindle's shelf with progress
8. `gallery-8-settings-online.jpg`: every network call, listed

Optional video slot: a 30–60 s screen recording of plugging in a Kindle, pressing **Send now**,
and the paper opening on the reader. Not made yet.

## Maker's first comment

> Hey Product Hunt 👋 I'm Jon, and I made Deckle.
>
> I wanted to read the news the way I read books: on an e-ink screen, in the morning, with an
> end. A feed reader is a pile of links. A paper has an order, a front page and a last page you
> can finish.
>
> So Deckle sets one every morning, on your own computer:
>
> 📰 **The front page is the cover.** Masthead, date, weather and the lead headlines, so your
> reader's library tile is today's paper.
> 🏡 **Your town and what you follow.** Local news, weather, any RSS feed, and topics searched
> every morning ("Rust", your team).
> 🔖 **What you saved.** Articles from the bookmarklet or an iPhone Shortcut print the next
> morning, whole, photos in grayscale.
> 🧩 **The back pages.** A mini crossword whose answers are yesterday's headlines, a word
> search, an almanac and On this day.
> 📚 **From your shelf.** The books you're in the middle of and a past highlight, read off your
> own Kindle.
>
> It goes to a Kindle (USB or email), Kobo, PocketBook, reMarkable, anything running KOReader,
> or a folder for a Boox or an iPad.
>
> The second half is **books**. A book you load onto a Kindle yourself is second-class: no
> Enhanced Typesetting, no X-Ray, no Word Wise, a grey cover, no real page numbers. Deckle
> fixes all of it in one pipeline, for EPUBs, Word documents, PDFs, comics and Markdown notes.
> Drop them on the window, plug in the Kindle, press Send.
>
> **What it isn't:** there's no Deckle server. No account, no subscription, no telemetry. The
> paper is fetched, set and kept on your machine, and every network call is listed on one
> settings page. It's free and GPL-3.0, with a command line that does everything the app does.
>
> It's built by one person (me), in Rust and Tauri. I'd love to know which reader you'd send
> it to, and what section your paper is missing. I'll be here all day.

## Short replies to have ready

- **"Does it need an Amazon account / Send to Kindle?"** No. Kindle over USB needs nothing;
  Kindle by email uses your own mail account. Kindle Previewer (free, from Amazon) is only
  needed to make Kindle books.
- **"Does it work with DRM books?"** DRM-protected files are refused, never decrypted.
- **"Why not Calibre?"** Keep calibre. Deckle reads your calibre library (read-only), answers
  KOReader's calibre connection and writes reading data back through `calibredb`.
- **"Linux?"** The CLI and a Docker container that prints the paper for email, folders and
  KOReader. Kindle books need Kindle Previewer, which is Mac and Windows only.
- **"Is the AI required?"** No. It's off by default. Turned on, a model you choose (a local one
  through Ollama works) folds duplicate stories and writes labeled briefs. Stories are never
  rewritten.
- **"How do you make money?"** I don't, from this. It's free and open source.

## Launch-day checklist

- [ ] Schedule for 12:01 AM PT on a Tuesday–Thursday
- [ ] Latest release up to date and download links working on the website
- [ ] Post the first comment the moment it's live
- [ ] Share to Mastodon, Bluesky, r/kindle, r/kobo, r/RemarkableTablet, r/koreader (follow
      each subreddit's self-promotion rules)
- [ ] Answer every comment the first day
