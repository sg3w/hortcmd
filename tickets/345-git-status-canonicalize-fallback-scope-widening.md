# TICKET-345: Don't silently widen git-status scope on a canonicalize failure

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src-tauri/src/commands/fs/git.rs` (around lines 108-117),
`dir_abs = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())`
falls back to the raw, non-canonical path on failure; if that then fails to
`strip_prefix(&top)` (which *is* canonical, from `git rev-parse
--show-toplevel`), `rel` silently becomes `""` via `unwrap_or_default()`.

If `canonicalize` fails for a directory `git -C` was still able to chdir into
(e.g. a transient permission/IO hiccup, or an intermediate symlink component
that resolves for `chdir` purposes but not for `realpath`), the code silently
computes and attributes *whole-repository* git status to the browsed
subfolder's top-level names instead of erroring — producing wrong
color-coding in the file list with no indication anything went wrong.

## Required change

- If `canonicalize` fails, treat it the same as a lookup failure (`return
  empty(false)` or otherwise abort) rather than silently widening the
  pathspec scope.

## Verification

- Test: simulate a canonicalize failure for a directory git can still chdir
  into; assert git-status output does not silently widen to the whole repo.
- `npm run check`.
