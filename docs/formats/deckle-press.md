<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Synced-folder press protocol v1

This public contract lets compatible file-based clients request a full fresh
edition from one designated producer and paper in an explicitly enabled folder.
The desktop must be awake with its watcher running. Sync has no delivery deadline.
The manifest advertises configuration, not liveness or authentication. Folder
writers can invoke the bounded operation using the producer's configured sources
and optional model; credentials stay local.

The JSON Schemas are MIT licensed. This prose is CC-BY-4.0. Client implementations
can use the contract without importing the producer's GPL implementation.

| Export-relative path | Writer | Schema definition |
| --- | --- | --- |
| `.deckle-press/producer.json` | Producer | `deckle-press.schema.json#/$defs/manifest` |
| `.deckle-press/requests/<reader-uuid>/<request-uuid>.json` | Client | `deckle-press.schema.json#/$defs/request` |
| `.deckle-press/status/<request-uuid>.json` | Producer | `deckle-press.schema.json#/$defs/status` |
| `.deckle-press/editions/<edition-uuid>.deckle/` | Producer | `edition-bundle.schema.json` for `edition.json` |
| `.deckle-returns/<reader-uuid>/editions/<edition-uuid>.json` | Client | `deckle-returns.schema.json`, version `1.1` |

Controls are UTF-8 JSON objects with at most **16,384 bytes**, including whitespace.
Manifest, request and status use integer version `1`; unsupported versions fail
closed. Unknown optional fields are ignored. UUIDs in paths use lowercase,
hyphenated form and agree with their corresponding fields. Each control's
producer and paper IDs must match the selected local folder binding. Display
names never establish identity. Reject absolute paths, traversal, alternate path
spelling, conflict copies, symlinks escaping the folder and non-regular files.
The byte decoders do no I/O; the filesystem reader must enforce file safety and
bounded reads before calling them.

Requests are immutable and permit only `fresh_full`. Clients durably save exact
bytes and binding before atomically publishing. Retries reuse those bytes and ID.
`expires_at` must be exactly 24 hours after `created_at`. Creation up to five
minutes ahead of the producer clock is accepted. Greater skew or a different TTL
is `clock_mismatch`. An unstarted request expires when `now >= expires_at`.
Running builds may finish after expiry. Different bytes for an accepted request
ID are `protocol_conflict`. No URLs, destinations, credentials or shell parameters
are accepted as operations.

Statuses have positive monotonic revisions and states `queued`, `building`,
`published`, `failed`, `expired` or `result_removed`. Ignore older revisions and
fail closed on conflicting equal revisions. Optional `job_id` and `started_at`
identify coalesced work. Only `published` and `result_removed` carry a required
`result` object with `edition_id`, `bundle` and lowercase 64-digit
`manifest_sha256`. The bundle string must equal
`.deckle-press/editions/<result.edition_id>.deckle`. The hash describes the exact
`edition.json` bytes; clients independently check it and all listed assets before
Ready. Publication is local and may arrive before assets through sync.

`result_removed` also requires `removed_at` and the safe message
`Fresh edition is no longer available; print another`. Other states omit these
tombstone fields. A tombstone supersedes published status. Keep it for 30 days
after removal. Producer pruning retains the newest seven fresh editions, every
published result for at least 48 hours, and all outputs referenced by pending
jobs. Client-owned requests/returns and scheduled bundles are outside that pruning.

Safe `error_code` values are `busy`, `clock_mismatch`, `expired`, `disabled`,
`presses_stopped`, `unsupported`, `source_failure`, `build_failed`, `timed_out`,
`storage_unavailable` and `protocol_conflict`. Never include provider bodies,
credentials or OS diagnostics in shared controls. Malformed partial controls
should be reconsidered when their bytes change; unchanged malformed bytes become
a local diagnostic after 60 seconds.

Fresh bundles use version `1.4` and require `edition.id`, `edition.paper_id` and
`edition.kind: fresh`, retaining the existing date and number. Their identity is
`(paper_id, edition.id)` across caches, state and returns. Bundle versions `1.0`
through `1.3` remain valid without fresh identity. Fresh returns use version `1.1`
with `edition_id`, `paper_id` and the original **`edition` date field**. Legacy
returns version `1.0` and date-based paths remain valid. Never copy scheduled
reading marks or puzzle progress into a fresh edition.

The schemas check types and structural branches. Implementations additionally
check byte limits, exact TTL, local binding, path/ID agreement and manifest/asset
hash consistency. `fixtures/press/cases.json` is the portable conformance matrix;
`tools/validate-press-fixtures.py` validates it offline using the pinned test-only
requirements. The Rust integration tests cover runtime clock and routing checks.
