// ============================================================
// Remembers the last-used width/height per dialog type
// (TICKET-012), persisted across application restarts.
// The position is deliberately not stored — dialogs are
// always reopened centered.
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { guardedStorage } from "./persisted/storage";
import { onRecovered,
  acceptVersion, PERSISTED_KEYS, rehydrate } from "./persisted/gate";
import {
  DIALOG_SIZES_VERSION,
  validateDialogSizes,
  type DialogSize,
} from "./persisted/domains";

export type { DialogSize };

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
    {
      name: PERSISTED_KEYS.dialogSizes,
      version: DIALOG_SIZES_VERSION,
      storage: guardedStorage("dialogSizes"),
      migrate: (stored, from) =>
        acceptVersion("dialogSizes", from, DIALOG_SIZES_VERSION) ? stored : {},
      merge: (stored, current) => ({
        ...current,
        sizes: rehydrate(
          "dialogSizes",
          (stored as { sizes?: unknown } | null)?.sizes,
          validateDialogSizes,
        ),
      }),
    },
  ),
);

// A repair made on the way in is written back once at startup, so the
// same value is not repaired — and reported — again on the next one
// (TICKET-389). `setState({})` is what makes `persist` write: it merges
// nothing and notifies, which is exactly a save of the current state.
onRecovered("dialogSizes", () => useDialogSizeStore.setState({}));
