// ============================================================
// Öffnen eines Eintrags: Ordner navigieren, in Archive
// eintreten/heraus, Dateien mit dem Standardprogramm öffnen.
// ============================================================

import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { openPath } from "@/ipc/client";
import { joinPath, parentPath } from "@/lib/path";
import { isArchive } from "@/lib/archive";

/** Öffnet den Eintrag an `index` im aktiven Tab von `side`. */
export function openEntry(side: Side, index: number): void {
  const store = usePanes.getState();
  const p = panelOf(store, side);
  const entry = p.entries[index];
  if (!entry) return;

  // ----- Innerhalb eines Archivs -----
  if (p.archive) {
    if (entry.parent) {
      if (p.path === "/" || p.path === "") {
        // Archivwurzel: Archiv verlassen → Ordner der Archivdatei.
        store.loadDir(side, parentPath(p.archive));
      } else {
        store.loadArchive(side, p.archive, parentPath(p.path));
      }
    } else if (entry.is_dir) {
      store.loadArchive(side, p.archive, joinPath(p.path, entry.name));
    }
    // Dateien im Archiv: über Kopieren (F5) entpacken.
    return;
  }

  // ----- Reales Dateisystem -----
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
