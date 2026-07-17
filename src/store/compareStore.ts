// ============================================================
// Small store for the directory-comparison dialog: open/closed.
// The dialog always compares the left against the right pane.
// ============================================================

import { create } from "zustand";

interface CompareStore {
  open: boolean;
  setOpen: (open: boolean) => void;
}

export const useCompareDialog = create<CompareStore>((set) => ({
  open: false,
  setOpen: (open) => set({ open }),
}));

/** Opens the directory-comparison dialog. */
export function openCompare(): void {
  useCompareDialog.getState().setOpen(true);
}
