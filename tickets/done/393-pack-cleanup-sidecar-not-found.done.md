# TICKET-393: Packing reports "not found" for the cleanup it just completed

Priority: **P1**. Status: **done**.
Reported from use on 2026-09-29: every ZIP created on an external exFAT volume
ends with the error „Nicht gefunden", while the archive itself is correct and
complete.

## Problem

The pack succeeds and then fails. The journal record of such a run
(`transfers-v1/…json`) has both items in phase `Committed` — the staging
directory and the published archive — and still carries
`"reason": "fs.notFound"`, and the diagnostics log holds exactly one error for
the whole operation, ~10 ms before the operation ends. That is
`Stage::cleanup`, the last step of `pack::run_with_recovery`
(`src-tauri/src/commands/fs/archive/pack.rs:257-264`): it maps
`(Ok(_), Err(cleanup))` to `Err(cleanup)`, so a failed cleanup turns a finished
archive into a failed operation, reports `ok = 0`, and leaves the transfer row
sitting in the list with a message that contradicts what is on disk.

The cleanup fails because `Dir::remove_dir_all` (cap-std) lists a directory and
then deletes what it listed, treating a `NotFound` as fatal — and on FAT and
exFAT macOS keeps an AppleDouble sidecar `._x` beside every entry `x` that
carries metadata the filesystem cannot hold, and deletes that sidecar
*together with* `x`. The staging directory is created as
`data`, `._data`, `backups`, `._backups` in that order, so the removal deletes
`data`, the filesystem takes `._data` with it, and the next name in the listing
is gone:

```
$ ls -a /Volumes/Samsung_T5/…/.hortcmd-extract-2fd53e36…
.  ..  ._backups  backups
```

58 such directories were left behind on one volume. They are invisible to the
leftovers notice, which only knows the copy/move prefixes
(`is_working_entry`, `transfer/plan.rs:63`), so nothing ever offers to remove
them. `std::fs::remove_dir_all` tolerates the same race and is unaffected,
which is why deleting a folder on the same volume works.

## Required change

- A missing entry is what a removal is trying to achieve, so the staging
  cleanup must treat `NotFound` as success — for the entry it is deleting, for
  a listing that no longer resolves, and for the staged directory itself.
  Whatever is *really* left over still fails, with its own error.
- Both removals of a staged tree go through that one path: the cleanup and the
  failure path of `Stage::new_checked`.
- A pack whose archive was published and whose cleanup could not finish keeps
  reporting the cleanup error — that part is correct and stays: the error names
  the directory to inspect, and nothing silently swallows it.

## Verification

- Rust test: a nested staged tree is removed completely, and a tree that is
  already gone is not an error.
- Rust test (`#[ignore]`d, skips without the volume): on a directory given by
  `HORTCMD_SIDECAR_DIR` — a FAT/exFAT mount — a stage whose entries carry
  AppleDouble sidecars is created, committed and cleaned up without an error,
  and leaves nothing behind. It fails on the current code.
- `npm run check`.

## Implementation

The tolerance sits in one place, `remove_tree` in
`src-tauri/src/commands/fs/archive/staging.rs`, and both removals of a staged
tree now go through it — `Stage::cleanup` and the failure path of
`Stage::new_checked`. It lists and deletes like `Dir::remove_dir_all`, keeps the
capability discipline (every step through the staging `Dir` handle, the entry
type taken from the listing so a symlink is unlinked rather than descended
into), and treats `NotFound` as done rather than as a failure — for a listing
that no longer resolves, for the entry being deleted, and for the staged
directory itself. `std::fs::remove_dir_all`, which the user-facing folder delete
uses, already does the same, which is why only the archive staging was affected.

What was *not* changed: a cleanup that really fails still fails the operation
with `extract-cleanup` and the directory to inspect. The bug was the false
alarm, not the report.

### Verification

- `a_nested_tree_is_removed_and_one_already_gone_is_not_an_error`: a tree with
  files two levels down goes completely, and removing the same name again is
  `Ok`.
- `a_stage_beside_sidecars_cleans_up_without_an_error` (`#[ignore]`d, skips
  without `HORTCMD_SIDECAR_DIR`): run against a real exFAT stick
  (`/Volumes/Samsung_T5`, fskit) it passes on the fix and reproduces the report
  exactly on the previous code — `AppError { code: "fs.notFound", op:
  "extract-cleanup", detail: "Staging cleanup failed; inspect
  .hortcmd-extract-…" }`.
- Diagnosed from the user's own data rather than guessed: the journal records of
  the failing packs (both items `Committed`, `reason: "fs.notFound"`), the
  single error in `diagnostics/events-0.jsonl` ~10 ms before each operation
  ended, and the 58 half-removed `.hortcmd-extract-*` directories left on the
  volume, each holding exactly `backups` and `._backups`.
- `npm run check` passed on macOS (arm64), bundle report within budget.
