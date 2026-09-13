# TICKET-043: Establish accessibility and localization quality gates

## Goal

Make all core workflows usable with keyboard and assistive technology in both
German and English, and prevent untranslated or inaccessible UI from entering the
application unnoticed.

## Background

Radix components provide a useful accessible base and several controls already use
ARIA attributes. However, user-facing labels remain hard-coded in components,
including German labels such as the drive selector and directory-tree expansion
controls. Some dynamically built labels append German text, while backend errors
are not consistently localized. The application relies heavily on global function
keys, focus restoration, virtualized file lists, custom tree behavior, dialogs,
and icon-only buttons, all of which require deliberate semantics and testing.

TICKET-014 covers one concrete Enter-key leak, TICKET-033 establishes component
tests, and TICKET-036 owns structured backend errors. This ticket supplies the
cross-cutting accessibility and translation standard.

[TICKET-040](040-native-application-menu.md) owns the full React menu inside the
Tauri window on all platforms and a minimal native macOS menu. Test both surfaces;
do not replace the in-window menu with a full native macOS application menu.

## Requirements

- Inventory user-visible strings, accessible names, titles, status messages, and
  keyboard instructions; move them to typed German/English resources.
- Enforce translation-key parity and detect hard-coded user-facing text in review
  or CI, with narrow documented exceptions for file content and protocol names.
- Define semantic patterns for the two file panes, virtualized tables, directory
  tree, tab bars, menus, dialogs, progress displays, notifications, and terminals.
- Give every icon-only control an accurate localized accessible name and state.
- Ensure focus order is predictable, visible, and restored to the invoking control
  after menus and dialogs close.
- Announce transfer progress, completion, cancellation, collisions, connection
  changes, and errors through appropriately throttled live regions.
- Preserve platform-standard shortcuts and document application-specific shortcuts;
  never trap keyboard focus outside an intentional modal or terminal session.
- Support 200% zoom, increased font/icon settings, reduced motion, high contrast,
  and system light/dark preferences without clipped critical controls.
- Define minimum contrast and target-size requirements for custom themes and file
  colors, including a non-color status cue where meaning depends on color.
- Add automated accessibility checks and a documented manual screen-reader matrix.

## Acceptance Criteria

- [ ] Core copy, move, rename, delete, search, settings, connections, and terminal
      workflows are keyboard-complete in German and English.
- [x] Automated checks report no serious or critical accessibility violations in
      representative screens.
- [ ] Every interactive control has a localized name, role, state, and focus style.
- [ ] Dynamic operation and connection state is announced without excessive noise.
- [ ] No production component contains an undocumented hard-coded user-facing
      German or English string.
- [ ] Core layouts remain usable at 200% zoom and with increased application scale.
- [ ] Status is not communicated by color alone.

## Tests

The user moved native Windows/NVDA and Linux/Orca manual verification to
[TICKET-250](250-windows-linux-screen-reader-verification.md) on 2026-09-13.
Those checks remain unverified but no longer block this ticket. The macOS manual
checks and the shared automated accessibility/localization requirements remain
in scope here.

Current evidence is recorded in `docs/accessibility.md`. The loaded shell,
representative dialogs, menus, virtual lists/tree/tabs and terminal controls pass
DE/EN semantic axe checks. This completes the automated-violation criterion,
not the manual screen-reader or actual-zoom requirements. Those remain open.

- Add axe-based component tests for the application shell and representative
  dialogs, menus, tree, table, tab, progress, and terminal controls.
- Add keyboard-only interaction tests for pane navigation and every core operation.
- Test focus restoration and modal isolation with nested prompts and async errors.
- For TICKET-040, verify arrow/Enter/Escape menubar navigation, focus restoration,
  checked/radio/disabled states, accessible unavailable-action explanations, and
  overflow at 200% zoom. Menu keystrokes must not leak into quick filtering or
  file operations. Cover text inputs, contenteditable and SSH terminal focus,
  native macOS text editing, and duplicate native/WebView shortcut dispatch.
- Snapshot translation-key parity and render representative long strings in both
  languages.
- Manually test VoiceOver using the documented smoke-test script. The NVDA and
  Orca runs are tracked in TICKET-250 at the user's request.
- Test 200% zoom, reduced motion, high contrast, and light/dark system changes.

## Definition of Done

- Accessibility semantics and translation ownership are documented and enforced.
- Core workflows pass automated and manual keyboard/screen-reader checks.
- German and English resources have complete parity.
- Hard-coded accessible labels and user-facing strings are removed or justified.
- Accessibility regressions fail CI through the foundation from TICKET-033.
- Rename this file to `043-accessibility-and-localization-quality.done.md`.
