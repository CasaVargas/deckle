# Fresh paper on demand through a synced folder

Date: 2026-10-27. Status: approved by Jonathan on 2026-10-27; implementation not started.
Components: desktop producer and compatible file-based clients.

## Intent and scope

Request a fresh edition of the selected paper on a compatible client. An awake desktop running Deckle's watcher fetches current sources and publishes a separate edition in the shared folder. The existing paper stays readable throughout.

User decisions: synced-folder transport first; isolated worktree based on freshly fetched origin/main. The following author-proposed defaults were approved with the full design on 2026-10-27:

- Build a full fresh paper. Delta editions (“only new stories”) are deferred.
- Deliver only to the selected client folder, not email, USB queues, or other exports.
- One paper and one designated producer installation per enabled folder; multiple clients may request from it.
- Opt-in on desktop. No new app networking, hosted service, account, subscription, remote wake, or push notifications.
- “Stop the presses” blocks client requests too. Explicit requests may run outside scheduled weekdays.
- Cross-device reading-state merge, Share extension, and intraday desktop Morgue UI/search are deferred.

## Current foundation

The desktop watcher is the producer integration point. Existing scheduled runs retain their current delivery behavior; on-demand jobs are isolated. The edition bundle contract gains additive identity fields and generic client request/status schemas. Schema licensing remains MIT and prose CC-BY-4.0.

## Setup and experience

Daily → Delivery → Bundle export gains “Allow fresh-paper requests.” Explain that the desktop must be awake, its watcher running, and the folder synced to client. Validate folder write access and persistent paper/producer UUIDs; use normal supported watcher setup to install/start it if necessary. Setup failure must not report ready. Disable stops new work; a running job may finish.

Reject duplicate local folder mappings. Never overwrite a foreign producer manifest: explicit disconnection/reconnection is required. Synced files are not a distributed lock. Concurrent producer setup is unsupported; detected ownership conflicts disable remote work. Reinstallation/moving producers requires deliberate setup again.

client adds “Print a fresh paper” to the selected paper menu and the empty-library view when a valid enabled producer manifest exists. Pull-to-refresh continues to rescan/download, not commission builds. Missing/disabled/unsupported manifests show setup/update guidance. Pending requests disable the local button for this paper and reopen status; another device may request and be coalesced.

| client state | Meaning |
|---|---|
| Saving request | Private outbox is durable; shared write still needs retry. |
| Requested | Shared write succeeded locally, not proof of cloud upload. “Waiting for your computer. It needs to be awake with Deckle running.” |
| Queued | Producer durably accepted the request. |
| Building | Job began; show last received update, not a fictitious percentage. |
| Arriving | Producer published; device has not verified the complete result. |
| Ready | This device verified manifest and assets. Offer Open fresh paper; never interrupt reading. |
| Failed / expired | Safe explanation and explicit retry. |

Reconcile on launch/foreground, folder notifications, and periodic scans while active. Background delivery is best effort. Silence is not proof the desktop is asleep: use Waiting / Last update. Revoked permission preserves private outbox; reselection must not redirect an old request to another folder/producer.

## Protocol v1

Paths relative to the enabled export:

```
.deckle-press/
  producer.json
  requests/<reader-uuid>/<request-uuid>.json
  status/<request-uuid>.json
  editions/<edition-uuid>.deckle/
```

Fresh editions are nested deliberately: old clients and date-based root pruning continue to see scheduled bundles only. Updated client scans both locations.

Producer manifest fields: `format: deckle-press`, integer `version: 1`, `producer_id`, `paper_id`, display `paper_name`, `enabled`, `operations: [fresh_full]`, `updated_at`. IDs persist in local desktop config. This advertises configuration, not liveness/authentication. Unknown major versions disable requests.

Immutable request example:

```json
{
  "format": "deckle-press-request",
  "version": 1,
  "request_id": "78c08b13-6bed-46c2-aab7-73376423bc9d",
  "reader_id": "37824cbc-561b-4a90-8823-33f83bc0c1d6",
  "producer_id": "ba9e81c1-d5e3-43cb-bca1-acaf771caf79",
  "paper_id": "ea249ae5-771c-4d2b-9f11-5119fbe3feda",
  "operation": "fresh_full",
  "created_at": "2026-10-27T16:00:00Z",
  "expires_at": "2026-10-28T16:00:00Z"
}
```

client persists exact request bytes plus folder/producer binding in its private outbox, then atomically publishes using coordinated file access. Transport retries reuse the same bytes/ID. No credentials, URLs, config overrides, shell arguments, or destination paths are accepted.

Producer alone writes status snapshots: `format: deckle-press-status`, `version: 1`, request/producer/paper IDs, monotonic `revision`, `state` (queued/building/published/failed/expired/result_removed), `updated_at`, optional `job_id`, `started_at`, `error_code`, result `{edition_id, bundle, manifest_sha256}`. Result path is exactly `.deckle-press/editions/<edition-uuid>.deckle`. Published means local publication, not delivery. client independently derives Arriving/Ready and can receive published without intermediate states. Ignore older revisions; conflicting equal revisions fail closed. A shared job still produces a separate status per request.

Validate types, versions, UUIDs, path/ID agreement, bindings, timestamps, allowlisted operation, and 16 KiB maximum per control file. Ignore unknown optional fields. Validate producer/status files on client too. Reject traversal, escaping symlinks, non-regular files, and unexpected result paths. Use existing bundle path safety and asset-manifest checks.

Cloud placeholders/malformed partial JSON are retried; unchanged malformed bytes become a local diagnostic after 60 seconds, and changed bytes are reconsidered. Never derive an output path from invalid data. Ignore conflict-copy filenames. Different valid bytes for an accepted immutable request ID trigger protocol conflict. IDs are routing, not authentication: folder writers can request this bounded operation. Explain that opt-in can invoke existing configured source/model calls; secrets remain local.

## Jobs, idempotency, and recovery

Add bounded request scanning and a worker to `digest watch`. Scan configured enabled folders on its normal tick; cloud latency is unbounded. Builds run outside its scan loop and share the existing local build lock.

1. Validate request. Persist acceptance/hash/first-seen/job assignment in a local durable journal before queued status.
2. Coalesce queued requests for the same producer/paper/folder. Requests arriving during a running job attach to that result, showing its actual start time. Do not silently reuse a completed result for a later request.
3. Approved limits: one fresh job per paper per five minutes, one queued next job per paper, one active fresh job globally, 100 pending request IDs per paper. Excess valid requests receive busy. Scan at most 100 candidates per tick using a persistent round-robin cursor, so stale files cannot starve new requests.
4. At build admission, snapshot config and persist job/start/output UUID. Desktop configured timezone determines edition date at start. Check expiry and stopped/disabled state again after acquiring the build lock.
5. Gather/build into a job-specific local directory. Revalidate current sources, using conditional HTTP where supported; never return a previous entire paper as a fresh build. Source content may legitimately be unchanged. Preserve access rules and configured optional editor choice.
6. Publish assets then manifest into hidden staging under editions, verify locally, and rename into the immutable final UUID directory. Local atomic rename is not atomic cloud delivery.
7. Commit journal result and publish statuses. client validates manifest hash and assets before Ready.

Recover journal before scanning after restart. A valid completed output for that job is reused to republish status without fetching again. Incomplete job-owned staging can be discarded and the same job/output ID rebuilt. Guarantee one visible result per job, not exactly-once HTTP/model calls: crash recovery may repeat external calls.

Request TTL: 24 hours from creation; allow five minutes future clock skew. Reject invalid/far-future timestamps as clock_mismatch; expire unstarted requests, including queued ones. Desktop time is authoritative. Running jobs may finish after request expiry. Use bounded source timeouts plus a 15-minute fresh-job deadline; terminal timeout/failure requires explicit retry with a new ID. Transport retry keeps the original ID. A published result wins over client-side elapsed-time guesses.

client releases its pending-button lock at the request expiry plus five minutes according to its local clock when no terminal status has arrived. This is a local “Request expired without a reply,” not an assertion about producer state; retain the request/outbox for reconciliation and offer a new explicit request. Clock disagreement is explained if later reported by producer. A replaced/disabled producer manifest releases the local lock with “Producer changed/disabled”; never redirect or automatically resend the old request. Late published editions still appear in the archive and can satisfy the old request. A new request may coalesce with work already running; after that job completes it can legitimately produce another edition. V1 has no remote cancellation and does not promise that dismissing status stops work.

Persist an absolute UTC deadline (first build start + 15 minutes) and recovery attempt count in the journal. Restarts never extend the deadline; allow at most three attempts total. A queued job waiting for the build lock has not started this clock, but its request TTL still applies. Every attempt carries a journal generation token. Timeout/recovery invalidates that token before publishing failure or scheduling a new attempt. The final publication transaction must check active generation, nonterminal state, and deadline under the local lock; late workers cannot publish. Use cancellation-aware bounded I/O or an isolated worker process that can be terminated, and do not release the build slot while an old worker can still mutate shared state. Recover an already-valid committed final bundle before deciding a deadline failure.

## Edition identity and state isolation

Add bundle version **1.4** fields `edition.id` (UUID), `edition.paper_id` (UUID), and `edition.kind: fresh`. Keep 1.3 content semantics, UTC built_at, existing daily edition number, and date in configured timezone. Internal story IDs may retain their existing format, but every persisted mark/progress key must include edition identity.

Fresh client identity is `(paper_id, edition.id)` across library, caches, controller lookup, and private outbox. Fresh returns live at `.deckle-returns/<reader-id>/editions/<edition-id>.json`, contract version 1.1 with edition_id/paper_id and existing date/read/puzzle/marks. Legacy scheduled bundles retain their date-based identity and return paths. No silent merging/copying of morning marks or puzzle state into fresh editions; cross-edition carryover is deferred.

Group client archive by paper/date and sort by built_at descending within each date, with ID tie-break. Display “Fresh edition · 12:04 PM.” Keep scheduled editions accessible. Display names are never identity. Preserve old bundle/returns fixtures and add new ones.

Do not write fresh output into date-keyed Morgue, daily readership layouts, scheduled receipts, email/device queues, or other export destinations. Read saved-reading queue without consuming it; do not scan/move Inbox files or mutate tomorrow's preview. Reuse an existing daily puzzle; if absent, generate deterministically in job-local storage without replacing scheduled puzzle/solve state. Auth-session refresh may persist through existing handling. Audit gather-time writes and route remaining transient state to job-local paths.

Keep a local fresh-edition archive by paper/edition UUID for recovery; intraday desktop Morgue UI/search is deferred. client archive works in v1.

## Retention and errors

Keep newest seven fresh editions per folder independently of scheduled keep. Retain published results at least 48 hours even above that limit, and never prune outputs referenced by nonterminal jobs. Only producer-recorded outputs may be removed; never scheduled bundles, unrelated files, or client returns. client privately caches a verified opened edition for its reading session so pruning cannot break the open story. Apply the same result retention to local fresh archives.

Before pruning a result, producer first commits a higher-revision `result_removed` status for every request referencing it, including edition_id and a safe “Fresh edition is no longer available; print another” message. Then it removes only that recorded output. Status and deletion may sync in either order: client tolerates temporary Arriving, reconciles the later tombstone, and permits explicit retry. An already privately cached open edition stays readable, but is not advertised as a shared result. `result_removed` supersedes published and is terminal; retain its tombstone for 30 days after removal. If no bytes or status progress arrives for five minutes in Arriving, offer “Still syncing — check again or print another” and unlock the local action without claiming deletion; cloud delivery has no fixed timeout. Deleted shared results without a producer tombstone (for example, manual file deletion) use this same recoverable waiting behavior.

Keep terminal statuses and local journal records for 30 days. Producer never deletes client-owned requests; client can remove its own terminal/expired requests after 30 days. Producer may remove its statuses after 30 days. Immutable request expiry prevents replay after journal pruning. Expiry filtering avoids perpetually rewriting status for ancient requests.

Partial source failures use existing unavailable-section notes; fatal builds publish no result. Previous papers survive all failures. Use localized safe errors from an allowlist: busy, clock_mismatch, expired, disabled, presses_stopped, unsupported, source_failure, build_failed, timed_out, storage_unavailable, protocol_conflict. Never expose credentials/provider bodies. Full disk, missing root, permission revocation, corrupt manifest, and disabled paper must have explicit outcomes.

## Acceptance and implementation boundary

- [ ] Schema/example validation; bounded parsing, path/symlink rejection, and malformed/cloud-partial control files.
- [ ] One new immutable full edition, preserving morning bundle and marks; sources revalidated.
- [ ] Multi-device coalescing, rate/pending limits, stale file fairness, and late requests.
- [ ] Crash injection around journal commit, rename, and status publication; one visible result after restart.
- [ ] Reordered statuses, missing/corrupt assets, permission revocation, clock skew, expiry, midnight, offline producer, disabled/stopped paper, changed producer, and persisted timeout/attempt fencing.
- [ ] No unintended deliveries, Inbox consumption, date-keyed archive overwrite, or scheduled puzzle changes.
- [ ] Correct paper/producer routing, old/new identity coexistence, and isolated returns.
- [ ] Retention protects pending jobs, recent/opened results, unrelated files, and client returns; pruned-result tombstones and unanswered requests release client actions.
- [ ] Rust fake-network/clock/folder integration tests; client decoding/state/outbox conformance tests.
- [ ] Build/test the producer and client conformance fixtures; review the running desktop UI and verify two-client synced-folder delivery. Local fixtures cannot establish cloud latency.

Implement schemas/fixtures, the isolated producer worker, client conformance, desktop setup, and failure verification in independently testable increments.

