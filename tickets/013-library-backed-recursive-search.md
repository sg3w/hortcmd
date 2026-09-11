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

- [ ] Local recursive search uses an explicit `jwalk` or `walkdir` dependency;
      the handwritten recursive search walker is removed.
- [ ] The library decision is documented with reproducible measurements.
- [ ] All four search modes retain their matching and result semantics.
- [ ] Ignored directories are pruned and link cycles cannot cause endless scans.
- [ ] Streaming modes produce results before the complete tree is collected.
- [ ] Memory, concurrency, and resource limits are documented and enforced;
      cancellation stops further work promptly.
- [ ] Partial, limited, and cancelled scans cannot appear complete.
- [ ] Local traversal changes do not alter remote provider behavior.

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
