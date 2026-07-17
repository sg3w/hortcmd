// ============================================================
// Store for the file/binary comparison dialog. Holds the two file
// paths to compare (null = closed). As with the directory
// comparison, the model is "always left against right".
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

/** Current cursor file of a pane (full path) or null. */
function cursorFile(side: "left" | "right"): string | null {
  const p = panelOf(usePanes.getState(), side);
  if (p.archive !== null) return null;
  const cur = p.entries[p.cursor];
  if (!cur || cur.parent || cur.is_dir) return null;
  return joinPath(p.path, cur.name);
}

/**
 * Opens the comparison of the cursor file on the left against the one on the right.
 * Returns false if a file is not selected on both sides.
 */
export function openFileCompare(): boolean {
  const left = cursorFile("left");
  const right = cursorFile("right");
  if (!left || !right) return false;
  useFileCompareDialog.getState().open(left, right);
  return true;
}
