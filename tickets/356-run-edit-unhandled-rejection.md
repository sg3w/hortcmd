# TICKET-356: Catch and report openPath failures in runEdit (F4)

Discovered during a full-codebase review (2026-09-22).

## Problem

`runEdit` (F4, the default "open file" shortcut) in
`src/features/commander/fileOps.ts` (around line 431) calls `openPath(...)`
without awaiting, void-ing, or catching it, so a rejected promise becomes a
silent unhandled rejection.

`navigate.ts`'s `openEntry` (the Enter-key path, which calls the exact same
`openPath`) explicitly does
`void openPath(...).catch(e => useOps.getState().requestConfirm({title: translate("open.error"), ...}))`
to show a friendly error dialog. `runEdit` has none of that — when a file's
default program fails to launch (no associated app, permission denied, or a
remote `systemOpen` failure), the user presses F4 and nothing visibly
happens, with only a console "Uncaught (in promise)" they never see.

## Required change

- Reuse the same catch-and-report pattern as `openEntry` in `runEdit`,
  ideally via one shared helper to avoid the duplication/drift going
  forward.

## Verification

- Test: press F4 on a file whose open fails (mocked IPC rejection); assert
  the same error dialog `openEntry` would show appears.
- `npm run check`.
