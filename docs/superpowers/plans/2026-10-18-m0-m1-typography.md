# Deckle M0 + M1 (Typography) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From the CLI, turn an EPUB into a monolithic KFX with Enhanced Typesetting, stamped PDOC or EBOK with an ASIN, and copy it to a folder or a mass-storage Kindle; plus a Tauri shell that opens.

**Architecture:** A Cargo workspace. `deckle-kfx` is an isolated, fixture-heavy Rust port of the KPF-to-KFX path of kfxlib (Ion codec, symbol table, KDF reader, fix-ups, container writer), tested byte-for-byte against kfxlib goldens. `deckle-core` holds the pipeline stages (Inspect, Normalize, Convert via Kindle Previewer 4 CLI, Deliver) and a `run` orchestrator. `deckle-cli` exposes every stage. `apps/desktop` is the Tauri 2 shell (window only in this plan).

**Tech Stack:** Rust stable (edition 2024), `rusqlite` 0.40 (bundled), `zip` 9, `roxmltree` 0.21, `image` 0.25, `clap` 4, `serde`/`serde_json`, `thiserror`, `tempfile`, `csv` 1; Tauri `2.12.4` / `tauri-build` 2.7.3 / `@tauri-apps/cli` 2.12.3 / `@tauri-apps/api` 2.12.4, React 19.3, Vite 8, TypeScript, pnpm.

**Spec:** `docs/superpowers/specs/2026-10-18-deckle-design.md` (sections 3, 4.1, 4.2, 4.4, 4.6, 9, 10, 11, 12)

## Global Constraints

- License header on every source file: `// SPDX-License-Identifier: GPL-3.0-or-later`.
- No Python, Calibre, or interpreter at runtime. `tools/ref/*.py` is dev-only oracle tooling.
- Kindle Previewer is detected, never bundled. Primary target 4.x; CLI flags are double-dash.
- Output is KFX only. A failed conversion is reported with the first useful Previewer log line, never downgraded.
- Deterministic output: all randomness (container id, ASIN generation) goes through injectable options so tests can pin it.
- Platform: macOS 14+ and Windows 11 first class; Linux builds and runs everything except Convert.
- Working name `Deckle`; crate names `deckle-kfx`, `deckle-core`, `deckle-cli`; Tauri app id `com.casavargas.deckle`.
- Commit after every task with a conventional message; push the branch; never commit to `main`.
- Reference implementation for every `deckle-kfx` task: kfxlib in `github.com/frutadoconde/KFX-Input` at commit `57fd6cb3` (`kfxlib/*.py`). Clone it to a scratch dir; the plan cites files and functions, not line numbers.

## Review Focus

Five inputs the spec implies but no happy-path test covers, most likely to bite first. Each has a test pinned to its owning task below.

1. EPUB with DRM (`META-INF/encryption.xml` present): Inspect must refuse with `InspectError::Drm` before Previewer runs (Task 14).
2. Previewer refuses Enhanced Typesetting (summary says `Not Supported`, output is `.mobi`): Convert returns `ConvertError::EnhancedTypesettingUnsupported` carrying the first `Error` row of the per-book log (Task 16).
3. Previewer not installed: Convert returns `ConvertError::PreviewerMissing` with the download URL `https://kdp.amazon.com/en_US/help/topic/G202131170`, and `run` fails before any work (Task 16, 18).
4. Non-ASCII or filesystem-hostile title/author (`Ærø: “Hello”/World?`): device filename is sanitized to ASCII-safe `Author - Title.kfx` (Task 17).
5. KFX over 16 MiB: `run` report carries `warnings: ["KFX is 25.4 MB; some devices lock up above 16 MB"]`, not silence (Task 18).

---

### Task 1: Workspace scaffold, fixtures test, CI

**Files:**
- Create: `Cargo.toml` (workspace), `rust-toolchain.toml` (`stable`), `.rustfmt.toml`, `clippy.toml`
- Create: `crates/deckle-kfx/Cargo.toml`, `crates/deckle-kfx/src/lib.rs`
- Create: `crates/deckle-core/Cargo.toml`, `crates/deckle-core/src/lib.rs`
- Create: `crates/deckle-cli/Cargo.toml`, `crates/deckle-cli/src/main.rs`
- Create: `.github/workflows/ci.yml`
- Test: `crates/deckle-kfx/tests/fixtures_present.rs`
- Already present: `fixtures/mini/*`, `fixtures/README.md`, `tools/ref/*`

**Interfaces:**
- Produces: `deckle_kfx::fixtures::path(name: &str) -> std::path::PathBuf` (cfg(test)/doc-hidden helper resolving `<workspace>/fixtures/mini/<name>` via `CARGO_MANIFEST_DIR`).

- [ ] **Step 1: Write the failing test**

```rust
// crates/deckle-kfx/tests/fixtures_present.rs
#[test]
fn mini_fixtures_exist_with_expected_sizes() {
    let p = deckle_kfx::fixtures::path("deckle-mini.kpf");
    assert_eq!(std::fs::metadata(&p).unwrap().len(), 19241);
    let g = deckle_kfx::fixtures::path("deckle-mini-pdoc-nopages.golden.kfx");
    assert_eq!(std::fs::metadata(&g).unwrap().len(), 93_845);
}
```

(Replace the two byte counts with `ls -l fixtures/mini` values if they differ; they are pinned so a stray regeneration is noticed.)

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p deckle-kfx --test fixtures_present`
Expected: compile error, `deckle_kfx::fixtures` not found.

- [ ] **Step 3: Create the workspace and three crates; implement `fixtures::path`**

Workspace `[workspace] members = ["crates/*"]`, `resolver = "3"`, shared `[workspace.package] edition = "2024", license = "GPL-3.0-or-later", version = "0.1.0"`, `[workspace.dependencies]` pinning the stack above. `deckle-cli` depends on `deckle-core` which depends on `deckle-kfx`. `main.rs` prints `deckle 0.1.0` for now.

- [ ] **Step 4: Run tests and make sure they pass**

Run: `cargo test --workspace`
Expected: 1 passed.

- [ ] **Step 5: CI**

`ci.yml`: on push and PR; matrix `ubuntu-latest`, `macos-latest`, `windows-latest`; steps `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Previewer-gated tests are skipped unless `DECKLE_PREVIEWER` is set (see Task 16), so CI stays green without Amazon software.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "chore: cargo workspace, fixtures helper, CI"
```

---

### Task 2: CLI skeleton

**Files:**
- Modify: `crates/deckle-cli/src/main.rs`
- Create: `crates/deckle-cli/src/cli.rs`
- Test: `crates/deckle-cli/tests/cli.rs` (uses `assert_cmd` 2 + `predicates` 3 as dev-deps)

**Interfaces:**
- Produces: subcommands `inspect <epub>`, `normalize <epub> -o <epub>`, `convert <epub|kpf> -o <kfx> [--cde PDOC|EBOK] [--asin X] [--pages N] [--container-id X]`, `send <kfx> --to <folder|device>`, `devices`, `run <epub>... [--to ...] [--cde ...]`. Each prints a JSON report to stdout when `--json` is passed, human text otherwise. Later tasks fill the bodies; this task wires the parser and a `NotImplemented` exit code 2.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn help_lists_all_subcommands() {
    let out = assert_cmd::Command::cargo_bin("deckle").unwrap().arg("--help").output().unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    for sub in ["inspect", "normalize", "convert", "send", "devices", "run"] { assert!(s.contains(sub), "{sub}"); }
}
#[test]
fn unimplemented_subcommand_exits_2() {
    assert_cmd::Command::cargo_bin("deckle").unwrap().args(["devices"]).assert().code(2);
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p deckle-cli` — Expected: FAIL (no `--help` subcommands).
- [ ] **Step 3: Implement `cli::Cli` with `clap::Parser` derive, `enum Command`, and `fn main() -> std::process::ExitCode`.**
- [ ] **Step 4: Run tests** — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(cli): subcommand skeleton"`

---

### Task 3: Ion value model and binary decoder

**Files:**
- Create: `crates/deckle-kfx/src/ion/mod.rs`, `crates/deckle-kfx/src/ion/value.rs`, `crates/deckle-kfx/src/ion/decode.rs`
- Test: `crates/deckle-kfx/tests/ion_decode.rs`
- Reference: `kfxlib/ion.py`, `kfxlib/ion_binary.py` (`deserialize_value`, `VALUE_DESERIALIZERS`, `deserialize_vluint`, `deserialize_struct_value`, `deserialize_annotation_value`, `deserialize_decimal_value`, `deserialize_timestamp_value`)

**Interfaces:**
- Produces:
  ```rust
  pub enum Value { Null, Bool(bool), Int(i64), BigInt(i128), Float(f64), Decimal(Decimal), Timestamp(Timestamp),
                   Symbol(Sym), String(String), Clob(Vec<u8>), Blob(Vec<u8>), List(Vec<Value>), SExp(Vec<Value>),
                   Struct(Struct), Annotated { annots: Vec<Sym>, value: Box<Value> } }
  pub struct Sym(pub String);                        // symbol text, e.g. "$490" or "content_1"
  pub struct Struct(pub Vec<(Sym, Value)>);          // insertion-ordered; get(&str), insert, remove
  pub struct Decimal { pub coefficient: i128, pub exponent: i32 }
  pub struct Timestamp { /* fields per Ion spec; only needed for completeness */ }
  pub fn decode_single(data: &[u8], symtab: &SymbolTable) -> Result<Value, IonError>   // expects BVM E0 01 00 EA prefix
  pub fn decode_annotated(data: &[u8], symtab: &SymbolTable, expect: Option<&str>) -> Result<(Vec<Sym>, Value), IonError>
  ```
  Symbol ids resolve through `SymbolTable` (Task 5); for this task use a `SymbolTable::yj_default()` stub that maps `10..=859 -> "$N"` and `1..=9` to the system symbols, which Task 5 replaces.

- [ ] **Step 1: Write failing tests from hand-built byte vectors**

```rust
#[test] fn decodes_int_string_symbol_list_struct() {
    // E0 01 00 EA | struct: DE 0x.. ... use these minimal vectors:
    assert_eq!(decode_single(&hex("E00100EA 2105"), &st()).unwrap(), Value::Int(5));
    assert_eq!(decode_single(&hex("E00100EA 83616263"), &st()).unwrap(), Value::String("abc".into()));
    assert_eq!(decode_single(&hex("E00100EA 71 0A"), &st()).unwrap(), Value::Symbol(Sym("$10".into()))); // sid 10
    assert_eq!(decode_single(&hex("E00100EA B4 2101 2102"), &st()).unwrap(), Value::List(vec![Value::Int(1), Value::Int(2)]));
    // decimal 1.5 = coefficient 15 exponent -1 : 52 C1 0F
    assert_eq!(decode_single(&hex("E00100EA 52C10F"), &st()).unwrap(), Value::Decimal(Decimal{coefficient:15, exponent:-1}));
}
#[test] fn decodes_every_fragment_blob_in_mini_kdf() {
    // open fixtures/mini/deckle-mini.kpf -> book.kdf (use Task 6 helper if present; else inline zip+fingerprint strip+rusqlite here)
    // for every row with payload_type='blob' whose value starts with E0 01 00 EA: decode_annotated must succeed
    // expect 82 - 1($417 raw) - 1(symbol table handled separately) >= 75 successful decodes
}
```

- [ ] **Step 2: Run** — `cargo test -p deckle-kfx --test ion_decode` — Expected: FAIL to compile.
- [ ] **Step 3: Implement `value.rs` and `decode.rs`** — port of `ion_binary.py` deserialize side: type descriptor nibbles (0 null, 1 bool, 2 posint, 3 negint, 4 float, 5 decimal, 6 timestamp, 7 symbol, 8 string, 9 clob, A blob, B list, C sexp, D struct, E annotation), length nibble 14 = VarUInt length follows, 15 = null of type; struct fields are VarUInt sid + value; annotations wrapper = VarUInt annots length then VarUInt sids then value; NOP padding (type 0 non-null) skipped.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): Ion value model and binary decoder"`

---

### Task 4: Ion binary encoder, byte-exact against kfxlib

**Files:**
- Create: `crates/deckle-kfx/src/ion/encode.rs`
- Test: `crates/deckle-kfx/tests/ion_roundtrip.rs`
- Reference: `kfxlib/ion_binary.py` serialize side (`serialize_value`, `descriptor`, `serialize_vluint`, `serialize_int_value`, `serialize_decimal_value`, `serialize_struct_value`, `serialize_annotation_value`, `serialize_single_value`)

**Interfaces:**
- Produces: `pub fn encode_single(v: &Value, symtab: &SymbolTable) -> Result<Vec<u8>, IonError>` (BVM prefix included, matching `serialize_single_value`); `pub fn encode_value(v: &Value, symtab: &SymbolTable, out: &mut Vec<u8>)`.

- [ ] **Step 1: Failing test: every entity payload of the golden roundtrips byte-exact**

```rust
#[test] fn golden_entities_roundtrip_byte_exact() {
    let kfx = std::fs::read(fixtures::path("deckle-mini-pdoc-nopages.golden.kfx")).unwrap();
    // parse the CONT header and entity table by hand here (18-byte header, 24-byte table rows: u32 id, u32 type, u64 off, u64 len; offsets relative to end of header_len)
    // for each ENTY whose type is not $417/$418: payload = bytes after ENTY header; decode_single -> encode_single == payload
    // assert 38 of 40 entities compared (two raw types skipped)
}
```

- [ ] **Step 2: Run** — Expected: FAIL (encode_single missing).
- [ ] **Step 3: Implement encoder.** Ints: magnitude big-endian minimal bytes, posint/negint by sign, zero is `0x20`. Decimal: VarInt exponent then Int coefficient (coefficient 0 → empty). Struct: fields in stored order, sid as VarUInt, length nibble 14 for any length ≥ 14 (kfxlib never uses the sorted flag). Strings UTF-8. Lists/annotations nested lengths computed bottom-up.
- [ ] **Step 4: Run** — Expected: PASS, 38 compared.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): Ion binary encoder, byte-exact with kfxlib"`

---

### Task 5: Symbol table and YJ catalog

**Files:**
- Create: `crates/deckle-kfx/src/symtab.rs`, `crates/deckle-kfx/src/yj_symbols.rs` (generated), `tools/ref/gen_yj_symbols.py`
- Test: `crates/deckle-kfx/tests/symtab.rs`
- Reference: `kfxlib/yj_symbol_catalog.py` (`SYSTEM_SYMBOL_TABLE`, `YJ_SYMBOLS` version 10), `kfxlib/ion_symbol_table.py` (`LocalSymbolTable.create`, `import_shared_symbol_table`, `create_local_symbol`, `get_id`, `get_symbol`, `create_import`, `local_min_id`)

**Interfaces:**
- Produces:
  ```rust
  pub const YJ_SYMBOLS_VERSION: u32 = 10; pub static YJ_SYMBOLS: [&str; 850];  // "$10".."$859", '?' stripped
  pub struct SymbolTable { .. }
  impl SymbolTable {
      pub fn yj_default() -> Self;                               // $ion (9) + YJ_symbols(850); local_min_id() == 860
      pub fn from_import(&mut self, imports: &[(String,u32,u32)], locals: &[String]); // from a $ion_symbol_table value
      pub fn create_local(&mut self, text: &str) -> Sym;          // idempotent
      pub fn id_of(&self, s: &Sym) -> Option<u32>;  pub fn symbol_of(&self, id: u32) -> Option<&str>;
      pub fn local_symbols(&self) -> &[String];  pub fn local_min_id(&self) -> u32;
      pub fn import_fragment(&self) -> Value;                     // {max_id, imports:[{name:"YJ_symbols",version:10,max_id:850}], symbols:[..]}
      pub fn replace_locals(&mut self, locals: Vec<String>);
  }
  ```

- [ ] **Step 1: Failing tests**

```rust
#[test] fn default_table_layout() { let t = SymbolTable::yj_default();
  assert_eq!(t.symbol_of(1), Some("$ion")); assert_eq!(t.symbol_of(10), Some("$10")); assert_eq!(t.symbol_of(859), Some("$859"));
  assert_eq!(t.local_min_id(), 860); assert_eq!(t.id_of(&Sym("$348".into())), Some(348)); }
#[test] fn local_symbols_are_numbered_from_860_and_idempotent() { let mut t = SymbolTable::yj_default();
  let a = t.create_local("content_1"); let b = t.create_local("content_1"); assert_eq!(a, b); assert_eq!(t.id_of(&a), Some(860)); }
#[test] fn golden_symbol_table_fragment_reloads() { /* decode the golden's doc-symbols blob (between $415 offset/$416 len) with yj_default, build from_import, assert local_symbols().len()==31 and symbol_of(860)=="APPROXIMATE_PAGE_LIST" for the pdoc golden, and import_fragment() re-encodes byte-exact */ }
```

- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Generate `yj_symbols.rs`** with `tools/ref/gen_yj_symbols.py <KFX-Input>` (reads `YJ_SYMBOLS.symbols`, strips trailing `?`, emits the static array; check the file in). **Implement `symtab.rs`.** Serialized `max_id` in `import_fragment` = 9 + 850 + locals (the golden shows `max_id: 890` with 31 locals).
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): symbol table with YJ_symbols v10 catalog"`

---

### Task 6: KPF/KDF reader

**Files:**
- Create: `crates/deckle-kfx/src/fragment.rs`, `crates/deckle-kfx/src/kpf.rs`
- Test: `crates/deckle-kfx/tests/kpf_read.rs`
- Reference: `kfxlib/kpf_container.py` (`SQLiteFingerprintWrapper.remove`, `open_kpf_database`, `deserialize`, `process_db_fragment`, `prep_payload_blob`, `get_resource_data`), `kfxlib/yj_container.py` (`ROOT_FRAGMENT_TYPES`, `RAW_FRAGMENT_TYPES`, `CONTAINER_FRAGMENT_TYPES`, `KNOWN_FRAGMENT_TYPES`, `PREFERED_FRAGMENT_TYPE_ORDER`)

**Interfaces:**
- Produces:
  ```rust
  pub struct Fragment { pub ftype: Sym, pub fid: Option<Sym>, pub value: Value }   // fid None for root/singleton types
  pub struct FragmentList(pub Vec<Fragment>);  // get(ftype,fid), get_single(ftype), all(ftype), remove, push, sort_preferred()
  pub struct Kpf { pub fragments: FragmentList, pub symtab: SymbolTable, pub kcb: serde_json::Value, pub source_epub: Option<Vec<u8>> }
  pub fn strip_sqlite_fingerprints(kdf: &[u8]) -> (Vec<u8>, usize /*removed*/);
  pub fn read_kpf(path: &std::path::Path) -> Result<Kpf, KpfError>;   // KpfError::{Zip, NoKdf, Sqlite, Drm, Ion, UnexpectedSchema(String)}
  ```
  `read_kpf` appends the `$270` fragment `{ $587:"", $588:"", $161:"KPF" }` and the `$593` capabilities list like kfxlib, and sets `is_prepub` (true unless `$490` already carries ASIN/asset_id/cde_content_type/content_id).

- [ ] **Step 1: Failing tests**

```rust
#[test] fn fingerprint_strip_removes_two_records_from_mini_kdf() { /* unzip book.kdf from the fixture; assert removed == 2 and sqlite header at 0 and output len == input len - 2048 */ }
#[test] fn mini_kpf_fragment_inventory_matches_kfxlib() {
    let k = read_kpf(&fixtures::path("deckle-mini.kpf")).unwrap();
    assert_eq!(k.fragments.0.len(), 82);
    let count = |t: &str| k.fragments.all(&Sym(t.into())).count();
    for (t, n) in [("$157",4),("$164",1),("$258",1),("$259",4),("$260",4),("$270",1),("$389",1),("$417",1),("$490",1),("$538",1),("$550",1),("$585",1),("$593",1),("$597",5),("$608",22),("$609",4),("$610",27),("$611",1),("$ion_symbol_table",1)] { assert_eq!(count(t), n, "{t}"); }
    assert_eq!(k.symtab.local_min_id(), 860); assert_eq!(k.symtab.len(), 944);
    assert_eq!(k.kcb["metadata"]["tool_version"], "4.0.3");
}
#[test] fn drm_kdf_is_refused() { /* craft a KDF-less KPF zip whose fragments blob starts with b"\xeaDRMION\xee" -> KpfError::Drm */ }
```

- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement.** Zip (`zip` crate) → find `*.kdf`, `*.kcb`, `resources/res/*`. Strip fingerprints: at offset 1024, while `data[off..off+4] == FA 50 0A 5F`, drop 1024 bytes, `off += 1024*1024`. Write to a `tempfile::NamedTempFile`, open with `rusqlite` (bundled). Read `sqlite_master` to know which tables exist; `fragment_properties` → element_type map; `fragments` in three passes (`id='$ion_symbol_table'`, `id='max_id'`, rest) mirroring kfxlib so the symbol table is loaded before fragments. `payload_type='path'` → resource bytes as `$417` blob with fid = row id. `capabilities` → `$593`.
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): KPF/KDF reader with fingerprint stripping"`

---

### Task 7: KFX container reader and writer

**Files:**
- Create: `crates/deckle-kfx/src/container.rs`
- Test: `crates/deckle-kfx/tests/container_roundtrip.rs`
- Reference: `kfxlib/kfx_container.py` (`KfxContainer.deserialize/serialize`, `KfxContainerEntity`), `kfxlib/utilities.py` (`json_serialize_compact`, `sha1`)

**Interfaces:**
- Produces:
  ```rust
  pub struct KfxDoc { pub symtab: SymbolTable, pub fragments: FragmentList, pub container_id: String, pub creator: String /* $587 */, pub package_version: String /* $588 */ }
  pub fn read_kfx(data: &[u8]) -> Result<KfxDoc, ContainerError>;
  pub struct WriteOptions { pub container_id: String, pub creator: String, pub package_version: String }  // defaults: random CR!…, "deckle-<crate version>", ""
  pub fn write_kfx(doc: &KfxDoc, opts: &WriteOptions) -> Result<Vec<u8>, ContainerError>;
  ```
  `write_kfx` requires exactly one `$270`, one `$ion_symbol_table`, one `$419`, at most one `$593`; entity id is `$348` for fragments with `fid == None`.

- [ ] **Step 1: Failing test: read golden, write it back, bytes identical**

```rust
#[test] fn golden_reads_and_rewrites_byte_exact() {
    let bytes = std::fs::read(fixtures::path("deckle-mini-pdoc-nopages.golden.kfx")).unwrap();
    let doc = read_kfx(&bytes).unwrap();
    assert_eq!(doc.fragments.0.len(), 40); assert_eq!(doc.creator, "kfxlib-20260827");
    let out = write_kfx(&doc, &WriteOptions{ container_id: doc.container_id.clone(), creator: doc.creator.clone(), package_version: doc.package_version.clone() }).unwrap();
    assert_eq!(out, bytes);
}
```

- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement** per `docs/formats/kfx.md` container layout. The `kfxgen_info` trailer is the compact JSON of the four `{key,value}` structs with `"key":`→`key:` and `"value":`→`value:` replaced, ASCII; `kfxgen_payload_sha1` is hex SHA-1 of the concatenated entity bytes. Fragment order on write is the order in `doc.fragments` (reader preserves file order).
- [ ] **Step 4: Run** — PASS, byte-exact.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): monolithic KFX container read/write"`

---

### Task 8: KPF fix-ups A — structural

**Files:**
- Create: `crates/deckle-kfx/src/fixup/mod.rs`, `crates/deckle-kfx/src/fixup/structural.rs`, `crates/deckle-kfx/src/consts.rs`
- Test: `crates/deckle-kfx/tests/fixup_structural.rs`
- Reference: `kfxlib/kpf_book.py` (`fix_kpf_prepub_book` up to and including the `$585` features block, `kpf_fix_fragment`, `fix_resource_location`), `kfxlib/yj_structure.py` (`EID_REFERENCES`, `METADATA_SYMBOLS`, `MAX_CONTENT_FRAGMENT_SIZE`), `kfxlib/resources.py` (`FORMAT_SYMBOLS`), `kfxlib/yj_versions.py` (`GENERIC_CREATOR_VERSIONS`)

**Interfaces:**
- Produces: `pub struct Book { pub fragments: FragmentList, pub symtab: SymbolTable, pub kcb: serde_json::Value, pub is_prepub: bool, pub warnings: Vec<String> }` (constructed from `Kpf`), and `pub fn fix_structural(book: &mut Book) -> Result<(), FixupError>` covering: resource fid prefix; `$608` inlining into `$146`; eid symbol → id via `EID_REFERENCES` (`$598` renamed `$155`); `yj.*` key stripping; float→decimal (`%g`, |x|<0.001 → 0); `$266` zero `$143` removal; `$550` `$178` removal; `$490` category/defaults (asset_id = container id placeholder provided by caller, `is_sample` false, `override_kindle_font` false, `file_creator`/`creator_version` from kcb when generic); `$262`→`$418` font re-homing; `$164` dimension copy; `$585` features (`CanonicalFormat` 1.0 or 2.0, `yj_hdv`, `yj_jpg_rst_marker_present`, …).

- [ ] **Step 1: Failing test against the golden's fragments**

```rust
#[test] fn structural_fixups_match_golden_for_490_585_164_157() {
    let kpf = read_kpf(&fixtures::path("deckle-mini.kpf")).unwrap();
    let golden = read_kfx(&std::fs::read(fixtures::path("deckle-mini-pdoc-nopages.golden.kfx")).unwrap()).unwrap();
    let mut book = Book::from_kpf(kpf, &golden.container_id);
    fix_structural(&mut book).unwrap();
    // $585 and $164 and each $157 must equal the golden's (values compared as Ion, symbol text)
    for t in ["$585", "$164", "$157"] { for g in golden.fragments.all(&Sym(t.into())) { let ours = book.fragments.get(&g.ftype, g.fid.as_ref()).expect(t); assert_eq!(ours.value, g.value, "{t} {:?}", g.fid); } }
    // $490 must equal the golden's except keys ASIN, content_id, cde_content_type (Task 13 adds those)
}
```

- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement**, porting `kpf_fix_fragment` as a recursive `fn fix_ion(value, container_key, ctx)` over `Value`. Copy `EID_REFERENCES`, `FORMAT_SYMBOLS`, `METADATA_SYMBOLS` tables verbatim into `consts.rs` (they are short dictionaries).
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): structural KPF fix-ups"`

---

### Task 9: KPF fix-ups B — content fragments

**Files:**
- Create: `crates/deckle-kfx/src/fixup/content.rs`
- Test: `crates/deckle-kfx/tests/fixup_content.rs`
- Reference: `kfxlib/kpf_book.py` (`kpf_collect_content_strings`), `kfxlib/yj_structure.py` (`ordered_section_names`, `extract_section_story_names`, `reading_order_names`)

**Interfaces:**
- Produces: `pub fn create_content_fragments(book: &mut Book) -> bool /*has_text_block*/`: walks sections in reading order, each `$259` storyline, replaces every `{$145: "text"}` string with `{name: content_N, $403: index}` and emits `$145` fragments `{name, $146: [strings]}` with new chunks when the running UTF-8 size reaches 8192.

- [ ] **Step 1: Failing test** — after Tasks 8 and 9 on the mini KPF: exactly 9 `$145` fragments named `content_1..content_9`, and each equals the golden's `$145` of the same fid; every golden `$259` equals ours.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): content fragment extraction"`

---

### Task 10: KPF fix-ups C — position and location maps

**Files:**
- Create: `crates/deckle-kfx/src/fixup/positions.rs`
- Test: `crates/deckle-kfx/tests/fixup_positions.rs`
- Reference: `kfxlib/yj_position_location.py` (`collect_position_map_info` incl. `process_spim`, `create_position_map`, `collect_location_map_info`, `generate_approximate_locations`, `create_location_map`), `kfxlib/kpf_book.py` (the block after content fragments: `$395` default, `$593` rewrite)

**Interfaces:**
- Produces: `pub fn create_position_and_location_maps(book: &mut Book) -> PositionReport { has_spim: bool, has_pid_offset: bool, has_location_map: bool }` producing `$264`, `$265`, `$550` (only when absent), `$395` (when absent), and rewriting `$593` to `[kfxgen.positionMaps v2?][kfxgen.pidMapWithOffset v1?][kfxgen.textBlock v1?]`.

- [ ] **Step 1: Failing test** — `$264`, `$265`, `$550`, `$395`, `$593` each equal the golden's (nopages golden). The golden `$593` is `[{$492:"kfxgen.textBlock", version:1}]` and its `$270.$181` lists `$264`/`$265` as `$348` singletons.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** (port; keep the Python's data classes `PosInfo{pid, eid, eid_offset, length, section_name}`). **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): position and location maps"`

---

### Task 11: KPF fix-ups D — navigation, page-list transform, approximate pages

**Files:**
- Create: `crates/deckle-kfx/src/fixup/navigation.rs`
- Test: `crates/deckle-kfx/tests/fixup_navigation.rs`
- Reference: `kfxlib/kpf_book.py` (`$389` default, cover landmark, `page_list_entry:` transform), `kfxlib/yj_position_location.py` (`create_approximate_page_list`, `determine_approximate_pages`)

**Interfaces:**
- Produces: `pub fn fix_navigation(book: &mut Book)` and `pub fn add_approximate_pages(book: &mut Book, desired: Option<u32> /*None=auto*/) -> Result<u32 /*pages*/, FixupError>`; auto follows kfxlib (`set_approximate_pages=0`), which yields 45 for the mini book and 435 for Pride and Prejudice.

- [ ] **Step 1: Failing tests** — (a) after `fix_navigation`, `$389` equals the nopages golden's; (b) after `add_approximate_pages(None)`, `$389` and `$265`/`$264` equal the **pages** golden's, and the local symbol `APPROXIMATE_PAGE_LIST` exists; return value 45.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): navigation fix-ups and approximate page list"`

---

### Task 12: Rebuild and end-to-end oracle

**Files:**
- Create: `crates/deckle-kfx/src/fixup/rebuild.rs`, `crates/deckle-kfx/src/convert.rs`
- Test: `crates/deckle-kfx/tests/kpf_to_kfx_oracle.rs`
- Reference: `kfxlib/yj_structure.py` (`check_fragment_usage(rebuild=True, ignore_extra=True)`, `walk_fragment`, `determine_entity_dependencies`, `rebuild_container_entity_map`, `check_symbol_table(rebuild=True)`, `classify_symbol`), `kfxlib/yj_book.py` (`decode_book`, `convert_to_single_kfx`)

**Interfaces:**
- Produces:
  ```rust
  pub fn rebuild(book: &mut Book, container_id: &str, creator: &str) -> Result<(), FixupError>; // drops unreferenced, errors on missing, writes $270 "KFX main", $419, rebuilds symbol table, sorts fragments in PREFERED order
  pub struct ConvertOptions { pub cde_type: CdeType /*Pdoc|Ebok*/, pub asin: Option<String>, pub title: Option<String>, pub authors: Option<Vec<String>>, pub language: Option<String>, pub approximate_pages: Option<Option<u32>> /*None=skip, Some(None)=auto*/, pub container_id: Option<String>, pub creator: Option<String> }
  pub fn kpf_to_kfx(kpf: &std::path::Path, opts: &ConvertOptions) -> Result<ConvertOutput { bytes: Vec<u8>, pages: Option<u32>, warnings: Vec<String> }, ConvertError>;
  ```
  Metadata stamping inside `kpf_to_kfx` is Task 13; this task passes `asin: Some("B0DECK000002"), cde_type: Pdoc` through a stub that only appends the three keys so the oracle test can be written now and stay green after Task 13.

- [ ] **Step 1: Failing oracle tests**

```rust
#[test] fn mini_kpf_to_kfx_matches_nopages_golden_byte_for_byte() {
    let gbytes = std::fs::read(fixtures::path("deckle-mini-pdoc-nopages.golden.kfx")).unwrap();
    let g = read_kfx(&gbytes).unwrap();
    let out = kpf_to_kfx(&fixtures::path("deckle-mini.kpf"), &ConvertOptions{ cde_type: CdeType::Pdoc, asin: Some("B0DECK000002".into()), approximate_pages: None, container_id: Some(g.container_id.clone()), creator: Some(g.creator.clone()), ..Default::default() }).unwrap();
    assert_eq!(out.bytes, gbytes);
}
#[test] fn mini_kpf_to_kfx_with_pages_matches_pages_golden() { /* same with approximate_pages: Some(None) vs deckle-mini-pdoc.golden.kfx; assert pages == Some(45) */ }
#[test] fn over_16mib_output_is_a_warning_not_an_error() { /* unit: ConvertOutput::warn_if_large(len) -> Some("KFX is 25.4 MB; some devices lock up above 16 MB") for 25_467_398 */ }
```

- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement `rebuild` and `kpf_to_kfx`** (read_kpf → Book → fix_structural → create_content_fragments → maps → fix_navigation → pages → metadata → rebuild → write_kfx). If byte equality fails only on fragment order, port `PREFERED_FRAGMENT_TYPE_ORDER` sorting plus kfxlib's `YJFragment.__lt__` (type order, then fid natural sort) exactly.
- [ ] **Step 4: Run** — PASS, both goldens byte-exact.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): rebuild pass and KPF→KFX conversion, byte-exact with kfxlib"`

---

### Task 13: Metadata stamping

**Files:**
- Create: `crates/deckle-kfx/src/metadata.rs`
- Test: `crates/deckle-kfx/tests/metadata.rs`
- Reference: `kfxlib/yj_metadata.py` (`set_yj_metadata_to_book`, `author_sort_name`, `fix_cover_image_data`, `set_cover_image_data`, `fix_language_for_kfx`)

**Interfaces:**
- Produces: `pub struct Metadata { pub title: Option<String>, pub authors: Option<Vec<String>>, pub cde_type: Option<CdeType>, pub asin: Option<String>, pub language: Option<String>, pub cover_jpeg: Option<Vec<u8>> }`, `pub fn apply(book: &mut Book, md: &Metadata)`, `pub fn author_sort(name: &str) -> String` ("Jane Testwright" → "Testwright, Jane"; names with a comma unchanged; single token unchanged), `pub fn generate_asin(seed: &[u8]) -> String` deterministic `B0` + 8 chars from SHA-1 of seed over `[A-Z0-9]`, suffixed so it ends in `DECK`? No: `B0` + 4 hash chars + `DECK` (10 chars total, recognizable per spec §5).

- [ ] **Step 1: Failing tests** — (a) `apply` with `cde_type: Ebok, asin: "B0DECK000002"` on the fixed mini book makes `$490` equal the **ebok** golden's `$490`; (b) `author_sort` cases above; (c) `generate_asin(b"x")` is 10 chars, starts `B0`, ends `DECK`, stable across calls; (d) a PNG cover passed as `cover_jpeg` is rejected with `MetadataError::CoverNotJpeg`.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement**; `kpf_to_kfx` now calls `apply`. **Step 4: Run** — PASS, and Task 12's oracle still PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(kfx): title/author/cdeType/ASIN/cover metadata"`

---

### Task 14: Inspect stage

**Files:**
- Create: `crates/deckle-core/src/inspect.rs`, `crates/deckle-core/src/epub.rs` (zip + OPF/nav helpers)
- Test: `crates/deckle-core/tests/inspect.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Creator { pub name: String, pub file_as: Option<String> }
  pub struct Series { pub name: String, pub index: Option<f32> }
  pub struct TocEntry { pub title: String, pub href: String, pub depth: u8 }
  pub struct BookInfo { pub title: String, pub creators: Vec<Creator>, pub language: String, pub identifiers: Vec<String>, pub isbn: Option<String>, pub series: Option<Series>, pub cover_href: Option<String>, pub spine: Vec<String>, pub toc: Vec<TocEntry>, pub epub_version: String, pub warnings: Vec<String> }
  pub fn inspect(epub: &std::path::Path) -> Result<BookInfo, InspectError>;  // InspectError::{NotZip, NoContainerXml, NoOpf, Drm, Xml(String)}
  ```

- [ ] **Step 1: Failing tests** — on `fixtures/mini/deckle-mini.epub`: title `Deckle Mini Fixture`; creators `[("Jane Testwright", Some("Testwright, Jane"))]`; language `en`; series `("Fixture Series", Some(2.0))`; cover `cover.png`; spine len 3; toc titles `["Chapter One","Chapter Two","Chapter Three"]`; `epub_version == "3.0"`. Review Focus 1: a copy of the fixture with an added `META-INF/encryption.xml` → `InspectError::Drm`. Calibre-style `<meta name="calibre:series" content="X"/>` + `calibre:series_index` is also parsed (craft in-test).
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** with `zip` + `roxmltree`. **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(core): EPUB inspect stage"`

---

### Task 15: Normalize stage (v1 minimal)

**Files:**
- Create: `crates/deckle-core/src/normalize.rs`
- Test: `crates/deckle-core/tests/normalize.rs`

**Interfaces:**
- Produces: `pub struct NormalizeOptions { pub max_image_px: u32 /*1920*/, pub jpeg_quality: u8 /*85*/ }`, `pub struct NormalizeReport { pub images_downscaled: u32, pub scripts_removed: u32, pub remote_refs_removed: u32, pub cover_converted_to_jpeg: bool, pub toc_synthesized: bool }`, `pub fn normalize(src: &Path, dst: &Path, opts: &NormalizeOptions) -> Result<NormalizeReport, NormalizeError>`. v1 scope: copy EPUB; ensure `properties="cover-image"` on the cover item (derive from `<meta name="cover">` if needed); convert a PNG cover to JPEG/JFIF (`image` crate); downscale any raster whose longer side exceeds `max_image_px`; remove `<script>` elements and `http(s)://` `src`/`href` resource references; if no nav TOC, synthesize one from the first `h1`/`h2` of each spine item. Everything else passes through untouched.

- [ ] **Step 1: Failing tests** — (a) mini fixture: `cover_converted_to_jpeg == true`, output still inspects identically except cover href ends `.jpg`; (b) in-test EPUB with a 4000 px PNG → `images_downscaled == 1` and output image longer side == 1920; (c) in-test EPUB with `<script>` and a remote `<img src="https://…">` → both counters 1; (d) EPUB with no nav → `toc_synthesized == true` and `inspect(dst).toc.len() == spine.len()`.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(core): normalize stage (cover, images, scripts, toc)"`

---

### Task 16: Convert stage (Kindle Previewer runner)

**Files:**
- Create: `crates/deckle-core/src/previewer.rs`
- Test: `crates/deckle-core/tests/previewer.rs`, `crates/deckle-core/tests/fake_previewer/` (a shell/bat script that copies `fixtures/mini/deckle-mini.kpf` and the two CSVs into `--output`)

**Interfaces:**
- Produces:
  ```rust
  pub struct Previewer { pub path: PathBuf, pub version: String /* "4.0.3" from `--help` banner */, pub major: u8 }
  pub fn locate(override_path: Option<&Path>) -> Result<Previewer, ConvertError>;   // macOS: /Applications and ~/Applications "Kindle Previewer 4.app/Contents/MacOS/KindlePreviewer4CLI", then 3.x "Kindle Previewer 3.app/Contents/lib/fc/bin/kindlepreviewer"; Windows: %LOCALAPPDATA%\Amazon\Kindle Previewer 4\*.exe (verify on Windows), %LOCALAPPDATA%\Amazon\Kindle Previewer 3\lib\fc\bin\kindlepreviewer.exe; else env DECKLE_PREVIEWER; else PreviewerMissing
  pub struct KpfResult { pub kpf: PathBuf, pub et_supported: bool, pub log_errors: Vec<String>, pub duration_ms: u64 }
  pub fn convert(p: &Previewer, epub: &Path, out_dir: &Path) -> Result<KpfResult, ConvertError>; // creates out_dir first; flags "--convert --locale en --output" for major 4, "-convert -locale en -output" for 3
  pub enum ConvertError { PreviewerMissing { download_url: &'static str }, EnhancedTypesettingUnsupported { first_error: String }, PreviewerFailed { stderr: String }, NoOutput, Io(std::io::Error) }
  ```

- [ ] **Step 1: Failing tests** — (a) with `DECKLE_PREVIEWER` pointing at the fake script: `convert` returns `et_supported == true`, kpf path ends `deckle-mini_epub.kpf`; (b) fake script variant writing `"Not Supported"` summary and a log row of type `Error` → `EnhancedTypesettingUnsupported{first_error}` contains that row's description (Review Focus 2); (c) `locate(None)` with no install and no env → `PreviewerMissing` whose `download_url == "https://kdp.amazon.com/en_US/help/topic/G202131170"` (Review Focus 3; run this test with `HOME` pointed at a temp dir); (d) **gated**: if real `DECKLE_PREVIEWER_REAL=1` and a real install is located, convert `fixtures/mini/deckle-mini.epub` and assert `et_supported`.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement**, parsing `Summary_Log.csv` with the `csv` crate (UTF-8 BOM tolerant), picking the `Output File Path` column. **Step 4: Run** — PASS (gated test passes locally on the Mac with Previewer 4.0.3).
- [ ] **Step 5: Commit** — `git commit -m "feat(core): Kindle Previewer locate and convert"`

---

### Task 17: Deliver stage (folder and mass storage)

**Files:**
- Create: `crates/deckle-core/src/deliver.rs`
- Test: `crates/deckle-core/tests/deliver.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum Target { Folder(PathBuf), MassStorage(Device) }
  pub struct Device { pub root: PathBuf, pub model_hint: String /* from system/version.txt first line or volume name */, pub thumbnails_writable: bool /* system/thumbnails exists and is writable */ }
  pub fn detect_mass_storage() -> Vec<Device>;   // macOS: /Volumes/* containing documents/ and system/; Windows: drive letters likewise
  pub fn device_filename(info: &BookInfo) -> String;  // "<Author sort> - <Title>.kfx", ASCII-folded, [\\/:*?"<>|] -> '_', max 120 chars
  pub fn deliver(kfx: &[u8], info: &BookInfo, target: &Target) -> Result<PathBuf, DeliverError>; // Folder: writes file; MassStorage: writes to <root>/documents/<name>, fsyncs
  ```

- [ ] **Step 1: Failing tests** — (a) `device_filename` for title `Ærø: “Hello”/World?` author `Jane Testwright` == `Testwright, Jane - AEro_ _Hello__World_.kfx` (Review Focus 4; use `deunicode` crate for folding and document the exact mapping in the test); (b) `deliver` to a temp folder writes the bytes; (c) a temp dir with `documents/` and `system/version.txt` is detected by a `detect_in(roots: &[PathBuf])` helper the public function wraps, with `thumbnails_writable == true` when `system/thumbnails` exists.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(core): deliver to folder or mass-storage Kindle"`

---

### Task 18: Pipeline `run` and CLI wiring

**Files:**
- Create: `crates/deckle-core/src/pipeline.rs`, `crates/deckle-core/src/report.rs`
- Modify: `crates/deckle-cli/src/cli.rs` (all subcommands implemented), `crates/deckle-cli/src/main.rs`
- Test: `crates/deckle-core/tests/pipeline.rs`, `crates/deckle-cli/tests/cli.rs`

**Interfaces:**
- Produces: `pub struct RunOptions { pub cde_type: CdeType, pub asin: Option<String>, pub pages: Option<Option<u32>>, pub target: Target, pub previewer: Option<PathBuf>, pub keep_work_dir: bool }`, `pub struct RunReport { pub input: PathBuf, pub info: BookInfo, pub normalize: NormalizeReport, pub kpf: Option<PathBuf>, pub kfx: Option<PathBuf>, pub delivered: Option<PathBuf>, pub pages: Option<u32>, pub warnings: Vec<String>, pub error: Option<String>, pub stage_ms: BTreeMap<&'static str, u64> }` (serde Serialize), `pub fn run(epub: &Path, opts: &RunOptions) -> RunReport` (never panics; errors land in `report.error` with the failing stage name).
  CLI: `convert` accepts a `.kpf` directly (skips Inspect/Normalize/Previewer) so the ungated CLI test exists; `run` checks `previewer::locate` **before** touching the EPUB (Review Focus 3).

- [ ] **Step 1: Failing tests** — (a) CLI `deckle convert fixtures/mini/deckle-mini.kpf -o out.kfx --cde PDOC --asin B0DECK000002 --container-id <golden id> --creator kfxlib-20260827` produces bytes equal to the nopages golden; (b) `deckle inspect fixtures/mini/deckle-mini.epub --json` has `.title == "Deckle Mini Fixture"`; (c) pipeline with fake Previewer: `run` → `report.error == None`, `delivered` file exists in temp folder, `warnings` empty; (d) Review Focus 5: a `RunReport::push_size_warning(len)` produces `"KFX is 25.4 MB; some devices lock up above 16 MB"` for 25_467_398 and nothing for 10 MB; (e) gated real-Previewer run on the mini EPUB succeeds end to end.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS. Also run manually: `cargo run -p deckle-cli -- run fixtures/mini/deckle-mini.epub --to ./out` and paste the report into the PR.
- [ ] **Step 5: Commit** — `git commit -m "feat: end-to-end run pipeline and CLI"`

---

### Task 19: Tauri shell (M0 window)

**Files:**
- Create: `apps/desktop/package.json`, `apps/desktop/vite.config.ts`, `apps/desktop/tsconfig.json`, `apps/desktop/index.html`, `apps/desktop/src/main.tsx`, `apps/desktop/src/App.tsx`, `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src-tauri/capabilities/default.json`, `apps/desktop/src-tauri/icons/*` (generated with `pnpm tauri icon` from a placeholder 1024 px PNG)
- Modify: `Cargo.toml` workspace members to include `apps/desktop/src-tauri`

**Interfaces:**
- Produces: a window titled `Deckle`, 960x640, showing a centered drop zone placeholder ("Drop EPUBs here") and a bottom bar with a disabled **Send to Kindle** button and the text "No Kindle connected". One Tauri command `app_version() -> String` wired to a footer. No pipeline wiring yet.

- [ ] **Step 1: Scaffold** with `pnpm create tauri-app@latest` equivalents pinned to the versions in the Tech Stack (write files by hand to keep control); app identifier `com.casavargas.deckle`; `withGlobalTauri: false`.
- [ ] **Step 2: Build check** — `pnpm --dir apps/desktop install && pnpm --dir apps/desktop build && cargo build -p deckle-desktop` — Expected: both succeed.
- [ ] **Step 3: Visual check** — `pnpm --dir apps/desktop tauri build --debug`, open the produced `.app`, `screencapture -l $(window id)` or a manual screenshot into `docs/screenshots/m0-shell.png`; the window shows the three regions. Commit the screenshot.
- [ ] **Step 4: Commit** — `git commit -m "feat(desktop): Tauri shell window"`

---

### Task 20: Docs and devices matrix

**Files:**
- Create: `docs/devices.md` (table: model, firmware, transport, thumbnails writable, tested date, result; seed rows for Paperwhite (gen TBD) and Scribe 2024 as "untested")
- Modify: `README.md` (CLI usage section with the `run` example and the Previewer requirement), `docs/formats/kfx.md` (anything learned during Tasks 3–13 that contradicts the notes)

- [ ] **Step 1: Write the docs.** **Step 2: `cargo test --workspace` green; `cargo clippy -- -D warnings` clean.** **Step 3: Commit** — `git commit -m "docs: devices matrix, CLI usage, format notes"`; push; open PR "M0+M1: typography pipeline" with `Closes #<issue>` once an issue exists.

---

## Device verification (end of M1, needs the owner)

Not a task for the agent alone: plug in the Paperwhite (mass storage) and run
`deckle run <book>.epub --to device --cde PDOC`. Pass criteria from spec §10 spike 1: the book
opens, the cover shows in the library, the `Aa` menu offers Enhanced Typesetting options
(hyphenation/justification toggles present), and page numbers appear when `--pages auto` was
used. Record the result in `docs/devices.md`. If the Scribe is the only device, use
`--to <folder>` and copy with Amazon's Send to Kindle desktop app's USB mode or any MTP client
for now; MTP transport is M2.

## Self-review notes

- Spec coverage for M0/M1: §4.1 Inspect (T14), §4.2 Normalize minimal (T15; SVG rasterizing and font pruning deferred to M2 with a note), §4.4 Convert + repackage (T3–T13, T16), §4.6 Deliver folder/mass storage (T17), §4.7 Verify is folded into `RunReport` (T18), §12 M0 shell (T19), §10 spike 1 (device section). Match, MTP, X-Ray, Word Wise are M2+ and not in this plan.
- Types are consistent: `Sym`, `Value`, `Fragment`, `FragmentList`, `SymbolTable`, `Book`, `KfxDoc`, `ConvertOptions`, `BookInfo`, `Target`, `RunReport` are named identically across tasks.
- Review Focus items are pinned to T14, T16 (two), T17, T18.
- Proportion: the plan describes interfaces and tests, not bodies; the one algorithmic hint (fragment sort order) is called out where the oracle would otherwise be undebuggable.
