// ============================================================
// Small store for the batch-rename dialog: remembers for
// which side the dialog is open (null = closed).
// ============================================================

import { create } from "zustand";
import type { Side } from "./panesStore";

interface RenameStore {
  side: Side | null;
  revision: number;
  open: (side: Side) => void;
  close: () => void;
}

export const useRenameDialog = create<RenameStore>((set) => ({
  side: null, revision: 0,
  open: (side) => set(s => ({ side, revision: s.revision + 1 })),
  close: () => set(s => ({ side: null, revision: s.revision + 1 })),
}));

/** Opens the batch-rename dialog. */
export function openRename(side: Side): void {
  useRenameDialog.getState().open(side);
}
