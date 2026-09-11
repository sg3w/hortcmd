// ============================================================
// Open state of the app-level dialogs that are reachable from
// several places at once (TICKET-040).
//
// Settings, the shortcut overview and the About box are opened by the
// in-window menu, by the toolbar and — on macOS — by the reduced native
// menu. The native menu arrives as an event outside React, so the state
// cannot live in `App.tsx` any more.
// ============================================================

import { create } from "zustand";

interface UiStore {
  settingsOpen: boolean;
  shortcutsOpen: boolean;
  aboutOpen: boolean;

  setSettingsOpen: (open: boolean) => void;
  setShortcutsOpen: (open: boolean) => void;
  setAboutOpen: (open: boolean) => void;
}

export const useUi = create<UiStore>((set) => ({
  settingsOpen: false,
  shortcutsOpen: false,
  aboutOpen: false,

  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
  setAboutOpen: (aboutOpen) => set({ aboutOpen }),
}));

/** Opens the settings dialog from anywhere (menu, toolbar, native menu). */
export function openSettings(): void {
  useUi.getState().setSettingsOpen(true);
}

/** Opens the keyboard overview (Help menu). */
export function openShortcuts(): void {
  useUi.getState().setShortcutsOpen(true);
}

/** Opens the About box (Help menu and native macOS app menu). */
export function openAbout(): void {
  useUi.getState().setAboutOpen(true);
}
