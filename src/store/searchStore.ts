// ============================================================
// Store für den Such-Dialog: offen/geschlossen plus die Seite,
// deren aktueller Ordner als Startpunkt dient und in der ein
// Treffer angezeigt („Reveal") wird.
// ============================================================

import { create } from "zustand";
import { panelOf, usePanes, type Side } from "./panesStore";
import { parentPath } from "@/lib/path";

interface SearchStore {
  side: Side | null;
  open: (side: Side) => void;
  close: () => void;
}

export const useSearchDialog = create<SearchStore>((set) => ({
  side: null,
  open: (side) => set({ side }),
  close: () => set({ side: null }),
}));

/** Startordner der Suche: aktueller Ordner der Seite (nicht im Archiv). */
export function searchRoot(side: Side): string {
  return panelOf(usePanes.getState(), side).path;
}

/** Zeigt einen Treffer im Fenster `side`: Ordner öffnen + Cursor setzen. */
export async function revealPath(side: Side, path: string): Promise<void> {
  const dir = parentPath(path);
  const s = usePanes.getState();
  await s.loadDir(side, dir);
  // Nach dem Laden den Eintrag mit passendem Namen fokussieren.
  const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";
  const entries = panelOf(usePanes.getState(), side).entries;
  const idx = entries.findIndex((e) => !e.parent && e.name === name);
  if (idx >= 0) usePanes.getState().setCursor(side, idx);
}

export function openSearch(side: Side): void {
  useSearchDialog.getState().open(side);
}
