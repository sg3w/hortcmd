// ============================================================
// Merkt sich die zuletzt verwendete Breite/Höhe je Dialogtyp
// (TICKET-012), persistiert über Anwendungsneustarts hinweg.
// Die Position wird bewusst nicht gespeichert — Dialoge werden
// immer erneut zentriert geöffnet.
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface DialogSize {
  w: number;
  h: number;
}

interface DialogSizeStore {
  sizes: Record<string, DialogSize>;
  setSize: (key: string, size: DialogSize) => void;
}

export const useDialogSizeStore = create<DialogSizeStore>()(
  persist(
    (set) => ({
      sizes: {},
      setSize: (key, size) =>
        set((s) => ({ sizes: { ...s.sizes, [key]: size } })),
    }),
    { name: "hortcmd-dialog-sizes" },
  ),
);
