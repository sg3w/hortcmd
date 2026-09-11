// ============================================================
// Central state for both file panes.
// Each side has several tabs; the active tab is the working
// and target surface. Actions always affect the active tab.
// ============================================================

import { create } from "zustand";
import type { DirEntry, GitStatus } from "@/ipc/bindings";
import { homeDir, listArchive, listDir, requestGitStatus } from "@/ipc/client";
import { allows, primeCaps } from "./capsStore";
import type { SavedSession, SavedSide } from "./sessionStore";
import { splitName } from "@/lib/format";
import { isRoot } from "@/lib/path";
import { globToRegExp } from "@/lib/glob";
import { translate, useSettings } from "./settingsStore";
import { pushRecent } from "./historyStore";
import { useOps } from "./opsStore";
import {
  archivePassword,
  isPasswordError,
  promptArchivePassword,
  PW_WRONG,
} from "@/features/commander/archivePw";

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

interface PanesStore {
  active: Side;
  left: SideState;
  right: SideState;

  setActive: (side: Side) => void;
  setViewMode: (side: Side, mode: ViewMode) => void;
  /** Report the current column count of the grid view (idempotent). */
  setGridCols: (side: Side, cols: number) => void;

  // Tabs
  addTab: (side: Side, path?: string) => Promise<void>;
  closeTab: (side: Side, index: number) => void;
  selectTab: (side: Side, index: number) => void;

  // Navigation (active tab)
  /** Load a folder; `record` (default true) adds it to the history. */
  loadDir: (side: Side, path: string, record?: boolean) => Promise<void>;
  loadArchive: (side: Side, archive: string, inner: string) => Promise<void>;
  goBack: (side: Side) => void;
  goForward: (side: Side) => void;
  /** Re-read the current folder, preserving cursor/selection (watcher).
   *  With `reveal: true` (after our own file operations), newly
   *  added entries are selected instead and the cursor is set to the
   *  first of them (scrolls only if it is not already visible
   *  — see `FileTable`'s `scrollToIndex(…, {align:"auto"})`).
   *  `names` names the entries to reveal explicitly (new folder, rename), so
   *  the reveal does not depend on diffing against the previous listing — a
   *  watcher refresh in between would otherwise make the entry look "not new"
   *  and leave the cursor where it was. */
  refresh: (
    side: Side,
    opts?: { reveal?: boolean; names?: string[] },
  ) => Promise<void>;
  /** Request the Git status of the active tab (or clear it when disabled).
   *  Runs asynchronously in the backend; the result comes back via applyGitStatus. */
  loadGit: (side: Side) => void;
  /** Reload the Git status of both panes (after a settings toggle). */
  reloadGit: () => void;
  /** Apply a Git result delivered by event to all matching tabs
   *  (both panes, all tabs with the same path – no race conditions). */
  applyGitStatus: (path: string, git: GitStatus) => void;
  moveCursor: (side: Side, delta: number) => void;
  setCursor: (side: Side, index: number) => void;
  jumpCursor: (side: Side, to: "top" | "bottom") => void;

  // Selection
  toggleMark: (side: Side, index: number, advance?: boolean) => void;
  shiftMove: (side: Side, delta: number) => void;
  shiftTo: (side: Side, index: number) => void;
  /** "*": invert the *file* selection; folder marks stay untouched. */
  invertSelection: (side: Side) => void;
  /** Pattern selection; folders only take part with `includeFolders`. */
  selectPattern: (
    side: Side,
    pattern: string,
    add: boolean,
    includeFolders?: boolean,
  ) => void;
  /** Mark every visible file; existing folder marks stay. */
  selectAllFiles: (side: Side) => void;
  /** Clear the whole selection of the active tab, folders included. */
  clearSelection: (side: Side) => void;
  /** Mark every visible file sharing the cursor file's extension. */
  selectSameExt: (side: Side) => void;
  /** Store the current selection as this tab's snapshot. */
  rememberSelection: (side: Side) => void;
  /** Replace the selection by the still existing, visible snapshot
   *  names. Reports how many were applied and how many are gone. */
  restoreSelection: (side: Side) => SelectionReport;
  /** Replace the selection by the given names (selection file). */
  applySelection: (side: Side, names: string[]) => SelectionReport;
  /** Result of a pane comparison: hidden names + marks, both sides at
   *  once so a half-applied comparison can never be seen. */
  applyCompare: (result: Record<Side, CompareApply>) => void;
  /** Drop the comparison filter of one side (or of both). */
  clearCompareFilter: (side?: Side) => void;
  /** Toolbar/column click: same key toggles the direction. */
  setSort: (side: Side, key: SortKey) => void;
  /** Menu: set the sort key without toggling (TICKET-040). */
  setSortKey: (side: Side, key: SortKey) => void;
  /** Menu: set the sort direction explicitly (TICKET-040). */
  setSortAsc: (side: Side, asc: boolean) => void;
  /** Set the quick filter of the active tab (filter while typing). */
  setFilter: (side: Side, filter: string) => void;

  // after a settings change (e.g. hide system files)
  rebuildAll: () => void;

  /** Restore a saved session (tabs of both panes). */
  restoreSession: (session: SavedSession) => Promise<void>;
}

const COLLATOR = new Intl.Collator("de", { numeric: true, sensitivity: "base" });

function newTab(path = ""): TabState {
  return {
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
function applySort(
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

function clamp(n: number, min: number, max: number): number {
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
function pruneSelection(selected: Set<string>, entries: Row[]): Set<string> {
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
function cursorFor(entries: Row[], name: string | undefined, old: number): number {
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
function rebuildEntries(
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

/** Replaces the active tab of a side via a transformation function. */
function withActiveTab(
  state: PanesStore,
  side: Side,
  fn: (t: TabState) => TabState,
): Pick<PanesStore, Side> {
  const s = state[side];
  const tabs = s.tabs.slice();
  tabs[s.activeTab] = fn(tabs[s.activeTab]);
  return { [side]: { ...s, tabs } } as Pick<PanesStore, Side>;
}

/** Selector helper: active tab of a side. */
export function panelOf(state: PanesStore, side: Side): TabState {
  return state[side].tabs[state[side].activeTab];
}

export const usePanes = create<PanesStore>((set, get) => ({
  active: "left",
  left: { tabs: [newTab()], activeTab: 0, viewMode: "details", gridCols: 1 },
  right: { tabs: [newTab()], activeTab: 0, viewMode: "details", gridCols: 1 },

  setActive: (side) => set({ active: side }),

  setViewMode: (side, viewMode) =>
    set(
      (s) =>
        ({ [side]: { ...s[side], viewMode } }) as Pick<PanesStore, Side>,
    ),

  setGridCols: (side, gridCols) =>
    set((s) =>
      s[side].gridCols === gridCols
        ? s
        : ({ [side]: { ...s[side], gridCols } } as Pick<PanesStore, Side>),
    ),

  // ---------- Tabs ----------

  addTab: async (side, path) => {
    const s = get()[side];
    const src = s.tabs[s.activeTab];
    const target = path ?? src.path;
    const res = await listDir(target);
    const tab = newTab(res.path);
    tab.raw = res.entries;
    tab.entries = rebuildEntries(res.entries, res.path, tab.sort, false);
    set({
      active: side,
      [side]: { ...s, tabs: [...s.tabs, tab], activeTab: s.tabs.length },
    } as Pick<PanesStore, "active" | Side>);
    pushRecent(res.path);
    get().loadGit(side);
  },

  closeTab: (side, index) => {
    const s = get()[side];
    if (s.tabs.length <= 1) return; // letzter Tab bleibt bestehen
    const tabs = s.tabs.filter((_, i) => i !== index);
    let activeTab = s.activeTab;
    if (index < activeTab) activeTab -= 1;
    else if (index === activeTab) activeTab = Math.min(activeTab, tabs.length - 1);
    set({ [side]: { ...s, tabs, activeTab } } as Pick<PanesStore, Side>);
  },

  selectTab: (side, index) => {
    const s = get()[side];
    // A comparison always describes the two tabs that were visible when
    // it ran; switching tabs invalidates it on both sides at once.
    set({
      active: side,
      [side]: { ...s, activeTab: clamp(index, 0, s.tabs.length - 1) },
    } as Pick<PanesStore, "active" | Side>);
    get().clearCompareFilter();
  },

  // ---------- Navigation ----------


  loadDir: async (side, path, record = true) => {
    // Ask what the new location supports while the listing is on its
    // way (TICKET-031), so the menus and the command bar have their
    // answer by the time the folder is on screen.
    primeCaps(path);
    const res = await listDir(path);
    set(
      withActiveTab(get(), side, (t) => {
        let history = t.history;
        let historyIndex = t.historyIndex;
        // Put the new folder into the history (discard the forward branch);
        // don't duplicate the same path (e.g. a refresh).
        if (record && res.path !== history[historyIndex]) {
          history = [...history.slice(0, historyIndex + 1), res.path];
          historyIndex = history.length - 1;
        }
        return {
          ...t,
          path: res.path,
          archive: null,
          raw: res.entries,
          filter: "", // Navigation setzt den Schnellfilter zurück
          entries: rebuildEntries(res.entries, res.path, t.sort, false),
          cursor: 0,
          selected: new Set<string>(),
          anchor: 0,
          shiftBase: null,
          // Leaving the folder invalidates both the snapshot and a
          // comparison result – they belong to the old location.
          snapshot: null,
          compareHidden: null,
          git: null,
          history,
          historyIndex,
        };
      }),
    );
    if (record) pushRecent(res.path);
    get().clearCompareFilter();
    get().loadGit(side);
  },

  goBack: (side) => {
    const s = get();
    const t = panelOf(s, side);
    if (t.historyIndex <= 0) return;
    const idx = t.historyIndex - 1;
    const target = t.history[idx];
    set(withActiveTab(get(), side, (tt) => ({ ...tt, historyIndex: idx })));
    void s.loadDir(side, target, false);
  },

  goForward: (side) => {
    const s = get();
    const t = panelOf(s, side);
    if (t.historyIndex >= t.history.length - 1) return;
    const idx = t.historyIndex + 1;
    const target = t.history[idx];
    set(withActiveTab(get(), side, (tt) => ({ ...tt, historyIndex: idx })));
    void s.loadDir(side, target, false);
  },

  loadArchive: async (side, archive, inner) => {
    let res;
    try {
      res = await listArchive(archive, inner, archivePassword(archive));
    } catch (e) {
      const msg = String(e);
      if (isPasswordError(msg)) {
        // Encrypted archive: ask for the password and load again.
        promptArchivePassword(archive, msg.includes(PW_WRONG), () => {
          void get().loadArchive(side, archive, inner);
        });
        return;
      }
      useOps.getState().requestConfirm({
        title: translate("archive.error"),
        message: msg,
        danger: true,
        onConfirm: () => {},
      });
      return;
    }
    set(
      withActiveTab(get(), side, (t) => ({
        ...t,
        path: res.path,
        archive,
        raw: res.entries,
        filter: "",
        entries: rebuildEntries(res.entries, res.path, t.sort, true),
        cursor: 0,
        selected: new Set<string>(),
        anchor: 0,
        shiftBase: null,
        snapshot: null,
        compareHidden: null,
        git: null,
      })),
    );
    get().clearCompareFilter();
  },

  refresh: async (side, opts) => {
    const panel = get()[side].tabs[get()[side].activeTab];
    if (panel.archive) return; // Archive werden nicht extern überwacht
    const res = await listDir(panel.path);
    set(
      withActiveTab(get(), side, (t) => {
        // A refresh means the contents may have moved on, so a
        // comparison result from before it is dropped rather than left
        // hiding entries that might no longer be equal (TICKET-059).
        const entries = rebuildEntries(res.entries, res.path, t.sort, false, t.filter);
        const names = new Set(entries.map((e) => e.name));
        // Select newly added entries if applicable and bring them into view
        // (target of a copy/move/paste operation), instead of simply
        // preserving cursor/selection as with the watcher refresh.
        if (opts?.reveal) {
          const wanted = opts.names && new Set(opts.names);
          const oldNames = new Set(t.raw.map((e) => e.name));
          const added = entries.filter((e) => {
            if (e.parent) return false;
            return wanted ? wanted.has(e.name) : !oldNames.has(e.name);
          });
          if (added.length > 0) {
            return {
              ...t,
              raw: res.entries,
              entries,
              cursor: entries.indexOf(added[0]),
              selected: new Set(added.map((e) => e.name)),
              shiftBase: null,
              compareHidden: null,
            };
          }
        }
        return {
          ...t,
          raw: res.entries,
          entries,
          cursor: clamp(t.cursor, 0, Math.max(0, entries.length - 1)),
          selected: new Set([...t.selected].filter((n) => names.has(n))),
          shiftBase: null,
          compareHidden: null,
        };
      }),
    );
    get().clearCompareFilter();
    get().loadGit(side);
  },

  loadGit: (side) => {
    const enabled = useSettings.getState().gitEnabled;
    const s = get()[side];
    const tab = s.tabs[s.activeTab];
    if (!enabled || tab.archive || !tab.path) {
      if (tab.git) {
        set(withActiveTab(get(), side, (t) => ({ ...t, git: null })));
      }
      return;
    }
    // Fire-and-forget: runs in the background in the backend, the result comes
    // via the "git-support-ready" event → applyGitStatus. A location
    // without Git support is not asked at all (TICKET-031) — the `git`
    // CLI only runs in a local working directory.
    if (!allows(tab.path, "gitStatus")) {
      if (tab.git) {
        set(withActiveTab(get(), side, (t) => ({ ...t, git: null })));
      }
      return;
    }
    requestGitStatus(tab.path);
  },

  reloadGit: () => {
    get().loadGit("left");
    get().loadGit("right");
  },

  applyGitStatus: (path, git) => {
    // Discard results that arrived after disabling.
    if (!useSettings.getState().gitEnabled) return;
    const applySide = (side: SideState): SideState => ({
      ...side,
      tabs: side.tabs.map((t) =>
        !t.archive && t.path === path ? { ...t, git } : t,
      ),
    });
    set((s) => ({ left: applySide(s.left), right: applySide(s.right) }));
  },

  moveCursor: (side, delta) =>
    set(
      withActiveTab(get(), side, (t) => {
        const cursor = clamp(t.cursor + delta, 0, t.entries.length - 1);
        return { ...t, cursor, anchor: cursor, shiftBase: null };
      }),
    ),

  setCursor: (side, index) =>
    set(
      withActiveTab(get(), side, (t) => {
        const cursor = clamp(index, 0, t.entries.length - 1);
        return { ...t, cursor, anchor: cursor, shiftBase: null };
      }),
    ),

  jumpCursor: (side, to) =>
    set(
      withActiveTab(get(), side, (t) => {
        const cursor = to === "top" ? 0 : t.entries.length - 1;
        return { ...t, cursor, anchor: cursor, shiftBase: null };
      }),
    ),

  // ---------- Selection ----------

  toggleMark: (side, index, advance = false) =>
    set(
      withActiveTab(get(), side, (t) => {
        const row = t.entries[index];
        if (!row || row.parent) return t;
        const selected = new Set(t.selected);
        selected.has(row.name)
          ? selected.delete(row.name)
          : selected.add(row.name);
        const cursor = advance
          ? clamp(index + 1, 0, t.entries.length - 1)
          : index;
        return { ...t, selected, cursor, anchor: index, shiftBase: null };
      }),
    ),

  shiftMove: (side, delta) =>
    set(
      withActiveTab(get(), side, (t) => {
        const cursor = clamp(t.cursor + delta, 0, t.entries.length - 1);
        return applyShiftSelection(t, cursor);
      }),
    ),

  shiftTo: (side, index) =>
    set(
      withActiveTab(get(), side, (t) => {
        const cursor = clamp(index, 0, t.entries.length - 1);
        return applyShiftSelection(t, cursor);
      }),
    ),

  invertSelection: (side) =>
    set(
      withActiveTab(get(), side, (t) => {
        // File semantics (TICKET-059): only files flip, a folder the
        // user marked by hand keeps its mark. Menu entry and "*" run
        // through this very action, so they cannot disagree.
        const selected = new Set(t.selected);
        for (const r of t.entries) {
          if (r.parent || r.is_dir) continue;
          selected.has(r.name)
            ? selected.delete(r.name)
            : selected.add(r.name);
        }
        return { ...t, selected, shiftBase: null };
      }),
    ),

  selectPattern: (side, pattern, add, includeFolders = false) =>
    set(
      withActiveTab(get(), side, (t) => {
        const re = globToRegExp(pattern);
        const selected = new Set(t.selected);
        for (const r of t.entries) {
          if (r.parent) continue;
          if (r.is_dir && !includeFolders) continue;
          if (re.test(r.name)) {
            add ? selected.add(r.name) : selected.delete(r.name);
          }
        }
        return { ...t, selected, shiftBase: null };
      }),
    ),

  selectAllFiles: (side) =>
    set(
      withActiveTab(get(), side, (t) => {
        const selected = new Set(t.selected);
        for (const r of t.entries) {
          if (!r.parent && !r.is_dir) selected.add(r.name);
        }
        return { ...t, selected, shiftBase: null };
      }),
    ),

  clearSelection: (side) =>
    set(
      withActiveTab(get(), side, (t) =>
        t.selected.size === 0
          ? t
          : { ...t, selected: new Set<string>(), shiftBase: null },
      ),
    ),

  selectSameExt: (side) =>
    set(
      withActiveTab(get(), side, (t) => {
        const cur = t.entries[t.cursor];
        if (!cur || cur.parent || cur.is_dir) return t;
        // "Last extension", case-insensitively: `a.tar.gz` groups by
        // `gz`, while `.env` and `README` group by "no extension".
        const extOf = (r: Row) => splitName(r).ext.toLowerCase();
        const want = extOf(cur);
        const selected = new Set(t.selected);
        for (const r of t.entries) {
          if (r.parent || r.is_dir) continue;
          if (extOf(r) === want) selected.add(r.name);
        }
        return { ...t, selected, shiftBase: null };
      }),
    ),

  rememberSelection: (side) =>
    set(
      withActiveTab(get(), side, (t) => ({
        ...t,
        // An empty selection is a valid snapshot; "remember" always
        // replaces the previous one.
        snapshot: { location: locationOf(t), names: [...t.selected] },
      })),
    ),

  restoreSelection: (side) => {
    const t = panelOf(get(), side);
    const snap = t.snapshot;
    if (!snap || snap.location !== locationOf(t)) return { applied: 0, missing: 0 };
    return get().applySelection(side, snap.names);
  },

  applySelection: (side, names) => {
    const t = panelOf(get(), side);
    const visible = new Set(t.entries.filter((r) => !r.parent).map((r) => r.name));
    const selected = new Set<string>();
    let missing = 0;
    for (const name of names) {
      if (visible.has(name)) selected.add(name);
      else missing += 1;
    }
    set(withActiveTab(get(), side, (tt) => ({ ...tt, selected, shiftBase: null })));
    return { applied: selected.size, missing };
  },

  applyCompare: (result) =>
    set((s) => {
      const applySide = (side: Side): SideState => {
        const { hidden, selected } = result[side];
        const sd = s[side];
        const tabs = sd.tabs.map((t, i) => {
          if (i !== sd.activeTab) return t;
          const entries = rebuildEntries(
            t.raw,
            t.path,
            t.sort,
            t.archive !== null,
            t.filter,
            hidden,
          );
          return {
            ...t,
            compareHidden: hidden,
            entries,
            // Hiding may remove the row under the cursor; otherwise the
            // cursor stays on the same entry, not on the same index.
            cursor: cursorFor(entries, t.entries[t.cursor]?.name, t.cursor),
            selected: pruneSelection(selected, entries),
            shiftBase: null,
          };
        });
        return { ...sd, tabs };
      };
      return { left: applySide("left"), right: applySide("right") };
    }),

  clearCompareFilter: (side) =>
    set((s) => {
      const sides: Side[] = side ? [side] : ["left", "right"];
      const next: Partial<Record<Side, SideState>> = {};
      for (const sd of sides) {
        const state = s[sd];
        if (state.tabs.every((t) => t.compareHidden === null)) continue;
        next[sd] = {
          ...state,
          tabs: state.tabs.map((t) => {
            if (t.compareHidden === null) return t;
            const entries = rebuildEntries(
              t.raw,
              t.path,
              t.sort,
              t.archive !== null,
              t.filter,
            );
            // Lossless: the previously hidden files come back, the
            // selection and the cursor entry are untouched.
            return {
              ...t,
              compareHidden: null,
              entries,
              cursor: cursorFor(entries, t.entries[t.cursor]?.name, t.cursor),
            };
          }),
        };
      }
      return next as Partial<PanesStore>;
    }),

  setSort: (side, key) =>
    set(
      withActiveTab(get(), side, (t) =>
        applySort(t, { key, asc: t.sort.key === key ? !t.sort.asc : true }),
      ),
    ),

  setSortKey: (side, key) =>
    set(withActiveTab(get(), side, (t) => applySort(t, { ...t.sort, key }))),

  setSortAsc: (side, asc) =>
    set(withActiveTab(get(), side, (t) => applySort(t, { ...t.sort, asc }))),

  setFilter: (side, filter) =>
    set(
      withActiveTab(get(), side, (t) => {
        const entries = rebuildEntries(
          t.raw,
          t.path,
          t.sort,
          t.archive !== null,
          filter,
          t.compareHidden,
        );
        // Cursor to the first real match (after a possible ".." row).
        const first = entries.findIndex((e) => !e.parent);
        return {
          ...t,
          filter,
          entries,
          cursor: first >= 0 ? first : 0,
          anchor: first >= 0 ? first : 0,
          shiftBase: null,
          // Entries the filter hides must not stay operation targets
          // (TICKET-059); the snapshot keeps them regardless.
          selected: pruneSelection(t.selected, entries),
        };
      }),
    ),

  rebuildAll: () => {
    const s = get();
    const rebuildSide = (side: SideState): SideState => ({
      ...side,
      tabs: side.tabs.map((t) => {
        const entries = rebuildEntries(
          t.raw,
          t.path,
          t.sort,
          t.archive !== null,
          t.filter,
          t.compareHidden,
        );
        return {
          ...t,
          entries,
          cursor: clamp(t.cursor, 0, Math.max(0, entries.length - 1)),
          selected: new Set<string>(),
          shiftBase: null,
        };
      }),
    });
    set({ left: rebuildSide(s.left), right: rebuildSide(s.right) });
  },

  restoreSession: async (session) => {
    // Build a loaded tab from a folder path (null on error).
    const buildTab = async (path: string): Promise<TabState | null> => {
      try {
        const res = await listDir(path);
        const tab = newTab(res.path);
        tab.raw = res.entries;
        tab.entries = rebuildEntries(res.entries, res.path, tab.sort, false);
        return tab;
      } catch {
        return null; // Ordner existiert nicht mehr → überspringen
      }
    };

    const buildSide = async (
      saved: SavedSide,
      home: string,
    ): Promise<SideState> => {
      const built = await Promise.all(saved.tabs.map((t) => buildTab(t.path)));
      let tabs = built.filter((t): t is TabState => t !== null);
      // No tab restorable → home as a fallback.
      if (tabs.length === 0) {
        const fallback = await buildTab(home);
        tabs = [fallback ?? newTab(home)];
      }
      return {
        tabs,
        activeTab: clamp(saved.activeTab, 0, tabs.length - 1),
        viewMode: saved.viewMode,
        gridCols: 1,
      };
    };

    const home = await homeDir();
    const [left, right] = await Promise.all([
      buildSide(session.left, home),
      buildSide(session.right, home),
    ]);
    set({ left, right, active: session.active });
    get().loadGit("left");
    get().loadGit("right");
  },
}));

/**
 * Range selection based on a snapshot (shiftBase) + anchor.
 * Allows correct growing and shrinking when the direction changes.
 */
function applyShiftSelection(t: TabState, cursor: number): TabState {
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
