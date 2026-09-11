// ============================================================
// Bridge from the reduced native macOS menu into the app (TICKET-040).
//
// The native menu (`src-tauri/src/menu.rs`) only carries the app,
// text-editing and window items. Its three app-level entries — About,
// Settings and Quit — must reuse exactly the dialogs and the close flow
// of the in-window menu, so they arrive here as an event and are routed
// to the same commands. Cut/Copy/Paste/Undo and the window items are
// handled natively and never reach the frontend.
// ============================================================

import { useEffect } from "react";
import { onNativeMenu, setMenuLanguage } from "@/ipc/client";
import { runCommand, type CommandId } from "./commands";
import { runAction } from "./actions";
import { usePanes } from "@/store/panesStore";
import { useSettings } from "@/store/settingsStore";

/** Native menu id → command of the shared definition. */
const NATIVE_COMMANDS: Record<string, CommandId> = {
  "app.about": "help.about",
  "app.settings": "cfg.settings",
};

export function useNativeMenu(): void {
  const language = useSettings((s) => s.language);

  // The native menu is built in Rust at startup and cannot read the
  // persisted language, so the frontend hands it over — on startup and
  // on every change.
  useEffect(() => {
    setMenuLanguage(language);
  }, [language]);

  useEffect(() => {
    return onNativeMenu((id) => {
      const command = NATIVE_COMMANDS[id];
      if (command) {
        runCommand(command);
        return;
      }
      if (id === "app.quit") {
        // Deliberately not a process exit: this runs through the same
        // window-close path as the window button, including the prompt
        // for running transfers (TICKET-004).
        runAction("quit", usePanes.getState().active);
      }
    });
  }, []);
}
