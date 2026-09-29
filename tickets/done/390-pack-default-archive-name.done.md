# TICKET-390: Propose a real archive name when packing

Priority: **P2**. Status: **done**.
Reported from use on 2026-09-29: every ZIP the app creates is offered as
`archiv.zip`, so the name has to be typed by hand every single time.

## Problem

`runPack` (`src/features/commander/fileOps.ts:428-441`) opens the name prompt
with a hard-coded `initial: "archiv.zip"`. It already knows exactly what is
being packed — `targetPaths(side)` resolved the selection or the cursor entry a
few lines above — and throws that information away.

Three things follow from it:

- The proposed name says nothing about the content. Packing `Fotos 2024/`
  proposes `archiv.zip`, and the user retypes the folder name every time.
- Packing twice into the same destination proposes the same name twice, so the
  second pack walks into the overwrite dialog rather than landing beside the
  first archive.
- The prompt opens with the caret behind `archiv.zip` and nothing selected
  (`PromptDialog` only preselects when `selectRange` is given, which pack does
  not give — unlike `runRename`, `fileOps.ts:600`). Replacing the proposal
  means selecting it by hand first, and the shared prompt is 420px wide, which
  is narrow for a name that carries a timestamp.

## Required change

- A new pure module derives the proposal from what is being packed and is
  tested on its own, the way `renameRules.ts` is:
  - one folder, or several entries of which at least one is a folder → the
    name of the **first folder** in list order;
  - files only → the name of the **first file**, without its extension
    (`splitName` decides where the extension starts, so a dotfile keeps its
    name);
  - nothing usable → the previous fallback base.
- The proposal carries the local date and time, sortable and free of
  characters a file name cannot hold: `<base>_YYYY-MM-DD_HH-MM-SS.zip`. Two
  packs of the same folder no longer collide.
- The base is made safe for a file name: path separators and a colon become
  `_`, surrounding whitespace and trailing dots or spaces go, an empty result
  falls back, and an over-long base is cut so the proposal stays within what
  every target filesystem accepts.
- The prompt opens with the whole proposal except `.zip` preselected, so
  typing replaces name *and* timestamp while the extension survives; and the
  shared prompt dialog is wide enough for such a name.
- The entries the name is derived from are read in **list order**, not in the
  order the user happened to mark them in, so the same selection always
  proposes the same name. The packed sources themselves are unchanged.

## Verification

- Frontend test: the name module covers a single folder, a mixed selection
  (folder wins, first in list order), files only (extension dropped),
  a dotfile, an unsafe base (separators, colon, trailing dot, whitespace),
  an over-long base, and the empty fallback.
- Frontend test: `runPack` on a marked folder opens the prompt with
  `<folder>_<timestamp>.zip` and a `selectRange` that ends before `.zip`, and
  submitting it packs to that name in the other panel.
- `npm run check`.

## Implementation

`archiveName.ts` holds the whole proposal as two pure functions, next to
`renameRules.ts` for the same reason: a name computation is testable without a
store, a dialog or a runtime.

- `proposedArchiveName(entries, now)` picks the first folder in the given order
  and falls back to the first entry, so a mixed selection is named after the
  folder the user was most likely thinking of. A file loses its extension via
  `splitName`, which leaves a dotfile whole — the one place where "everything
  after the last dot" would have been wrong.
- `sanitize` replaces `/`, `\` and `:` with `_`, trims, cuts the base at 100
  characters and only then strips trailing dots and spaces, so a cut that lands
  on a dot cannot leave one behind. An empty result becomes `archiv`, which is
  what the prompt used to offer unconditionally.
- `timestamp` is local time as `YYYY-MM-DD_HH-MM-SS`: sortable, no colon, and
  distinct per second, so packing the same folder twice no longer walks into the
  overwrite dialog.
- `stemRange` is the preselection, derived from the name rather than from the
  constant `4`, so it stays right if the packer ever writes another extension.
- `runPack` reads its entries through a new `packedRows(side)`, which filters
  `entries` by the selection set instead of iterating the set. `targetPaths`
  keeps the set order for the sources, where order does not matter; the name
  must not depend on the order the user happened to mark things in.
- The shared `PromptDialog` is 520px wide instead of 420px. A name carrying a
  timestamp did not fit in the old field, and a name the user cannot see whole
  is one they retype — the point of the proposal.

### Verification

- `archiveName.test.ts`: the timestamp format; a single folder; a folder winning
  over surrounding files and over a later folder; the first file without its
  extension; a dotfile kept whole; separators, a colon, a trailing dot and
  surrounding whitespace sanitized; a 400-character base cut to 100 with the
  whole name under 255; and both fallbacks (nothing packable, and a base that
  sanitizes to nothing).
- `fileOpsErrors.test.ts`, "proposes the folder name with a timestamp when
  packing": a selection of `notes.txt`, `Fotos`, `Videos` marked in a different
  order proposes `Fotos_<timestamp>.zip`, the `selectRange` ends before `.zip`,
  and submitting it calls `create_archive` with that destination under the other
  panel's path.
- `npm run check` passed on macOS (arm64).
