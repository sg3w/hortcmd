# TICKET-013: Replace custom recursive search traversal with jwalk or walkdir

## Goal

Use `jwalk` or `walkdir` for local recursive search traversal, preserving search
behavior while improving scalability, resource control, and maintainability.

## Background

`src-tauri/src/commands/fs/search.rs` implements its own recursive `walk` function
using `std::fs::read_dir`. `run_search` collects all files and directories before
processing matches, so the existing 50,000-hit limit does not bound traversal
memory or time to the first result.

Neither library is a direct dependency in `src-tauri/Cargo.toml`. `walkdir` occurs
in `Cargo.lock` only through other dependencies; application search does not use
it. `jwalk` is absent.

## Requirements

- Evaluate `jwalk` and `walkdir` against representative deep and wide local trees;
  document the choice using scan time, time to first result, peak memory, resource
  use, cancellation behavior, and cross-platform compatibility.
- Add the selected library as an explicit dependency and replace the custom
  recursive search walker with a reusable local traversal component.
- Prune ignored directory names before descending and preserve existing name,
  content, empty-directory, large-file, and duplicate-search semantics, including
  result metadata and duplicate grouping.
- Do not follow symlinks by default. Define and test equivalent Windows link and
  reparse-point handling to prevent loops or unintended traversal.
- Process entries incrementally and send bounded batches for modes that can emit
  results immediately. For modes requiring global aggregation or sorting, define
  explicit memory/work limits and report incomplete results accurately.
- Keep traversal off the UI thread. Bound parallelism and open resources, support
  cancellation and a disconnected result channel, and avoid recursive call-stack
  growth on deeply nested trees.
- Handle unreadable, deleted, and changing entries without panics; distinguish
  partial scans from complete results through an appropriate status contract.
- Keep remote enumeration behind the VFS provider interface; local filesystem
  walkers must not receive remote addresses.

## Acceptance Criteria

- [x] Local recursive search uses an explicit `jwalk` or `walkdir` dependency;
      the handwritten recursive search walker is removed.
- [x] The library decision is documented with reproducible measurements.
- [x] All four search modes retain their matching and result semantics.
- [x] Ignored directories are pruned and link cycles cannot cause endless scans.
- [x] Streaming modes produce results before the complete tree is collected.
- [x] Memory, concurrency, and resource limits are documented and enforced;
      cancellation stops further work promptly.
- [x] Partial, limited, and cancelled scans cannot appear complete.
- [x] Local traversal changes do not alter remote provider behavior.

## Tests

- Compare results for all four modes on fixtures with nested directories, ignored
  names, empty folders, duplicate files, and content/name filters.
- Cover deep and wide trees, symlink loops, Windows directory links, permission
  failures, and files disappearing during enumeration.
- Verify cancellation, channel closure, hit/work limits, and partial-scan status.
- Benchmark the existing walker and both candidates under the same conditions;
  include memory and time to first result alongside total scan time.

## Related Tickets

- TICKET-052: performance benchmarks and budgets.
- TICKET-054: indexed search; this ticket improves its non-indexed scan foundation.
- TICKET-056: folder usage visualization; assess reuse of the traversal component
  without expanding this ticket into a rewrite of every recursive operation.

## Definition of Done

- The selected library is integrated, measured, and covered by regression tests.
- `cargo check`, relevant Rust tests, `npm run typecheck`, and `npm run build` pass.
- Run `npm run gen:types` if IPC types change.
- Document traversal limits, error behavior, and the dependency choice.
- Rename this file to `013-library-backed-recursive-search.done.md`.


---

## Implementation

**Library choice: `jwalk` 0.9, two reader threads.** Measured against the old
handwritten walker and `walkdir` 2 on two fixtures (a 60,070-entry wide tree and a
400-level deep chain), fastest of three runs each, release build — see
`docs/traversal-benchmark.md` for the full tables and the reproduction command
(`cargo test --release --lib walk::bench -- --ignored --nocapture`). On the wide
tree: first result after 153 µs instead of 225 ms, total 168 ms vs. 353 ms
(`walkdir`) and 225 ms (handwritten collect-all). Four threads were no faster than
two but cost a ~3.7 MB rayon pool, so the default is two with a hard clamp of four.
`walkdir` stays in `[dev-dependencies]` so the comparison remains reproducible.

**New `src-tauri/src/commands/fs/walk.rs`** — the reusable local traversal
component. `walk_local(root, &WalkOptions, &cancel, on_entry) -> WalkReport`: the
directory reads run on the bounded jwalk pool, the callback runs sequentially on
the caller's thread, so hit limits and batching need no locks. `process_read_dir`
drops symlinks and ignored folder names *before* they are queued (their subtrees
are never read — asserted by a test on the reported file/folder counts), counts
files/folders/unreadable entries, and answers "is this folder empty?" from the read
that already happened, which removes the extra `read_dir` per folder the empty-folder
mode used to do. jwalk also invokes the callback once for the root itself
(`depth == None`); that call is skipped, otherwise a folder named `target` could not
be searched when it *is* the start folder. Unwanted entry kinds are dropped in the
same pass, so `empty_dirs` never `stat`s a file.

**`search.rs` rewritten on top of it.** All four modes keep their matching and
result semantics (covered by one shared fixture per mode). `files` and `empty_dirs`
stream; `large_files` replaces "sort the whole tree, then truncate" with a bounded
max-heap of 50,000 (same result set, bounded memory); `duplicates` is the only mode
that must see the whole tree and now stops at 300,000 candidate files. The limits
live in one `Limits` struct so the tests can shrink them instead of building
50,000-file fixtures.

**Status contract.** `search` returns `SearchSummary { status, hits, files, dirs,
unreadable }` instead of a `truncated` bool; `SearchStatus` is
`complete | truncated | limited | partial | cancelled`. Precedence: cancelled >
truncated > limited > partial, so a scan that hit a limit, skipped unreadable
entries or was stopped can never be reported as complete. The dialog shows the four
non-complete states (amber warning, grey for cancelled, skipped count for partial).

**Cancellation.** New command `cancel_search(token)`. The token registry that
`dir_size` already had was extracted into `commands/fs/cancel.rs` (namespaced per
task kind) and both commands now share it. The dialog cancels on Stop, when a new
search starts, and when it closes. A closed result channel is treated the same way:
`Sink::flush` notices the failed `send` and the walk stops.

**Errors.** Unreadable folders, entries whose `file_type` fails and files that
vanish between listing and `stat` are counted in `unreadable` and skipped — never a
panic, never an abort. Symlinks are neither followed nor reported, so `a → b → a`
loops terminate; on Windows the same code covers junctions and mount points because
`FileType::is_symlink` is true for both reparse tags (`#[cfg(windows)]` test using
`mklink /J`, which cannot run on this machine).

### Verified

`cargo check`, `cargo test --lib` (110 tests, 0 failures, no warnings),
`npm run gen:types`, `npm run typecheck`, `npm run build` all pass.

New tests: 12 in `walk.rs` (root not reported, pruning before descent, root never
pruned, empty-folder detection incl. "only ignored children is not empty", 400-level
deep tree, `Flow::Stop`, cancel before and during the walk, files vanishing mid-walk,
symlink loop, unreadable folder, Windows junction) and 12 in `search.rs` (all four
modes against a shared fixture, content search, hit limit, bounded large-file heap,
duplicate candidate limit, closed channel, cancelled run, unreadable → partial, deep
tree, symlink loop).

UI verified in browser demo mode: the dialog runs, lists hits, and renders the new
status line — amber "unvollständig – Einträge übersprungen (3)" for `partial` and
grey "abgebrochen" for `cancelled` (checked by temporarily returning those statuses
from the demo stub, then reverted). No console errors. The Stop button and the real
cancel path are backend-driven and covered by tests, not by the demo stub, which
returns instantly.
