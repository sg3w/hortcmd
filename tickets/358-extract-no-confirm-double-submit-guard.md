# TICKET-358: Serialize/confirm archive extraction like copy and move

Discovered during a full-codebase review (2026-09-22).

## Problem

Extraction (F5/F6 while browsing an archive, or the "Extract" command) in
`src/features/commander/fileOps.ts` (`beginExtract` 272-282, `runCopy`/
`runMove` archive branches 315-329/331-345, `runExtractArchive` 379-394)
calls `startExtract` directly with no confirmation dialog and no
double-submission guard, unlike ordinary copy/move, which always go through
`beginTransfer` → `useOps.requestConfirm` (a real, explicit gate that
`transferQueue.test.ts` specifically tests: "queues a second request instead
of starting it"). `hasActiveTransfer()` only considers `t.op === "copy" ||
"move"`, so extract transfers are never queued or deduplicated either.

A user browses into a ZIP, selects files, and presses F5 twice in quick
succession (auto-repeat or a fast double press) before any UI state changes.
Each press runs `runCopy` → `beginExtract` → `startExtract` with a fresh id,
with nothing blocking the second call — two concurrent extract operations
race to write the same destination files, unlike a normal copy/move of the
same files which the app explicitly serializes and shows a confirm dialog
for.

## Required change

- Route `beginExtract` through the same `requestConfirm`/preflight gate
  `beginTransfer` uses, or at minimum extend `hasActiveTransfer`/queueing to
  cover `"extract"` so concurrent extracts of the same archive are
  serialized.

## Verification

- Test: trigger extraction twice in quick succession on the same archive;
  assert the second request is queued/blocked rather than racing the first.
- `npm run check`.
