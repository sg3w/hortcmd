const PADDING = 8;
const MAX_WIDTH = 400;

/** Radix shifts along the menu's vertical axis but only flips horizontally.
 * When neither adjacent column fits, align over the trigger's column instead;
 * cap width so one of the two horizontal placements fits with edge padding. */
export function submenuPlacement(
  anchor: { left: number; right: number; width: number },
  viewportWidth: number,
) {
  const wanted = Math.max(1, Math.min(MAX_WIDTH, viewportWidth - 2 * PADDING));
  const adjacent = Math.max(anchor.left - PADDING, viewportWidth - PADDING - anchor.right);
  const overlap = adjacent < wanted;
  const available = overlap
    ? Math.max(anchor.right - PADDING, viewportWidth - PADDING - anchor.left)
    : adjacent;
  const maxWidth = Math.max(1, Math.min(wanted, available));
  return { sideOffset: overlap ? -anchor.width : 0, maxWidth, minWidth: Math.min(240, maxWidth) };
}
