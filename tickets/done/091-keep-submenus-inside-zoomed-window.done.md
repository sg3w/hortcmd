# TICKET-091: Keep nested menus inside a zoomed window

## Problem

At native 200% zoom in a 700×720 window, More menus remains visible, but opening
its Commands submenu places that submenu at screen x=396 while the window starts
at x=730. The submenu is 564 pixels wide; labels on its left are clipped. Radix's
side flipping cannot fit two menu columns when neither side has enough room.

## Requirements

- Keep nested menu labels and controls within the viewport at increased zoom.
- Overlap the parent when there is insufficient space for adjacent columns.
- Preserve Arrow Right/Left, Escape, focus return and ordinary wide-window menus.
- Recalculate placement after viewport/trigger size changes and bound menu width.
- Add geometry regression coverage and verify the actual narrow native DE/EN
  overflow-menu flow, including nested navigation and focus restoration.
- Run `npm run check` and create a local ticket-only commit without pushing.

## Scheduling

Urgent during 249's required native menu and zoom verification.

## Implementation and verification

Menu widths are bounded by the viewport and a 400 CSS-pixel maximum. Submenus
measure their trigger and use adjacent columns when space permits. Otherwise,
they overlap the trigger's column and cap their width so Radix can fit either
horizontal placement. Placement updates on trigger resizing and window resize;
Radix retains ownership of keyboard interaction and focus.

Three geometry regressions cover ordinary adjacent columns, the reproduced
overflow and 50 combinations of viewport width, trigger size and position. The
existing menu interaction checks also pass. `npm run check` completed with 387
Rust tests (25 ignored), 491 frontend tests, and all other project gates.

The rebuilt unsigned native macOS test bundle was checked on 2026-09-13 using
the isolated 043 configuration. At 200% zoom in a 700×720 window starting at
(730,330):

- DE More menus → Commands placed its submenu at (840,433), size 564×542,
  wholly inside the window, instead of the prior x=396. A screenshot verified
  the complete labels. Special search → Duplicates opened a further submenu at
  (850,495), size 481×176. Left returned to Special search, then Commands;
  Escape returned focus to More menus. No command was invoked.
- EN More menus → View → Layout opened on Details view at (942,677), size
  461×209. Resizing the open window to 760×720 repositioned it to (982,677), still
  inside the window. Left returned to Layout, then View; Escape to More menus.
- At 1100×720 and 100%, Commands remained at (840,392), size 255×271, while its
  Special search submenu appeared beside it at (1089,423), size 241×88. Escape
  returned to Commands.

The app was left in German at 100%, with the original 1100×720 size and no open
menu or dialog. Ticket 249's other native and screen-reader checks remain open.
