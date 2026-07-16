// ============================================================
// Globaler Verlauf zuletzt besuchter Ordner (persistiert).
// Neueste zuerst, dedupliziert, begrenzt auf MAX Einträge.
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";

const MAX = 20;

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
    { name: "hortcmd-history" },
  ),
);

/** Verlauf-Eintrag außerhalb von React ergänzen (z. B. im panesStore). */
export function pushRecent(path: string): void {
  useHistory.getState().push(path);
}
