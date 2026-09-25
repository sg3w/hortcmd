# TICKET-314: Surface a failed rollback in the fast move/replace path

Discovered during a full-codebase review (2026-09-22).

## Problem

`commit()`'s replace-branch in `src-tauri/src/commands/fs/transfer/plan.rs`
(around lines 287-298) does `dstp.rename(to, &backup)?` then, on failure of
`dstp.rename(from, to)`, tries `let _ = dstp.rename(&backup, to);` and
swallows the rollback's own error.

`engine.rs`'s fast move path (`run.is_none() && remove_source && same_store &&
commit(...).is_ok()`, lines 392-396 — the ordinary move path on Windows/Linux,
where the recovery journal is `None` per `recovery::supported()`) calls this
`commit()` directly, with no `Step`/journal backing it up. Contrast with
`publish()`'s own copy of the same dance (`plan.rs:371-378`), which propagates
the rollback failure via `?` and has a journal `Item.backup` recording the
backup's location for later recovery.

On Windows/Linux, a move/overwrite where the destination filesystem briefly
becomes read-only or ACL-restricted between the two renames can make both the
primary rename and the rollback fail. The user's original file then sits under
a hidden `.hortcmd-backup-<pid>-<n>` name with nothing in any journal pointing
to it (no recovery record exists at all since `run` is `None` on these
platforms), and the surfaced error gives no hint the original file was
renamed away.

## Required change

- When the rollback rename also fails, don't discard that error — return a
  distinct error naming the backup path (or at least log it), and/or report
  it the same way `stale_working_entries` reports other leftovers, instead of
  folding it into `let _ =`.

## Verification

- Test: simulate the second rename failing and the rollback rename also
  failing (mocked provider); assert the returned error names the backup path
  rather than the generic move failure.
- `npm run check`.
