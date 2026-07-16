// ============================================================
// Zentraler Zustand für beide Dateifenster.
// Jede Seite hat mehrere Tabs; der aktive Tab ist die Arbeits-
// und Zielfläche. Aktionen wirken immer auf den aktiven Tab.
// ============================================================

import { create } from "zustand";
import type { DirEntry, GitStatus } from "@/ipc/bindings";
import { homeDir, listArchive, listDir, requestGitStatus } from "@/ipc/client";
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
export type SortKey = "name" | "ext" | "size" | "date";
/** Darstellung der Dateiliste (pro Fenster). */
export type ViewMode = "details" | "list" | "thumbnails" | "tree";

/** Eine Zeile in der Liste – DirEntry plus optionale ".."-Markierung. */
export interface Row extends DirEntry {
  parent?: boolean;
}

export interface TabState {
  path: string;
  /** Beim Browsen in einem Archiv: Pfad der .zip-Datei, sonst null. */
  archive: string | null;
  raw: DirEntry[]; // ungefiltert, wie vom Backend geliefert
  entries: Row[]; // gefiltert + sortiert (+ ".." als erste Zeile)
  /** Schnellfilter (Teilstring des Namens); leer = kein Filter. */
  filter: string;
  cursor: number;
  selected: Set<string>;
  anchor: number; // Ankerzeile für Bereichsauswahl
  shiftBase: Set<string> | null; // Auswahl-Snapshot während einer Shift-Sitzung
  sort: { key: SortKey; asc: boolean };
  /** Git-Status des Ordners (null = noch nicht geladen / deaktiviert). */
  git: GitStatus | null;
  /** Navigations-Verlauf dieses Tabs (Zurück/Vor). */
  history: string[];
  historyIndex: number;
}

export interface SideState {
  tabs: TabState[];
  activeTab: number;
  /** Ansichtsmodus dieses Fensters (Detail/Liste/Miniaturansicht). */
  viewMode: ViewMode;
  /** Vom FileTable gemeldete Spaltenzahl (für Grid-Tastaturnavigation). */
  gridCols: number;
}

interface PanesStore {
  active: Side;
  left: SideState;
  right: SideState;

  setActive: (side: Side) => void;
  setViewMode: (side: Side, mode: ViewMode) => void;
  /** Aktuelle Spaltenzahl der Grid-Ansicht melden (idempotent). */
  setGridCols: (side: Side, cols: number) => void;

  // Tabs
  addTab: (side: Side, path?: string) => Promise<void>;
  closeTab: (side: Side, index: number) => void;
  selectTab: (side: Side, index: number) => void;

  // Navigation (aktiver Tab)
  /** Ordner laden; `record` (Standard true) trägt ihn in den Verlauf ein. */
  loadDir: (side: Side, path: string, record?: boolean) => Promise<void>;
  loadArchive: (side: Side, archive: string, inner: string) => Promise<void>;
  goBack: (side: Side) => void;
  goForward: (side: Side) => void;
  /** Aktuellen Ordner neu einlesen, Cursor/Auswahl bewahren (Watcher).
   *  Mit `reveal: true` (nach eigenen Dateioperationen) werden neu
   *  hinzugekommene Einträge stattdessen markiert und der Cursor auf den
   *  ersten davon gesetzt (scrollt nur, falls dieser nicht schon sichtbar
   *  ist — siehe `FileTable`s `scrollToIndex(…, {align:"auto"})`). */
  refresh: (side: Side, opts?: { reveal?: boolean }) => Promise<void>;
  /** Git-Status des aktiven Tabs anfordern (bzw. leeren, wenn deaktiviert).
   *  Läuft asynchron im Backend; Ergebnis kommt über applyGitStatus zurück. */
  loadGit: (side: Side) => void;
  /** Git-Status beider Fenster neu laden (nach Settings-Umschaltung). */
  reloadGit: () => void;
  /** Per Event geliefertes Git-Ergebnis auf alle passenden Tabs anwenden
   *  (beide Fenster, alle Tabs mit gleichem Pfad – keine Race Conditions). */
  applyGitStatus: (path: string, git: GitStatus) => void;
  moveCursor: (side: Side, delta: number) => void;
  setCursor: (side: Side, index: number) => void;
  jumpCursor: (side: Side, to: "top" | "bottom") => void;

  // Markierung
  toggleMark: (side: Side, index: number, advance?: boolean) => void;
  shiftMove: (side: Side, delta: number) => void;
  shiftTo: (side: Side, index: number) => void;
  invertSelection: (side: Side) => void;
  selectPattern: (side: Side, pattern: string, add: boolean) => void;
  setSort: (side: Side, key: SortKey) => void;
  /** Schnellfilter des aktiven Tabs setzen (Filtern beim Tippen). */
  setFilter: (side: Side, filter: string) => void;

  // nach Settings-Änderung (z. B. Systemdateien ausblenden)
  rebuildAll: () => void;

  /** Gespeicherte Sitzung (Tabs beider Fenster) wiederherstellen. */
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
    git: null,
    history: path ? [path] : [],
    historyIndex: path ? 0 : -1,
  };
}

function clamp(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, n));
}

/** Sortiert echte Einträge in-place; Ordner zuerst je nach Einstellung. */
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

/** Rohdaten → angezeigte Zeilen (Filter „Systemdateien"/Schnellfilter, Sortierung, ".."). */
function rebuildEntries(
  raw: DirEntry[],
  path: string,
  sort: TabState["sort"],
  inArchive: boolean,
  filter = "",
): Row[] {
  const hideSystem = useSettings.getState().hideSystemFiles;
  let list = raw;
  if (hideSystem) list = list.filter((e) => !e.name.startsWith("."));
  if (filter) {
    const f = filter.toLowerCase();
    list = list.filter((e) => e.name.toLowerCase().includes(f));
  }

  const rows: Row[] = list.map((e) => ({ ...e }));
  sortEntries(rows, sort.key, sort.asc);
  // Im Archiv immer eine "..“-Zeile (auch auf Wurzelebene → verlässt das Archiv).
  if (inArchive || !isRoot(path)) {
    rows.unshift({
      name: "..",
      is_dir: true,
      is_symlink: false,
      size: 0,
      modified: 0,
      mode: null,
      parent: true,
    });
  }
  return rows;
}

/** Ersetzt den aktiven Tab einer Seite über eine Transformationsfunktion. */
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

/** Selektor-Helfer: aktiver Tab einer Seite. */
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
    set({
      active: side,
      [side]: { ...s, activeTab: clamp(index, 0, s.tabs.length - 1) },
    } as Pick<PanesStore, "active" | Side>);
  },

  // ---------- Navigation ----------

  loadDir: async (side, path, record = true) => {
    const res = await listDir(path);
    set(
      withActiveTab(get(), side, (t) => {
        let history = t.history;
        let historyIndex = t.historyIndex;
        // Neuen Ordner in den Verlauf legen (Vorwärts-Zweig verwerfen);
        // gleichen Pfad (z. B. Refresh) nicht doppeln.
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
          git: null,
          history,
          historyIndex,
        };
      }),
    );
    if (record) pushRecent(res.path);
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
        // Verschlüsseltes Archiv: Passwort erfragen und erneut laden.
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
        git: null,
      })),
    );
  },

  refresh: async (side, opts) => {
    const panel = get()[side].tabs[get()[side].activeTab];
    if (panel.archive) return; // Archive werden nicht extern überwacht
    const res = await listDir(panel.path);
    set(
      withActiveTab(get(), side, (t) => {
        const entries = rebuildEntries(res.entries, res.path, t.sort, false, t.filter);
        const names = new Set(entries.map((e) => e.name));
        // Neu hinzugekommene Einträge ggf. markieren und in den Blick rücken
        // (Ziel eines Kopier-/Verschiebe-/Einfüge-Vorgangs), statt einfach
        // nur Cursor/Auswahl wie beim Watcher-Refresh zu bewahren.
        if (opts?.reveal) {
          const oldNames = new Set(t.raw.map((e) => e.name));
          const added = entries.filter((e) => !e.parent && !oldNames.has(e.name));
          if (added.length > 0) {
            return {
              ...t,
              raw: res.entries,
              entries,
              cursor: entries.indexOf(added[0]),
              selected: new Set(added.map((e) => e.name)),
              shiftBase: null,
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
        };
      }),
    );
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
    // Fire-and-forget: läuft im Backend im Hintergrund, Ergebnis kommt
    // über das Event "git-support-ready" → applyGitStatus.
    requestGitStatus(tab.path);
  },

  reloadGit: () => {
    get().loadGit("left");
    get().loadGit("right");
  },

  applyGitStatus: (path, git) => {
    // Nach dem Deaktivieren eingetroffene Ergebnisse verwerfen.
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

  // ---------- Markierung ----------

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
        const selected = new Set<string>();
        for (const r of t.entries) {
          if (!r.parent && !t.selected.has(r.name)) selected.add(r.name);
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

  setSort: (side, key) =>
    set(
      withActiveTab(get(), side, (t) => {
        const asc = t.sort.key === key ? !t.sort.asc : true;
        const sort = { key, asc };
        return {
          ...t,
          sort,
          entries: rebuildEntries(t.raw, t.path, sort, t.archive !== null, t.filter),
          cursor: 0,
          anchor: 0,
          shiftBase: null,
        };
      }),
    ),

  setFilter: (side, filter) =>
    set(
      withActiveTab(get(), side, (t) => {
        const entries = rebuildEntries(
          t.raw,
          t.path,
          t.sort,
          t.archive !== null,
          filter,
        );
        // Cursor auf den ersten echten Treffer (nach einer evtl. "..“-Zeile).
        const first = entries.findIndex((e) => !e.parent);
        return {
          ...t,
          filter,
          entries,
          cursor: first >= 0 ? first : 0,
          anchor: first >= 0 ? first : 0,
          shiftBase: null,
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
    // Einen geladenen Tab aus einem Ordnerpfad bauen (null bei Fehler).
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
      // Kein Tab wiederherstellbar → Home als Rückfall.
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
 * Bereichsauswahl auf Basis eines Snapshots (shiftBase) + Anker.
 * Erlaubt korrektes Wachsen und Schrumpfen beim Richtungswechsel.
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
