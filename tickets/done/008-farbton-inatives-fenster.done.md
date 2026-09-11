# TICKET-008: Highlight the selection in the inactive file pane

## Goal
The currently selected file should stay clearly visible in the inactive file pane too.

## Background
Currently the selection in the inactive file pane is hard to see or not visible at all. As a result the user quickly loses orientation during file operations, especially when working with two directories.

The selected file in the inactive panel should be shown – similar to Total Commander – with a subtle green highlight. The highlight should be visually clearly distinct from the selection in the active panel.

## Requirements
- The selected file in the inactive file pane gets a green highlight.
- The highlight uses slight transparency so the file type or icon stays clearly recognizable.
- The presentation is clearly distinct from the selection in the active file pane.
- Only one file per panel may ever be marked as the current selection.
- When the active panel changes, the highlight is updated accordingly.
- The highlight must work with the light and dark theme.
- The highlight uses the hue `#9AC27B` (RGB 154, 194, 123) with a transparency of about 35–45% (`rgba(154, 194, 123, 0.35–0.45)`).

## Acceptance criteria
- [x] The selection in the inactive panel is clearly visible at all times.
- [x] The green highlight uses slight transparency.
- [x] The active selection stays visually more dominant than the inactive one.
- [x] The highlight is updated correctly on a panel switch.
- [x] The presentation works regardless of the theme used.

## Tests
- [x] Switch between the left and right panel. **Verified**: the `active` prop
      controls the presentation correctly per panel.
- [x] Select a file in the inactive panel and check visibility. **Verified**
      via store manipulation: cursor+selection in the inactive panel still shows
      the (more dominant) yellow selection color, not green — priority correct.
- [x] Switch between the panels multiple times.
- [x] Check the behavior in the light and dark theme. **Verified**: the computed
      background in both themes is exactly `rgba(154, 194, 123, 0.4)`.
- [x] Check the presentation with different icon and file types.

## Definition of Done
- [x] Code compiles.
- [x] `npm run typecheck`, `npm run build` succeed.
- [x] Existing functionality unchanged.
- [x] Documentation updated (Concept.md).
- [x] Ticket renamed to `*.done.md`.

## Implementation note
Cause: `stateClass()` in `src/features/panel/FileTable.tsx` gave the
cursor row in the **active** panel both a background color (`!bg-accent-dim`)
and an outline, but the cursor row in the **inactive** panel only a
1px outline (`outline-[color:var(--cursor-inactive)]`) — no fill, hence
"hard to see or not visible at all".

- `src/index.css`: new CSS variable `--cursor-inactive-marker:
  rgba(154, 194, 123, 0.4)` in both theme blocks (`:root` and
  `:root[data-theme="light"]`) — identical value for both themes, since the
  specification (`#9AC27B`, 35–45% transparency) is theme-independent.
- `FileTable.tsx`'s `stateClass()`: in the inactive branch additionally
  `!bg-[color:var(--cursor-inactive-marker)]`, but only when the row is not
  additionally selected (`!isSelected`) — so the yellow
  multi-selection color (`!bg-selection`) with cursor+selection stays
  dominant, as required. `active`/`isCursor`/`isSelected` were already
  parameters, no new prop wiring needed. A redundant, ineffective
  line (`isCursor && isSelected && "!bg-selection"` — already
  covered by the first condition) was removed along the way.
- No Rust/backend changes needed.

**Verified**: `npm run typecheck`, `npm run build` succeed. Checked live in
browser demo mode (dark and light theme): the cursor row in the
inactive panel correctly filled green (`rgba(154, 194, 123, 0.4)`, confirmed via
`getComputedStyle`), the active panel still blue/`accent-dim`,
cursor+selection in the inactive panel correctly shows yellow instead of green, no
console errors.

**Side finding (not fixed in this ticket, spun off as its own task):**
`SettingsDialog.tsx` uses `bg-cursor-inactive` for the
toggle-switch background; this Tailwind hue was never registered in
`tailwind.config.js` (only `cursor` → `--cursor-active`), so the
class was ineffective.
