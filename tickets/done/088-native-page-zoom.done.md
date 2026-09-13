# TICKET-088: Enable native page zoom without selection shortcut conflicts

## Problem

While verifying ticket 043 after 087, inspection of the configured Tauri window
and pinned runtime showed that native page zoom is disabled. The existing native
Window → Zoom item maximizes the window; application font/icon settings do not
zoom the full page. Tauri's optional hotkey polyfill also handles numeric-keypad
Plus/Minus, conflicting with ticket 059's selection commands.

## Requirements

- Provide actual WebView page zoom through the native Tauri command.
- Offer localized View-menu controls and standard main-keyboard Cmd (macOS) or
  Ctrl (Windows/Linux) Plus/Equals, Minus and Zero, including inside dialogs.
- Support at least 200%, with bounded increments and a reset to 100%.
- Preserve browser-owned zoom, numeric-keypad selection and Alt/IME combinations.
- Serialize asynchronous zoom changes, recover from failures and report errors.
- Add regression tests, run `npm run check`, and verify actual native enlargement
  and reset. Record any remaining platform/layout verification honestly.
- Create a local ticket-only commit; do not push.

## Scheduling

Urgent prerequisite for ticket 043's native zoom acceptance check.

## Implementation and verification

View offers localized zoom in/out/reset controls, also listed in the generated
shortcut overview. The native key listener applies 25-percentage-point steps
from 50% to 300%, with 100% on reset. It owns only the platform modifier and main
keyboard zoom keys, including modal and terminal focus. The existing IPC layer
calls Tauri's WebView zoom command with its narrowly scoped main-window
permission. Browser zoom stays with the browser. Failed native requests leave
the confirmed scale unchanged and show a localized error notice.

Eight new tests cover both platform modifiers, modal input, main-keyboard and
numpad separation, browser passthrough, Alt/IME exclusions, request ordering,
failure recovery, bounds and menu dispatch. `npm run check` passed with 387 Rust
tests (25 ignored) and 488 frontend tests, plus all remaining project gates.

An unsigned macOS test bundle using `com.hortcmd.a11ytest` and the isolated 043
configuration was built and exercised on 2026-09-13. At fixed 1100×720 window
size, the native settings toolbar button measured 25×25 at 100%, 32×32 after the
View-menu zoom-in action, and 50×50 at 200%. Physical Cmd-Plus on the German
keyboard (System Events key code 30) increased zoom. Settings opened at 200%;
Cmd-Minus and Cmd-0 worked within the modal without losing General-tab focus.
Escape returned to Settings, again 25×25. A separate physical Cmd-Minus test
reduced the button to 21×20, and Cmd-0 restored 25×25. Synthetic `keystroke "+"`
did not establish the same keyboard behavior, so the physical-key test is the
shortcut evidence. The app was left at 100% with the dialog closed.

Actual native page enlargement and reset are verified; the complete 043 layout
and screen-reader matrix, and Windows/Linux native verification, remain open.
