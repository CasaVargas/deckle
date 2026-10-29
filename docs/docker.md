# Deckle in a container

The Daily on a home server: a small Linux container that builds your paper every morning and
emails it to your Kindle, with nothing running on your Mac or PC. This page covers Docker,
Docker Compose and Unraid. [docs/daily.md](daily.md) has every setting; this page is only what
differs in the container.

## What it does and doesn't do

- **Does:** the whole Daily. Sources, sections, the front page, the puzzle, the Morgue, your saved
  reading (the bookmarklet and the Inbox folder), and export folders. The paper is built on the
  schedule you set.
- **Kindle, by email only.** A container has no USB Kindle, so the paper reaches a Kindle through
  [Email to Kindle](daily.md#email-to-kindle-opt-in). It builds without Kindle Previewer.
- **No Kindle book conversion.** Enhanced Typesetting, X-Ray and Word Wise need Kindle Previewer,
  which only runs on Mac and Windows. Books stay on the desktop app.
- **No keychain.** Passwords come from environment variables (below).

## Quick start

Copy [`docker/compose.example.yml`](../docker/compose.example.yml) to `compose.yml`, set `TZ`
and the two paths, then:

```sh
docker compose up -d
docker compose logs deckle
```

The image is `ghcr.io/casavargas/deckle`. `:latest` is the newest published release; `:<version>`
(for example `:0.10.0`, no leading `v`) pins one.

| Setting | Default | |
|---|---|---|
| `PUID`, `PGID` | `99`, `100` | The user and group files are written as. Unraid's `nobody:users`. |
| `TZ` | `UTC` | Papers print on this zone's clock. Use a name like `America/New_York`. |
| `DECKLE_SMTP_PASSWORD` | unset | The password for `[email]`. |
| `DECKLE_SITE_PASSWORD_<SITE>` | unset | A paper you subscribe to ([below](#email-to-kindle-and-passwords)). |
| `DECKLE_BOOKMARKLET_URL` | unset | The address your browser reaches the bookmarklet at ([below](#the-bookmarklet-over-your-network)). |

| Folder | Holds |
|---|---|
| `/config` | `deckle/digest.toml`, papers, state |
| `/data` | `Deckle/`: the Morgue, the Inbox, export folders |

Port `47219` is the bookmarklet's listener. Publish it only if you use the bookmarklet.

With plain `docker run`, add `--stop-timeout 90`. A running edition gets up to 60 seconds to
finish when the container stops; the compose file's `stop_grace_period: 90s` covers that and
the watcher's shutdown.

The entrypoint sets the time zone, creates the `PUID`/`PGID` user, and runs Deckle as that user.
It changes the owner of the top of `/config` and `/data`, and only when it differs. If you change
`PUID` or `PGID` on an existing install, `chown -R` the folders yourself.

## Unraid

The template is [`docker/unraid/deckle.xml`](../docker/unraid/deckle.xml). It maps `/config` to
`/mnt/user/appdata/deckle` and `/data` to `/mnt/user/deckle`, sets `PUID` 99 and `PGID` 100, and
gives the stop timeout. Pick your `TZ` and, for email, fill in the masked password field. There
is no WebUI button yet; you set Deckle up by editing a file.

## First start

The first start writes a starter `/config/deckle/digest.toml` and says so in the log:

```
deckle-entrypoint: wrote a starter /config/deckle/digest.toml — edit it, then restart
```

Edit it (your place, feeds, the `[schedule]` time, `[email]`), then restart the container. Its
settings are the same as the desktop app's ([daily.md](daily.md)). Papers and the Morgue go under
`/data/Deckle/` unless you point them elsewhere.

## The schedule and catch-up

`deckle digest daemon` is the container's command. It's a clock for the container, where there's
no launchd or Task Scheduler. A paper prints at its `[schedule] at`, on time to within 15
minutes, at any hour.

If the container was down at press time, it catches up a missed edition after a restart, but only
before 12:00 local time, and never sends a second copy of a paper it already printed.

Change the time without editing the file by hand:

```sh
docker exec deckle deckle digest schedule --at 05:30
docker exec deckle deckle digest schedule --off
docker exec deckle deckle digest stop        # skip the next paper; --undo restarts the presses
```

`schedule` edits `digest.toml` and the daemon picks the change up within a minute. Nothing is
installed in the container.

Start-up also lists any configured path that can't exist in the container, such as a Mac or
Windows folder. The paper still builds; fix the line in `digest.toml`.

## Email to Kindle and passwords

Follow [Email to Kindle](daily.md#email-to-kindle-opt-in) for the `[email]` block and for adding
the `from` address to Amazon's Approved Personal Document E-mail List. The one difference is the
password. The container never uses a keychain, so:

- Put the SMTP password in `DECKLE_SMTP_PASSWORD`.
- `deckle digest email password` and `deckle digest subscribe` refuse to store a password in the
  container and name the variable to set.
- For a [paper you subscribe to](daily.md#papers-you-subscribe-to), add the `[[subscriptions]]`
  block to `digest.toml` by hand, and set `DECKLE_SITE_PASSWORD_<SITE>`. The name is the site in
  capitals with every other character turned into `_`: `gazette.example` is
  `DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE`.

`docker exec deckle deckle digest email test` sends the test page.

## The bookmarklet over your network

Copy the bookmarklet from `docker exec deckle deckle save --bookmarklet`, or from the desktop
app. By default it points at `127.0.0.1:47219`, which is your own computer, not the server. To
use it from another machine:

1. Publish the port (`"47219:47219"` in compose).
2. Set `DECKLE_BOOKMARKLET_URL` to the address your browser uses. It must be a bare origin,
   `scheme://host[:port]`: no path and no trailing slash. For example `http://tower.local:47219`.
3. Copy the bookmarklet again, so it carries that address.

The token is still required. Anyone on your network who has it can add to tomorrow's paper, so
don't expose the port to the internet.

A browser may block a call to an `http://` address from an `https://` page. If saving from some
sites fails that way, put the listener behind a reverse proxy with TLS and set
`DECKLE_BOOKMARKLET_URL` to its `https://` address. Or skip the bookmarklet and drop links
(`.webloc`, `.url`, `.txt`) and saved pages into `/data/Deckle/Inbox`.

## Moving from the desktop app

1. Copy the desktop's config folder (the one holding `digest.toml`; `deckle digest init` prints
   its path, and on a Mac it's `~/Library/Application Support/deckle`) to `/config/deckle`.
2. Start the container and read the log. It lists each configured path that can't exist there.
   Re-point each one at `/data/…`.
3. Move passwords into environment variables.
4. So two installs never print or claim the same paper, turn off **Fresh-paper requests**
   (Delivery → Bundle export) on the desktop and stop its schedule: `deckle digest schedule --off`.

Mount your old Morgue under `/data/Deckle` if you want the history.

## Linux without Docker

The release has `deckle-<version>-x86_64-unknown-linux-gnu.tar.gz` and
`deckle-<version>-aarch64-unknown-linux-gnu.tar.gz`: the command line, with PDFium beside it.
They need glibc 2.35 or newer (Ubuntu 22.04, Debian 12) and `libdbus-1-3`. Run
`deckle digest daemon` under your own supervisor, with `DECKLE_DAEMON=1` set so it behaves as
the container does. Kindle book conversion is still unavailable.
Set `DECKLE_BOOKMARKLET_BIND=0.0.0.0` to reach the bookmarklet over your network. `TZ` sets the
daemon's clock.
