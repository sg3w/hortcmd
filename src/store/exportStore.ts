// ============================================================
// Kleiner Store für den Dateilisten-Export-Dialog: merkt sich, für
// welche Seite der Dialog geöffnet ist (null = geschlossen).
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

/** Öffnet den Dateilisten-Export-Dialog. */
export function openExport(side: Side): void {
  useExportDialog.getState().open(side);
}
