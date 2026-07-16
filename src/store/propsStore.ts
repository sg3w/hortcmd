// ============================================================
// Kleiner Store für den Eigenschaften-/Rechte-Dialog: merkt sich
// den Pfad des Eintrags, dessen Rechte angezeigt werden (null = zu).
// ============================================================

import { create } from "zustand";

interface PropsStore {
  path: string | null;
  open: (path: string) => void;
  close: () => void;
}

export const usePropsDialog = create<PropsStore>((set) => ({
  path: null,
  open: (path) => set({ path }),
  close: () => set({ path: null }),
}));

/** Bequemer Aufruf von außerhalb von React. */
export function openProps(path: string): void {
  usePropsDialog.getState().open(path);
}
