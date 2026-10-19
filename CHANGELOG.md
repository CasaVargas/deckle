# Changelog

## 0.1.0 — first release (pre-alpha)

- EPUB → KFX with Enhanced Typesetting through Kindle Previewer 4, repackaged by a Rust port of
  kfxlib (byte-exact against its output); approximate page numbers.
- Cleans EPUBs for Previewer: JPEG cover marked `cover-image`, large images downscaled, scripts
  and remote resources removed, a TOC when the book has none; font-obfuscated (DRM-free) EPUBs
  are accepted, DRM is refused.
- The cover rule: EBOK plus a Deckle-written thumbnail on mass-storage Kindles (restored after
  Amazon's "no image" overwrite), PDOC on MTP-only Kindles and folder exports; per-book and
  per-Kindle overrides; optional Amazon edition matching through Open Library.
- Delivery to mass-storage Kindles and, through Deckle's own PTP/MTP client, to MTP-only
  Kindles (Scribe 2024, Colorsoft, Paperwhite 2024); export to a folder.
- X-Ray sidecars: heuristic people and terms, optional LLM descriptions (Anthropic or any
  OpenAI-compatible endpoint), optional Wikipedia summaries.
- Word Wise sidecars from Deckle's Wiktionary-derived dictionary (downloaded once, 12 MB),
  pointing at the Kindle's own dictionary when present.
- Desktop app for macOS 14+ and Windows 11 (Tauri); `deckle` command line for everything.
- macOS builds are signed with a Developer ID and notarized; Windows builds are unsigned in v1.
- Known gaps: not yet verified on real Kindles (see `docs/devices.md`).
