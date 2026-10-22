# .yjf fixtures

Kindle reader data store (KRDS) sidecars pulled read-only from a Kindle Scribe (2024, MTP) on
2026-10-22. They hold reading positions, page-turn history and reading-speed samples, nothing
personal.

- `daily-2026-10-21.yjf`: from `deckle-daily-2026-10-21.sdr/`. `fpr` and `lpr` both 103303 (a
  contents jump to "On This Day"), `lpr` time 2026-10-22T02:06:30Z.
- `daily-2026-10-20.yjf`: from `Deckle - The Deckle Daily -- Tuesday, October 20, 2026.sdr/`.
  `fpr` 195894 ahead of `lpr` 153130.
- `book.yjf`: a sideloaded book's. `fpr` 56657, `lpr` 54797.

Decoded with `deckle_readership::parse_yjf` (a port of `krds.py`, K-R-D-S/KRDS).

`book.yjf` also carries `book.info.store` (words read 9519, the Kindle's fraction 0.15048…) and
`timer.average.calculator` (four pace samples) inside `timer.model`. The Kindle's own progress for
that book hasn't been compared with the fraction yet; see `docs/devices.md` (From Your Shelf).
