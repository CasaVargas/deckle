# Search across library and devices (#41)

Search titles, authors and series from Deckle's library sidecars, connected Kindle inventories,
and the library's send records. Each whitespace-separated query term must occur in the combined
metadata, ignoring case and combining accents. An empty query lists everything.

The Books tab keeps its immediate local filter. Submitting its search form explicitly scans all
connected Kindles once and shows results with locations. This avoids opening MTP sessions on
every keystroke. The search is also available before a library is configured. Existing Kindle-tab
filtering uses the same matching rules. Search is read-only and results are a snapshot; submit
again after connecting or changing a device. Searches are disabled while the app is busy sending.

`deckle library search <query words> [--json]` uses the same Rust collector and search function.
JSON is an object with `results` and `warnings`. Each result has `title`, `authors`, nullable
`series`, and `locations`. A location has `kind` (`library`, `connected`, or `recorded_send`),
nullable `device`, `name`, and `path`. Results sort by folded title then authors.

A live row merges into a library book by a recorded path on that device or a known ASIN, never
by title alone. Successful inventories supersede past sends to that device, including removed
books. Failed inventories produce warnings and preserve historical results. Unplugged-device
locations explicitly say “previously sent; presence unverified.” Folder exports are excluded.

No full-text index or persistent inventory is added. Offline books Deckle never sent are unknown.
MTP uses the existing lightweight inventory and metadata cache; where metadata is unavailable,
the filename is searchable. It does not download entire books solely to search their contents.
Calibre and configured source folders retain their existing local Books filter but are outside
the cross-device search scope. Device-only editions are kept separate unless matched to a library
book by strong identity. The feature supersedes the earlier design's cross-device search non-goal.
