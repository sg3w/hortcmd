# Directory and metadata cache

Directory navigation shares a memory-only Rust cache. Keys bind canonical local
paths (remote paths preserve symlink-sensitive components), hashed store identity,
connection generation and capability/transport policy. Reconnecting never reuses a
previous connection's entries. Local keys include filesystem object identity.

Listings expire after two seconds, with a 32 MiB weighted budget, 256 entries,
8 MiB and 64 entries per store. Listings above 50,000 children bypass retention.
Presentation-only local lstat metadata expires after one second, with independent
2 MiB / 4096 entry limits and 512 KiB / 1024 entries per store. Weights include
payload strings and estimated allocation overhead; this is a retention budget,
not a limit on memory required to return a single live directory. Expiration is
pruned on access and metrics reads. Nothing is persisted; process exit discards
all data.

Up to 32 distinct reads coalesce behind four workers. Dropping the last consumer
cancels queued work and signals active work. Local enumeration checks cancellation
per entry; protocol libraries that cannot interrupt a synchronous request finish
that request before discarding its abandoned result.

Application mutations invalidate before and after work, including failed or
partial operations. Retention is disabled while mutations run. Native events,
explicit refresh, disconnect, profile edits/deletion and trust changes clear both
caches. An epoch prevents a late in-flight result from repopulating invalid data.
Invalidation is deliberately global: conservative correctness takes priority over
reuse during concurrent mutations. Unobserved external changes are bounded by TTL;
explicit refresh and provider polling always read live state.

Only fresh cache entries are served. During refresh the pane labels its retained
rows as refreshing; failed refreshes and disconnected views show stale/offline
status. There is no persistent offline cache. Delete, overwrite, undo and transfer
commit checks use direct provider reads, never these presentation caches.

The `cache_metrics` command returns only listing/metadata entry and weighted byte
counts, hits, misses, evictions and invalidations. It exposes no keys, paths,
profiles, usernames or server addresses.

Regression coverage includes a real 200-file local directory: ten repeated reads
produce ten hits and zero additional listing misses, with identical rows. Explicit
refresh sees a newly created file and mutation guards prevent retained results.
Separate tests cover expiry, byte/item limits, provider fairness, connection and
capability isolation, invalidation races, redacted metrics and shared cancellation.
This verifies avoided provider work for the repeated-navigation scenario; broader
cross-platform timing and memory benchmarks remain in ticket 112.
