import { expect, it } from "vitest";
import { submenuPlacement } from "./menuPlacement";

it("keeps adjacent columns when the viewport has room", () => {
  expect(submenuPlacement({ left: 16, right: 246, width: 230 }, 1100)).toEqual({
    sideOffset: 0, minWidth: 240, maxWidth: 400,
  });
});

it("allows at least one complete horizontal placement at every tested anchor", () => {
  for (const viewport of [200, 350, 550, 700, 1100]) {
    for (const width of [100, Math.min(230, viewport - 16)]) {
      for (const fraction of [0, 0.25, 0.5, 0.75, 1]) {
        const left = 8 + (viewport - 16 - width) * fraction;
        const anchor = { left, right: left + width, width };
        const placement = submenuPlacement(anchor, viewport);
        const rightStart = anchor.right + placement.sideOffset;
        const leftEnd = anchor.left - placement.sideOffset;
        const rightFits = rightStart >= 8 && rightStart + placement.maxWidth <= viewport - 8;
        const leftFits = leftEnd - placement.maxWidth >= 8 && leftEnd <= viewport - 8;
        expect(rightFits || leftFits, JSON.stringify({ viewport, anchor, placement })).toBe(true);
        expect(placement.minWidth).toBeLessThanOrEqual(placement.maxWidth);
      }
    }
  }
});

it("overlaps the overflowing native 200% menu column", () => {
  const placement = submenuPlacement({ left: 115, right: 345, width: 230 }, 350);
  expect(placement.sideOffset).toBe(-230);
  expect(placement.maxWidth).toBeLessThanOrEqual(337);
});
