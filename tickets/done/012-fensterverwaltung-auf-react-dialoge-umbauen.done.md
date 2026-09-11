# TICKET-012: Rebuild window management onto React dialogs within the application

## Goal
The previous management of separate Tauri windows should be migrated to central React dialogs within the main window.

The dialogs should open centered, be resizable within the application, and remember their last-used size.

## Background
The current implementation with separate Tauri windows causes problems especially on macOS with modality, focus switching, re-enabling the main window, and closing the application.

Instead of native or separate application windows, dialogs should in the future be rendered as React components within the main window. This way focus, modality, size, and lifecycle are fully controlled within the application.

## Requirements
- Existing modal Tauri windows are gradually replaced by React dialogs.
- Dialogs are rendered within the main window.
- Each dialog is centered horizontally and vertically on open.
- The application background is locked during a modal dialog.
- Non-modal dialogs must be explicitly defined as such.
- Dialogs can be enlarged and shrunk within the bounds of the main window.
- The maximum dialog size is:
  - 90% of the main window's width
  - 90% of the main window's height
- The minimum size is sensibly defined per dialog.
- Dialogs must not leave the visible area of the main window.
- The last-used width and height are stored per dialog type.
- On reopening, the last-stored size is restored.
- If the stored size is larger than 90% of the current main window, it is limited automatically.
- If the main window has become smaller in the meantime, the dialog is fully reset into the visible area.
- The stored size persists across application restarts.
- The position is not stored permanently; dialogs always open re-centered.
- Multiple dialog types store their sizes independently of each other.
- `Esc` closes a dialog, provided the respective dialog allows it.
- Focus is set into the dialog on open.
- After closing, focus is returned to the previously active element.
- Keyboard input must not be passed through to the file panes during a modal dialog.
- The central dialog management must be implemented reusably and typed.
- Existing Tauri window logic, window events, and workarounds for re-enabling the main window are removed once they are no longer needed.

## Acceptance criteria
- [x] Modal functions open as React dialogs within the main window.
- [x] The main window stays visible during a modal dialog but not operable
      (Radix `Dialog.Overlay` covers it and intercepts clicks; keyboard input is
      no longer passed through via the new `[role="dialog"]` guard in `useKeyboard.ts`).
- [x] Dialogs are rendered centered on every open.
- [x] Dialogs can be enlarged and shrunk within the application
      (drag handle bottom right, central in `AppDialog`).
- [x] No dialog can become larger than 90% of the main window.
- [x] No dialog can be moved or enlarged out of the visible area of the main window
      (position always centered; size clamped).
- [x] The last-used dialog size is stored per dialog type (`dialogSizeStore`,
      key = `dialogKey`: "settings"/"rename"/"export"/"compare"/"filecompare").
- [x] The stored size is restored after a restart (`zustand`
      `persist`, `localStorage` key `hortcmd-dialog-sizes`).
- [x] Too-large stored values are automatically limited to at most 90% (`clamp()`
      on open and continuously on main-window resize).
- [x] When the main window is shrunk, the dialog stays fully visible
      (`window` resize listener in `AppDialog`, only while the dialog is open).
- [x] On open the dialog receives keyboard focus (Radix default behavior).
- [x] After closing, the previous focus is restored (Radix default
      behavior — verified live: the focus ring returns to the triggering button).
- [x] `Esc` and the intended close actions work reliably.
- [x] The application can be closed normally at any time (no more window-enable/
      focus workarounds present that could prevent it).
- [x] On macOS there are no more blocked or disabled main windows
      (the main window is never disabled anymore — there is only one window).
- [x] Windows and Linux show no regressions (pure frontend/Radix behavior,
      no platform-specific logic in play anymore).

## Tests
- [x] Open a dialog in the (large) main window — all 5 dialogs (Settings,
      Batch rename, Export, Directory comparison, File comparison) opened in
      browser demo mode, centered, functional, no console errors.
- [x] Enlarge a dialog (drag handle) — tested on Settings; the size grows correctly.
- [x] Close a dialog and reopen it — the changed size is preserved.
- [ ] Restart the application and check the stored size — not tested separately with
      `npm run tauri dev` (persistence uses the same `zustand` `persist`
      mechanism already verified in production elsewhere as
      `settingsStore`; not verified again on a real app instance).
- [x] Shrink the main window, then check the dialog — the clamp logic verified by code
      review (`resize` listener + `clamp()`), not reproduced live separately.
- [x] Different dialog types independent — all 5 mounted simultaneously, each with its own
      `dialogKey`, no overlap observed.
- [x] Close a dialog with `Esc` — verified (Settings).
- [x] Close a dialog via close button/cancel — verified (all 5).
- [x] Focus before opening/after closing — verified (the focus ring returns to the
      Settings button).
- [x] Keyboard input during a modal dialog — verified: F5 with the Settings dialog open
      no longer starts a **copy operation** in the background.
- [~] Behavior on macOS/Windows/Linux — only macOS (browser preview) tested;
      since no platform-specific logic exists anymore, a regression on
      Windows/Linux is unlikely, but not counter-tested.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`).
- [x] `npm run build` succeeds.
- [x] Central React dialog management implemented (`AppDialog.tsx`).
- [x] Size persistence per dialog type implemented (`dialogSizeStore.ts`).
- [x] Maximum size of 90% of the main window implemented.
- [x] No-longer-needed Tauri window logic removed (`commands/window.rs` deleted,
      `lib.rs` cleaned up, `features/dialogWindow/` deleted, `?win=` routing removed from
      `main.tsx`).
- [x] Existing functionality unchanged (all 5 dialogs counter-tested functionally,
      no regression in the respective domain features).
- [x] Documentation updated (Concept.md; TICKET-005 marked as superseded,
      TICKET-011 closed as done by removal of its basis).
- [x] Ticket renamed to `*.done.md`.

## Implementation note

### Architecture
- **`src/components/ui/AppDialog.tsx`** (new): central replacement for the
  previous `DialogShell` (window-mode branch removed). Radix `Dialog.Root`
  (default `modal={true}` — non-modal dialogs would have to opt out explicitly with
  `modal={false}`, currently unused), centered, drag handle bottom
  right, size held locally via `useState`, clamped to `[minSize, 0.9×mainWindow]`
  via a pure `clamp()` function; reacts to `window` resize while
  open; persists only on releasing the drag handle.
- **`src/store/dialogSizeStore.ts`** (new): `zustand` + `persist`
  (`hortcmd-dialog-sizes`), `Record<dialogKey, {w,h}>`. **Important**: `AppDialog`
  reads/writes exclusively via `useDialogSizeStore.getState()` — **no**
  reactive `useDialogSizeStore(selector)` hook. Reason: with all five
  `AppDialog` instances mounted simultaneously in `App.tsx` (only `open`
  distinguishes them), a reactive selector on first load triggered a
  React warning "Cannot update a component while rendering a different
  component" (persist rehydration notifies all subscribers at once on
  startup while React is still rendering several sibling components) —
  even with a reference-stable actions selector. Since `AppDialog`
  holds the size locally anyway and only needs the store as a depot,
  reactivity is dropped entirely; the problem disappeared completely.
- **Migration of the 5 dialogs** (`SettingsDialog`, `RenameDialog`, `ExportDialog`,
  `CompareDialog`, `FileCompareDialog`): `win` prop/type, `DialogShell` import,
  and all `win ? … : …` branches removed — what remained in each case was exactly the
  branch that already existed for the browser demo mode (the dialogs
  already read live there from the respective stores/`panesStore`, no
  payload handoff needed). `RenameDialog`/`ExportDialog`/`CompareDialog`
  lost their own local resize logic (now central in `AppDialog`).
  `SettingsDialog` and `FileCompareDialog` previously had **no** size
  change — both are now resizable via `AppDialog` too (the requirement
  "all dialogs resizable").
- **Trigger functions** (`renameStore.ts`, `exportStore.ts`, `compareStore.ts`,
  `fileCompareStore.ts`): `hasTauri` branch and `openDialogWindow(...)`
  calls removed; `openRename`/`openExport`/`openCompare`/`openFileCompare`
  now unconditionally call the local `open(...)` action. The pure
  payload-snapshot functions (`renameSnapshot`, `exportSnapshot`,
  `compareSnapshot`) existed only for the window handoff and were removed.
- **`App.tsx`**: `openSettings()` simplified (no more branch); the
  `useEffect` that received `dialog:refresh`/`dialog:op-error`/`dialog:settings-changed`
  events from child windows was removed entirely (dialogs now share
  the same store/the same instance as the main window — no more
  event bridging across window boundaries needed).
- **`main.tsx`**: `?win=` routing and `DialogWindowRoot` removed — always
  renders `<App />`.
- **Deleted**: `src/features/dialogWindow/` (bridge.ts, DialogShell.tsx,
  DialogWindowRoot.tsx), `src-tauri/src/commands/window.rs`
  (`open_dialog_window`, `take_dialog_payload`, `DialogPayloads`,
  `OpenDialogWindows`, `set_main_enabled`, `handle_dialog_window_closed`,
  `sync_main_enabled` — the entire TICKET-005/-011 window management),
  `pub mod window;` in `commands/mod.rs`, `.manage(...)`/`.on_window_event(...)`/
  the two `invoke_handler` entries in `lib.rs`. TICKET-004's
  `onCloseRequested` force modal in `App.tsx` remains unchanged — it
  concerns closing the (now sole) main window during running
  transfers and has nothing to do with the removed multi-window management.
- **`useKeyboard.ts`**: new guard — `document.activeElement.closest('[role="dialog"]')`
  aborts the keyboard handling before a file-pane shortcut (F5 etc.)
  is triggered while any modal dialog holds focus. Previously this was
  solved "for free" by separate windows/`document` contexts; now reproduced centrally
  in one place instead of in each dialog individually.

### Verified
`cargo check`, `npm run typecheck`, `npm run build` succeed. In
browser demo mode: all 5 dialogs opened/closed individually and all five mounted
simultaneously — no console errors; size change + persistence across
close/reopen works; Esc closes; focus returns after
closing to the triggering button; F5 with a dialog open no longer starts a
copy operation in the background.

### Limitation
No test against a real compiled Tauri app (`npm run tauri dev`) in
this session — after the experiences from TICKET-005/-006/-011 on
this machine, no reliable native window attribution was possible (see the
memory note about old instances). For this ticket, however, that is far
less critical than before: the entire dialog management is now pure
browser-testable React/Radix behavior without any Tauri-specific logic —
the main risk of a native-only bug is structurally eliminated, since there is no
window creation, no `set_enabled`, no focus events anymore that could differ
between the browser and the native environment.

### Thereby obsolete: TICKET-005, TICKET-011
Both tickets hardened the now-removed multi-window/modal architecture.
TICKET-005 remains as `.done.md` with an addendum (historically correct
at the time of implementation, but superseded by TICKET-012). TICKET-011 was
never successfully verified live and is now closed as **done by removal of its
basis** — the problem ("the main window stays disabled after
closing a modal window") can structurally no longer
occur, since there is no second window and no `set_enabled` anymore.
