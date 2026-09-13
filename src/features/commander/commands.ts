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
import type { Capability } from "@/ipc/bindings";
import { capState, guard, locationLive, pairState } from "./guard";
import { hasTauri, openPath, openTerminal } from "@/ipc/client";
import { changePageZoom } from "./useNativeZoom";
import {
  panelOf,
  usePanes,
  type Side,
  type SortKey,
  type ViewMode,
} from "@/store/panesStore";
import { useClipboard } from "@/store/clipboardStore";
import { connectionFor, useNet } from "@/features/net/netStore";
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
  runOpenWithBrowse,
  runOpenWithProgram,
  runQuickLook,
} from "./fileOps";
import { openEntry } from "./navigate";
import {
  hasSnapshot,
  invertSelection,
  promptSelect,
  rememberSelection,
  restoreSelection,
  selectAllFiles,
  selectNoFiles,
  selectSameExt,
} from "./selection";
import {
  copyNameDetails,
  copyNames,
  copyPathDetails,
  copyPaths,
} from "./markClipboard";
import {
  clearCompareFilter,
  compareBusy,
  compareFilterActive,
  compareReady,
  runCompare,
} from "./compareLevel";
import { openSelectionFile } from "@/store/selectionFileStore";
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
  /** Replacements for "{name}" placeholders in `reason` (TICKET-031). */
  reasonVars?: Record<string, string>;
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

/** False only for a remote folder whose connection is gone (TICKET-031).
 *  Reads the capability contract, like every other availability question
 *  here — a location that supports nothing at all is one whose session
 *  is gone, and there is no second opinion about it anywhere in the UI. */
function sessionLive(path = activePanel().path): boolean {
  return locationLive(path);
}

/** The folder of the opposite pane — destination of every two-pane
 *  operation, and null while that pane is inside an archive. */
function otherPanel() {
  const s = usePanes.getState();
  return panelOf(s, s.active === "left" ? "right" : "left");
}

/** Disabled unless the active folder offers `cap` (TICKET-031). */
function needs(cap: Capability, path = activePanel().path): CommandState {
  return capState(path, cap);
}

/** Disabled unless *both* ends of a two-location operation offer theirs.
 *  A destination inside an archive is refused before the question is
 *  even asked — an archive is not a writable location. */
function needsPair(
  fromCap: Capability,
  toCap: Capability,
  from = activePanel().path,
): CommandState {
  const dst = otherPanel();
  if (dst.archive !== null) {
    return { disabled: true, reason: "menu.why.archive" };
  }
  return pairState(from, fromCap, dst.path, toCap);
}

/** Combines an existing state check with a capability requirement. */
function withCap(cap: Capability, base: () => CommandState): CommandState {
  const st = base();
  return st.disabled || st.hidden ? st : needs(cap);
}

/** Same, for a requirement that spans both panes. */
function withPair(
  base: () => CommandState,
  caps: () => CommandState,
): CommandState {
  const st = base();
  return st.disabled || st.hidden ? st : caps();
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

/** At least one real entry is visible in the active tab. */
function hasVisibleEntry(): boolean {
  return activePanel().entries.some((r) => !r.parent);
}

/** Shared availability of the two pane comparisons (TICKET-059). */
function compareState(): CommandState {
  if (compareBusy()) return { disabled: true, reason: "menu.why.compareBusy" };
  if (!compareReady()) {
    return { disabled: true, reason: "menu.why.compareOffline" };
  }
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

/** Disabled inside an archive and wherever the location lacks `cap`.
 *  Replaces the former `scheme === …` test: which locations can hand a
 *  path to the OS is the backend's matrix to state, not the UI's to
 *  guess (TICKET-031). */
function osAction(cap: Capability): CommandState {
  return notInArchive(() => needs(cap));
}

/** F4 honours the editor setting, exactly like the global key handler. */
function runEditLikeF4(): void {
  const trigger = useSettings.getState().editorTrigger;
  runAction(trigger === "f4" ? "edit-with" : "edit", activeSide());
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
    state: () => withCap("preview", cursorIsFile),
  },
  "file.edit": {
    label: "menu.file.edit",
    shortcut: { key: "F4" },
    run: runEditLikeF4,
    // F4 hands the file either to the system default (`systemOpen`) or
    // to a configured program (`openWith`) — the setting decides which
    // capability is actually needed.
    state: () =>
      withCap(
        useSettings.getState().editorTrigger === "f4"
          ? "openWith"
          : "systemOpen",
        () => notInArchive(cursorIsFile),
      ),
  },
  "file.openWithOther": {
    label: "menu.file.openWithOther",
    dialog: true,
    // Same picker the context menu uses — pick a binary, open the file with it.
    run: () => runOpenWithBrowse(activeSide()),
    state: () => withCap("openWith", () => notInArchive(cursorIsFile)),
  },
  "file.programs": {
    label: "menu.file.programs",
    dialog: true,
    run: () => openSettings("programs"),
  },
  "file.copy": {
    label: "menu.file.copy",
    dialog: true,
    shortcut: { key: "F5" },
    run: () => runAction("copy", activeSide()),
    // Copying out of an archive is an extraction; otherwise both ends
    // are asked for their own half of the transfer.
    state: () =>
      withPair(hasTarget, () =>
        activePanel().archive !== null
          ? needsPair("archiveExtractFrom", "archiveExtractTo")
          : needsPair("copyFrom", "copyTo"),
      ),
  },
  "file.move": {
    label: "menu.file.move",
    dialog: true,
    shortcut: { key: "F6" },
    run: () => runAction("move", activeSide()),
    state: () =>
      withPair(
        () => notInArchive(hasTarget),
        () => needsPair("move", "copyTo"),
      ),
  },
  "file.rename": {
    label: "menu.file.rename",
    dialog: true,
    shortcut: { key: "F2" },
    run: () => runAction("rename", activeSide()),
    state: () => withCap("move", () => notInArchive(hasTarget)),
  },
  "file.mkdir": {
    label: "menu.file.mkdir",
    dialog: true,
    shortcut: { key: "F7" },
    run: () => runAction("mkdir", activeSide()),
    state: () => withCap("create", () => notInArchive()),
  },
  "file.delete": {
    label: "menu.file.delete",
    dialog: true,
    shortcut: { key: "F8" },
    run: () => runDelete(activeSide()),
    // Deleting falls back to a permanent delete where the location has
    // no trash, so that is the capability the command really needs.
    state: () => withCap("deletePermanent", () => notInArchive(hasTarget)),
  },
  "file.deletePerm": {
    label: "menu.file.deletePerm",
    dialog: true,
    shortcut: { mods: ["Shift"], key: "F8" },
    run: () => runDelete(activeSide(), { permanent: true }),
    state: () => withCap("deletePermanent", () => notInArchive(hasTarget)),
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
        const base = folderReady();
        if (base.disabled) return base;
        // The clipboard names a source folder of its own — it may have
        // gone offline since the copy (TICKET-031).
        return pairState(
          clip.sourcePath,
          clip.mode === "cut" ? "move" : "copyFrom",
          activePanel().path,
          "copyTo",
        );
      }),
  },
  "file.pack": {
    label: "menu.file.pack",
    dialog: true,
    shortcut: { mods: ["Alt"], key: "F5" },
    run: () => runAction("pack", activeSide()),
    state: () =>
      withPair(
        () => notInArchive(hasTarget),
        () => needsPair("archivePackFrom", "archivePackTo"),
      ),
  },
  "file.extract": {
    label: "menu.file.extract",
    dialog: true,
    run: () => runAction("extract", activeSide()),
    state: () =>
      withPair(
        () => notInArchive(hasTarget),
        () => needsPair("archiveExtractFrom", "archiveExtractTo"),
      ),
  },
  "file.batchRename": {
    label: "menu.file.batchRename",
    dialog: true,
    run: () => openRename(activeSide()),
    state: () => withCap("move", () => notInArchive(hasTarget)),
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
      if (!ok) return { disabled: true, reason: "menu.why.needTwoFiles" };
      // Both files are read by the same local comparison; each side is
      // asked for its own capability (TICKET-031).
      return pairState(
        panelOf(s, "left").path,
        "compareFiles",
        panelOf(s, "right").path,
        "compareFiles",
      );
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
      withCap("stat", () =>
        notInArchive(() =>
          cursorRow() ? {} : { disabled: true, reason: "menu.why.noTarget" },
        ),
      ),
  },
  "app.quit": {
    label: "menu.app.quit",
    // macOS offers Quit in the native app menu instead (⌘Q).
    run: () => runAction("quit", activeSide()),
    state: () => ({ hidden: isMacOS }),
  },

  // ----- Markieren (TICKET-059) -----
  // Group 1 – what is marked.
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
  "mark.allFiles": {
    label: "menu.mark.allFiles",
    shortcut: { mods: ["Mod"], key: "Num+" },
    run: () => selectAllFiles(activeSide()),
    state: () => (hasVisibleEntry() ? {} : { disabled: true, reason: "menu.why.empty" }),
  },
  "mark.noFiles": {
    label: "menu.mark.noFiles",
    shortcut: { mods: ["Mod"], key: "Num-" },
    // Clears the whole selection of the tab, folders included.
    run: () => selectNoFiles(activeSide()),
    state: () =>
      activePanel().selected.size > 0
        ? {}
        : { disabled: true, reason: "menu.why.noSelection" },
  },
  "mark.invert": {
    label: "menu.mark.invert",
    shortcut: { key: "*" },
    run: () => invertSelection(activeSide()),
    state: () => (hasVisibleEntry() ? {} : { disabled: true, reason: "menu.why.empty" }),
  },
  "mark.sameExt": {
    label: "menu.mark.sameExt",
    shortcut: { mods: ["Alt"], key: "Num+" },
    run: () => selectSameExt(activeSide()),
    state: cursorIsFile,
  },

  // Group 2 – remembering a selection.
  "mark.remember": {
    label: "menu.mark.remember",
    run: () => rememberSelection(activeSide()),
  },
  "mark.restore": {
    label: "menu.mark.restore",
    shortcut: { key: "Num/" },
    run: () => restoreSelection(activeSide()),
    state: () =>
      hasSnapshot(activeSide())
        ? {}
        : { disabled: true, reason: "menu.why.noSnapshot" },
  },
  "mark.saveFile": {
    label: "menu.mark.saveFile",
    dialog: true,
    run: () => openSelectionFile(activeSide(), "save"),
  },
  "mark.loadFile": {
    label: "menu.mark.loadFile",
    dialog: true,
    run: () => openSelectionFile(activeSide(), "load"),
  },

  // Group 3 – clipboard.
  "mark.copyNames": {
    label: "menu.mark.copyNames",
    run: () => copyNames(activeSide()),
    state: hasTarget,
  },
  "mark.copyPaths": {
    label: "menu.mark.copyPaths",
    run: () => copyPaths(activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "mark.copyDetails": {
    label: "menu.mark.copyDetails",
    run: () => copyNameDetails(activeSide()),
    state: hasTarget,
  },
  "mark.copyPathDetails": {
    label: "menu.mark.copyPathDetails",
    run: () => copyPathDetails(activeSide()),
    state: () => notInArchive(hasTarget),
  },
  "mark.export": {
    label: "menu.mark.export",
    dialog: true,
    run: () => openExport(activeSide()),
  },

  // Group 4 – comparing the two panes.
  "mark.compare": {
    label: "menu.mark.compare",
    shortcut: { mods: ["Shift"], key: "F2" },
    run: () => void runCompare("differences"),
    state: compareState,
  },
  "mark.compareHideSame": {
    label: "menu.mark.compareHideSame",
    run: () => void runCompare("newerHideSame"),
    state: compareState,
  },
  "mark.clearCompare": {
    label: "menu.mark.clearCompare",
    run: () => clearCompareFilter(),
    state: () =>
      compareFilterActive() ? {} : { disabled: true, reason: "menu.why.noCompare" },
  },

  // ----- Befehle -----
  "cmd.search": {
    label: "menu.cmd.search",
    dialog: true,
    shortcut: { mods: ["Alt"], key: "F7" },
    run: () => openSearch(activeSide()),
    state: () => withCap("search", () => notInArchive()),
  },
  "cmd.searchDuplicates": {
    label: "search.mode.duplicates",
    dialog: true,
    run: () => openSearch(activeSide(), "duplicates"),
    state: () => withCap("search", () => notInArchive()),
  },
  "cmd.searchEmptyDirs": {
    label: "search.mode.emptyDirs",
    dialog: true,
    run: () => openSearch(activeSide(), "empty_dirs"),
    state: () => withCap("search", () => notInArchive()),
  },
  "cmd.searchLargeFiles": {
    label: "search.mode.largeFiles",
    dialog: true,
    run: () => openSearch(activeSide(), "large_files"),
    state: () => withCap("search", () => notInArchive()),
  },
  "cmd.compareDirs": {
    label: "menu.cmd.compareDirs",
    dialog: true,
    run: () => openCompare(),
    // The dialog lets both folders be edited, so it only needs *a*
    // supported starting point; it validates each side itself.
    state: () =>
      withPair(
        () => notInArchive(),
        () => needsPair("compareDirs", "compareDirs"),
      ),
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
      if (supportsSshTerminal(dir)) {
        useTerminals.getState().openFor(dir);
      } else if (guard(activeSide(), dir, "localTerminal")) {
        openTerminal(dir, useSettings.getState().terminalProgram);
      }
    },
    state: () =>
      notInArchive(() => {
        // A local folder starts the configured terminal program; a
        // connection that carries a shell opens one in place. Which is
        // which comes from the capability matrix, not from a scheme name.
        const p = activePanel();
        if (supportsSshTerminal(p.path)) return needs("sshTerminal");
        const st = needs("localTerminal");
        return st.disabled && schemeOf(p.path) !== null && sessionLive()
          ? { disabled: true, reason: "menu.why.noShell" }
          : st;
      }),
  },
  "cmd.openFolder": {
    label: "menu.cmd.openFolder",
    run: () => void openPath(activePanel().path),
    state: () => osAction("systemOpen"),
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
      // The connection itself decides — the backend only advertises
      // `sshTerminal` for a provider that really carries an SSH session.
      return needs("sshTerminal", path);
    },
  },

  // ----- Ansicht -----
  "view.details": {
    label: "view.details",
    value: "details",
    run: () => usePanes.getState().setViewMode(activeSide(), "details"),
  },
  "view.zoomIn": {
    label: "zoom.in",
    shortcut: { mods: ["Mod"], key: "+" },
    run: () => void changePageZoom("in"),
    state: () => ({ hidden: !hasTauri }),
  },
  "view.zoomOut": {
    label: "zoom.out",
    shortcut: { mods: ["Mod"], key: "-" },
    run: () => void changePageZoom("out"),
    state: () => ({ hidden: !hasTauri }),
  },
  "view.zoomReset": {
    label: "zoom.reset",
    shortcut: { mods: ["Mod"], key: "0" },
    run: () => void changePageZoom("reset"),
    state: () => ({ hidden: !hasTauri }),
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
      // The displayed rows are built once per load — the settings page
      // pairs this toggle with the same rebuild (TICKET-058).
      usePanes.getState().rebuildAll();
    },
    state: () => ({ checked: !useSettings.getState().hideSystemFiles }),
  },
  "view.foldersFirst": {
    label: "menu.view.foldersFirst",
    role: "check",
    run: () => {
      const s = useSettings.getState();
      s.setFoldersFirst(!s.foldersFirst);
      // Sorting happens while the rows are built, so it has to run again.
      usePanes.getState().rebuildAll();
    },
    state: () => ({ checked: useSettings.getState().foldersFirst }),
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
      // Same follow-up as the settings page: fetch or clear the status
      // in both panes instead of only flipping the switch.
      usePanes.getState().reloadGit();
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
          ? needs("quickLook")
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
  "cfg.general": {
    label: "settings.cat.general",
    dialog: true,
    run: () => openSettings("general"),
  },
  "cfg.view": {
    label: "settings.cat.view",
    dialog: true,
    run: () => openSettings("view"),
  },
  "cfg.colors": {
    label: "settings.cat.colors",
    dialog: true,
    run: () => openSettings("colors"),
  },
  "cfg.files": {
    label: "settings.cat.files",
    dialog: true,
    run: () => openSettings("files"),
  },
  "cfg.operations": {
    label: "settings.cat.operations",
    dialog: true,
    run: () => openSettings("operations"),
  },
  "cfg.programs": {
    label: "settings.cat.programs",
    dialog: true,
    run: () => openSettings("programs"),
  },
  "cfg.favorites": {
    label: "settings.cat.favorites",
    dialog: true,
    run: () => openSettings("favorites"),
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

/**
 * Saved profiles that are not connected yet — connecting runs the same
 * two steps as the connection manager (connect, then show the remote root
 * in the active pane). A failure has no dialog of its own here, so the
 * manager is opened *with* the error rather than swallowing it.
 */
function profileItems(): DynamicItem[] {
  const net = useNet.getState();
  const live = new Set(
    net.connections.filter((c) => c.connected).map((c) => c.id),
  );
  return net.profiles
    .filter((p) => !live.has(p.id))
    .map((p) => ({
      id: `profile:${p.id}`,
      label: p.name || p.host,
      // Same one-line form the connection manager shows under the name.
      detail: `${p.protocol}://${p.user}@${p.host}:${p.port}`,
      run: () => {
        void useNet
          .getState()
          .connect(p.id)
          .then((address) => {
            if (address) {
              const s = usePanes.getState();
              void s.loadDir(s.active, address);
            } else {
              useNet.getState().openManager(true);
            }
          });
      },
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
          item("file.openWithOther"),
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
      item("mark.allFiles"),
      item("mark.noFiles"),
      item("mark.invert"),
      item("mark.sameExt"),
      sep(),
      item("mark.remember"),
      item("mark.restore"),
      item("mark.saveFile"),
      item("mark.loadFile"),
      sep(),
      item("mark.copyNames"),
      item("mark.copyPaths"),
      item("mark.copyDetails"),
      item("mark.copyPathDetails"),
      item("mark.export"),
      sep(),
      item("mark.compare"),
      item("mark.compareHideSame"),
      item("mark.clearCompare"),
    ],
  },
  {
    id: "commands",
    label: "menu.commands",
    nodes: [
      item("cmd.search"),
      {
        kind: "sub",
        label: "menu.cmd.searchMode",
        nodes: [
          item("cmd.searchDuplicates"),
          item("cmd.searchEmptyDirs"),
          item("cmd.searchLargeFiles"),
        ],
      },
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
        label: "menu.net.connect",
        nodes: [
          {
            kind: "list",
            build: profileItems,
            empty: "menu.empty.profiles",
          },
        ],
      },
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
      item("view.zoomIn"),
      item("view.zoomOut"),
      item("view.zoomReset"),
      sep(),
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
      item("view.foldersFirst"),
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
      {
        kind: "sub",
        label: "menu.cfg.pages",
        nodes: [
          item("cfg.general"),
          item("cfg.view"),
          item("cfg.colors"),
          item("cfg.files"),
          item("cfg.operations"),
          item("cfg.programs"),
          item("cfg.favorites"),
        ],
      },
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
