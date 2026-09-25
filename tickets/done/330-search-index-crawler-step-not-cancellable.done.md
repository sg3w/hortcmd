# TICKET-330: Make the index crawler's per-item loop cancellable and O(1) per item

Discovered during a full-codebase review (2026-09-22).

## Problem

The per-item insertion loop in `step()`
(`src-tauri/src/commands/fs/search_index/crawler.rs`, lines 351-373,
particularly line 354 `for item in rows` and line 364
`!snapshot.queue.contains(&item.path)`) has two problems.

`local()`/`remote()` can return up to 50,000 `Item`s for a single folder (the
`count >= 50_000` cap). Once returned, the `for item in rows` loop has **no
`cancel.load()` check** anywhere inside it (unlike `local()`'s own read loop,
which does check per-entry) — cancellation is only re-examined at the top of
the outer `for _ in 0..8` folder loop.

Within that uncancellable loop, every directory item does
`snapshot.queue.contains(&item.path)`, an O(queue length) scan of a
`VecDeque` (queue can hold up to 5000 entries) — up to 50,000 × 5000
comparisons for a single wide folder. This all runs while holding both the
`worker()` semaphore and the `Scope`'s mutex, so it also blocks every other
index operation and, per `index_cancel`, does not actually stop.

Indexing a local root containing one folder with many thousands of direct
subdirectories while the queue already holds several thousand pending
folders, then calling `index_cancel` mid-scan, leaves the running
`index_step`/live `index_query` unresponsive for a noticeable stretch and
starves other scopes of the shared worker permit.

## Required change

- Check `cancel.load(Ordering::Relaxed)` inside the `for item in rows` loop
  too.
- Replace `snapshot.queue.contains(&item.path)` with a `HashSet`/companion
  index (or rely on `snapshot.rows` membership, since a path already queued
  would also already be a `Row`) to avoid the O(n) scan per item.

## Verification

- Test: index a folder with many thousands of direct children while the
  queue already holds thousands of entries, then cancel mid-scan; assert
  cancellation takes effect promptly.
- `npm run check`.

## Resolution (2026-09-25)

Per-row insertion now checks cancellation and uses a hash set for queued paths. Cancelled folders remain queued for a safe retry; repeated rows replace their previous byte accounting. A deterministic wide-folder regression cancels after 1,000 insertions with 3,000 pending folders and verifies resumption without duplicate queue entries.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
