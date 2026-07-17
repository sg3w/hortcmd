// ============================================================
// Opening an entry: navigate folders, enter/leave
// archives, open files with the default program.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { openPath } from "@/ipc/client";
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
    openPath(joinPath(p.path, entry.name));
  }
}
