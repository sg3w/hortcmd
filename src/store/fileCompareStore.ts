// ============================================================
// Store für den Datei-/Binärvergleich-Dialog. Hält die beiden zu
// vergleichenden Dateipfade (null = geschlossen). Wie beim Verzeichnis-
// vergleich gilt das Modell „immer links gegen rechts".
// ============================================================

import { create } from "zustand";
import { panelOf, usePanes } from "./panesStore";
import { joinPath } from "@/lib/path";

interface FileCompareStore {
  left: string | null;
  right: string | null;
  open: (left: string, right: string) => void;
  close: () => void;
}

export const useFileCompareDialog = create<FileCompareStore>((set) => ({
  left: null,
  right: null,
  open: (left, right) => set({ left, right }),
  close: () => set({ left: null, right: null }),
}));

/** Aktuelle Cursor-Datei eines Fensters (voller Pfad) oder null. */
function cursorFile(side: "left" | "right"): string | null {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive !== null) return null;
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || cur.is_dir) return null;
  return joinPath(p.path, cur.name);
}

/**
 * Öffnet den Vergleich für die Cursor-Datei links gegen die rechts.
 * Gibt false zurück, wenn nicht auf beiden Seiten eine Datei markiert ist.
 */
export function openFileCompare(): boolean {
  const left = cursorFile("left");
  const right = cursorFile("right");
  if (!left || !right) return false;
  useFileCompareDialog.getState().open(left, right);
  return true;
}
