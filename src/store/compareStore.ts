// ============================================================
// Kleiner Store für den Verzeichnisvergleich-Dialog: offen/geschlossen.
// Der Dialog vergleicht immer das linke gegen das rechte Fenster.
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

/** Öffnet den Verzeichnisvergleich-Dialog. */
export function openCompare(): void {
  useCompareDialog.getState().setOpen(true);
}
