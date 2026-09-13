// ============================================================
// What a pane *is*, and the transitions on it (TICKET-035).
//
// Pure functions over pane state: no store, no IPC, no React. Sorting,
// rebuilding the displayed rows from a raw listing, pruning a selection
// against what still exists, keeping the cursor on the entry it was on,
// and the range selection a Shift-drag builds.
//
// These are the parts where a mistake is silent — a selection that
// still names deleted files, a cursor that lands on the wrong row —
// which is exactly why they are separated from the asynchronous
// loading in `panesStore.ts` and tested directly.
// ============================================================

import type { AppError, DirEntry, GitStatus } from "@/ipc/bindings";
import { splitName } from "@/lib/format";
import { isRoot } from "@/lib/path";
import { useSettings } from "../settingsStore";

export type Side = "left" | "right";

/** How many names of a restore/load actually reached the selection. */
export interface SelectionReport {
  applied: number;
  /** Names that no longer exist or are hidden by a filter. */
  missing: number;
}

/** One side's outcome of a pane comparison. */
export interface CompareApply {
  /** File names to hide (null = no comparison filter on this side). */
  hidden: Set<string> | null;
  /** The new file selection of that side. */
  selected: Set<string>;
}

export type SortKey = "name" | "ext" | "size" | "date";

/** Rendering of the file list (per pane). */
export type ViewMode = "details" | "list" | "thumbnails" | "tree";

/** A row in the list – DirEntry plus an optional ".." marker. */
export interface Row extends DirEntry {
  parent?: boolean;
}

export interface TabState {
  id: string;
  request: number;
  loading: boolean;
  loadError: AppError | string | null;
  freshness: "current" | "refreshing" | "stale" | "offline" | "manual";
  refreshError: AppError | null;
  path: string;
  /** While browsing an archive: path of the .zip file, otherwise null. */
  archive: string | null;
  raw: DirEntry[]; // ungefiltert, wie vom Backend geliefert
  entries: Row[]; // gefiltert + sortiert (+ ".." als erste Zeile)
  /** Quick filter (substring of the name); empty = no filter. */
  filter: string;
  cursor: number;
  selected: Set<string>;
  anchor: number; // Ankerzeile für Bereichsauswahl
  shiftBase: Set<string> | null; // Auswahl-Snapshot während einer Shift-Sitzung
  sort: { key: SortKey; asc: boolean };
  /** Explicit selection snapshot ("Mark → remember", TICKET-059).
   *  Bound to the canonical location, never to row indices, so sorting,
   *  a refresh or a rename cannot make it select the wrong entry. */
  snapshot: { location: string; names: string[] } | null;
  /** Names hidden by the comparison filter (TICKET-059); null = no
   *  comparison filter. Temporary and independent of `filter`. */
  compareHidden: Set<string> | null;
  /** Git status of the folder (null = not loaded yet / disabled). */
  git: GitStatus | null;
  /** Navigation history of this tab (back/forward). */
  history: string[];
  historyIndex: number;
}

export interface SideState {
  tabs: TabState[];
  activeTab: number;
  /** View mode of this pane (detail/list/thumbnails). */
  viewMode: ViewMode;
  /** Column count reported by the FileTable (for grid keyboard navigation). */
  gridCols: number;
}

export const COLLATOR = new Intl.Collator("de", { numeric: true, sensitivity: "base" });

let nextTabId = 0;

export function newTab(path = ""): TabState {
  return {
    id: `tab-${++nextTabId}`,
    request: 0,
    loading: false,
    loadError: null,
    freshness: "current",
    refreshError: null,
    path,
    archive: null,
    raw: [],
    entries: [],
    cursor: 0,
    selected: new Set(),
    anchor: 0,
    shiftBase: null,
    filter: "",
    sort: { key: "name", asc: true },
    snapshot: null,
    compareHidden: null,
    git: null,
    history: path ? [path] : [],
    historyIndex: path ? 0 : -1,
  };
}

/** Applies a sort order to a tab and rebuilds its visible rows. */
/** Applies a sort order to a tab and rebuilds its visible rows. */
export function applySort(
  t: TabState,
  sort: { key: SortKey; asc: boolean },
): TabState {
  return {
    ...t,
    sort,
    entries: rebuildEntries(
      t.raw,
      t.path,
      sort,
      t.archive !== null,
      t.filter,
      t.compareHidden,
    ),
    cursor: 0,
    anchor: 0,
    shiftBase: null,
  };
}

export function clamp(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, n));
}

/**
 * Canonical address of a tab: the provider identity and folder, plus the
 * inner path when an archive is open. Selection snapshots and selection
 * files are bound to this string — never to row positions, so sorting,
 * a refresh or a rename cannot shift a remembered mark onto its
 * neighbour (TICKET-059).
 */
export function locationOf(t: Pick<TabState, "path" | "archive">): string {
  return t.archive ? `${t.archive}!${t.path}` : t.path;
}

/**
 * Drops marks on entries that are not visible any more. A hidden entry
 * must never stay a target for a later copy or delete (TICKET-059), so
 * every path that changes the visible rows runs the selection through
 * here. The snapshot is deliberately *not* pruned: it survives a filter.
 */
export function pruneSelection(selected: Set<string>, entries: Row[]): Set<string> {
  if (selected.size === 0) return selected;
  const visible = new Set(entries.filter((e) => !e.parent).map((e) => e.name));
  const next = new Set<string>();
  for (const name of selected) if (visible.has(name)) next.add(name);
  return next.size === selected.size ? selected : next;
}

/**
 * Index of `name` in freshly built rows, falling back to the clamped old
 * index. Hiding or re-showing rows must not drag the cursor onto an
 * unrelated entry (TICKET-059).
 */
export function cursorFor(entries: Row[], name: string | undefined, old: number): number {
  const found = name ? entries.findIndex((r) => !r.parent && r.name === name) : -1;
  return found >= 0 ? found : clamp(old, 0, Math.max(0, entries.length - 1));
}

/** Sorts real entries in place; folders first depending on the setting. */
export function sortEntries(entries: Row[], key: SortKey, asc: boolean): void {
  const dir = asc ? 1 : -1;
  const foldersFirst = useSettings.getState().foldersFirst;
  entries.sort((a, b) => {
    if (foldersFirst && a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    let cmp = 0;
    switch (key) {
      case "size":
        cmp = a.size - b.size;
        break;
      case "date":
        cmp = a.modified - b.modified;
        break;
      case "ext":
        cmp = COLLATOR.compare(splitName(a).ext, splitName(b).ext);
        break;
      default:
        cmp = COLLATOR.compare(a.name, b.name);
    }
    if (cmp === 0) cmp = COLLATOR.compare(a.name, b.name);
    return cmp * dir;
  });
}

/** Raw data → displayed rows ("system files"/quick filter, sorting, ".."). */
/** Raw data → displayed rows ("system files"/quick filter, sorting, ".."). */
export function rebuildEntries(
  raw: DirEntry[],
  path: string,
  sort: TabState["sort"],
  inArchive: boolean,
  filter = "",
  hidden: Set<string> | null = null,
): Row[] {
  const hideSystem = useSettings.getState().hideSystemFiles;
  let list = raw;
  if (hideSystem) list = list.filter((e) => !e.hidden);
  if (filter) {
    const f = filter.toLowerCase();
    list = list.filter((e) => e.name.toLowerCase().includes(f));
  }
  // Comparison filter (TICKET-059): only equal *files* disappear —
  // folders and ".." always stay reachable.
  if (hidden) list = list.filter((e) => e.is_dir || !hidden.has(e.name));

  const rows: Row[] = list.map((e) => ({ ...e }));
  sortEntries(rows, sort.key, sort.asc);
  // Inside an archive always a ".." row (also at root level → leaves the archive).
  if (inArchive || !isRoot(path)) {
    rows.unshift({
      name: "..",
      is_dir: true,
      is_symlink: false,
      size: 0,
      modified: 0,
      mode: null,
      hidden: false,
      readonly: false,
      executable: false,
      parent: true,
    });
  }
  return rows;
}

/**
 * Range selection based on a snapshot (shiftBase) + anchor.
 * Allows correct growing and shrinking when the direction changes.
 */
export function applyShiftSelection(t: TabState, cursor: number): TabState {
  const base = t.shiftBase ?? new Set(t.selected);
  const anchor = t.shiftBase ? t.anchor : t.cursor;
  const selected = new Set(base);
  const [lo, hi] = anchor <= cursor ? [anchor, cursor] : [cursor, anchor];
  for (let i = lo; i <= hi; i++) {
    const r = t.entries[i];
    if (r && !r.parent) selected.add(r.name);
  }
  return { ...t, cursor, anchor, shiftBase: base, selected };
}
