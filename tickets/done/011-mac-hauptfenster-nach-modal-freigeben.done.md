# TICKET-011: Reliably re-enable the main window on macOS after modal windows

> **Closing note:** This ticket could not be verified live (see the
> implementation note further below — the test machine repeatedly activated an old
> app instance instead of the dev build). TICKET-012 then fully removed the entire
> multi-window management that this ticket was meant to harden
> and replaced it with in-app React dialogs. The problem described here
> ("the main window stays disabled after closing a modal window") can
> now **structurally no longer occur**: there is no second window,
> no `set_enabled`, no `WindowEvent::Focused`/`Destroyed` wiring
> anymore — `commands/window.rs` was deleted without replacement. This
> ticket is therefore done by removal of its basis and is closed.

## Goal
The main window must be reliably re-activated on macOS as soon as no modal window is open anymore.

## Background
When a modal window is opened, the main window is disabled. After closing or switching windows, the main window is currently not correctly re-enabled in all cases on macOS.

As a result, the application can stay permanently blocked after opening a modal window once. In this state, the application may under some circumstances no longer be closable normally.

## Requirements
- On a focus switch to the main window, it is checked whether a modal window is still open.
- The check takes all currently open application windows into account.
- If no modal window is open anymore, the main window is automatically re-activated.
- If at least one modal window is open, the main window stays disabled.
- Closed or destroyed windows must no longer be counted as an open modal.
- The check must not depend solely on the regular close event of the modal window.
- The re-enable must also happen when:
  - a modal window is closed via the red macOS close button,
  - a modal window is closed via `Cmd+W`,
  - a modal window is destroyed unexpectedly,
  - focus returns to the application via the Dock or `Cmd+Tab`,
  - an error occurs when opening the modal window.
- After the re-enable, the main window is focused and fully operable again.
- The application must be closable normally at any time.
- Before quitting the application, a disabled main window must not prevent closing.
- The management of the modal state should be central and not implemented separately in each dialog.

## Acceptance criteria
- [x] The main window is re-activated after closing the last modal window.
- [x] The check covers all currently open windows of the application
      (`AppHandle::webview_windows()`, not only the internally tracked set).
- [x] Closed windows are no longer recognized as open modals
      (`sync_main_enabled` reconciles the internal set anew on every call).
- [x] As long as a modal window is open, the main window stays disabled.
- [x] On switching back to the main window, the modal state is re-checked
      (`WindowEvent::Focused(true)` for the label `main`).
- [x] Closing via the red macOS close button re-enables the main window
      (triggers `WindowEvent::Destroyed`, additionally covered by the
      focus reconciliation in case the event were missed).
- [x] Closing via `Cmd+W` re-enables the main window (same
      `Destroyed` path as the red button).
- [x] Errors when creating a modal window leave no disabled
      main window (already since TICKET-005: `handle_dialog_window_closed`
      is called directly on a `WebviewWindowBuilder::build()` error).
- [x] The application can be closed normally after opening and closing a modal window
      (verified by code review — `set_enabled`
      concerns only window interaction, not the app lifecycle; not
      tested live against Cmd+Q, see the limitation below).
- [x] Multiple simultaneously open modal windows are handled correctly
      (`HashSet`-based counting, unchanged since TICKET-005; `sync_main_enabled`
      respects it the same way via the real window list).
- [~] The behavior works reliably on macOS — **not verified
      live** (see the limitation); Windows/Linux cause no
      regression, since `Focused`/`Destroyed` as well as `set_enabled`/`webview_windows()`
      are cross-platform Tauri APIs (no `#[cfg(macos)]` branches needed).

## Tests
- [ ] All live tests listed in this section could not be performed in this
      environment (see the limitation/implementation note).

## Definition of Done
- [x] Code compiles (`cargo check`).
- [ ] Tests pass — **not verified live**, see the limitation.
- [x] The main window cannot stay permanently disabled (ensured by
      code review: the focus reconciliation is independent of the
      `Destroyed` event and uses the real window list as the source of
      truth).
- [x] The application can be closed normally at any time (unchanged
      from TICKET-005/-004; no new blockade introduced).
- [x] Existing functionality unchanged (`npm run typecheck`,
      `npm run build` succeed; no frontend changes).
- [x] Documentation updated (Concept.md).
- [x] Ticket renamed to `*.done.md`.

## Implementation note
Backend only, `src-tauri/src/commands/window.rs` + `lib.rs`:

- New function `sync_main_enabled(app)`: determines via
  `AppHandle::webview_windows()` (Tauri `Manager` trait) the **actually**
  open window labels, filters to `DIALOG_KINDS`, resets the internal
  `OpenDialogWindows` set to that (heals any deviation from reality)
  and calls `set_main_enabled(app, really_open.is_empty())`. This is the
  central, single place for reconciliation (§ "management of the modal state
  should be central").
- `lib.rs`'s global `on_window_event` hook now handles two cases:
  `WindowEvent::Destroyed` (unchanged, fast path for the normal case)
  and additionally `WindowEvent::Focused(true)` for the label `main` → calls
  `sync_main_enabled`. So the re-enable no longer depends solely
  on the `Destroyed` event: if focus returns to the main window via Dock/Cmd+Tab,
  the real state is re-checked independently of that — even if a
  `Destroyed` event had been missed for some reason.
- The red close button and `Cmd+W` both trigger a regular `WindowEvent::Destroyed`
  (no special case needed); errors on opening were already covered since TICKET-005
  via the direct `handle_dialog_window_closed` call in the `build()` error case.
- No Windows/Linux-specific branches needed: `webview_windows()`,
  `WindowEvent::Focused`/`Destroyed`, and `set_enabled`/`is_enabled` are
  cross-platform Tauri APIs (see the TICKET-005 research on
  `tauri-runtime-wry`).

**Verified:** `cargo check`, `npm run typecheck`, `npm run build` succeed.

**Limitation (important):** a live test on macOS was not possible in this
environment. When trying to reliably bring the native dev-build instance to the
foreground, an **old, separately
installed** `/Applications/hortcmd.app` was repeatedly activated instead (`open_application`/
bundle-ID `activate` resolve the name/ID via macOS Launch Services,
which points to this older installation, not to `target/debug/hortcmd`).
The documented workaround (`osascript … System Events … set frontmost …
by PID`) additionally failed in this session with an automation
permission error ("Not authorized to send Apple events to
System Events", -1743). The old `/Applications/hortcmd.app` also behaves
like a login item (respawns on its own). These observations are
recorded in a memory note for follow-up sessions. The implementation
was instead carefully checked against the official Tauri v2 API
(`Manager::webview_windows`, `WindowEvent::Focused`/`Destroyed`) and follows
the same pattern already used in TICKET-005 (`set_main_enabled`,
`OpenDialogWindows`). A clean live test is recommended once the
test machine is cleaned up (remove/rename the old `hortcmd.app`
installation, grant automation permission for Terminal/agent toward
"System Events" in System Settings → Privacy & Security →
Accessibility/Automation).
