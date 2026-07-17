// ============================================================
// Small store for the batch-rename dialog: remembers for
// which side the dialog is open (null = closed).
// ============================================================

import { create } from "zustand";
import type { Side } from "./panesStore";

interface RenameStore {
  side: Side | null;
  open: (side: Side) => void;
  close: () => void;
}

export const useRenameDialog = create<RenameStore>((set) => ({
  side: null,
  open: (side) => set({ side }),
  close: () => set({ side: null }),
}));

/** Opens the batch-rename dialog. */
export function openRename(side: Side): void {
  useRenameDialog.getState().open(side);
}
