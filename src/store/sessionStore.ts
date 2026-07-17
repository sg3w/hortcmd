// ============================================================
// Session/workspace: the minimal tab layout of both panes is
// kept in localStorage and restored on the next start.
// Deliberately only the folder paths + active tab + view mode – no
// entries/selection (those are determined anew on load).
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";
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
    { name: "hortcmd-session", partialize: (s) => ({ session: s.session }) },
  ),
);
