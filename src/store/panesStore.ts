// ============================================================
// The pane store (TICKET-035).
//
// What is left here after the split is the part that is genuinely
// stateful and asynchronous: loading a folder or an archive, the
// navigation history, tabs, and restoring a saved session.
//
// Everything pure — what a pane *is*, sorting, rebuilding the rows,
// pruning a selection, keeping the cursor, the Shift range — lives in
// `panes/state.ts` and is tested there directly, without a store.
// ============================================================

import { create } from "zustand";
import { historyLocation, recordNavigation, type HistoryNavigation } from "./panes/history";
import { asAppError } from "@/lib/appError";
import type { RefreshStatus } from "@/ipc/bindings/RefreshStatus";
import type { RefreshSubscription } from "@/ipc/bindings/RefreshSubscription";
import type { DirListing, GitStatus } from "@/ipc/bindings";
import { homeDir, listArchive, listDir, requestGitStatus } from "@/ipc/client";
import type { SavedSession, SavedSide } from "./sessionStore";
import { globToRegExp } from "@/lib/glob";
import { splitName } from "@/lib/format";
import { translate, useSettings } from "./settingsStore";
import { pushRecent } from "./historyStore";
import { useOps } from "./opsStore";
import { allows, primeCaps } from "./capsStore";
import {
  applyShiftSelection,
  applySort,
  clamp,
  cursorFor,
  newTab,
  locationOf,
  pruneSelection,
  rebuildEntries,
  type CompareApply,
  type Row,
  type SelectionReport,
  type Side,
  type SideState,
  type SortKey,
  type TabState,
  type ViewMode,
} from "./panes/state";
import {
  archivePassword,
  isPasswordError,
  promptArchivePassword,
  isWrongPassword,
} from "@/features/commander/archivePw";

// The pane vocabulary stays importable from here: every caller uses
// `@/store/panesStore`, and the split must not ripple into them.
export { locationOf, sortEntries } from "./panes/state";
export type {
  CompareApply,
  Row,
  SelectionReport,
  Side,
  SideState,
  SortKey,
  TabState,
  ViewMode,
} from "./panes/state";

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
  loadDir: (side: Side, path: string, record?: HistoryNavigation) => Promise<void>;
  loadArchive: (side: Side, archive: string, inner: string, record?: HistoryNavigation) => Promise<void>;
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
  /** "*": invert all visible entry marks, excluding the parent row. */
  invertSelection: (side: Side) => void;
  /** Pattern selection includes visible files and folders. */
  selectPattern: (
    side: Side,
    pattern: string,
    add: boolean,
  ) => void;
  /** Mark every visible entry except the parent row. */
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

interface RequestTarget { side: Side; id: string; request: number }

/** Response ownership belongs to a tab and request, never the active index. */
function beginRequest(side: Side, navigation: boolean): RequestTarget | undefined {
  const tab = panelOf(usePanes.getState(), side);
  if (!navigation && tab.loading) return;
  const target = { side, id: tab.id, request: tab.request + 1 };
  usePanes.setState(withActiveTab(usePanes.getState(), side, t => ({
    ...t, request: target.request, loading: navigation, loadError: null, freshness: "refreshing",
  })));
  return target;
}

function applyRequest(target: RequestTarget, fn: (tab: TabState) => TabState): boolean {
  let applied = false;
  usePanes.setState(state => {
    const side = state[target.side];
    const index = side.tabs.findIndex(t => t.id === target.id && t.request === target.request);
    if (index < 0) return state;
    const tabs = side.tabs.slice();
    const updated = fn(tabs[index]);
    applied = true;
    if (updated === tabs[index]) return state;
    tabs[index] = updated;
    return { [target.side]: { ...side, tabs } };
  });
  return applied;
}

function requestFailed(target: RequestTarget, error: unknown) {
  applyRequest(target, t => ({ ...t, loading: false, loadError: asAppError(error) ?? String(error), freshness: "stale" }));
}

/** Backend work carries the generation captured before its filesystem read. */
export function beginRefresh(target: RefreshSubscription) {
  for (const side of ["left", "right"] as const) {
    applyRequest({ side, id: target.id, request: target.generation }, tab => tab.path === target.path && !tab.loading ? { ...tab, freshness: "refreshing" } : tab);
  }
}

export function applyRefreshResult(target: RefreshSubscription, listing: DirListing | null, error: unknown = null) {
  for (const side of ["left", "right"] as const) {
    applyRequest({ side, id: target.id, request: target.generation }, tab => {
      if (tab.loading || tab.archive || tab.path !== target.path) return tab;
      if (!listing) return { ...tab, loadError: asAppError(error) ?? String(error), freshness: "stale" };
      const entries = rebuildEntries(listing.entries, listing.path, tab.sort, false, tab.filter);
      const names = new Set(entries.map(entry => entry.name));
      return { ...tab, raw: listing.entries, entries, loadError: null,
        freshness: "current", refreshError: null, compareHidden: null,
        cursor: cursorFor(entries, tab.entries[tab.cursor]?.name, tab.cursor),
        selected: new Set([...tab.selected].filter(name => names.has(name))), shiftBase: null };
    });
  }
}

export function applyRefreshStatus(status: RefreshStatus) {
  for (const target of status.targets) {
    for (const side of ["left", "right"] as const) {
      applyRequest({ side, id: target.id, request: target.generation }, tab => {
        const freshness = !status.connected ? "offline" : status.mode === "manual" ? "manual" : status.error ? "stale" : tab.freshness === "refreshing" ? "refreshing" : "current";
        if (tab.freshness === freshness && tab.refreshError?.id === status.error?.id) return tab;
        return { ...tab, freshness, refreshError: status.error };
      });
    }
  }
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
    const target = path ?? s.tabs[s.activeTab].path;
    set({ active: side, [side]: { ...s, tabs: [...s.tabs, newTab()], activeTab: s.tabs.length } });
    await get().loadDir(side, target);
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
    const target = beginRequest(side, true)!;
    let res;
    try { res = await listDir(path); } catch (error) { requestFailed(target, error); return; }
    const applied = applyRequest(target, (t) => {
        return {
          ...t,
          path: res.path,
          loading: false,
          loadError: null,
          freshness: "current",
          refreshError: null,
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
          ...recordNavigation(t, { path: res.path, archive: null }, record),
        };
      });
    if (!applied) return;
    if (record === true) pushRecent(res.path);
    if (panelOf(get(), side).id !== target.id) return;
    get().clearCompareFilter();
    get().loadGit(side);
  },

  goBack: (side) => {
    const s = get();
    const t = panelOf(s, side);
    if (t.historyIndex <= 0) return;
    const idx = t.historyIndex - 1;
    const target = historyLocation(t.history[idx]);
    if (target.archive !== null) void s.loadArchive(side, target.archive, target.path, idx);
    else void s.loadDir(side, target.path, idx);
  },

  goForward: (side) => {
    const s = get();
    const t = panelOf(s, side);
    if (t.historyIndex >= t.history.length - 1) return;
    const idx = t.historyIndex + 1;
    const target = historyLocation(t.history[idx]);
    if (target.archive !== null) void s.loadArchive(side, target.archive, target.path, idx);
    else void s.loadDir(side, target.path, idx);
  },

  loadArchive: async (side, archive, inner, record = true) => {
    const target = beginRequest(side, true)!;
    let res;
    try {
      res = await listArchive(archive, inner, archivePassword(archive));
    } catch (e) {
      if (!applyRequest(target, t => ({ ...t, loading: false, loadError: asAppError(e) ?? String(e) }))) return;
      const msg = String(e);
      if (isPasswordError(e)) {
        // Encrypted archive: ask for the password and load again.
        promptArchivePassword(archive, isWrongPassword(e), () => {
          const current = panelOf(get(), side);
          if (current.id === target.id && current.request === target.request) void get().loadArchive(side, archive, inner, record);
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
    const applied = applyRequest(target, (t) => ({
        ...t,
        loading: false,
        loadError: null,
        path: res.path,
        archive,
        freshness: "current",
        refreshError: null,
        ...recordNavigation(t, { path: res.path, archive }, record),
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
      }));
    if (applied && panelOf(get(), side).id === target.id) get().clearCompareFilter();
  },

  refresh: async (side, opts) => {
    const panel = get()[side].tabs[get()[side].activeTab];
    if (panel.archive) return; // Archive werden nicht extern überwacht
    const target = beginRequest(side, false);
    if (!target) return;
    let res;
    try { res = await listDir(panel.path, true); } catch (error) { requestFailed(target, error); return; }
    const applied = applyRequest(target, (t) => {
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
              ...t, freshness: "current", loadError: null, refreshError: null,
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
          ...t, freshness: "current", loadError: null, refreshError: null,
          raw: res.entries,
          entries,
          cursor: cursorFor(entries, t.entries[t.cursor]?.name, t.cursor),
          selected: new Set([...t.selected].filter((n) => names.has(n))),
          shiftBase: null,
          compareHidden: null,
        };
      });
    if (!applied || panelOf(get(), side).id !== target.id) return;
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
        // General selection includes folders (TICKET-060). Menu and
        // keyboard share this action and the same visible target set.
        const selected = new Set(t.selected);
        for (const r of t.entries) {
          if (r.parent) continue;
          selected.has(r.name)
            ? selected.delete(r.name)
            : selected.add(r.name);
        }
        return { ...t, selected, shiftBase: null };
      }),
    ),

  selectPattern: (side, pattern, add) =>
    set(
      withActiveTab(get(), side, (t) => {
        const re = globToRegExp(pattern);
        const selected = new Set(t.selected);
        for (const r of t.entries) {
          if (r.parent) continue;
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
          if (!r.parent) selected.add(r.name);
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
          cursor: cursorFor(entries, t.entries[t.cursor]?.name, t.cursor),
          selected: new Set<string>(),
          shiftBase: null,
        };
      }),
    });
    set({ left: rebuildSide(s.left), right: rebuildSide(s.right) });
  },

  restoreSession: async (session) => {
    const initial = get();
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
    const current = get();
    set({
      left: current.left === initial.left ? left : current.left,
      right: current.right === initial.right ? right : current.right,
      active: current === initial ? session.active : current.active,
    });
    get().loadGit("left");
    get().loadGit("right");
  },
}));

