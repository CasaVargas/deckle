# Testing fresh-paper delivery

All automated scenarios use temporary papers, an injected clock and fictional local source responses. No paid editor, production configuration, running watcher, or cloud account is required. Desktop CI is self-contained: schemas and fixtures are checked into this repository, and schema resolution is offline.

## Repeatable automated checks

```sh
python3 -m venv /tmp/deckle-press-venv
/tmp/deckle-press-venv/bin/python -m pip install -r tools/requirements-press.txt
/tmp/deckle-press-venv/bin/python tools/validate-press-fixtures.py
/tmp/deckle-press-venv/bin/python tools/press-smoke.py \
  --target-dir /tmp/deckle-press-target --output /tmp/deckle-press-exchange
```

The output directory must not exist. Reuse an isolated target for this checkout only; do not clean a shared target. The smoke prints the checkout, target directory and Cargo test executable. It launches the actual worker child with injected network responses and exports actual requests, queued/building/published statuses and the generated bundle. It checks two installations coalesce, a queued request crosses local midnight, restart does not rebuild, SHA-256/size inventories match, and an older scheduled bundle and marks retain their exact bytes. UUIDs and rendered archive bytes can vary between runs; the expected behavior and fake inputs are fixed. `--validate-only PATH` validates an exported exchange without running Cargo. This validates protocol/data interoperability, not any particular client UI or cloud-provider delivery.

```sh
export CARGO_TARGET_DIR=/tmp/deckle-press-target
cargo test -p deckle-digest
cargo test -p deckle-cli
cargo test -p deckle-desktop
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
# Existing pinned PDFium setup (tools/fetch-pdfium.sh) or DECKLE_PDFIUM is required.
cargo nextest run --workspace --profile ci
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
```

The CI Rust matrix runs the workspace suite and the standalone exchange smoke on macOS and Windows. The separate contract job validates local fixtures. The public UUID spelling is lowercase and hyphenated; uppercase, compact, braced and URN spellings are rejected by control decoders. Optional future fields remain tolerated. The fresh returns fixture uses a realistic 5×5 crossword: the broader returns schema permits other grid dimensions, so schema acceptance alone does not establish a client's puzzle support. Consumers must enforce their supported puzzle dimensions safely.

## Manual delivery matrix

Use a disposable paper/folder and free fixture sources with a compatible client. Record date, producer/client version, device/provider, observed phases and elapsed times for each row. Preserve screenshots locally; do not put private folder contents or account details into reports.

| Scenario | Expected result | Physical evidence |
| --- | --- | --- |
| Desktop awake, opted in, watcher running | Saving → queued → building → arriving → Ready; open only on explicit action | Open |
| Desktop asleep/stopped, then awake/restarted | Waiting persists; no remote-wake claim; original request resumes if still valid | Open |
| Two installations request together | Both receive the same edition UUID; one build | Open |
| Network lost/restored during request and result sync | Immutable request retries; partial result stays arriving; restoration completes | Open |
| Restart producer mid-build | Persisted deadline/attempt ceiling survives; incomplete staging is not advertised | Open |
| Revoke/reselect folder permission | No new writes while revoked; original bound outbox is retained | Open |
| Sources unchanged | Explicit fresh request produces a new identity without consuming scheduled state | Open |
| Request queued across local midnight | Fresh UUID isolation; scheduled date-keyed bundle/marks unchanged | Open |
| Published status precedes bundle/assets; old status arrives late | Arriving until every required asset verifies; stale status cannot regress progress | Open |
| Retain/prune fresh editions | Seven-result target, 48-hour minimum and pending protection; tombstone after removal | Open |

Automated tests cover injected equivalents of these failures, including worker crashes/timeouts, deadlines, ownership checks, invalid controls, and retention recovery. Filesystem simulation does not establish real cloud ordering, device sleep/wake, physical permission prompts, or accessibility gestures. The physical matrix remains explicitly unverified until observations are recorded.

## Local validation record — 2026-10-28

The implementation run used an isolated Cargo target and an existing local PDFium library. Actual worker smoke: two requests, three captured phases, one coalesced result, two hashed assets, unchanged scheduled bundle and marks. Offline contract validation: 35 schema fixtures plus two semantic rejection cases. On macOS: digest 324 unit + 542 integration tests; CLI 35 unit + 208 integration tests (two intentional helper/fixture ignores); desktop 109 unit + four integration tests; workspace nextest 1,988 passed and nine skipped; frontend 272 tests in 32 files. Rust formatting, warning-denying clippy and frontend production build passed. The final local-midnight test refinement is covered by the workspace run. The smoke validator was also run against a deliberately corrupted asset and correctly rejected it. Earlier development checks exposed noncanonical UUID acceptance and the one-row puzzle interoperability gap; both now have passing regression coverage. Linux/Windows executions and hosted CI results are not established by this local run. No physical delivery pass or screenshots are claimed by these automated results.
