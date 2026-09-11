// ============================================================
// Shared command definition of the application menu (TICKET-040).
//
// One entry per command: stable id, translation key, shortcut,
// execution, availability and check/radio state. The in-window menu
// bar, the Help → keyboard overview and the reduced native macOS menu
// all read from here, so a command exists exactly once and cannot drift
// apart between the places that offer it.
//
// Commands never re-implement a file operation — they delegate to the
// existing runners (`actions.ts`, `fileOps.ts`, `selection.ts`) and to
// the store actions, which is what the context menu, the toolbar and
// the keyboard already do.
//
// Availability is a *function*, not a snapshot: a menu that has been
// open for a while may target a folder that has been left or a remote
// connection that has been dropped in the meantime, so `runCommand`
// re-evaluates the state immediately before executing.
// ============================================================

import type { Lang, TransKey } from "@/i18n/dictionaries";
import type { Shortcut } from "@/lib/shortcut";
import { isMacOS } from "@/lib/platform";
import { schemeOf } from "@/lib/path";
import { writeClipboard } from "@/lib/clipboard";
import { openPath, openTerminal } from "@/ipc/client";
import {
  panelOf,
  usePanes,
  type Side,
  type SortKey,
  type ViewMode,
} from "@/store/panesStore";
import { useClipboard } from "@/store/clipboardStore";
import { useNet } from "@/features/net/netStore";
import { translate, useSettings, type Theme } from "@/store/settingsStore";
import { useHistory } from "@/store/historyStore";
import { useTransfers, type TransferOp } from "@/store/transfersStore";
import {
  supportsSshTerminal,
  useTerminals,
} from "@/features/net/terminalStore";
import { openCompare } from "@/store/compareStore";
import { openExport } from "@/store/exportStore";
import { openFileCompare } from "@/store/fileCompareStore";
import { openProps } from "@/store/propsStore";
import { openRename } from "@/store/renameStore";
import { openSearch } from "@/store/searchStore";
import { openAbout, openSettings, openShortcuts } from "@/store/uiStore";
import { runAction } from "./actions";
import {
  runDelete,
  runOpenWithProgram,
  runQuickLook,
  targetPaths,
} from "./fileOps";
import { openEntry } from "./navigate";
import { invertSelection, promptSelect } from "./selection";
import { joinPath, baseName } from "@/lib/path";

// ---------- Command type ----------

/** Availability and state of a command in the current context. */
export interface CommandState {
  /** Not offered at all here (wrong platform). Hidden ≠ disabled. */
  hidden?: boolean;
  /** Offered but not executable right now. */
  disabled?: boolean;
  /** Why it is not executable — read out next to the label. */
  reason?: TransKey;
  /** Checkbox / radio state. */
  checked?: boolean;
}

export interface Command {
  label: TransKey;
  /** Trailing "…" in the menu: the command opens a dialog. */
  dialog?: boolean;
  shortcut?: Shortcut;
  /** Rendering as a checkbox; radio items are grouped by a radio node. */
  role?: "check";
  /** Value inside a radio group (see `RadioNode`). */
  value?: string;
  run: () => void;
  state?: () => CommandState;
}

export type CommandId = keyof typeof COMMANDS;

// ---------- Context helpers ----------

const activeSide = (): Side => usePanes.getState().active;

const activePanel = () => {
  const s = usePanes.getState();
  return panelOf(s, s.active);
};

/** `scheme://authority` of a remote address, null for local paths. */
function authorityOf(path: string): string | null {
  const m = /^([a-z][a-z0-9+.-]*:\/\/[^/]*)/i.exec(path);
  return m ? m[1].toLowerCase() : null;
}

/** The live connection serving `path`, or null (also null for local paths). */
export function connectionFor(path: string) {
  const auth = authorityOf(path);
  if (!auth) return null;
  return (
    useNet
      .getState()
      .connections.find(
        (c) => c.connected && authorityOf(c.address) === auth,
      ) ?? null
  );
}

/** False only for a remote folder whose connection is gone (TICKET-031). */
function sessionLive(path = activePanel().path): boolean {
  return schemeOf(path) === null || connectionFor(path) !== null;
}

/** Disabled state for everything that touches the current folder. */
function folderReady(): CommandState {
  if (!sessionLive()) return { disabled: true, reason: "menu.why.offline" };
  return {};
}

/** Number of entries a file command would act on (selection or cursor). */
function targetCount(): number {
  const p = activePanel();
  if (p.selected.size > 0) return p.selected.size;
  const cur = p.entries[p.cursor];
  return cur && !cur.parent ? 1 : 0;
}

/** Disabled unless at least one real entry is selected or under the cursor. */
function hasTarget(): CommandState {
  const base = folderReady();
  if (base.disabled) return base;
  if (targetCount() === 0) return { disabled: true, reason: "menu.why.noTarget" };
  return {};
}

/** Cursor entry, or undefined for ".."/an empty list. */
function cursorRow() {
  const p = activePanel();
  const cur = p.entries[p.cursor];
  return cur && !cur.parent ? cur : undefined;
}

/** Disabled unless the cursor sits on a real file. */
function cursorIsFile(): CommandState {
  const base = folderReady();
  if (base.disabled) return base;
  const cur = cursorRow();
  if (!cur || cur.is_dir) return { disabled: true, reason: "menu.why.noFile" };
  return {};
}

/** Disabled inside an archive (read-only, no real folder behind it). */
function notInArchive(extra: () => CommandState = () => ({})): CommandState {
  if (activePanel().archive !== null) {
    return { disabled: true, reason: "menu.why.archive" };
  }
  return extra();
}

/** Disabled for remote folders (the command only works on local paths). */
function localOnly(): CommandState {
  const p = activePanel();
  if (p.archive !== null) return { disabled: true, reason: "menu.why.archive" };
  if (schemeOf(p.path) !== null) {
    return { disabled: true, reason: "menu.why.localOnly" };
  }
  return {};
}

/** F4 honours the editor setting, exactly like the global key handler. */
function runEditLikeF4(): void {
  const trigger = useSettings.getState().editorTrigger;
  runAction(trigger === "f4" ? "edit-with" : "edit", activeSide());
}

function copyToSystemClipboard(values: string[]): void {
  if (values.length) void writeClipboard(values.join("\n"));
}

// ---------- The commands ----------

export const COMMANDS = {
  // ----- Dateien -----
  "file.open": {
    label: "menu.file.open",
    shortcut: { key: "Enter" },
    run: () => {
      const side = activeSide();
      openEntry(side, panelOf(usePanes.getState(), side).cursor);
    },
    state: () => {
      const base = folderReady();
      if (base.disabled) return base;
      // Also valid on the ".." row — that opens the parent folder.
      return activePanel().entries.length > 0
        ? {}
        : { disabled: true, reason: "menu.why.empty" };
    },
  },
  "file.view": {
    label: "menu.file.view",
    dialog: true,
    shortcut: { key: "F3" },
    run: () => runAction("view", activeSide()),
    state: cursorIsFile,
  },
  "file.edit": {
    label: "menu.file.edit",
    shortcut: { key: "F4" },
    run: runEditLikeF4,
    state: () => notInArchive(cursorIsFile),
  },
  "file.programs": {
    label: "menu.file.programs",
    dialog: true,
    run: openSettings,
  },
  "file.copy": {
    label: "menu.file.copy",
    dialog: true,
    shortcut: { key: "F5" },
    run: () => runAction("copy", activeSide()),
    state: hasTarget,
  },
  "file.move": {
    label: "menu.file.move",
    dialog: true,
    shortcut: { key: "F6" },
    run: () => runAction("move", activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "file.rename": {
    label: "menu.file.rename",
    dialog: true,
    shortcut: { key: "F2" },
    run: () => runAction("rename", activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "file.mkdir": {
    label: "menu.file.mkdir",
    dialog: true,
    shortcut: { key: "F7" },
    run: () => runAction("mkdir", activeSide()),
    state: () => notInArchive(),
  },
  "file.delete": {
    label: "menu.file.delete",
    dialog: true,
    shortcut: { key: "F8" },
    run: () => runDelete(activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "file.deletePerm": {
    label: "menu.file.deletePerm",
    dialog: true,
    shortcut: { mods: ["Shift"], key: "F8" },
    run: () => runDelete(activeSide(), { permanent: true }),
    state: () => notInArchive(hasTarget),
  },
  "file.clipCopy": {
    label: "menu.file.clipCopy",
    shortcut: { mods: ["Mod"], key: "C" },
    run: () => runAction("clip-copy", activeSide()),
    state: hasTarget,
  },
  "file.clipCut": {
    label: "menu.file.clipCut",
    shortcut: { mods: ["Mod"], key: "X" },
    run: () => runAction("clip-cut", activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "file.clipPaste": {
    label: "menu.file.clipPaste",
    shortcut: { mods: ["Mod"], key: "V" },
    run: () => runAction("clip-paste", activeSide()),
    state: () =>
      notInArchive(() => {
        const clip = useClipboard.getState();
        if (!clip.mode || clip.names.length === 0) {
          return { disabled: true, reason: "menu.why.clipEmpty" };
        }
        return folderReady();
      }),
  },
  "file.pack": {
    label: "menu.file.pack",
    dialog: true,
    shortcut: { mods: ["Alt"], key: "F5" },
    run: () => runAction("pack", activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "file.extract": {
    label: "menu.file.extract",
    dialog: true,
    run: () => runAction("extract", activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "file.batchRename": {
    label: "menu.file.batchRename",
    dialog: true,
    run: () => openRename(activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "file.compareFiles": {
    label: "menu.file.compareFiles",
    dialog: true,
    run: () => {
      openFileCompare();
    },
    state: () => {
      const s = usePanes.getState();
      const ok = (["left", "right"] as const).every((side) => {
        const p = panelOf(s, side);
        const cur = p.entries[p.cursor];
        return p.archive === null && !!cur && !cur.parent && !cur.is_dir;
      });
      return ok ? {} : { disabled: true, reason: "menu.why.needTwoFiles" };
    },
  },
  "file.props": {
    label: "menu.file.props",
    dialog: true,
    shortcut: { mods: ["Alt"], key: "Enter" },
    run: () => {
      const p = activePanel();
      const cur = cursorRow();
      if (!cur) return;
      openProps(joinPath(p.path, cur.name), p.git?.entries[cur.name]);
    },
    state: () =>
      notInArchive(() => (cursorRow() ? {} : { disabled: true, reason: "menu.why.noTarget" })),
  },
  "app.quit": {
    label: "menu.app.quit",
    // macOS offers Quit in the native app menu instead (⌘Q).
    run: () => runAction("quit", activeSide()),
    state: () => ({ hidden: isMacOS }),
  },

  // ----- Markieren -----
  "mark.selectGroup": {
    label: "menu.mark.selectGroup",
    dialog: true,
    shortcut: { key: "+" },
    run: () => promptSelect(activeSide(), true),
  },
  "mark.unselectGroup": {
    label: "menu.mark.unselectGroup",
    dialog: true,
    shortcut: { key: "-" },
    run: () => promptSelect(activeSide(), false),
  },
  "mark.invert": {
    label: "menu.mark.invert",
    shortcut: { key: "*" },
    run: () => invertSelection(activeSide()),
  },
  "mark.all": {
    label: "menu.mark.all",
    // Uses the existing pattern selection; ".." is excluded there.
    run: () => usePanes.getState().selectPattern(activeSide(), "*", true),
  },
  "mark.none": {
    label: "menu.mark.none",
    run: () => usePanes.getState().selectPattern(activeSide(), "*", false),
    state: () =>
      activePanel().selected.size > 0
        ? {}
        : { disabled: true, reason: "menu.why.noSelection" },
  },
  "mark.copyNames": {
    label: "menu.mark.copyNames",
    run: () => copyToSystemClipboard(targetPaths(activeSide()).map(baseName)),
    state: hasTarget,
  },
  "mark.copyPaths": {
    label: "menu.mark.copyPaths",
    run: () => copyToSystemClipboard(targetPaths(activeSide())),
    state: () => notInArchive(hasTarget),
  },
  "mark.export": {
    label: "menu.mark.export",
    dialog: true,
    run: () => openExport(activeSide()),
  },

  // ----- Befehle -----
  "cmd.search": {
    label: "menu.cmd.search",
    dialog: true,
    shortcut: { mods: ["Alt"], key: "F7" },
    run: () => openSearch(activeSide()),
    state: () => notInArchive(),
  },
  "cmd.compareDirs": {
    label: "menu.cmd.compareDirs",
    dialog: true,
    run: () => openCompare(),
  },
  "nav.back": {
    label: "menu.nav.back",
    shortcut: { mods: ["Alt"], key: "Left" },
    run: () => usePanes.getState().goBack(activeSide()),
    state: () =>
      activePanel().historyIndex > 0
        ? {}
        : { disabled: true, reason: "menu.why.noHistory" },
  },
  "nav.forward": {
    label: "menu.nav.forward",
    shortcut: { mods: ["Alt"], key: "Right" },
    run: () => usePanes.getState().goForward(activeSide()),
    state: () => {
      const p = activePanel();
      return p.historyIndex < p.history.length - 1
        ? {}
        : { disabled: true, reason: "menu.why.noHistory" };
    },
  },
  "nav.up": {
    label: "menu.nav.up",
    shortcut: { key: "Backspace" },
    run: () => {
      const side = activeSide();
      if (panelOf(usePanes.getState(), side).entries[0]?.parent) {
        openEntry(side, 0);
      }
    },
    state: () =>
      activePanel().entries[0]?.parent
        ? {}
        : { disabled: true, reason: "menu.why.atRoot" },
  },
  "recent.clear": {
    label: "tb.recentClear",
    run: () => useHistory.getState().clear(),
    state: () =>
      useHistory.getState().recent.length > 0
        ? {}
        : { disabled: true, reason: "menu.why.empty" },
  },
  "cmd.terminal": {
    label: "menu.cmd.terminal",
    run: () => {
      const dir = activePanel().path;
      if (supportsSshTerminal(dir)) useTerminals.getState().openFor(dir);
      else openTerminal(dir, useSettings.getState().terminalProgram);
    },
    state: () =>
      notInArchive(() => {
        const p = activePanel();
        const scheme = schemeOf(p.path);
        // Only local folders and SSH-backed remotes have a shell.
        if (scheme !== null && scheme !== "sftp") {
          return { disabled: true, reason: "menu.why.noShell" };
        }
        return folderReady();
      }),
  },
  "cmd.openFolder": {
    label: "menu.cmd.openFolder",
    run: () => void openPath(activePanel().path),
    state: localOnly,
  },

  // ----- Netz -----
  "net.manage": {
    label: "menu.net.manage",
    dialog: true,
    // Always reachable — also without profiles or a live connection.
    run: () => useNet.getState().openManager(),
  },
  "net.disconnect": {
    label: "menu.net.disconnect",
    dialog: true,
    run: () => {
      const conn = connectionFor(activePanel().path);
      if (conn) void useNet.getState().disconnect(conn.id);
    },
    state: () =>
      connectionFor(activePanel().path)
        ? {}
        : { disabled: true, reason: "menu.why.noConnection" },
  },
  "net.terminal": {
    label: "menu.net.terminal",
    run: () => {
      const dir = activePanel().path;
      if (supportsSshTerminal(dir)) useTerminals.getState().openFor(dir);
    },
    state: () => {
      const path = activePanel().path;
      if (!supportsSshTerminal(path)) {
        return { disabled: true, reason: "menu.why.noSftp" };
      }
      return connectionFor(path)
        ? {}
        : { disabled: true, reason: "menu.why.offline" };
    },
  },

  // ----- Ansicht -----
  "view.details": {
    label: "view.details",
    value: "details",
    run: () => usePanes.getState().setViewMode(activeSide(), "details"),
  },
  "view.list": {
    label: "view.list",
    value: "list",
    run: () => usePanes.getState().setViewMode(activeSide(), "list"),
  },
  "view.thumbnails": {
    label: "view.thumbnails",
    value: "thumbnails",
    run: () => usePanes.getState().setViewMode(activeSide(), "thumbnails"),
  },
  "view.tree": {
    label: "view.tree",
    value: "tree",
    run: () => usePanes.getState().setViewMode(activeSide(), "tree"),
  },
  "sort.name": {
    label: "col.name",
    value: "name",
    run: () => usePanes.getState().setSortKey(activeSide(), "name"),
  },
  "sort.ext": {
    label: "col.ext",
    value: "ext",
    run: () => usePanes.getState().setSortKey(activeSide(), "ext"),
  },
  "sort.size": {
    label: "col.size",
    value: "size",
    run: () => usePanes.getState().setSortKey(activeSide(), "size"),
  },
  "sort.date": {
    label: "col.date",
    value: "date",
    run: () => usePanes.getState().setSortKey(activeSide(), "date"),
  },
  "sort.asc": {
    label: "menu.sort.asc",
    value: "asc",
    run: () => usePanes.getState().setSortAsc(activeSide(), true),
  },
  "sort.desc": {
    label: "menu.sort.desc",
    value: "desc",
    run: () => usePanes.getState().setSortAsc(activeSide(), false),
  },
  "view.hidden": {
    label: "menu.view.hidden",
    role: "check",
    run: () => {
      const s = useSettings.getState();
      s.setHideSystemFiles(!s.hideSystemFiles);
    },
    state: () => ({ checked: !useSettings.getState().hideSystemFiles }),
  },
  "view.extCol": {
    label: "menu.view.extCol",
    role: "check",
    run: () => {
      const s = useSettings.getState();
      s.setShowExtColumn(!s.showExtColumn);
    },
    state: () => ({ checked: useSettings.getState().showExtColumn }),
  },
  "view.permCol": {
    label: "menu.view.permCol",
    role: "check",
    run: () => {
      const s = useSettings.getState();
      s.setShowPermissions(!s.showPermissions);
    },
    state: () => ({ checked: useSettings.getState().showPermissions }),
  },
  "view.git": {
    label: "menu.view.git",
    role: "check",
    run: () => {
      const s = useSettings.getState();
      s.setGitEnabled(!s.gitEnabled);
    },
    state: () => ({ checked: useSettings.getState().gitEnabled }),
  },
  "view.clearFilter": {
    label: "menu.view.clearFilter",
    run: () => usePanes.getState().setFilter(activeSide(), ""),
    state: () =>
      activePanel().filter.length > 0
        ? {}
        : { disabled: true, reason: "menu.why.noFilter" },
  },
  "view.newTab": {
    label: "menu.view.newTab",
    shortcut: { mods: ["Mod"], key: "T" },
    run: () => void usePanes.getState().addTab(activeSide()),
  },
  "view.closeTab": {
    label: "menu.view.closeTab",
    shortcut: { mods: ["Mod"], key: "W" },
    run: () => {
      const s = usePanes.getState();
      s.closeTab(s.active, s[s.active].activeTab);
    },
    state: () => {
      const s = usePanes.getState();
      return s[s.active].tabs.length > 1
        ? {}
        : { disabled: true, reason: "menu.why.lastTab" };
    },
  },
  "view.openTab": {
    label: "menu.view.openTab",
    run: () => runAction("open-tab", activeSide()),
    state: () =>
      notInArchive(() => {
        const cur = cursorRow();
        return cur?.is_dir
          ? {}
          : { disabled: true, reason: "menu.why.noFolder" };
      }),
  },
  "view.refresh": {
    label: "menu.view.refresh",
    run: () => void usePanes.getState().refresh(activeSide()),
    state: folderReady,
  },
  "view.quickLook": {
    label: "ctx.quicklook",
    shortcut: { key: "Space" },
    run: () => {
      runQuickLook(activeSide());
    },
    state: () => {
      if (!isMacOS) return { hidden: true };
      return notInArchive(() =>
        cursorRow()
          ? localOnly()
          : { disabled: true, reason: "menu.why.noTarget" },
      );
    },
  },

  // ----- Konfiguration -----
  "cfg.settings": {
    label: "menu.cfg.settings",
    dialog: true,
    shortcut: { mods: ["Mod"], key: "," },
    run: openSettings,
  },
  "theme.system": {
    label: "settings.theme.system",
    value: "system",
    run: () => useSettings.getState().setTheme("system"),
  },
  "theme.light": {
    label: "settings.theme.light",
    value: "light",
    run: () => useSettings.getState().setTheme("light"),
  },
  "theme.dark": {
    label: "settings.theme.dark",
    value: "dark",
    run: () => useSettings.getState().setTheme("dark"),
  },
  "lang.de": {
    label: "menu.lang.de",
    value: "de",
    run: () => useSettings.getState().setLanguage("de"),
  },
  "lang.en": {
    label: "menu.lang.en",
    value: "en",
    run: () => useSettings.getState().setLanguage("en"),
  },

  // ----- Hilfe -----
  "help.keys": {
    label: "menu.help.keys",
    dialog: true,
    run: openShortcuts,
  },
  "help.about": {
    label: "menu.help.about",
    dialog: true,
    run: openAbout,
  },
} satisfies Record<string, Command>;

// ---------- Execution ----------

/** Current state of a command (never undefined). */
export function commandState(id: CommandId): CommandState {
  return (COMMANDS[id] as Command).state?.() ?? {};
}

/**
 * Runs a command after re-checking its availability. The menu may have
 * been open while the folder changed or a connection dropped, so the
 * state captured at render time is not trusted here.
 */
export function runCommand(id: CommandId): void {
  const cmd = COMMANDS[id] as Command;
  const st = cmd.state?.() ?? {};
  if (st.hidden || st.disabled) return;
  cmd.run();
}

// ---------- Menu structure ----------

/** A dynamically built entry (favorites, connections, transfers …). */
export interface DynamicItem {
  id: string;
  label: string;
  /** Second, dimmed line (path, progress …). */
  detail?: string;
  disabled?: boolean;
  run?: () => void;
}

export type MenuNode =
  /** A command from `COMMANDS`. */
  | { kind: "item"; id: CommandId }
  | { kind: "sep" }
  | { kind: "sub"; label: TransKey; nodes: MenuNode[] }
  /** Inline list of dynamically built entries; `empty` labels the
   *  non-executable placeholder shown when the list is empty. */
  | { kind: "list"; build: () => DynamicItem[]; empty: TransKey }
  /** Mutually exclusive group; each command carries its `value`. */
  | { kind: "radio"; current: () => string; ids: CommandId[] };

export interface MenuCategory {
  id: string;
  label: TransKey;
  nodes: MenuNode[];
}

const item = (id: CommandId): MenuNode => ({ kind: "item", id });
const sep = (): MenuNode => ({ kind: "sep" });

/** "Öffnen mit ▸": the configured programs plus the way to manage them. */
function openWithItems(): DynamicItem[] {
  const s = useSettings.getState();
  const state = COMMANDS["file.edit"].state?.() ?? {};
  const items: DynamicItem[] = s.programs.map((prog) => ({
    id: `prog:${prog.id}`,
    label: prog.name,
    disabled: !!state.disabled,
    run: () => {
      // Re-check: the cursor may have moved onto a folder meanwhile.
      const cur = cursorRow();
      const p = activePanel();
      if (!cur || cur.is_dir || p.archive !== null) return;
      runOpenWithProgram(activeSide(), prog.path);
    },
  }));
  if (s.defaultEditor) {
    items.push({
      id: "prog:default",
      label: s.defaultEditor,
      disabled: !!state.disabled,
      run: () => runOpenWithProgram(activeSide(), s.defaultEditor),
    });
  }
  return items;
}

function favoriteItems(): DynamicItem[] {
  return useSettings.getState().favorites.map((fav) => ({
    id: `fav:${fav.path}`,
    label: fav.name,
    detail: fav.path,
    run: () => {
      const s = usePanes.getState();
      void s.loadDir(s.active, fav.path);
    },
  }));
}

function recentItems(): DynamicItem[] {
  return useHistory.getState().recent.map((path) => ({
    id: `recent:${path}`,
    label: baseName(path),
    detail: path,
    run: () => {
      const s = usePanes.getState();
      void s.loadDir(s.active, path);
    },
  }));
}

/** Labels of the transfer kinds — same wording as the transfer window. */
const TRANSFER_OP: Record<TransferOp, TransKey> = {
  copy: "fn.copy",
  move: "fn.move",
  extract: "op.extract",
  pack: "op.pack",
};

/** Running and queued transfers — each opens its existing transfer dialog. */
function transferItems(): DynamicItem[] {
  return useTransfers
    .getState()
    .transfers.filter((tr) => !tr.done)
    .map((tr) => ({
      id: `transfer:${tr.id}`,
      label: tr.fileName || translate(TRANSFER_OP[tr.op]),
      detail: tr.queued
        ? undefined
        : tr.bytesTotal > 0
          ? `${Math.round((tr.bytesDone / tr.bytesTotal) * 100)} %`
          : undefined,
      // Pause/resume/cancel stay in the transfer dialog itself.
      run: () => useTransfers.getState().minimize(tr.id, false),
    }));
}

/** Connections that are live right now — open them in the active pane. */
function connectionItems(): DynamicItem[] {
  return useNet
    .getState()
    .connections.filter((c) => c.connected)
    .map((c) => ({
      id: `conn:${c.id}`,
      label: c.name,
      detail: c.address,
      run: () => {
        const s = usePanes.getState();
        void s.loadDir(s.active, c.address);
      },
    }));
}

export const MENUS: MenuCategory[] = [
  {
    id: "files",
    label: "menu.files",
    nodes: [
      item("file.open"),
      item("file.view"),
      item("file.edit"),
      {
        kind: "sub",
        label: "menu.file.openWith",
        nodes: [
          { kind: "list", build: openWithItems, empty: "menu.empty.programs" },
          sep(),
          item("file.programs"),
        ],
      },
      sep(),
      item("file.copy"),
      item("file.move"),
      item("file.rename"),
      item("file.mkdir"),
      item("file.delete"),
      item("file.deletePerm"),
      sep(),
      item("file.clipCopy"),
      item("file.clipCut"),
      item("file.clipPaste"),
      sep(),
      item("file.pack"),
      item("file.extract"),
      item("file.batchRename"),
      item("file.compareFiles"),
      sep(),
      item("file.props"),
      sep(),
      item("app.quit"),
    ],
  },
  {
    id: "mark",
    label: "menu.mark",
    nodes: [
      item("mark.selectGroup"),
      item("mark.unselectGroup"),
      item("mark.invert"),
      sep(),
      item("mark.all"),
      item("mark.none"),
      sep(),
      item("mark.copyNames"),
      item("mark.copyPaths"),
      item("mark.export"),
    ],
  },
  {
    id: "commands",
    label: "menu.commands",
    nodes: [
      item("cmd.search"),
      item("cmd.compareDirs"),
      sep(),
      {
        kind: "sub",
        label: "menu.cmd.navigation",
        nodes: [item("nav.back"), item("nav.forward"), item("nav.up")],
      },
      {
        kind: "sub",
        label: "menu.cmd.favorites",
        nodes: [
          {
            kind: "list",
            build: favoriteItems,
            empty: "settings.favorites.empty",
          },
        ],
      },
      {
        kind: "sub",
        label: "menu.cmd.recent",
        nodes: [
          { kind: "list", build: recentItems, empty: "tb.recentEmpty" },
          sep(),
          item("recent.clear"),
        ],
      },
      sep(),
      item("cmd.terminal"),
      item("cmd.openFolder"),
      sep(),
      {
        kind: "sub",
        label: "menu.cmd.transfers",
        nodes: [
          {
            kind: "list",
            build: transferItems,
            empty: "menu.empty.transfers",
          },
        ],
      },
    ],
  },
  {
    id: "net",
    label: "menu.net",
    nodes: [
      item("net.manage"),
      {
        kind: "sub",
        label: "menu.net.active",
        nodes: [
          {
            kind: "list",
            build: connectionItems,
            empty: "menu.empty.connections",
          },
        ],
      },
      sep(),
      item("net.disconnect"),
      item("net.terminal"),
    ],
  },
  {
    id: "view",
    label: "menu.view",
    nodes: [
      {
        kind: "sub",
        label: "menu.view.mode",
        nodes: [
          {
            kind: "radio",
            current: () => {
              const s = usePanes.getState();
              return s[s.active].viewMode satisfies ViewMode;
            },
            ids: ["view.details", "view.list", "view.thumbnails", "view.tree"],
          },
        ],
      },
      {
        kind: "sub",
        label: "menu.view.sort",
        nodes: [
          {
            kind: "radio",
            current: () => activePanel().sort.key satisfies SortKey,
            ids: ["sort.name", "sort.ext", "sort.size", "sort.date"],
          },
          sep(),
          {
            kind: "radio",
            current: () => (activePanel().sort.asc ? "asc" : "desc"),
            ids: ["sort.asc", "sort.desc"],
          },
        ],
      },
      sep(),
      item("view.hidden"),
      item("view.extCol"),
      item("view.permCol"),
      item("view.git"),
      sep(),
      item("view.clearFilter"),
      sep(),
      item("view.newTab"),
      item("view.closeTab"),
      item("view.openTab"),
      sep(),
      item("view.refresh"),
      item("view.quickLook"),
    ],
  },
  {
    id: "config",
    label: "menu.config",
    nodes: [
      item("cfg.settings"),
      sep(),
      {
        kind: "sub",
        label: "menu.cfg.theme",
        nodes: [
          {
            kind: "radio",
            current: () => useSettings.getState().theme satisfies Theme,
            ids: ["theme.system", "theme.light", "theme.dark"],
          },
        ],
      },
      {
        kind: "sub",
        label: "menu.cfg.language",
        nodes: [
          {
            kind: "radio",
            current: () => useSettings.getState().language satisfies Lang,
            ids: ["lang.de", "lang.en"],
          },
        ],
      },
    ],
  },
  {
    id: "help",
    label: "menu.help",
    nodes: [item("help.keys"), item("help.about")],
  },
];

/**
 * All commands of a category that carry a shortcut, in menu order —
 * the source of the Help → keyboard overview.
 */
export function shortcutsOf(category: MenuCategory): CommandId[] {
  const out: CommandId[] = [];
  const walk = (nodes: MenuNode[]) => {
    for (const node of nodes) {
      if (node.kind === "item") {
        const cmd = COMMANDS[node.id] as Command;
        const st = cmd.state?.() ?? {};
        if (cmd.shortcut && !st.hidden) out.push(node.id);
      } else if (node.kind === "sub") {
        walk(node.nodes);
      } else if (node.kind === "radio") {
        for (const id of node.ids) {
          const cmd = COMMANDS[id] as Command;
          if (cmd.shortcut) out.push(id);
        }
      }
    }
  };
  walk(category.nodes);
  return out;
}
