# TICKET-076: Preserve control focus and access in narrow layouts

## Findings

Browser verification for 114 at 640 by 360 CSS pixels found two concrete faults:

- The global Tab handler consumed keyboard focus traversal even on controls,
  changing the active pane while leaving focus behind. Enter/Space on controls
  could also reach file actions, and already consumed events were reprocessed.
- The function bar lost its rightmost actions beyond the viewport. Fixed detail
  columns overflowed their pane without a horizontal scrolling surface.

Interpose these fixes during 114 before completing accessibility verification.

## Changes

Respect native Tab focus order on controls and already consumed events. Preserve
native Enter/Space activation. Only the unfocused workspace keeps the legacy
Tab pane switch. A pane becomes active when focus enters its controls.

Wrap the function bar when needed. Derive the detail view's minimum width from
its configured columns and allow horizontal scrolling shared by header and rows.

## Verification

Keyboard regression tests verify that control activation does not open/mark files,
that consumed events stay consumed, and that Tab on controls/grids stays native.
Browser inspection confirmed all seven function actions at 640 by 360, horizontal
scrolling (360 px content in a 320 px pane), visible keyboard focus and native Tab
movement from the left date-sort button to the right tab. The viewport override
was reset. This checks limited viewport space; it does not claim OS screen-reader
or actual browser-zoom verification.

Full `npm run check` passed on Node 20.19.0: 387 Rust tests (25 ignored),
405 frontend tests, formatting, Clippy, generated bindings, TypeScript,
translation checks, production build and bundle budgets. Local commit only;
no push.
