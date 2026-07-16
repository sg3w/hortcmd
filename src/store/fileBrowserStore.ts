// ============================================================
// Wiederverwendbarer Dateibrowser-Modal: kleiner Store, über den
// von überall im Programm ein Auswahl-Dialog geöffnet werden kann.
//   openFileBrowser({ selectMode, onPick, ... })
// ============================================================

import { create } from "zustand";

/** Was im Dateibrowser auswählbar sein soll. */
export type SelectMode = "file" | "folder" | "any";

export interface FileBrowserRequest {
  /** Titel des Dialogs (Standard: „Auswählen"). */
  title?: string;
  /** Was ausgewählt werden darf. */
  selectMode: SelectMode;
  /** Startordner (Standard: Home-Verzeichnis). */
  initialPath?: string;
  /** Beschriftung des Bestätigen-Buttons. */
  confirmLabel?: string;
  /** Callback mit dem gewählten vollständigen Pfad. */
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

/** Bequemer Aufruf von außerhalb von React. */
export function openFileBrowser(r: FileBrowserRequest): void {
  useFileBrowser.getState().open(r);
}
