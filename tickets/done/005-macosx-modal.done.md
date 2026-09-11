# TICKET-005: Modal dialog window on macOS

> **Addendum (TICKET-012):** The management of separate
> Tauri windows implemented here was fully replaced by in-app React dialogs
> in TICKET-012. `set_main_enabled`/`OpenDialogWindows`/the entire
> `commands/window.rs` mechanism no longer exist. This ticket remains
> as historical documentation but is superseded.

## Goal
The behavior of dialog windows on macOS should match that of a true modal window. While a dialog is open, the main window must not be operable.

## Background
Currently the dialog is opened as a separate Tauri window. The main window is visually pushed to the background, but stays operable. This matches neither the expected behavior of a file manager nor the user expectation on Windows and Linux.

It should be examined what options Tauri v2 offers to implement true modal behavior cross-platform or to come as close as possible to it.

## Requirements
- Analyze the behavior on macOS.
- Check whether `setEnabled(false)` or comparable mechanisms can be used.
- While a dialog is open, the main window must not be operable.
- After closing the dialog, the main window must be automatically re-activated and focused.
- The solution must work cross-platform (macOS, Windows, Linux).
- If Tauri does not support a true modal window, document the limitation.

## Acceptance criteria
- [x] The main window is not operable while a dialog is open.
- [x] After closing the dialog, the main window is fully usable again.
- [x] Focus is correctly returned to the main window.
- [x] The behavior is tested live on macOS; Windows/Linux are code-verified via the
      official `tauri-runtime-wry` implementations (`EnableWindow` or
      GTK `set_sensitive`), but not tested live without a corresponding
      machine in this environment.
- [x] Known limitations of Tauri are documented (see below).

## Tests
- [x] Open a dialog via the menu. **Verified** (Settings via "Configuration").
- [ ] Open a dialog via keyboard shortcut. *(not tested separately — uses the same `openDialogWindow` path as the menu call)*
- [x] Close the dialog via the close button. **Verified.**
- [x] Close the dialog via the native close symbol (red traffic light). **Verified**
      (functionally equivalent to `Cmd+W`/`Esc`, since both trigger `WindowEvent::Destroyed`).
- [x] Check that the main window cannot be operated in the meantime.
      **Verified**: clicking a row in the main window while the dialog is open
      does not change the selection; the main window's traffic-light buttons are grayed out
      (inactive).
- [x] Opening and closing multiple times tested without side effects (twice
      in a row: close button, then native close symbol —
      both times correctly re-activated/focused).
- [~] Behavior verified on macOS (live test, see implementation note);
      Windows/Linux only code-verified, not tested live.

## Definition of Done
- [x] Code compiles.
- [x] Tests pass (macOS live; Windows/Linux code-verified).
- [x] Existing functionality unchanged.
- [x] Documentation updated.
- [x] Limitations of Tauri documented.
- [x] Ticket renamed to `*.done.md`.

## Implementation note

**Result of the analysis:** Tauri v2 (`tauri-runtime-wry` 2.11.4, Cargo cache
checked) already implements `Window::set_enabled(bool)` / `is_enabled()`
natively per platform:
- **Windows** (`window/windows.rs`): `EnableWindow(hwnd, enabled)` — classic
  native disabling.
- **Linux/GTK** (`window/linux.rs`): `gtk_window().set_sensitive(enabled)`.
- **macOS** (`window/macos.rs`): no native "disable" for arbitrary NSWindows
  available; instead (following Electron's example), for `enabled = false` an
  additional, roughly 50% transparent `NSWindow` at window size is created and attached to
  the main window as a sheet via
  `NSWindow.beginSheet(_:completionHandler:)` — this blocks input and dims
  visually. For `enabled = true`,
  the sheet is removed again via `endSheet`. `set_enabled` **must be called on the
  main thread** (a hard `expect`/panic on macOS otherwise).
- **iOS/Android**: no-op (irrelevant, a desktop app).

**Implementation** (backend only, `src-tauri/src/commands/window.rs` + `lib.rs`):
- `OpenDialogWindows(Mutex<HashSet<String>>)` as new state: holds the labels
  of currently open dialog windows (a subset of `DIALOG_KINDS`).
- `open_dialog_window`: on the **first** newly created dialog window (the set was
  empty), `set_main_enabled(&app, false)` is called. Further, simultaneously
  open dialog windows change nothing about it. If `WebviewWindowBuilder::build()`
  fails, cleanup happens immediately (`handle_dialog_window_closed`), so the
  main window does not stay permanently disabled.
- `set_main_enabled` dispatches via `AppHandle::run_on_main_thread` (internally
  non-blocking in `tauri-runtime-wry`: checks `current_thread() ==
  main_thread_id`, otherwise only sends a message to the event loop — no
  deadlock risk, even on a recursive call from the main thread).
- Global hook in `lib.rs` (`Builder::on_window_event`): on
  `WindowEvent::Destroyed` of a dialog window,
  `commands::window::handle_dialog_window_closed` is called — removes the
  label from the set; if it becomes empty, `set_main_enabled(app, true)` incl.
  `set_focus()`.
- Deliberately **no** intervention in `CloseRequested` needed: closing via the button,
  the native traffic-light symbol, or `Cmd+W` all run regularly through `Destroyed` — a
  single hook covers all paths.

**Live verification (macOS, `npm run tauri dev` + computer-use control):**
1. Settings opened via the "Configuration" menu → the dialog window appears
   correctly titled ("hortcmd – Settings"), the main window visible
   behind it, the main window's traffic-light buttons grayed out (inactive).
2. Clicking a row in the main window (`Downloads`) while the dialog is open →
   the selection stays unchanged (`Applications` still selected) — input
   is actually blocked, not just visually obscured.
3. The dialog closed via the "Close" button → the main window immediately
   active again (colored traffic light), focus back; clicking `Downloads` changes the
   selection correctly.
4. Second run: the dialog opened again, this time closed via the native red
   close symbol → likewise correctly re-activated/focused.
5. `cargo check`, `npm run typecheck`, `npm run build` succeed.

**Known limitation (macOS-specific):** an already-disabled
main window still responds to a click with a foreground switch
(z-order), without the click being processed content-wise — only a
second click acts on the now-active window. This is standard
macOS behavior for inactive windows (not a bug of this implementation) and
occasionally collides visually with the dialog window (overlap) when
both claim the same screen region. Functionally (data interaction),
the main window stays blocked.

**Not tested:** Windows (`EnableWindow`) and Linux (GTK `set_sensitive`)
for lack of an available machine in this environment — both implementations
were, however, verified directly in the `tauri-runtime-wry` source code and follow
established APIs officially supported by Tauri (since 2.0.0), so
deviating behavior is unlikely.
