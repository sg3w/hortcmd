# TICKET-002: Use real windows instead of HTML overlays

## Goal
hortcmd should in many cases open a real modal window and feel like a real, native application on all platforms.

## Background
The overlays constrain things too much to the app's own surface

## Requirements
- Change all naming and namespaces

## Acceptance criteria
- [x] Settings opens in a modal window
- [x] Batch rename opens in a modal window
- [x] Export file list opens in a modal window
- [x] Export directory opens in a modal window *(implemented as "Compare
      directories" — the only directory function with its own dialog)*
- [x] Compare file opens in a modal window

## Implementation
Each item opens its own native Tauri `WebviewWindow` (`index.html?win=<kind>`)
instead of an HTML overlay. Backend: `src-tauri/src/commands/window.rs`
(`open_dialog_window` / `take_dialog_payload`). Frontend: `src/features/dialogWindow/`
(`bridge.ts`, `DialogShell.tsx`, `DialogWindowRoot.tsx`); the dialogs share their
state with the main window via payload handoff + Tauri events. Without a Tauri runtime
(browser demo mode) the previous overlay behavior still applies.

Note: the line under "Requirements" ("change naming and namespaces") was a
copy-paste leftover from TICKET-001 and not part of this task.

Checked: `cargo check`, `npm run typecheck`, `npm run build` error-free; overlay fallback
and `?win=` route in the browser verified without console errors. The native
multi-window behavior is only runnable under `npm run tauri dev`.

## Tests
- The mentioned items open in a new window

## Definition of Done
- Code compiles
- Tests pass
- Existing functionality unchanged
- Documentation updated
- Rename the ticket to *.done.md
