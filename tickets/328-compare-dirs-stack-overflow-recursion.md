# TICKET-328: Convert walk_compare's recursion to an explicit worklist

Discovered during a full-codebase review (2026-09-22).

## Problem

`walk_compare` in `src-tauri/src/commands/fs/compare.rs` (around line 186)
recurses directly, one native stack frame per directory level, instead of
using an explicit-queue walker. This is exactly the hazard `walk.rs` calls out
and solves by design ("No recursion: `jwalk` keeps its own explicit queue, so
deep trees cannot overflow the call stack", `walk.rs:17-18`).

Comparing two trees with several thousand nested levels (e.g. deeply nested
build/vendor output, plausible on systems with long-path support) can
overflow the stack of the `spawn_blocking` thread `compare_dirs` runs on
(`compare.rs:222`), aborting the process rather than returning an error — a
hard crash instead of a `Result`.

## Required change

- Convert `walk_compare`'s subdirectory descent into an explicit
  worklist/stack instead of recursive calls, matching `walk.rs`'s approach.

## Verification

- Test: compare two trees with a very deep nesting level (thousands of
  levels, within filesystem path-length limits where possible); assert the
  command returns an error/result rather than crashing the process.
- `npm run check`.
