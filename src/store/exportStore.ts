// ============================================================
// Small store for the file-list export dialog: remembers for
// which side the dialog is open (null = closed).
// ============================================================

import { create } from "zustand";
import type { Side } from "./panesStore";

interface ExportStore {
  side: Side | null;
  open: (side: Side) => void;
  close: () => void;
}

export const useExportDialog = create<ExportStore>((set) => ({
  side: null,
  open: (side) => set({ side }),
  close: () => set({ side: null }),
}));

/** Opens the file-list export dialog. */
export function openExport(side: Side): void {
  useExportDialog.getState().open(side);
}
