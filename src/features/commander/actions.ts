// ============================================================
// F-key and context-menu actions. Determine the target set
// and call the operation runners or fill the clipboard.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useClipboard } from "@/store/clipboardStore";
import { hasTauri } from "@/ipc/client";
import { joinPath, parentPath } from "@/lib/path";
import {
  runCopy,
  runDelete,
  runEdit,
  runEditWith,
  runExtractArchive,
  runMkdir,
  runMove,
  runPack,
  runPaste,
  runRename,
} from "./fileOps";
import { runView } from "./preview";

export type ActionId =
  | "view"
  | "edit"
  | "edit-with"
  | "copy"
  | "move"
  | "mkdir"
  | "rename"
  | "delete"
  | "quit"
  | "pack"
  | "extract"
  | "open-tab"
  | "clip-copy"
  | "clip-cut"
  | "clip-paste";

/** Names of the affected entries (selection or cursor). */
export function targetNames(side: Side): string[] {
  const p = panelOf(usePanes.getState(), side);
  if (p.selected.size > 0) return [...p.selected];
  const cur = p.entries[p.cursor];
  return cur && !cur.parent ? [cur.name] : [];
}

export function runAction(id: ActionId, side: Side): void {
  const state = usePanes.getState();
  const p = panelOf(state, side);

  switch (id) {
    case "quit":
      // window.close() is a browser API and does not close a native
      // Tauri window (a leftover from before the migration to Tauri).
      // Via getCurrentWindow().close(), the close runs through
      // the same onCloseRequested handler as the native window button
      // (App.tsx, TICKET-004), including the prompt for running transfers.
      if (hasTauri) {
        void import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
          getCurrentWindow().close(),
        );
      } else {
        window.close?.();
      }
      break;

    // ----- Clipboard -----
    case "clip-copy": {
      const names = targetNames(side);
      if (names.length) useClipboard.getState().set("copy", p.path, names);
      break;
    }
    case "clip-cut": {
      const names = targetNames(side);
      if (names.length) useClipboard.getState().set("cut", p.path, names);
      break;
    }
    case "clip-paste":
      void runPaste(side);
      break;

    // ----- File operations -----
    case "copy":
      void runCopy(side);
      break;
    case "move":
      void runMove(side);
      break;
    case "mkdir":
      runMkdir(side);
      break;
    case "rename":
      runRename(side);
      break;
    case "delete":
      runDelete(side);
      break;

    // ----- Preview (F3) / edit (F4) -----
    case "view":
      void runView(side);
      break;
    case "edit":
      runEdit(side);
      break;
    case "edit-with":
      runEditWith(side);
      break;

    // ----- Archives -----
    case "pack":
      runPack(side);
      break;
    case "extract":
      runExtractArchive(side);
      break;

    // ----- Open folder in a new tab -----
    case "open-tab": {
      if (p.archive) break; // in Archiven nicht sinnvoll
      const cur = p.entries[p.cursor];
      if (!cur || !cur.is_dir) break;
      const target = cur.parent
        ? parentPath(p.path)
        : joinPath(p.path, cur.name);
      void state.addTab(side, target);
      break;
    }
  }
}
