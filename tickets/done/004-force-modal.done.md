# TICKET-004: Force modal when closing during running file operations

## Goal
When the main window of `hortcmd` is closed (window X, Cmd+Q, quit app)
while copy/move/extract/pack operations are still running, the app should
not close without comment and silently abort the operations.
Instead, a modal forces an explicit decision.

## Background
Currently there is no interception at all when closing the window (no
`onCloseRequested`, no `on_window_event` in the Rust backend). Closing
during a running transfer leaves, in the worst case, the target file
incomplete/corrupted without the user being warned.

## Requirements
- On a close attempt with at least one active transfer
  (`useTransfers` entry with `done === false`), closing is first
  prevented and a modal is shown.
- The modal offers three options:
  1. **Cancel & Close** – all running transfers are aborted
     (`cancel_transfer` per operation), then the window closes.
  2. **Wait** – the modal closes again, the app stays open, transfers
     continue normally.
  3. **Close anyway (force)** – the window closes immediately, without waiting for
     the abort (a hard cut, e.g. when an operation hangs).
- Without active transfers, the window closes immediately as before, without a modal.
- Applies only to the main window (dialog windows such as Settings/Rename/
  Compare do not start transfers themselves and need no separate protection).

## Acceptance criteria
- [x] Close without running transfers: no modal, the window closes immediately
      (the effect only intervenes when `event.preventDefault()` is called).
- [x] Close with ≥1 running transfer: the modal appears, the window stays
      open until one of the three decisions.
- [x] "Cancel & Close": all active transfers receive `cancel_transfer`,
      the window closes (does not have to wait for `fs-done`, since the backend aborts
      promptly).
- [x] "Wait": the modal disappears, the app stays normally operable, the transfer(s)
      continue unchanged. **Verified in browser demo mode.**
- [x] "Close anyway": the window closes immediately, regardless of the
      transfer state.
- [x] Browser/demo mode (without Tauri): no error, the feature is a no-op
      (no `window` API present). **Verified: no console errors,
      app operable unchanged.**
- [x] German and English translations present.

## Implementation
Frontend-only, since Tauri v2's `Window.onCloseRequested` already provides the necessary
control (no Rust changes needed):

- `src/App.tsx`: new effect registers `getCurrentWindow().onCloseRequested`
  (only if `hasTauri`). Checks `useTransfers.getState().transfers.some(t => !t.done)`;
  if so: `event.preventDefault()` + open the modal (new state, e.g. in
  `opsStore.ts`: `forceClose: boolean`).
- New dialog component (pattern like `ConfirmDialog` in
  `src/components/ui/OperationDialogs.tsx`, three instead of two buttons), above
  all other dialogs (`z-[80]+`).
- "Cancel & Close": iterates `useTransfers.getState().transfers.filter(t => !t.done)`,
  calls `cancelTransfer(id)` per entry, then `getCurrentWindow().destroy()`.
- "Close anyway": directly `getCurrentWindow().destroy()` (avoids
  `onCloseRequested` again, no loop).
- "Wait": reset the modal state, no further effect.
- New i18n keys under `op.forceClose.*` in `src/i18n/dictionaries.ts` (de/en).

## Tests
- Start copying large files, click the window X → the modal appears.
- Choose "Wait" → the app stays normally operable, the transfer continues.
- Choose "Cancel & Close" → the window closes, the process ends.
- Choose "Close anyway" → the window closes immediately.
- Close without a running transfer → no modal.
- Browser demo mode → close behavior unchanged (no-op, no error
  in the console).

## Definition of Done
- [x] Code compiles (`cargo check`)
- [x] `npm run typecheck` / `npm run build` succeed
- [x] Existing functionality unchanged
- [x] Documentation updated (Concept.md, `[x]` on the roadmap item)
- [x] Rename the ticket to `*.done.md`

## Implementation note
- Frontend-only, as planned (no Rust/`on_window_event` changes needed):
  `Window.onCloseRequested` from `@tauri-apps/api/window` is enough.
- `src/App.tsx`: new effect registers `getCurrentWindow().onCloseRequested`
  only if `hasTauri` (dynamic import as in `lib/windowState.ts`); checks
  `useTransfers.getState().transfers.some(t => !t.done)`, calls on an active
  transfer `event.preventDefault()` + `useOps.getState().setForceClose(true)`.
- `src/store/opsStore.ts`: new field `forceClose: boolean` + `setForceClose`.
- `src/components/ui/OperationDialogs.tsx`: new component `ForceCloseDialog`
  (Radix dialog following the pattern of `ConfirmDialog`, `z-[80]`/`z-[81]` – above
  all existing dialogs/overlay `z-[70]`). Three buttons: "Wait" (close the modal,
  no further effect), "Cancel & Close" (iterates active
  transfers → `cancelTransfer(id)` per entry, then `getCurrentWindow().destroy()`),
  "Close anyway" (directly `destroy()`). `destroy()` does not trigger another
  `onCloseRequested` (no loop).
- i18n: `op.forceClose.{title,message,wait,cancelAndClose,closeAnyway}` in
  `src/i18n/dictionaries.ts` (de/en).
- Verified: `cargo check`, `npm run typecheck`, `npm run build` as well as
  manually in the browser preview (demo mode): the effect is a no-op there (no
  `hasTauri`), no console errors; the modal rendering + "Wait" path was
  checked visually via directly setting `opsStore.forceClose` (Vite dev module import in
  the browser console) (all three buttons, translation,
  z-index correct). The native window paths (`destroy()` after cancel
  or immediately) follow exactly the pattern already used in production from
  `lib/windowState.ts`; a live test with a real opened Tauri window
  (not the browser preview) was not performed in this environment.
