# TICKET-087: Preserve host zoom shortcuts in the global keyboard handler

## Problem

Ticket 114's browser zoom check reproduced Ctrl+Plus opening Dateien markieren.
The global handler processes modifier shortcuts, then falls through to the plain
`+`/`-` pattern-selection actions. Ctrl/Cmd zoom shortcuts therefore open an
unrelated application dialog and prevent the browser's default action.

## Requirements

- Leave main-keyboard Ctrl/Cmd Plus, Equals, Minus and Zero unconsumed for host
  zoom behavior. Do not add a new native zoom implementation in this fix.
- Preserve plain Plus/Minus pattern selection and the deliberate numeric-keypad
  Ctrl/Cmd selection commands from ticket 059.
- Regress both modifier families and the existing selection behavior.
- Verify in the browser that Ctrl+Plus no longer opens a selection dialog.
- Run `npm run check` and create a local ticket-only commit without pushing.

## Scheduling

Interpose during 114 because the bug directly interferes with its required
standard-shortcut and zoom verification. This fix alone does not prove that
the host applies 200% zoom or that all layouts meet the zoom acceptance criterion.

## Verification

The modifier branch now returns without cancelling main-keyboard zoom keys;
numeric-keypad handling still runs first. Twelve regression cases cover both
Ctrl and Cmd, unchanged selection/filter state, unmodified pattern prompts and
modified numeric-keypad selection. Four cases failed before the fix; all twelve
pass afterwards.

On the freshly reloaded browser demo, Ctrl+Plus no longer opens a selection
dialog. The browser automation host still reported a 1280×720 viewport and scale
1, so this is shortcut-interception evidence only. Ticket 114's actual 200% zoom
validation remains open.

`npm run check` passed on 2026-09-13, including formatting, strict Clippy, Rust,
bindings, project gates, TypeScript, Vitest, production build and bundle budget.
