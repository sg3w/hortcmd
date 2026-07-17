// ============================================================
// Remembers the last-used width/height per dialog type
// (TICKET-012), persisted across application restarts.
// The position is deliberately not stored — dialogs are
// always reopened centered.
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
