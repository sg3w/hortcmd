import { migrateLegacyLocation } from "@/lib/location";
// ============================================================
// Session/workspace: the minimal tab layout of both panes is
// kept in localStorage and restored on the next start.
// Deliberately only the folder paths + active tab + view mode – no
// entries/selection (those are determined anew on load).
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { guardedStorage } from "./persisted/storage";
import { onRecovered,
  acceptVersion, PERSISTED_KEYS, rehydrate } from "./persisted/gate";
import { SESSION_VERSION, validateSession } from "./persisted/domains";
import { usePanes, type Side, type ViewMode } from "./panesStore";
import { parentPath } from "@/lib/path";

export interface SavedTab {
  path: string;
}

export interface SavedSide {
  tabs: SavedTab[];
  activeTab: number;
  viewMode: ViewMode;
}

export interface SavedSession {
  left: SavedSide;
  right: SavedSide;
  active: Side;
}

interface SessionStore {
  session: SavedSession | null;
  /** Take over the current layout from the panesStore. */
  capture: () => void;
}

/** Effective folder path of a tab (archive → containing folder). */
function tabFolder(t: { path: string; archive: string | null }): string {
  return t.archive ? parentPath(t.archive) : t.path;
}

export const useSession = create<SessionStore>()(
  persist(
    (set) => ({
      session: null,
      capture: () => {
        const s = usePanes.getState();
        const side = (sd: Side): SavedSide => ({
          tabs: s[sd].tabs
            .map((t) => tabFolder(t))
            .filter((p) => p)
            .map((path) => ({ path })),
          activeTab: s[sd].activeTab,
          viewMode: s[sd].viewMode,
        });
        set({
          session: { left: side("left"), right: side("right"), active: s.active },
        });
      },
    }),
    {
      name: PERSISTED_KEYS.session,
      version: SESSION_VERSION,
      storage: guardedStorage("session"),
      partialize: (s) => ({ session: s.session }),
      migrate: (stored, from) =>
        acceptVersion("session", from, SESSION_VERSION) ? (from < 2 ? migrateLocations(stored) : stored) : {},
      merge: (stored, current) => ({
        ...current,
        session: rehydrate(
          "session",
          (stored as { session?: unknown } | null)?.session,
          validateSession,
        ),
      }),
    },
  ),
);

function migrateLocations(stored: unknown): unknown {
  if (!stored || typeof stored !== "object") return stored;
  // Migrate only this domain's known path fields, preserving unrelated data.
  return JSON.parse(JSON.stringify(stored), (key: string, value: unknown) => {
    if (typeof value !== "string") return value;
    try { return key === "path" ? migrateLegacyLocation(value) : value; }
    catch { return value; }
  }) as unknown;
}

// A repair made on the way in is written back once at startup, so the
// same value is not repaired — and reported — again on the next one
// (TICKET-389). `setState({})` is what makes `persist` write: it merges
// nothing and notifies, which is exactly a save of the current state.
onRecovered("session", () => useSession.setState({}));
