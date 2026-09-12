// ============================================================
// Window geometry (TICKET-039).
//
// The two pure halves: what a stored box is allowed to say, and whether
// it lands anywhere the user can reach. The Tauri half around them is
// not testable without a window, and is also the part that does nothing
// interesting — it calls `setSize` and `setPosition` with what these two
// decide.
// ============================================================

import { describe, expect, it } from "vitest";
import { isOnAScreen, parseBox, WINDOW_VERSION } from "./windowState";

const box = (x: number, y: number, w = 1200, h = 800) => ({ x, y, w, h });
const SCREEN = { x: 0, y: 0, w: 1920, h: 1080 };
const SECOND = { x: -1920, y: 0, w: 1920, h: 1080 };

describe("parseBox", () => {
  it("keeps a well-formed box", () => {
    const raw = JSON.stringify({ x: 100, y: 50, w: 1200, h: 800, v: WINDOW_VERSION });
    expect(parseBox(raw)).toEqual({ x: 100, y: 50, w: 1200, h: 800 });
  });

  it("reads a v0 box, which had no version at all", () => {
    expect(parseBox(JSON.stringify({ x: 10, y: 20, w: 900, h: 600 }))).toEqual({
      x: 10,
      y: 20,
      w: 900,
      h: 600,
    });
  });

  it("returns nothing for anything unusable", () => {
    for (const raw of [null, "", "{ truncated", "42", "[]", "null"]) {
      expect(parseBox(raw)).toBeNull();
    }
    // A box with no size is not a box: there is nothing to restore.
    expect(parseBox(JSON.stringify({ x: 1, y: 2 }))).toBeNull();
    expect(parseBox(JSON.stringify({ x: 1, y: 2, w: "wide", h: 600 }))).toBeNull();
    expect(parseBox(JSON.stringify({ x: 1, y: 2, w: NaN, h: 600 }))).toBeNull();
  });

  it("clamps a size to something with a title bar and a corner", () => {
    expect(parseBox(JSON.stringify({ x: 0, y: 0, w: 10, h: 10 }))).toEqual({
      x: 0,
      y: 0,
      w: 320,
      h: 320,
    });
    expect(parseBox(JSON.stringify({ x: 0, y: 0, w: 1e9, h: 1e9 }))).toEqual({
      x: 0,
      y: 0,
      w: 20000,
      h: 20000,
    });
  });

  /** A monitor to the left of the primary one has negative coordinates. */
  it("allows a negative position", () => {
    expect(parseBox(JSON.stringify({ x: -1900, y: 40, w: 1000, h: 700 }))).toEqual({
      x: -1900,
      y: 40,
      w: 1000,
      h: 700,
    });
  });

  it("refuses a newer version rather than misreading it", () => {
    const raw = JSON.stringify({ x: 1, y: 2, w: 900, h: 600, v: WINDOW_VERSION + 1 });
    expect(parseBox(raw)).toBeNull();
  });
});

describe("isOnAScreen", () => {
  it("accepts a window well inside a monitor", () => {
    expect(isOnAScreen(box(100, 100), [SCREEN])).toBe(true);
  });

  it("accepts one hanging off an edge but still grabbable", () => {
    expect(isOnAScreen(box(-1000, 0), [SCREEN])).toBe(true);
    expect(isOnAScreen(box(1800, 900), [SCREEN])).toBe(true);
  });

  /** The unplugged-monitor case: every number plausible, nothing visible. */
  it("rejects one on a monitor that is no longer there", () => {
    expect(isOnAScreen(box(-3000, 0), [SCREEN])).toBe(false);
    expect(isOnAScreen(box(0, 4000), [SCREEN])).toBe(false);
  });

  it("checks every monitor, not just the first", () => {
    expect(isOnAScreen(box(-1800, 100), [SCREEN, SECOND])).toBe(true);
    expect(isOnAScreen(box(-5000, 100), [SCREEN, SECOND])).toBe(false);
  });

  it("rejects a sliver too thin to click", () => {
    // Ten pixels of a window peeking onto the screen is not a window
    // anyone can move.
    expect(isOnAScreen(box(1910, 100), [SCREEN])).toBe(false);
  });

  /**
   * With no monitor information the answer is yes: refusing to restore
   * because the query failed would be worse than restoring to a
   * position that is almost certainly still fine.
   */
  it("restores when it cannot tell", () => {
    expect(isOnAScreen(box(-3000, 0), [])).toBe(true);
  });
});
