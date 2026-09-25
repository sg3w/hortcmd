# TICKET-326: Don't abort the whole directory compare on one bad entry

Discovered during a full-codebase review (2026-09-22).

## Problem

`read_level`'s `let entry = entry?;` and `let meta = entry.metadata()?;` in
`src-tauri/src/commands/fs/compare.rs` (around lines 67-68) propagate any
single-entry read/stat failure via `?`, aborting the *entire* `compare_dirs`
walk. This directly contradicts the sibling local walker's guarantee:
`walk.rs` explicitly treats "unreadable, vanished or changing entries" as
"counted, never fatal" (`walk.rs:19`, implemented at `walk.rs:225-234` and
`245-252`).

Comparing two large trees where a file is deleted or briefly unreadable
mid-scan (e.g. a log file being rotated, a build artifact being rewritten)
causes the whole `compare_dirs` command to return `Err`, discarding the
comparison the user was waiting on, even though the rest of the tree was
fine.

## Required change

- In `read_level`, skip an entry whose `metadata()` call fails (and treat a
  `read_dir` iteration error as "one bad entry", not a hard error) instead of
  using `?`.

## Verification

- Test: compare two trees where a file is deleted mid-scan; assert
  `compare_dirs` still returns a result for the rest of the tree instead of
  erroring out entirely.
- `npm run check`.
