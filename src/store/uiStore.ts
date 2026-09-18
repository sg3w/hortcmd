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

/**
 * The pages of the settings dialog. Lives here rather than in the dialog
 * itself so the Konfiguration menu can open a page directly (TICKET-058)
 * without importing the component — the dialog reads the same list.
 */
export type SettingsCategory =
  | "general"
  | "view"
  | "colors"
  | "files"
  | "operations"
  | "programs"
  | "favorites"
  | "mcp"
  | "plugins";

interface UiStore {
  settingsOpen: boolean;
  /** Page shown when the settings dialog opens. */
  settingsCategory: SettingsCategory;
  shortcutsOpen: boolean;
  aboutOpen: boolean;

  setSettingsOpen: (open: boolean) => void;
  setSettingsCategory: (cat: SettingsCategory) => void;
  setShortcutsOpen: (open: boolean) => void;
  setAboutOpen: (open: boolean) => void;
}

export const useUi = create<UiStore>((set) => ({
  settingsOpen: false,
  settingsCategory: "general",
  shortcutsOpen: false,
  aboutOpen: false,

  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setSettingsCategory: (settingsCategory) => set({ settingsCategory }),
  setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
  setAboutOpen: (aboutOpen) => set({ aboutOpen }),
}));

/**
 * Opens the settings dialog from anywhere (menu, toolbar, native menu).
 * With a category, the dialog starts on that page — the Konfiguration menu
 * and "Programme verwalten" use it to land where the user asked to go;
 * without one the last visited page stays selected.
 */
export function openSettings(category?: SettingsCategory): void {
  const ui = useUi.getState();
  if (category) ui.setSettingsCategory(category);
  ui.setSettingsOpen(true);
}

/** Opens the keyboard overview (Help menu). */
export function openShortcuts(): void {
  useUi.getState().setShortcutsOpen(true);
}

/** Opens the About box (Help menu and native macOS app menu). */
export function openAbout(): void {
  useUi.getState().setAboutOpen(true);
}
