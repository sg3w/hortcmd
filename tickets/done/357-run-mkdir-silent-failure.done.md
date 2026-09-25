# TICKET-357: Report makeDir failures in runMkdir (F7)

Discovered during a full-codebase review (2026-09-22).

## Problem

`runMkdir` (`src/features/commander/fileOps.ts`, around lines 612-633) calls
`await makeDir(...)` inside `void withBusy(async () => {...})` with no
try/catch, unlike the structurally identical `runRename` a few lines above
(598-608), which wraps its single-item async IPC call in try/catch and
reports failures via `reportMessage(describeErrorFull(e))`.

A user presses F7 and types a folder name that collides with an existing
entry (or hits a permission error, or the location disconnects mid-dialog):
`makeDir` → `invoke("make_dir", ...)` rejects; `withBusy` re-throws (it only
has `finally`, no `catch`); the rejection is unhandled. The busy flag resets
correctly, but the folder is never refreshed and the user gets zero feedback
that the mkdir failed — they may believe it succeeded or just be confused.

## Required change

- Wrap the `makeDir` call in try/catch and call
  `reportMessage(describeErrorFull(e))`, mirroring `runRename`.

## Verification

- Test: F7 mkdir with a name colliding with an existing entry (mocked IPC
  rejection); assert an error message is shown to the user.
- `npm run check`.

## Resolution (2026-09-25)

F7 mkdir now catches and formats creation failures rather than leaving rejected promises unhandled. A mocked collision regression verifies the reported message, trimmed destination path and restored busy state.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
