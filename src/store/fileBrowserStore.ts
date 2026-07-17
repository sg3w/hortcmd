// ============================================================
// Reusable file-browser modal: a small store through which
// a selection dialog can be opened from anywhere in the program.
//   openFileBrowser({ selectMode, onPick, ... })
// ============================================================

import { create } from "zustand";

/** What should be selectable in the file browser. */
export type SelectMode = "file" | "folder" | "any";

export interface FileBrowserRequest {
  /** Title of the dialog (default: "Select"). */
  title?: string;
  /** What may be selected. */
  selectMode: SelectMode;
  /** Start folder (default: home directory). */
  initialPath?: string;
  /** Label of the confirm button. */
  confirmLabel?: string;
  /** Callback with the chosen full path. */
  onPick: (path: string) => void;
}

interface FileBrowserStore {
  request: FileBrowserRequest | null;
  open: (r: FileBrowserRequest) => void;
  close: () => void;
}

export const useFileBrowser = create<FileBrowserStore>((set) => ({
  request: null,
  open: (request) => set({ request }),
  close: () => set({ request: null }),
}));

/** Convenience call from outside React. */
export function openFileBrowser(r: FileBrowserRequest): void {
  useFileBrowser.getState().open(r);
}
