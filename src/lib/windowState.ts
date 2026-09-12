// ============================================================
// Remember window position and size across restarts.
// Uses the existing @tauri-apps/api (no extra plugin);
// in browser/demo mode (without Tauri) nothing happens.
//
// The geometry is persisted state like any other (TICKET-039), with one
// failure mode of its own: the display it was saved on may be gone. A
// window restored to a monitor that was unplugged is not "wrong data" —
// every number in it is plausible — it is simply invisible, and the only
// way back is to delete the key nobody knows about.
// ============================================================

import { hasTauri } from "@/ipc/client";
import { isObject, num, Repairs } from "@/lib/schema";

const KEY = "hortcmd-window";

/** Smallest window worth restoring — below this there is nothing to click. */
const MIN_SIZE = 320;
/** Larger than any single display; a stored 10⁹ is corruption, not a wish. */
const MAX_SIZE = 20000;
/** How much of the window has to be on a monitor for it to count as visible. */
const VISIBLE_MARGIN = 80;

export interface WindowBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * The geometry's schema version.
 *
 * 0 — up to TICKET-038: `{x,y,w,h}` with no version and no bounds.
 * 1 — versioned and bounded. The shape is unchanged, so reading a v0
 *     box needs no migration — but the number has to exist before the
 *     *next* change, or that one has nothing to migrate from.
 */
export const WINDOW_VERSION = 1;

/** A monitor as `@tauri-apps/api` reports it, in logical pixels. */
interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * The stored geometry, clamped — or `null` when there is nothing usable.
 *
 * Exported for the tests: this is pure, and it is the half that decides
 * whether a window ends up somewhere the user can reach.
 */
export function parseBox(raw: string | null): WindowBox | null {
  if (!raw) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isObject(parsed)) return null;

  const r = new Repairs();
  const w = num(parsed.w, 0, "w", r, { min: MIN_SIZE, max: MAX_SIZE, int: true });
  const h = num(parsed.h, 0, "h", r, { min: MIN_SIZE, max: MAX_SIZE, int: true });
  // A size is the part that must be present and sane; without it there
  // is nothing to restore, whatever the position says.
  if (typeof parsed.w !== "number" || typeof parsed.h !== "number") return null;
  if (!Number.isFinite(parsed.w) || !Number.isFinite(parsed.h)) return null;

  // Position may legitimately be negative — a monitor to the left of the
  // primary one — so it is bounded rather than clamped to zero.
  const x = num(parsed.x, 0, "x", r, { min: -MAX_SIZE, max: MAX_SIZE, int: true });
  const y = num(parsed.y, 0, "y", r, { min: -MAX_SIZE, max: MAX_SIZE, int: true });

  // A newer version may mean something different by these four numbers,
  // and a window placed from a misread box is one the user cannot find.
  const version = typeof parsed.v === "number" ? parsed.v : 0;
  if (version > WINDOW_VERSION) return null;

  return { x, y, w, h };
}

/**
 * Whether enough of `box` lands on at least one monitor to be grabbed.
 *
 * With no monitor information at all the answer is yes: refusing to
 * restore because the query failed would be worse than restoring to a
 * position that is probably still fine.
 */
export function isOnAScreen(box: WindowBox, monitors: Rect[]): boolean {
  if (!monitors.length) return true;
  return monitors.some((m) => {
    const overlapX =
      Math.min(box.x + box.w, m.x + m.w) - Math.max(box.x, m.x);
    const overlapY =
      Math.min(box.y + box.h, m.y + m.h) - Math.max(box.y, m.y);
    return overlapX >= VISIBLE_MARGIN && overlapY >= VISIBLE_MARGIN;
  });
}

function read(): WindowBox | null {
  try {
    return parseBox(localStorage.getItem(KEY));
  } catch {
    return null;
  }
}

/**
 * Restores the saved window geometry and stores future
 * moves/resizes (debounced). Without Tauri a no-op.
 */
export async function initWindowState(): Promise<void> {
  if (!hasTauri) return;
  try {
    const { getCurrentWindow, availableMonitors, LogicalPosition, LogicalSize } =
      await import("@tauri-apps/api/window");
    const win = getCurrentWindow();

    // Restore (size first, then position).
    const saved = read();
    if (saved) {
      await win.setSize(new LogicalSize(saved.w, saved.h));

      let monitors: Rect[] = [];
      try {
        const found = await availableMonitors();
        monitors = found.map((m) => ({
          x: m.position.x / m.scaleFactor,
          y: m.position.y / m.scaleFactor,
          w: m.size.width / m.scaleFactor,
          h: m.size.height / m.scaleFactor,
        }));
      } catch {
        // Treated as "unknown", which means the position is restored.
      }

      if (isOnAScreen(saved, monitors)) {
        await win.setPosition(new LogicalPosition(saved.x, saved.y));
      } else {
        // The display is gone. Keep the size the user chose, let the
        // window manager place the window, and say so once — this is
        // not an error and must not look like one on every start.
        console.info(
          "[hortcmd] the saved window position is off-screen — centring instead",
        );
        await win.center();
      }
    }

    // Save on move/resize – debounced.
    let timer: number | undefined;
    const save = async () => {
      try {
        const factor = await win.scaleFactor();
        const pos = (await win.outerPosition()).toLogical(factor);
        const size = (await win.innerSize()).toLogical(factor);
        const box: WindowBox = {
          x: Math.round(pos.x),
          y: Math.round(pos.y),
          w: Math.round(size.width),
          h: Math.round(size.height),
        };
        localStorage.setItem(KEY, JSON.stringify({ ...box, v: WINDOW_VERSION }));
      } catch {
        /* window API or storage not available → ignore */
      }
    };
    const schedule = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(save, 400);
    };
    await win.onMoved(schedule);
    await win.onResized(schedule);
  } catch {
    /* Tauri window API not available → silently ignore */
  }
}
