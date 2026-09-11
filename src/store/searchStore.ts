// ============================================================
// Store for the search dialog: open/closed plus the side
// whose current folder serves as the starting point and in which a
// match is revealed.
// ============================================================

import { create } from "zustand";
import { panelOf, usePanes, type Side } from "./panesStore";
import { parentPath } from "@/lib/path";
import type { SearchMode } from "@/ipc/client";

interface SearchStore {
  side: Side | null;
  /** Mode the dialog starts in — the Befehle menu opens it per mode. */
  mode: SearchMode;
  open: (side: Side, mode?: SearchMode) => void;
  close: () => void;
}

export const useSearchDialog = create<SearchStore>((set) => ({
  side: null,
  mode: "files",
  open: (side, mode = "files") => set({ side, mode }),
  close: () => set({ side: null }),
}));

/** Start folder of the search: current folder of the side (not in an archive). */
export function searchRoot(side: Side): string {
  return panelOf(usePanes.getState(), side).path;
}

/** Reveals a match in the pane `side`: open the folder + set the cursor. */
export async function revealPath(side: Side, path: string): Promise<void> {
  const dir = parentPath(path);
  const s = usePanes.getState();
  await s.loadDir(side, dir);
  // After loading, focus the entry with the matching name.
  const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";
  const entries = panelOf(usePanes.getState(), side).entries;
  const idx = entries.findIndex((e) => !e.parent && e.name === name);
  if (idx >= 0) usePanes.getState().setCursor(side, idx);
}

export function openSearch(side: Side, mode?: SearchMode): void {
  useSearchDialog.getState().open(side, mode);
}
