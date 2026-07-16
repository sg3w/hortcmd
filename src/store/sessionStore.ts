// ============================================================
// Sitzung/Workspace: das minimale Tab-Layout beider Fenster wird
// in localStorage gehalten und beim nächsten Start wiederhergestellt.
// Bewusst nur die Ordnerpfade + aktiver Tab + Ansichtsmodus – keine
// Einträge/Auswahl (die werden beim Laden neu ermittelt).
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
  /** Aktuelles Layout aus dem panesStore übernehmen. */
  capture: () => void;
}

/** Effektiver Ordnerpfad eines Tabs (Archiv → enthaltender Ordner). */
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
