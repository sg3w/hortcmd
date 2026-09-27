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

## Resolution (2026-09-27)

`buildPreview` no longer compares target names by exact string. It folds every
name through one `key()` helper before building the duplicate counter, the
source-name set and the folder-name set, so a target that collides with an
existing name only by case is flagged `exists` in the preview instead of
failing later with `fs.exists`, and two targets differing only by case are
flagged `duplicate`.

Folding is case-insensitive by default, which is what the default targets
(macOS, Windows) do. A fourth parameter `caseSensitive` lets a caller that
knows better opt into exact matching; the preview has no
`ProviderLimits.caseSensitive` to read today (it only reaches the frontend
inside a transfer's `OperationPlan`), so `RenameDialog` keeps the default.
Unknown therefore warns about a collision that might not happen rather than
hiding one that will — the safe direction, since the backend's
`provider.exists` check remains the authority at execution time.

Case-only renames of entries inside the batch keep working: the folded source
set still absorbs them, and `unchanged` is still decided by exact comparison so
"Readme.txt" → "readme.txt" reads as a real rename. Four tests cover the
collision the ticket describes, its case-sensitive counterpart, a case-only
duplicate pair, and the case-only rename that must stay `ok`.

Validation: `npm run check` passed on macOS.
