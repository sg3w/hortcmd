# TICKET-089: Confine command-bar overflow to its own scroll area

## Problem

The native 200% zoom check for ticket 249 found the Settings toolbar button
outside the 1100×720 window. Focusing it scrolled the document horizontally:
the window began at screen x=730, but the Files menu then began at x=692 and was
partly clipped. The command bar's unbounded row lets focus scrolling move other
application surfaces, despite the body's hidden overflow.

## Requirements

- Keep overflowing toolbar controls in a horizontal scroll area owned by the
  command bar, with their normal target sizes.
- Keyboard focus must reveal both ends without shifting the menu or file panes.
- Verify actual native 200% zoom, focus traversal, menu position and reset.
- Preserve vertical space for file rows; do not add wrapped toolbar rows.
- Run `npm run check`, rename the completed ticket and commit it locally.

## Scheduling

Urgent during 249 because this directly breaks its zoom/focus acceptance.

## Implementation and verification

The command bar owns horizontal overflow and does not shrink its direct children.
The row stays on one line, and keyboard focus scrolls that row independently.

On 2026-09-13 the unsigned native macOS test bundle was rebuilt with this change
and the isolated 043 configuration. At actual 200% zoom in a 1100×720 window
starting at (730,330), twelve Tab presses from Up reached Settings at (1764,431)
with size 50×50, fully inside the window. Shift-Tab traversal returned to Up at
(786,431), also 50×50. Files stayed at (746,368) before traversal, at Settings and
after returning. A screenshot confirmed the independent toolbar scrollbar,
stationary menu/pane split, and single toolbar row. In the unfixed build,
focusing Settings had moved Files to x=692, outside the window's x=730 edge.

Cmd-0 restored 100%; Settings was inside the window at (1797,404), size 25×26,
and Files at (738,365). The app remains at 100% with no dialog open.

`npm run check` passed: 387 Rust tests (25 ignored), 488 frontend tests and all
formatting, lint, bindings, documentation, type, build and bundle checks. This
layout-only correction was verified through native rendering and traversal;
no test asserting Tailwind class strings was added. Ticket 249's remaining
layouts and screen-reader matrix are still open.
