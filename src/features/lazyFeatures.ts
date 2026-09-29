// ============================================================
// The frontend's lazy boundaries, in one list (TICKET-034).
//
// Each entry is a feature that is *not* needed to render the two file
// panes: a heavyweight dependency (Markdown, highlighting, terminal
// emulation) or a dialog large enough to be worth its own chunk. The
// loaders live here rather than inline in `App.tsx` so the boundaries
// can be listed, preloaded on intent, and checked against the bundle
// budget from one place.
//
// What stays eager, deliberately: pane rendering, navigation, file
// operations, the operation prompts (`OperationDialogs` — a confirm can
// be raised by any action and must never wait on a fetch) and the
// transfer surface (`TransferTray`/`TransferWindow`, which shows
// progress for work already running).
// ============================================================

import type { ComponentType } from "react";
import type { Loader } from "@/components/ui/LazyFeature";

/** The components are named exports; `lazy()` wants a default. */
const asDefault =
  <K extends string, M extends Record<K, ComponentType<never>>>(
    load: () => Promise<M>,
    name: K,
  ): Loader =>
  () =>
    load().then((m) => ({ default: m[name] as ComponentType }));

export const LAZY = {
  undo: () => import("@/components/ui/UndoDialog").then(m => ({ default: m.UndoDialog })),
  recovery: () => import("@/components/ui/RecoveryDialog").then(m => ({ default: m.RecoveryDialog })),
  /** Markdown, syntax highlighting, EXIF, hex view (F3). */
  preview: asDefault(() => import("@/components/ui/PreviewDialog"), "PreviewDialog"),
  /** xterm.js plus its fit addon (ticket 026). */
  terminal: asDefault(() => import("@/features/net/SshTerminal"), "SshTerminalWindow"),
  /** Connection manager, credential store, host-key panels. */
  connections: asDefault(
    () => import("@/features/net/ConnectionsDialog"),
    "ConnectionsDialog",
  ),
  settings: asDefault(() => import("@/components/ui/SettingsDialog"), "SettingsDialog"),
  search: asDefault(() => import("@/components/ui/SearchDialog"), "SearchDialog"),
  compareDirs: asDefault(() => import("@/components/ui/CompareDialog"), "CompareDialog"),
  compareFiles: asDefault(
    () => import("@/components/ui/FileCompareDialog"),
    "FileCompareDialog",
  ),
  properties: asDefault(
    () => import("@/components/ui/PropertiesDialog"),
    "PropertiesDialog",
  ),
  rename: asDefault(() => import("@/components/ui/RenameDialog"), "RenameDialog"),
  /** The archive-name options of "Pack" (TICKET-392). */
  pack: asDefault(() => import("@/components/ui/PackDialog"), "PackDialog"),
  export: asDefault(() => import("@/components/ui/ExportDialog"), "ExportDialog"),
  selectionFile: asDefault(
    () => import("@/components/ui/SelectionFileDialog"),
    "SelectionFileDialog",
  ),
  fileBrowser: asDefault(
    () => import("@/components/ui/FileBrowserDialog"),
    "FileBrowserDialog",
  ),
  shortcuts: asDefault(
    () => import("@/components/ui/ShortcutsDialog"),
    "ShortcutsDialog",
  ),
  about: asDefault(() => import("@/components/ui/AboutDialog"), "AboutDialog"),
} satisfies Record<string, Loader>;

export type LazyId = keyof typeof LAZY;

/**
 * Which chunks to warm when a menu category is opened (TICKET-034).
 *
 * Opening a menu is strong intent: the user is looking at the actions
 * that category offers and is one click from one of them. Warming those
 * chunks then hides the fetch behind the time it takes to read the menu
 * — without an unconditional startup preload, which would put the cost
 * straight back into the launch path.
 *
 * Only the heavyweights are listed. The small dialogs load faster than
 * the click that opens them.
 */
export const PRELOAD_ON_MENU: Record<string, Loader[]> = {
  files: [LAZY.preview, LAZY.properties, LAZY.rename],
  commands: [LAZY.search, LAZY.compareDirs, LAZY.terminal],
  net: [LAZY.connections, LAZY.terminal],
  config: [LAZY.settings],
  mark: [LAZY.export, LAZY.selectionFile],
};
