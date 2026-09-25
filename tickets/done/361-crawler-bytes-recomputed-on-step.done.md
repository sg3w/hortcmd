# TICKET-361: Track the index crawler's byte-weight incrementally

Discovered during a full-codebase review (2026-09-22).

## Problem

`bytes(snapshot)` (O(total rows)) in
`src-tauri/src/commands/fs/search_index/crawler.rs` (line 353, inside the
`for _ in 0..8` loop at line 329, `bytes()` defined at lines 54-67) is
recomputed from scratch on every folder processed within one `step` call.

`bytes()` sums the size of every row and queued path in the whole snapshot
(up to `MAX_ROWS` = 50,000 rows). `step()` recomputes it up to 8 times per
call (once per dequeued folder), and `index_step` is invoked repeatedly
(poll-driven) while the index is building — so a large, near-full index pays
an O(rows) scan up to 8× on every single poll just to maintain a byte-weight
bound, on a path that's already time-boxed to 200ms.

## Required change

- Track the running weight incrementally on the `Snapshot` (add on insert,
  subtract on removal/retain) instead of recomputing it from all rows every
  folder.

## Verification

- Existing crawler/index tests continue to pass; optionally a test asserting
  the tracked weight matches a from-scratch `bytes()` computation after a
  sequence of inserts/removals.
- `npm run check`.

## Resolution (2026-09-25)

Snapshots now maintain a derived byte weight through row insertion/replacement, clearing, reconciliation and folder queue changes. Crawler insertion no longer rescans every row for each folder. The weight is omitted from storage and recalculated after deserialization. Regressions compare it with a full recount, including cancellation/resume.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
