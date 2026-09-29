// ============================================================
// The file-undo dialog's open state, and the one gate every entry
// point passes through (TICKET-391).
//
// File undo is beta and off by default. The check lives here rather
// than at each caller, so the menu command, the ⌘⌥Z shortcut and
// anything added later cannot open a dialog for a feature that records
// nothing.
// ============================================================

import { create } from "zustand";
import { useSettings } from "./settingsStore";

export const useUndo = create<{ open: boolean; setOpen: (open: boolean) => void }>((set) => ({
  open: false,
  setOpen: open => set({ open: open && undoAvailable() }),
}));

/** Whether file undo is switched on (read, never subscribed — see TICKET-354). */
export function undoAvailable(): boolean {
  return useSettings.getState().undoEnabled;
}

/** The same gate for a component that has to re-render when it moves. */
export function useUndoAvailable(): boolean {
  return useSettings(s => s.undoEnabled);
}

export function openUndo(): void { useUndo.getState().setOpen(true); }
