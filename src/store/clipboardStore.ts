// ============================================================
// Clipboard for file operations (copy/cut → paste).
// Holds only metadata; the actual execution follows via the
// Rust commands (copy_entries / move_entries).
// ============================================================

import { create } from "zustand";

export type ClipMode = "copy" | "cut";

interface ClipboardStore {
  mode: ClipMode | null;
  sourcePath: string;
  names: string[];

  set: (mode: ClipMode, sourcePath: string, names: string[]) => void;
  clear: () => void;
}

export const useClipboard = create<ClipboardStore>((set) => ({
  mode: null,
  sourcePath: "",
  names: [],
  set: (mode, sourcePath, names) => set({ mode, sourcePath, names }),
  clear: () => set({ mode: null, sourcePath: "", names: [] }),
}));
