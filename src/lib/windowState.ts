// ============================================================
// Remember window position and size across restarts.
// Uses the existing @tauri-apps/api (no extra plugin);
// in browser/demo mode (without Tauri) nothing happens.
// ============================================================

import { hasTauri } from "@/ipc/client";

const KEY = "hortcmd-window";

interface WindowBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

function read(): WindowBox | null {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return null;
    const b = JSON.parse(raw) as WindowBox;
    if ([b.x, b.y, b.w, b.h].every((n) => Number.isFinite(n)) && b.w > 0 && b.h > 0) {
      return b;
    }
  } catch {
    /* invalid → ignore */
  }
  return null;
}

/**
 * Restores the saved window geometry and stores future
 * moves/resizes (debounced). Without Tauri a no-op.
 */
export async function initWindowState(): Promise<void> {
  if (!hasTauri) return;
  try {
    const { getCurrentWindow, LogicalPosition, LogicalSize } = await import(
      "@tauri-apps/api/window"
    );
    const win = getCurrentWindow();

    // Restore (size first, then position).
    const saved = read();
    if (saved) {
      await win.setSize(new LogicalSize(saved.w, saved.h));
      await win.setPosition(new LogicalPosition(saved.x, saved.y));
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
        localStorage.setItem(KEY, JSON.stringify(box));
      } catch {
        /* window API not available → ignore */
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
