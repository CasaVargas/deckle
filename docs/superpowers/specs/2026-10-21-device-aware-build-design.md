# Device-aware build: finish queued books for the Kindle that takes them

Status: draft, 2026-10-21. Issue #72; follows PR #71.

## 1. Why

Some books are built before Deckle knows which Kindle will take them:

- long reads from `deckle digest run`;
- books dropped in the Inbox (#45);
- the paper itself.

They're built into a folder, so the cover rule (design §5) sees `DeviceClass::Folder` and always picks PDOC. The pipeline only builds X-Ray and Word Wise for EBOK. So every queued book reaches the Kindle as a personal document with a cover and nothing else. The same file sent with `deckle run --to device` gets the full treatment: EBOK with Deckle's thumbnail on a mass-storage Kindle, X-Ray and Word Wise.

The save-to-paper spec wanted long reads "EBOK with Word Wise where allowed". That needs the device, and the device is only known when it's plugged in.

## 2. Goals

- A queued book gets the same decision `deckle run --to device` would make for the Kindle that takes it: the cover rule, X-Ray and Word Wise.
- Push stays fast: the slow, device-independent step (Kindle Previewer, often 10–60 s) still runs at queue time. Push time only does the pure-Rust work, a few seconds per book.
- On MTP, push stays one connection: the probe, the delivery and the prune share one session, as `sync` (#43) requires.
- A failure at push time never loses the book. If finishing fails, it ships as today (PDOC, no extras), with a warning.

## 3. Non-goals

- The paper stays PDOC with its cover always (design §5). It isn't finished at push time.
- No Amazon edition match at push time. It needs the network, push must work offline, and a long read has no ISBN anyway.
- No LLM or Wikipedia X-Ray at push time: heuristic X-Ray only. Those slow, networked options stay with `deckle run`.
- No re-running Previewer on the device's behalf.

## 4. The design

### 4.1 The pipeline splits at the KPF

`pipeline::run` already runs in this order:

1. inspect
2. normalize
3. **convert** (Previewer → KPF)
4. cover rule
5. **repackage** (`kpf_to_kfx` with `cde_type` and ASIN)
6. enrich (X-Ray, Word Wise)
7. deliver

Everything after step 3 depends on the device, and nothing before it does.

- Split `run` into `prepare(input, opts) -> Prepared` (steps 1–3) and `finish(prepared, conn, opts) -> RunReport` (steps 4–7).
- `run` becomes `finish(prepare(..), Conn::new(target), ..)`. `deckle run` and the GUI behave exactly as before.
- `Prepared` holds:
  - the KPF bytes;
  - the `BookInfo` (title, creators, language, cover);
  - the options that travel: pages, the X-Ray and Word Wise wishes, the CDE override;
  - the warnings so far.

### 4.2 A queue entry can be a prepared book

The outbox (`deckle_core::queue`) holds `<name>.kfx` files today. A prepared book becomes a folder beside them:

```
outbox/
  deckle-daily-2026-10-21.kfx          # finished: delivered as is (the paper)
  deckle-reads-2026-10-21-slug.book/   # prepared: finished at push
    book.kpf
    prepared.json                      # BookInfo + travelling options, versioned
    fallback.kfx                       # the PDOC build, used if finishing fails
```

- `Queue::pending()` lists both kinds, ordered by name as today.
- `fallback.kfx` is the build every queued book gets today. Producing it costs one `kpf_to_kfx`, which is fast, and it means a broken `finish` can never strand a book.
- `prepared.json` has a `version`. An entry with an unknown version is delivered from `fallback.kfx`.

### 4.3 Finishing at push

`Queue::flush_on(conn, prune)` already delivers every pending entry over the caller's connection. For a prepared entry it now:

1. Takes the device class and profile from the connection's target and `devices.json`: `thumbnails_writable`, `verified_real_asin` and the #46 compat rows.
2. Calls `pipeline::finish` with that connection, so the Word Wise dictionary probe (`wordwise::probe_on`) uses the same MTP session.
3. On error, delivers `fallback.kfx` under the same name, then reports a warning naming the stage that failed.
4. Removes the entry once it's delivered, as today.

For a folder target (`--to <folder>`), `finish` with `DeviceClass::Folder` gives PDOC, which is today's output.

### 4.4 Names and replacement

- The device file name is the queue name (`deckle-reads-…​.kfx`). The `.sdr` sidecar folder follows that name, so prune (`prune_named_on`) already removes it with its book.
- `Queue::add` with `replaces` removes prepared entries by prefix as well as `.kfx` files.

### 4.5 Who queues prepared books

- **Long reads in `digest run`:** they ask for X-Ray Auto and Word Wise at the default level, which is what #71 removed because it had no effect.
- **Inbox books (#45):** they ask for X-Ray Auto and Word Wise at the default level, for the same reason.
- **The GUI's "send when connected":** the GUI has no outbox today. That's out of scope here, and the API is ready for it.

## 5. Calls made in this draft (owner to confirm)

1. **Split at the KPF**, not at the source EPUB. Converting at push time would put Previewer, the slow step, inside the plug-in moment and inside an open MTP session. The Scribe is fragile about long sessions.
2. **Heuristic X-Ray only at push time.** It's offline and fast. LLM descriptions stay with `deckle run`.
3. **Always keep a PDOC fallback.** It costs about 2× the disk per queued book and guarantees nothing is lost.
4. **Word Wise follows the existing dictionary rules** (`wordwise::build`):
   - the Kindle's own English dictionary when it's readable;
   - otherwise Deckle's in the 中文 slot.

   On the Scribe, Word Wise needs the dictionary staged first; until then it's skipped with the existing warning.
5. **The lexicon database must already be cached.** Push doesn't download it. If it's missing, Word Wise is skipped with a warning, and the next `deckle run` with Word Wise downloads it.

## 6. Open questions for the owner

- **Paper:** should it stay PDOC forever? EBOK with Deckle's thumbnail would keep the cover on a mass-storage Kindle and allow Word Wise. On MTP Kindles it would lose the cover, so it's a per-device trade-off. This draft says PDOC forever.
- **Limit per push:** should there be a limit on how many prepared books finish in one push? A backlog of 20 long reads at about 3 s each is a minute on the cable. This draft has no limit and shows progress on the CLI.

## 7. Tests the build must add

- `prepare` then `finish` to a folder gives byte-for-byte the same KFX as `run` (regression).
- A prepared entry flushed to a fake mass-storage Kindle with writable thumbnails: EBOK, thumbnail written, X-Ray `.sdr` present.
- The same entry flushed to the fake MTP Kindle (the PTP responder from #43):
  - exactly one session;
  - the dictionary probe and the sends in that session;
  - prune last.
- A `finish` failure, such as a corrupt KPF or an unknown `prepared.json` version, delivers `fallback.kfx` under the same name, with a warning.
- `replaces` removes prepared entries by prefix.
- Push with no network and no cached lexicon: the book ships with X-Ray, Word Wise is skipped with a warning.

## 8. Sizing

M–L:

- the pipeline split is mechanical, but `run` is the core path;
- the queue gains a second entry kind;
- flush gains a device-profile lookup.

No UI work beyond what `sync` already shows.
