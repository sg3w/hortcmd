// ============================================================
// Small store for the selection-file dialog (TICKET-059):
// which pane it works on and whether it saves or loads.
// ============================================================

import { create } from "zustand";
import type { Side } from "./panesStore";

export type SelectionFileMode = "save" | "load";

interface SelectionFileStore {
  mode: SelectionFileMode | null;
  side: Side;
  open: (side: Side, mode: SelectionFileMode) => void;
  close: () => void;
}

export const useSelectionFile = create<SelectionFileStore>((set) => ({
  mode: null,
  side: "left",
  open: (side, mode) => set({ side, mode }),
  close: () => set({ mode: null }),
}));

/** Opens the dialog for one pane. */
export function openSelectionFile(side: Side, mode: SelectionFileMode): void {
  useSelectionFile.getState().open(side, mode);
}
