# Several Kindles in one home — design

**Status:** scope settled by the owner on 2026-10-25 for [#37](https://github.com/CasaVargas/deckle/issues/37)
("Multi-Kindle household profiles") ·
**Owner:** Jonathan (CasaVargas LLC)
**Builds on:** device-aware build (the outbox, `queue.rs`), Daily `[sync.devices]`, the device
registry
**Related:** #110 (one paper per Kindle) reuses the outbox ownership defined here

A household with two or three Kindles can send a book to *Mom's Kindle*, even while it's in her
bag, and that book waits in the outbox until her Kindle connects. Today the first Kindle to
connect takes everything queued, and `deckle run --to device` takes whichever Kindle it finds
first.

## 1. Decisions (owner, 2026-10-25)

- **Routing only.** A book can be meant for one Kindle, and only that Kindle takes it. Nothing new
  is written on the Kindle: no collections or shelves. That stays with the v1
  library-management non-goal.
- **Chosen per send.** The sender picks the Kindle: `--kindle` in the CLI, a named Inbox folder,
  or the picker in the desktop app. No rules by author, folder or tag. The default stays "any
  Kindle", so a one-Kindle home sees no change.
- **The Daily paper is #110's.** The paper, its long reads and Morgue resends stay as they are:
  any Kindle that takes the outbox gets them, and each Kindle's `push` switch still applies.
  #110 can mark them for a Kindle with the same ownership tag.

## 2. Rules

**Identity and names.**
- A Kindle's id is the one Daily already uses (`sync::device_id`):
  - the serial over MTP;
  - `<model>/<volume>#<serial>` on mass storage, or `<model>/<volume>` when the serial is
    unknown.
- Its name stays where it is today, in `[sync.devices."<id>"] name`. There is one source of names.
- Two ids are the same Kindle when they are equal or carry the same serial. That covers one
  Kindle seen over MTP and over mass storage, and an older entry written without the serial.
- A Kindle is named by:
  - `deckle devices name "<name>"`, for the connected Kindle; with several connected,
    `--kindle <current name or id>` picks one;
  - a Name field on the desktop's This Kindle view.

  Both write `[sync.devices."<id>"] name` with the config's existing comment-keeping save.
  Names must be unique, ignoring case. A clash is refused.

**Choosing a Kindle.** `--kindle <name or id>` is matched, ignoring case, against:
1. the connected Kindles, by their names and ids;
2. the named Kindles in `[sync.devices]`.

An unknown value is refused, with the list of known names.

**Sending.**
- `deckle run … --kindle "Mom's Kindle"`:
  - when that Kindle is connected, the book is delivered to it, as `--to device` does today;
  - when it isn't, each book is prepared (converted up to the KPF, as the Inbox does) and
    queued for it. The run says "queued for Mom's Kindle".
- `--kindle` implies `--to device`. With `--to <folder>` it is refused.
- `deckle run --to device` without `--kindle` and with several Kindles connected still takes the
  first one, but now says which one and how to choose another.

**The outbox.**
- A queued entry can carry an owner: a `<entry>.for` file beside it holding the Kindle's id.
  An entry without one is anyone's, as today.
- A flush to a connected Kindle delivers the entries that are anyone's or its own, and leaves
  the others queued.
- The flush report lists what is waiting and for whom: "2 books wait for Mom's Kindle", in
  `push`, `watch` and the desktop's push notice.
- Replacing or removing an entry removes its `.for` file too.

**The Inbox.** A book dropped in a subfolder of `~/Deckle/Inbox` named after a Kindle (the
folder name matched like `--kindle`) is queued for that Kindle.
- `Sent/` and `Failed/` keep their meaning.
- Any other subfolder is left alone, with a note in the scan report.
- Articles in a Kindle's folder go to the reading queue as they do today. Articles have no owner,
  because they become the paper.

**The desktop app.**
- The "Target Kindle" picker shows each connected Kindle's name ("Mom's Kindle — Paperwhite"),
  else its model as today.
- This Kindle gets the Name field.
- Queuing for a Kindle that isn't connected is CLI and Inbox only in this increment. The desktop
  sends only to connected Kindles today, and giving it an outbox for books is its own piece of
  work.

## 3. Where it sits

| Area | Change |
| --- | --- |
| `deckle-core::queue` | `Queue::set_owner`/`owner`, owner-aware `flush_with`; `FlushReport::waiting` |
| `deckle-core::sync` | `same_kindle(a, b)`; `run_with` passes the connected id to the flush |
| `deckle-digest::config` | `SyncConfig::set_device_name`, uniqueness, and `find_device(name)` |
| `deckle-digest::inbox` | Kindle-named subfolders, through the existing `send_later` callback with the owner |
| `deckle-cli` | `run --kindle`; the `devices` listing shows names and ids; `devices name`; push and watch report what waits |
| desktop | picker labels, the This Kindle Name field |

## 4. Verification

- **Core:**
  - an owned entry isn't delivered to another Kindle, stays queued, and is reported as waiting;
  - it is delivered to its own Kindle, over mass storage and over MTP by serial;
  - unowned entries behave as before;
  - replacing an entry drops its `.for`.
- **Config:** a name round-trips through the save with the comments kept; a clashing name is
  refused; lookup ignores case and falls back to the id.
- **Inbox:** a book in `Inbox/Mom's Kindle/` is queued with that owner; an unknown folder is
  noted and left alone; `Sent/` and `Failed/` are ignored.
- **CLI:**
  - `run --kindle` with the named Kindle connected delivers there and not to the other
    connected one (two fake mass-storage Kindles);
  - with it absent, the book is queued with the owner, and `push` to the other Kindle leaves it;
  - an unknown name is refused with the known names;
  - `devices name` writes the config.
- **Desktop:** the picker label and naming command have unit tests; the This Kindle field is
  checked in the running app.
- **Unchanged:** one Kindle and no names behave exactly as before; all existing tests pass.
