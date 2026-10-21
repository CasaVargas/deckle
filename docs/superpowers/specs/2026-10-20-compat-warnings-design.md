# Firmware-aware compatibility warnings — design

**Status:** plan settled on the issue (#46, owner call 2026-10-20) · **Date:** 2026-10-20 ·
**Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-18-deckle-design.md` (§5 cover rule, §6 device profiles: warn, never refuse)

Deckle knows a few things about particular Kindles that the owner should hear before a book goes
wrong on them: an MTP Kindle can't show a cover for an EBOK book without a real ASIN, Word Wise
on an MTP Kindle needs the dictionary staged first, a Scribe 2024 drops off the bus on a USB
reset. Today those facts live only in `docs/devices.md`. This puts them in a table Deckle checks
against the connected Kindle, its model and its firmware, and shows the matches in the device
card and in a send's warnings.

## 1. Data source: an embedded table

`deckle_core::compat::RULES`, a `&'static [Rule]` compiled into the binary (option 1 on the
issue). Simple, testable, offline; a data change needs a release, which is fine at this rate of
change. `docs/devices.md` stays the human record; a row is added to the table only once a
device check there records the fact ("How to add a compatibility row" in that file).

## 2. Firmware

A new `firmware: Option<String>` on `Seen` and `DeviceProfile`, saved in `devices.json` with a
serde default, so older files load with `None`.

- **Mass storage:** `system/version.txt` is one line, `Kindle 5.16.2.1.1 (4184940002)`. The
  firmware is the first whitespace token made only of digits and dots with at least one dot
  (`5.16.2.1.1`). `deliver::Device` gains `firmware`; `model_hint` (and so the `ms:` profile key)
  is unchanged.
- **MTP:** the PTP DeviceInfo dataset's `DeviceVersion` string (GetDeviceInfo, 0x1001). It is
  read on the send's own connection, after delivery, never by the 4-second device poll: opening
  and claiming the Scribe's interface while idle, or while a send holds it, is exactly the kind
  of USB traffic it is fragile about. `RunReport.firmware` carries it out; the app stores it on
  the profile, so the device card knows it from the first send on.
- `DeviceRegistry::remember` refreshes `firmware` only when the sighting has one; an MTP sighting
  (always `None`) keeps what a send recorded.

## 3. Rules

```rust
pub struct Rule {
    pub model: Option<&'static str>,       // case-insensitive substring of model_hint; None = any
    pub transport: Option<TransportKind>,  // None = any
    pub firmware: Option<FirmwareRange>,   // inclusive min/max; None = every firmware
    pub feature: Feature,                  // cover | kfx | xray | wordwise
    pub on_send: bool,                     // also a send warning, or the device card only
    pub message: &'static str,
}
```

Some facts key on transport rather than model (every MTP Kindle lacks `system/thumbnails`), so
both are optional matchers and a row matches when every set matcher matches.

- A row with a firmware range never matches unknown or unparseable firmware (no false warnings).
- Firmware comparison is numeric per dot-separated component, missing components are `0`.

`check(model, transport, firmware) -> Vec<Warning>` runs the table; `check_rules` takes any table
(tests, and firmware-range rows before real ones exist).

**Seed rows** (all from `docs/devices.md`; firmware ranges come later from owner device checks):

| Model | Transport | Feature | On send | Message |
|---|---|---|---|---|
| any | MTP | cover | yes | EBOK books without a real Amazon ASIN show no cover on this Kindle: `system/thumbnails` isn't writable over MTP. Type the book's ASIN, or send as PDOC. |
| any | MTP | wordwise | no | Word Wise needs the Kindle's dictionary staged first: `system/` isn't reachable over MTP, so copy `kll.en.en.klld` to `documents/deckle/` (see docs/devices.md). |
| `scribe` | MTP | kfx | no | A USB reset drops the Scribe 2024 off the bus until it's replugged. Deckle never resets it; if another tool does, unplug and replug. |

`on_send: false` rows are device-card only: the pipeline already says "Word Wise skipped: …" when
the dictionary is missing, and Deckle never resets the Scribe, so repeating either on every send
is noise.

## 4. Where warnings appear

- **Pipeline** (`pipeline::run`): after a successful delivery, `on_send` rows that match the
  target are added to `report.warnings`, only when the run used that feature:
  cover → shipped EBOK with a generated ASIN and no Deckle thumbnail; kfx → always;
  xray → an X-Ray sidecar shipped; wordwise → a Word Wise sidecar shipped. A folder target has no
  device and gets none. The send never fails because of a rule (§6).
- **Desktop app:** `DeviceRow` gains `firmware` and `compat` (every matching row's message); the
  Kindles card in Settings shows the firmware and the messages.

## 5. Tests

- `tests/devices.rs`: firmware round-trips through `devices.json`; an old file without it loads.
- `tests/deliver.rs`: `detect_in` reads the firmware from a fixture `system/version.txt`.
- `compat` unit tests: in-range firmware warns; out-of-range, unknown and unparseable firmware
  don't; transport-only and model-only rows; seed rows hit MTP Kindles and miss mass storage.
- `deckle-transport`: DeviceInfo round-trips; `Mtp::device_info` over a scripted transport.
- Pipeline: the cover warning is added for an MTP EBOK send with a generated ASIN, not for PDOC.
