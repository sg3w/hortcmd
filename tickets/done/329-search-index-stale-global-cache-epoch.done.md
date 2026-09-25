# TICKET-329: Stop restarting the search index on unrelated filesystem activity

Discovered during a full-codebase review (2026-09-22).

## Problem

`epoch()`/`Scope::stale()` in
`src-tauri/src/commands/fs/search_index/mod.rs` (lines 64-69 and 214-218,
consumed at 428-441) reuse an unrelated, app-wide cache ticket as this scope's
staleness signal. `epoch()` reads `cache::listings().lock().ticket()` — the
global epoch of the app-wide presentation directory-listing cache
(`cache.rs:70-71,175`). That epoch is bumped on `cache::invalidate()`, which
fires on **any** filesystem-change event observed by **any** open pane's file
watcher anywhere in the app (`refresh/subscriptions.rs:164`) and on any
explicit `read_fresh` refresh (`listings.rs:155`) — not scoped to the indexed
root at all. `Scope::stale()` treats `epoch() != self.observed` as "stale,"
and `index_step` calls `scope.restart()` (bumping `cycle`/`revision`,
clearing the queue) whenever the queue is empty and `stale()` is true.

Browsing/writing to an *unrelated* folder anywhere in the app forces a full
re-crawl of every finished index scope, and because `restart()` bumps
`revision`, any in-flight `index_query` pagination gets invalidated
(`query.rs:41-42,54-58`: "pagination requires an index revision" / "index
changed; restart pagination"). The scope's own dedicated `notify` watcher
(`Scope::new`, lines 178-204) already tracks real changes to the indexed root
precisely — this global-epoch check is both redundant and overly broad.

## Required change

- Drop the `epoch()`/global-cache check from `stale()` entirely (the
  per-scope `notify` watcher plus the 300s `completed` fallback already
  cover staleness correctly), or replace it with a signal actually scoped to
  the index's own root.

## Verification

- Test: create a persistent index over one folder, then browse/write in an
  unrelated folder; assert the index scope does not restart and any in-flight
  paginated query is not invalidated.
- `npm run check`.

## Resolution (2026-09-25)

Index freshness now follows the indexed scope’s watcher and five-minute reconciliation fallback instead of the global directory-cache epoch. Unrelated file activity preserves snapshot revisions and pagination cursors. Regression coverage verifies unrelated invalidations, stable pagination, root watcher invalidation, and the reconciliation timeout.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
