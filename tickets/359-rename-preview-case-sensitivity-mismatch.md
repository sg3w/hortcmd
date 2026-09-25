# TICKET-359: Match the target filesystem's case-sensitivity in rename preview

Discovered during a full-codebase review (2026-09-22).

## Problem

Duplicate/collision detection in the batch-rename preview
(`buildPreview`, `src/features/commander/renameRules.ts`, around lines
141-174: `toCount` map keys, `folderNames.has(it.to)`) is exact-case string
comparison, but the default target filesystems (macOS, Windows) are
case-insensitive.

A folder contains "Readme.txt" (not part of the rename batch); the user
batch-renames "notes.txt" with a mask/case-mode that produces "readme.txt".
`folderNames.has("readme.txt")` is `false` (exact match against
`"Readme.txt"` fails), so the preview marks the row "ok" even though the OS
will treat the two names as the same file. The backend's `rename_batch_on`
does catch this at execution time via `provider.exists(&target)` (which is
case-insensitive on the OS filesystem) and reports `FS_EXISTS`, so no data is
actually lost — but the preview misleads the user into thinking the rename
is safe, and it then fails after the fact with no forewarning.

## Required change

- Normalize case (or otherwise match the target filesystem's
  case-sensitivity) when comparing `it.to` against `folderNames` and other
  computed targets in `buildPreview`.

## Verification

- Test: batch-rename a file to a name that case-insensitively collides with
  an existing unrelated file; assert the preview flags the collision.
- `npm run check`.
