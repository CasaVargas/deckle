# M2: Covers and devices — Implementation Plan

**Goal:** Every book shows its cover on both owner Kindles; X-Ray-capable (EBOK) output wherever the cover rule allows; Deckle talks to MTP-only Kindles (Scribe 2024) directly.

**Spec:** docs/superpowers/specs/2026-10-18-deckle-design.md §4.3, §4.5 (thumbnail), §4.6, §5, §6, §10 spikes 2–3.

**Facts gathered (2026-10-19):**
- Kindle thumbnail: `system/thumbnails/thumbnail_<content_id>_<EBOK|PDOC>_portrait.jpg`, JPEG, height 500 (330 on older Paperwhites, 500 verified on PW3+). Source: calibre `devices/kindle/driver.py`, `metadata/kfx.py` (`content_id` else `ASIN`).
- Amazon cover bug: an EBOK whose ASIN is unknown to the store gets its thumbnail overwritten by a 60×40 "no image" file when the Kindle goes online. Calibre keeps a copy (`amazon-cover-bug/`) and restores it on every connect (MobileRead t=329945). Deckle keeps `deckle-covers/` on the device and re-syncs on every Send.
- Scribe/Paperwhite 2024 MTP: USB 1949:9981. libmtp #436: the device exposes two nameless root folders that trip libmtp's 64-bit ObjectSize heuristic; `SendObjectInfo` must always use the 32-bit ObjectInfo layout. Writes to `documents/` work. Only one program may hold the MTP session (close Android File Transfer / Send to Kindle).
- Open Library search: `https://openlibrary.org/search.json?title=…&author=…&fields=key,title,author_name,isbn&limit=5`; ISBN-10 doubles as Amazon's print ASIN.

## Tasks (TDD each; commit per task)
1. **Cover rule** `deckle-core::cover_rule::decide(DeviceClass, MatchResult, Override) -> Decision{cde, write_thumbnail, reason}` — the §5 table as tests.
2. **Thumbnails** `deckle-core::thumbnail`: `render(cover_bytes, height=500) -> jpeg`, `thumbnail_name(content_id, cde)`, `install(device_root, name, jpeg)` writing `system/thumbnails/` + `deckle-covers/`, `sync(device_root)` restoring missing/overwritten ones. Deliver gains `DeliverOptions{thumbnail: Option<Vec<u8>>}`; KFX cover bytes come from the converted book (`deckle-kfx::container` metadata).
3. **Device profiles** `deckle-core::devices`: `DeviceClass::{MassStorage, Mtp}`, `DeviceProfile` persisted in `devices.json` (root/serial, model_hint, transport, thumbnails_writable, first_seen, override: Option<Cde>), `remember(&mut profiles, &Device)`.
4. **Match** `deckle-core::matcher`: `MatchResult{asin, confidence, source}`; `match_book(info, fetch: &dyn Fn(&str)->Result<String>)` with ISBN → ISBN-10 conversion, else Open Library title+author; canned-JSON tests; `ureq` for the real fetcher, opt-in.
5. **MTP transport** `deckle-transport` crate: pure-Rust PTP over `rusb` (vendored libusb): container codec, ObjectInfo dataset (32-bit size only), `MtpDevice::{open, storages, find_folder("documents"), send_file(parent, name, bytes), list(parent)}`; packet-level unit tests; live path gated on a connected device (`DECKLE_MTP_LIVE=1`). `detect_mtp()` lists 1949:* devices.
6. **Wire-up**: `Target::Mtp(MtpInfo)`; pipeline picks cde/thumbnail from the cover rule; CLI `devices` lists both transports; GUI device bar shows transport and the rule's badge per book; settings `amazon_match` becomes live.
7. **Docs**: `docs/devices.md` rows, `docs/formats/kfx.md` thumbnail section; spikes 2–3 runbook for the owner.

Hardware-gated (owner): spike 2 (real ASIN on the Scribe), spike 3 (MTP write from macOS).
