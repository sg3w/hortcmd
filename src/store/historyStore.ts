import { migrateLegacyLocation } from "@/lib/location";
// ============================================================
// Global history of recently visited folders (persisted).
// Newest first, deduplicated, limited to MAX entries.
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { guardedStorage } from "./persisted/storage";
import { onRecovered,
  acceptVersion, PERSISTED_KEYS, rehydrate } from "./persisted/gate";
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
        acceptVersion("history", from, HISTORY_VERSION) ? (from < 2 ? migrateLocations(stored) : stored) : {},
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

function migrateLocations(stored: unknown): unknown {
  if (!stored || typeof stored !== "object") return stored;
  // Migrate only this domain's known path fields, preserving unrelated data.
  return JSON.parse(JSON.stringify(stored), (key: string, value: unknown) => {
    if (typeof value !== "string") return value;
    try { return /^\d+$/.test(key) ? migrateLegacyLocation(value) : value; }
    catch { return value; }
  }) as unknown;
}

// A repair made on the way in is written back once at startup, so the
// same value is not repaired — and reported — again on the next one
// (TICKET-389). `setState({})` is what makes `persist` write: it merges
// nothing and notifies, which is exactly a save of the current state.
onRecovered("history", () => useHistory.setState({}));
