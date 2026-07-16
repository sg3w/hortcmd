// ============================================================
// Kleiner Store für den Massenumbenennen-Dialog: merkt sich, für
// welche Seite der Dialog geöffnet ist (null = geschlossen).
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

/** Öffnet den Massenumbenennen-Dialog. */
export function openRename(side: Side): void {
  useRenameDialog.getState().open(side);
}
