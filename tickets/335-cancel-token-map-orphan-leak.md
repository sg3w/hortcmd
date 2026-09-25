# TICKET-335: Bound growth of the cancel-token map

Discovered during a full-codebase review (2026-09-22).

## Problem

`cancel()` in `src-tauri/src/commands/fs/cancel.rs` (around lines 48-53)
unconditionally inserts a map entry via `.or_insert_with(...)` for tokens
that have already completed and unregistered, and nothing ever removes such
an entry — the module only shrinks the map via `unregister()`, called solely
by the *task's own* code path.

`cancel_dir_size(token)` (`size.rs:60-63`) or the search/`preflight`/
`search_index` cancel commands can be invoked by the frontend after the
corresponding task has already finished and called `unregister` (e.g. user
clicks Cancel just as a folder-size calculation completes). `cancel()` then
creates a fresh `CANCELS` entry for that now-dead token that nothing will
ever remove, growing the map by one orphaned `Arc<AtomicBool>` per such race
for the lifetime of the process.

## Required change

- Preserve "cancel-before-register" pre-arming while bounding growth: either
  have `cancel()` only set the flag on an *existing* entry (would need a
  different mechanism for the pre-arm case), or use a bounded/LRU map instead
  of an unbounded `HashMap`, or periodically/opportunistically prune entries
  whose `Arc` strong count is 1 (no task holds it).

## Verification

- Test: call `cancel()` for a token whose task has already finished and
  unregistered; assert the map doesn't grow unboundedly across many such
  races (e.g. assert eventual cleanup, or a capped map size).
- `npm run check`.
