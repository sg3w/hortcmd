// ============================================================
// Global history of recently visited folders (persisted).
// Newest first, deduplicated, limited to MAX entries.
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { guardedStorage } from "./persisted/storage";
import { acceptVersion, PERSISTED_KEYS, rehydrate } from "./persisted/gate";
import { HISTORY_MAX, HISTORY_VERSION, validateHistory } from "./persisted/domains";

const MAX = HISTORY_MAX;

interface HistoryStore {
  recent: string[];
  push: (path: string) => void;
  clear: () => void;
}

export const useHistory = create<HistoryStore>()(
  persist(
    (set) => ({
      recent: [],
      push: (path) =>
        set((s) => {
          if (!path) return s;
          const recent = [path, ...s.recent.filter((p) => p !== path)].slice(
            0,
            MAX,
          );
          return { recent };
        }),
      clear: () => set({ recent: [] }),
    }),
    {
      name: PERSISTED_KEYS.history,
      version: HISTORY_VERSION,
      storage: guardedStorage("history"),
      migrate: (stored, from) =>
        acceptVersion("history", from, HISTORY_VERSION) ? stored : {},
      merge: (stored, current) => ({
        ...current,
        recent: rehydrate(
          "history",
          (stored as { recent?: unknown } | null)?.recent,
          validateHistory,
        ),
      }),
    },
  ),
);

/** Add a history entry from outside React (e.g. in the panesStore). */
export function pushRecent(path: string): void {
  useHistory.getState().push(path);
}
