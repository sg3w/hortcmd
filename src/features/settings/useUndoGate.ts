// ============================================================
// Keeps the backend's copy of the file-undo switch in step with the
// setting (TICKET-391).
//
// The backend cannot read `localStorage`, and the journal is written
// from seven commands that carry no settings, so the switch is pushed
// once at startup and again whenever it changes. The backend defaults
// to off, which is what makes the gap before the first push harmless.
// ============================================================

import { useEffect } from "react";
import { setUndoEnabled } from "@/ipc/client";
import { useSettings } from "@/store/settingsStore";
import { useUndo } from "@/store/undoStore";

export function useUndoGate(): void {
  const enabled = useSettings((s) => s.undoEnabled);
  useEffect(() => {
    void setUndoEnabled(enabled).catch(() => {
      /* The backend keeps its previous value; off is its default. */
    });
    // Switching the feature off while its history is open closes it —
    // the commands behind it refuse from this moment on.
    if (!enabled) useUndo.getState().setOpen(false);
  }, [enabled]);
}
