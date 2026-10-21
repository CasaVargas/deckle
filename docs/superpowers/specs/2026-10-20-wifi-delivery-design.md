# Wi-Fi delivery over the LAN — design

**Status:** owner's call on #44 (2026-10-20): build it as an opt-in server; a real Kindle
passing the spike in §7 gates the merge · **Date:** 2026-10-20 · **Owner:** Jonathan
(CasaVargas LLC)

**Builds on:** `2026-10-18-deckle-design.md` (§2 non-goal "USB/MTP only", amended below),
`2026-10-20-daily-digest-design.md` (the outbox), `2026-10-20-save-to-paper-design.md` (the
bookmarklet listener this copies).

## 1. Intent and why

The Deckle Daily is built at 05:30 and waits in the outbox until a Kindle is plugged in. A
Scribe on the nightstand is rarely plugged in. Every Kindle has a web browser and Wi-Fi, so
let the Kindle fetch the paper: `deckle serve` puts the outbox on a small, read-only web page
on the home network (or the tailnet), and the reader opens one bookmarked URL on the Kindle.

## 2. Goals, non-goals, anti-goals

**Goals (this increment)**

1. `deckle serve --bind <ip>` serves the outbox read-only over HTTP and prints its URL. Off
   by default: nothing listens unless the command is running.
2. It binds only to the address given (a LAN or Tailscale IP). There is no default bind
   address and the wildcard addresses (`0.0.0.0`, `::`) are refused.
3. Every URL carries a random token in the path, `/t/<token>/`. Anything else gets 403.
4. `GET /t/<token>/` is an HTML index of the outbox; `GET /t/<token>/<name>` is the file.
5. Serving never changes the outbox: no file is moved, renamed or deleted, so `push` and
   `watch` deliver the same files afterwards.

**Non-goals (this increment)**

- The desktop app toggle (a follow-up issue).
- X-Ray and Word Wise sidecars (`.sdr`). A Wi-Fi delivery is the book alone; the index says
  so. The sidecars need USB, where Deckle can write next to the book.
- Pruning old editions on the Kindle. Only `push` over USB prunes.
- HTTPS. The token is a capability on a trusted network; see §6.
- Producing AZW3/MOBI. That stays a v1 non-goal unless the spike (§7) forces the fallback.

**Anti-goals**

- A listener the user didn't start, or one on every interface.
- Anything reachable from the network that can write, delete or list outside the outbox.
- A new HTTP server dependency.

## 3. Amendment to the v1 non-goal

`2026-10-18-deckle-design.md` §2 says "Deckle is USB/MTP only". This spec adds a labelled
exception: **`deckle serve`, an opt-in, read-only LAN/Tailscale server over the outbox**. The
book still never goes through Amazon (unlike the digest's planned Send-to-Kindle email); the
Kindle fetches it straight from the user's machine.

## 4. Behaviour

```
deckle serve --bind 192.168.1.20            # or a Tailscale 100.x address
deckle serve --bind 192.168.1.20 --port 8080 --dir ./out   # serve another folder
deckle serve --bind 192.168.1.20 --new-token               # rotate the token
```

Prints `http://<bind>:<port>/t/<token>/` (or `{"url": …}` with `--json`) and serves until
interrupted. Default port **47220** (the bookmarklet has 47219).

Request handling (pure `deckle_core::serve::handle`, no I/O beyond reading the outbox):

| Request | Response |
|---|---|
| path not under `/t/<token>` (wrong, missing token; `/`, `/favicon.ico`) | 403 |
| method other than `GET`/`HEAD` (with the token) | 405 |
| `/t/<token>` or `/t/<token>/` | 200, HTML index |
| `/t/<token>/<name>` where `<name>` is a file in the listing | 200, the bytes |
| `/t/<token>/<stem>.azw` where `<stem>.kfx` is in the listing | 200, the same bytes (§7 variant) |
| anything else under the token (unknown name, `..`, `%2e%2e%2f…`, subfolders) | 404 |

- **The listing** is `Queue::pending()`: the outbox's `<name>.kfx` files. `.sdr` folders and
  any other file are never listed or served. A request is served only if its decoded name
  equals a listed file name exactly, so traversal has nothing to match.
- **File responses** carry `Content-Length` (the file size), `Content-Disposition: attachment;
  filename="<name>"` and a type by extension: `.kfx` `application/octet-stream`, `.azw`
  `application/vnd.amazon.ebook`. `HEAD` returns the same headers without the body.
- **The index** is plain HTML for the experimental browser (no script, big tap targets): one
  link per book with its size, an "as .azw" link under it, and the note *"Wi-Fi delivers the
  book only. X-Ray and Word Wise need USB (`deckle digest push`)."* Links are absolute paths
  with percent-encoded names.
- **The token** compare is constant-time.
- **One thread per connection**, so a stalled download doesn't block the index. Request
  headers are read and ignored (8 KiB cap, 10 s read timeout).

## 5. Code layout

- `crates/deckle-core/src/serve.rs`: `token(dir, rotate)`, `handle(request_line, token,
  &Queue) -> Response`, `serve(TcpListener, Queue, token)`, `url(addr, token)`.
- `crates/deckle-cli/src/cli.rs` + `lib.rs`: the `serve` command; refuses unspecified bind
  addresses; prints the URL.
- Tests: unit tests on `handle` (403, 405, index, traversal, sidecars, alias); an integration
  test binding `127.0.0.1:0` with `ureq` as the client (bytes, `Content-Length`, outbox
  unchanged afterwards and still flushable).

## 6. Security

The threat is another device on the same network. Mitigations: the server exists only while
the command runs; it binds one address the user chose; the 128-bit token is required for every
path; the server can only read files whose names it listed itself. Plain HTTP means anyone who
can sniff the Wi-Fi sees the token: acceptable for a home LAN; on a tailnet the transport is
already encrypted. Rotate with `--new-token`.

## 7. Device spike (owner, before merge)

Known risk: the Kindle experimental browser has historically downloaded only `.azw`, `.mobi`,
`.prc`, `.txt` and `.pdf` and refused everything else, which may include `.kfx`.

1. On the Mac: `deckle digest run --to device` with no Kindle connected (queues today's edition),
   or `deckle run book.epub --to ./out` and use `--dir ./out` below. Find the Mac's LAN IP
   (`ipconfig getifaddr en0`) and run `deckle serve --bind <ip>`.
2. On the Kindle (same Wi-Fi), record model and firmware (Settings → Device Options → Device
   Info). Open the browser, type the printed URL, bookmark it.
3. **KFX:** tap the `.kfx` link. Record: does it offer the download or refuse it? If it
   downloads: does the book appear in the library? With its cover? Does it open, with
   Enhanced Typesetting options in `Aa`?
4. **The `.azw` variant:** tap "as .azw" for the same book (delete the copy from step 3 first).
   Same questions. (Same bytes under an extension the browser accepts; whether the Kindle then
   indexes a KFX container named `.azw` is the unknown.)
5. On the Mac, `deckle digest push` with the Kindle plugged in still delivers the edition (the
   outbox was not touched).
6. Record the result in `docs/devices.md` and in this section.

**Outcomes.** KFX (or the `.azw` variant) lands with its cover: merge, and the follow-up adds
the app toggle. Both refused, or the book lands without a cover: don't merge as is. The
fallback is serving an AZW3 or MOBI build of the edition, which means lifting the v1 non-goal
"Producing AZW3/MOBI" for the digest (owner call, separate issue). The token and server code
stay as they are; only what the outbox holds changes.

**Result:** _pending._

## 8. Calls made in this draft (owner to check)

- **The token persists** in `serve.token` beside the outbox (`<cache dir>/serve.token`, 32 hex
  characters, made on first use), like the bookmarklet's. Why: the URL is typed once on a
  Kindle keyboard and bookmarked; a new token per run would mean retyping 60 characters every
  morning. `--new-token` rotates it.
- **Wildcard binds refused.** `--bind 0.0.0.0` is technically explicit, but it is exactly
  "every interface", which the call ruled out.
- **The `.azw` alias** is served for every listed `.kfx` so the spike can test both names
  without a rebuild. If the spike shows it's useless, it goes.
- **`--dir`** serves another folder of `.kfx` files instead of the outbox, for the spike and
  for books built with `--to <folder>`.
- **Port 47220** by default, next to the bookmarklet's.
