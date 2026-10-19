# KFX / KPF notes (what we learned, 2026-10-18)

Working notes for the Rust port in `crates/deckle-kfx`. The format is undocumented; the
authority is kfxlib (GPL-3) and a real device. Confidence tags: **device** / **reference** /
**inferred**.

## Kindle Previewer 4 CLI (reference)

- Binary: `Kindle Previewer 4.app/Contents/MacOS/KindlePreviewer4CLI` (macOS). Windows path
  to be confirmed (v3 lived in `%LOCALAPPDATA%\Amazon\Kindle Previewer 3\`).
- `KindlePreviewer4CLI <in.epub> --convert --locale en --output <dir>`. **The output dir must
  already exist**, or the CLI prints "Output should point to a valid folder." and exits 0 with
  no files.
- Writes `<dir>/KPF/<name>_epub.kpf`, `<dir>/Logs/<name>_epub_log.csv`, `<dir>/Summary_Log.csv`.
  Summary columns: `Book Name, Enhanced Typesetting Status, Conversion Status, Error Count,
  Quality Issue Count, Output File Path, Log File Path, Quality Report Path`. ET refused means
  status `Not Supported` and a `.mobi` instead of `.kpf`.
- About 38 s for a 47 MB illustrated novel and 10 s for a 90 KB text-only EPUB on an M4 Mac.
- The pkg payload can be installed per-user: `pkgutil --expand-full KindlePreviewer4.pkg x`
  then `ditto "x/component.pkg/Payload/Applications/Kindle Previewer 4.app" ~/Applications/`.

## KPF (reference)

Zip containing `book.kdf` (SQLite), `book.kcb` (JSON: tool_name, tool_version, source_path),
`book.epub` (Previewer's cleaned copy of the source), `resources/ManifestFile`,
`resources/res/rsrc*` (images, referenced by `path` payloads).

**The KDF SQLite has fingerprint records injected**: starting at offset 1024, a 1024-byte record
beginning `FA 50 0A 5F` appears every 1024 x 1024 bytes. SQLite reports "malformed database
schema" until they are removed (kfxlib `SQLiteFingerprintWrapper`). Strip them, then open.

Tables: `fragments(id, payload_type, payload_value)`, `fragment_properties(id, key, value)`
(key `element_type`), `capabilities(key, version)`, `gc_*` (ignore). Special `fragments` rows:
`$ion_symbol_table` or `max_id` (symbol table import), `max_eid_in_sections`. `payload_type`
is `blob` (Ion binary, or raw bytes for `$417`) or `path` (resource file). Fragment type is the
Ion annotation of the value; fid is the row id unless the type is a root type.

## Symbols (reference)

System table `$ion` has 9 symbols; shared table `YJ_symbols` version 10 has 850 (`$10` to
`$859`; entries ending in `?` are unobserved placeholders, strip the `?`). Local symbols start
at id **860**. The container's `$ion_symbol_table` fragment lists imports with `max_id` 850 and
the local `symbols`; the serialized max_id adds the 9 system symbols.

## Monolithic KFX container (reference, kfxlib `KfxContainer.serialize`)

```
"CONT" u16 version=2  u32 header_len  u32 cinfo_off  u32 cinfo_len
entity table: per entity  u32 id_idnum  u32 type_idnum  u64 offset  u64 length
doc symbols (Ion, the $ion_symbol_table fragment)
[format capabilities (Ion), only if local_min_id > 595]
container info (Ion struct): $409 id, $410 0, $411 0, $413 table_off, $414 table_len,
                             $415 symtab_off, $416 symtab_len, $412 4096 [, $594/$595]
kfxgen_info JSON-ish text: [{key:"kfxgen_package_version",...},{key:"kfxgen_acr",value:id}]
entities: "ENTY" u16 1  u32 header_len  Ion{$410:0,$411:0}  payload (Ion, or raw for $417/$418)
```

Entity id for singleton fragments is symbol `$348`. Container id is `CR!` + 28 `[A-Z0-9]`.
Books over 16 MB get a kfxlib warning ("may be too large for some devices"). Pride and
Prejudice with 164 Gutenberg images hit 25 MB, so Normalize must downscale images.

## Fix-ups needed to turn a KPF into a device KFX (reference, kfxlib `fix_kpf_prepub_book`)

In order: resource ids prefixed `resource/`; `$608` structures inlined into `$146` lists; eid
symbols translated to ids where `EID_REFERENCES` says so; `yj.*` keys stripped; floats to
decimals; style name fix; `$262` fonts re-homed to `$418`; `$490` metadata categories ensured
(`asset_id`, `is_sample`, `override_kindle_font`, `cover_image`); `$585` content features
(`CanonicalFormat`, `yj_hdv`, ...); `$389` navigation ensured, `page_list_entry:` landmarks
transformed into a `$237` page list; `$145` content fragments created from `$259` strings
(8192-byte chunks); `$264`/`$265` position maps and `$550` location map generated; `$395`
added; `$593` format capabilities rewritten (`kfxgen.positionMaps`, `kfxgen.pidMapWithOffset`,
`kfxgen.textBlock`); then a fragment-usage walk (drop unreferenced, error on missing), `$270`
rewritten as "KFX main", `$419` entity map rebuilt, symbol table rebuilt from used locals.
The approximate page list (`APPROXIMATE_PAGE_LIST`) is optional and is what gives real page
numbers to a sideload.

## Metadata (reference)

`$490.$491[kindle_title_metadata].$258` is a list of `{ $492: key, $307: value }`, sorted by
key. Keys we set: `ASIN`, `content_id` (= ASIN), `cde_content_type` (`PDOC` or `EBOK`),
`title`, `author` (one per author, sort form), `language`, `cover_image` (resource symbol).
Cover must be JPEG/JFIF (`FF D8 FF E0`) for the lockscreen.

## Learned while porting (2026-10-18, Tasks 3–13)

- The KFX container stores the shared-table import `max_id` **inflated by the 9 system
  symbols** (859 on disk for a real 850); the KDF stores the raw 850. Local symbols start at 860
  either way.
- Unknown symbol ids decode to `$N` placeholders and `$N` always encodes to id N (kfxlib
  `get_symbol`/`get_id`); byte-exact re-encoding depends on this.
- `$264` section eid lists come from a Python `set`; their order is CPython's hash-table order
  (`cpython_int_set_order`). The device does not care, the oracle does.
- Floats are stored as decimals from `"%g" % x` (6 significant digits), `|x| < 0.001` → 0.
- kfxlib with `authors=[]` (the default `YJ_Metadata`) **removes** the KPF's author entries; the
  goldens therefore have no author. Deckle stamps authors in sort form (`Last, First`).
- `$490` kindle_title_metadata entries are sorted by key after stamping (uppercase `ASIN` first).
- The mini KDF has exactly one fingerprint record; records recur every 1 MiB of the *stripped*
  stream.
- The approximate page list (`$391::{$235:$237,$239:APPROXIMATE_PAGE_LIST}`) is derived from the
  position-id map plus storyline text: 1850 positions per page, backing up to a whitespace
  boundary by at most 50 characters. 45 pages for the mini book, 435 for Pride and Prejudice.
- Kindle Previewer 4's `--help` banner line `Kindle Previewer 4.0.3 Copyright (c) Amazon.com` is
  the version probe; the per-book log CSV has two legend lines before the `"Type"` header.
