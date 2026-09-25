import { openFile } from "./openFile";
// ============================================================
// Opening an entry: navigate folders, enter/leave
// archives, open files with the default program.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { guard } from "./guard";
import { joinPath, parentPath } from "@/lib/path";
import { isArchive } from "@/lib/archive";

/** Opens the entry at `index` in the active tab of `side`. */
export function openEntry(side: Side, index: number): void {
  const store = usePanes.getState();
  const p = panelOf(store, side);
  const entry = p.entries[index];
  if (!entry) return;

  // ----- Inside an archive -----
  if (p.archive) {
    if (entry.parent) {
      if (p.path === "/" || p.path === "") {
        // Archive root: leave the archive → folder of the archive file.
        store.loadDir(side, parentPath(p.archive));
      } else {
        store.loadArchive(side, p.archive, parentPath(p.path));
      }
    } else if (entry.is_dir) {
      store.loadArchive(side, p.archive, joinPath(p.path, entry.name));
    }
    // Files inside an archive: extract via copy (F5).
    return;
  }

  // ----- Real file system -----
  if (entry.parent) {
    store.loadDir(side, parentPath(p.path));
  } else if (entry.is_dir) {
    store.loadDir(side, joinPath(p.path, entry.name));
  } else if (isArchive(entry.name)) {
    store.loadArchive(side, joinPath(p.path, entry.name), "");
  } else {
    // A broken symlink lands here (it is not a directory), and so does any
    // other unopenable file: report the reason rather than doing nothing.
    // Handing a path to the OS is a capability of the location
    // (TICKET-031), so a remote entry says so instead of being launched.
    if (!guard(side, p.path, "systemOpen")) return;
    openFile(joinPath(p.path, entry.name));
  }
}

/** Open an explicit location in the opposite active tab, without changing focus. */
export async function openOpposite(side: Side, target?: { path: string; archive: string | null }): Promise<void> {
  const store = usePanes.getState();
  const source = target ?? panelOf(store, side);
  const destination = side === "left" ? "right" : "left";
  if (source.archive) await store.loadArchive(destination, source.archive, source.path);
  else await store.loadDir(destination, source.path);
}
