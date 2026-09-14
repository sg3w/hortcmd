import { openOpposite } from "@/features/commander/navigate";
import { formatShortcut } from "@/lib/shortcut";
// ============================================================
// Right-click context menu over the file list (Radix UI).
// First items: copy, cut, paste.
// ============================================================

import * as ContextMenu from "@radix-ui/react-context-menu";
import type { ReactNode, RefObject } from "react";
import {
  AppWindow,
  ClipboardPaste,
  Copy,
  Eye,
  ScanEye,
  FileCog,
  FileText,
  FolderOpen,
  FolderPlus,
  FolderSymlink,
  Package,
  PackageOpen,
  Pencil,
  PenLine,
  Scissors,
  ShieldCheck,
  Terminal,
  Trash2,
  Type,
  type LucideIcon,
} from "lucide-react";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { useClipboard } from "@/store/clipboardStore";
import { runAction, type ActionId } from "@/features/commander/actions";
import {
  runOpenWithBrowse,
  runOpenWithProgram,
  runPaste,
  runQuickLook,
  targetPaths,
} from "@/features/commander/fileOps";
import { isMacOS } from "@/lib/platform";
import { openProps } from "@/store/propsStore";
import { openPath, openTerminal } from "@/ipc/client";
import { writeClipboard } from "@/lib/clipboard";
import { baseName, joinPath } from "@/lib/path";
import { locationKey, useCaps } from "@/store/capsStore";
import {
  guard,
  locationReason,
  pairState,
} from "@/features/commander/guard";
import type { Capability } from "@/ipc/bindings";
import {
  supportsSshTerminal,
  useTerminals,
} from "@/features/net/terminalStore";
import { useSettings, useT } from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";

const itemCls =
  "flex cursor-default items-center gap-2 rounded px-2 py-1 text-[12px] text-text outline-none data-[highlighted]:bg-accent-dim data-[disabled]:opacity-40";

interface Props {
  side: Side;
  /** Index of the most recently right-clicked row (from FileTable). */
  targetRef: RefObject<number | null>;
  children: ReactNode;
}

export function FileContextMenu({ side, targetRef, children }: Props) {
  const t = useT();
  const setActive = usePanes((s) => s.setActive);
  const hasClip = useClipboard((s) => s.mode !== null && s.names.length > 0);
  const inArchive = usePanes((s) => panelOf(s, side).archive !== null);
  const terminalProgram = useSettings((s) => s.terminalProgram);
  const programs = useSettings((s) => s.programs);
  const defaultEditor = useSettings((s) => s.defaultEditor);
  const path = usePanes((s) => panelOf(s, side).path);
  const clipSource = useClipboard((s) => s.sourcePath);
  const clipMode = useClipboard((s) => s.mode);
  // What this location supports comes from the backend's matrix
  // (TICKET-031) — never from a scheme name here. Subscribing to the
  // cache re-renders the menu when a connection drops while it is open,
  // so an item cannot stay enabled against a dead session.
  const caps = useCaps((s) => s.known[locationKey(path)]);
  const can = (cap: Capability) => !!caps?.includes(cap);
  // One explanation for every item this location cannot run — the same
  // wording the application menu shows. `useT` above is what makes the
  // text follow a language switch while the menu is open.
  const why = locationReason(path);
  // A terminal is either a local one or an SSH shell on the connection.
  const canTerminal = can("localTerminal") || can("sshTerminal");
  const canOpenWith = can("openWith");
  // Pasting spans two locations: the clipboard's source folder and this
  // one. Each is asked for its own half.
  const pasteOk =
    !clipMode ||
    !pairState(
      clipSource,
      clipMode === "cut" ? "move" : "copyFrom",
      path,
      "copyTo",
    ).disabled;
  // Whether the current cursor entry is an (openable) file.
  const cursorIsFile = usePanes((s) => {
    const p = panelOf(s, side);
    const cur = p.entries[p.cursor];
    return !!cur && !cur.parent && !cur.is_dir && p.archive === null;
  });

  // Target folder: right-click on a folder → that folder, otherwise
  // (file/empty space) → current folder of the window.
  const targetDir = (): string => {
    const p = panelOf(usePanes.getState(), side);
    const idx = targetRef.current;
    const entry = idx != null ? p.entries[idx] : undefined;
    return entry && entry.is_dir && !entry.parent
      ? joinPath(p.path, entry.name)
      : p.path;
  };

  // Paste is context-dependent: right-click on a folder → into that folder,
  // otherwise (file/empty space) → into the current folder of the active window.
  const onPaste = () => {
    const p = panelOf(usePanes.getState(), side);
    const idx = targetRef.current;
    const entry = idx != null ? p.entries[idx] : undefined;
    if (entry && entry.is_dir && !entry.parent) {
      runPaste(side, joinPath(p.path, entry.name));
    } else {
      runPaste(side);
    }
  };

  // Remote folders open an SSH shell in place (ticket 026); local ones
  // start the configured terminal program.
  const onTerminal = () => {
    const dir = targetDir();
    if (supportsSshTerminal(dir)) {
      useTerminals.getState().openFor(dir);
    } else if (guard(side, dir, "localTerminal")) {
      openTerminal(dir, terminalProgram);
    }
  };
  const onOpenFolder = () => {
    const dir = targetDir();
    // The right-click may have landed on a subfolder — the capability is
    // re-asked for that target, not for the pane (TICKET-031).
    if (guard(side, dir, "systemOpen")) void openPath(dir);
  };

  // Properties/permissions of the right-clicked entry (otherwise the cursor).
  const onProps = () => {
    const p = panelOf(usePanes.getState(), side);
    const idx = targetRef.current;
    const entry =
      idx != null && p.entries[idx] ? p.entries[idx] : p.entries[p.cursor];
    if (!entry || entry.parent) return;
    openProps(joinPath(p.path, entry.name), p.git?.entries[entry.name]);
  };

  // Full paths of the target set (selection or right-clicked entry).
  const onCopyPath = () => {
    const paths = targetPaths(side);
    if (paths.length) void writeClipboard(paths.join("\n"));
  };
  // Only the file/folder names of the target set.
  const onCopyName = () => {
    const names = targetPaths(side).map(baseName);
    if (names.length) void writeClipboard(names.join("\n"));
  };

  const item = (
    id: ActionId,
    key: TransKey,
    Icon: LucideIcon,
    opts?: { shortcut?: string; disabled?: boolean },
  ) => (
    <ContextMenu.Item
      className={itemCls}
      disabled={opts?.disabled}
      title={opts?.disabled ? why : undefined}
      onSelect={() => runAction(id, side)}
    >
      <Icon size={14} className="shrink-0 text-dim" aria-hidden />
      <span>{t(key)}</span>
      {opts?.shortcut && (
        <span className="ml-auto pl-6 text-dim">{opts.shortcut}</span>
      )}
    </ContextMenu.Item>
  );

  return (
    <ContextMenu.Root>
      {/* The trigger must be a real DOM element so that Radix can attach its
          contextmenu handling (preventDefault + open). A component as the
          asChild child would swallow the props. */}
      <ContextMenu.Trigger asChild>
        <div
          className="flex min-h-0 min-w-0 flex-1 flex-col"
          onContextMenu={() => setActive(side)}
        >
          {children}
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className="z-50 min-w-[210px] rounded-md border border-edge bg-panel p-1 shadow-xl">
          {item("clip-copy", "ctx.copy", Copy, { shortcut: formatShortcut({key:"C",mods:["Mod"]}) })}
          {item("clip-cut", "ctx.cut", Scissors, { shortcut: formatShortcut({key:"X",mods:["Mod"]}) })}
          <ContextMenu.Item
            className={itemCls}
            disabled={!hasClip || !pasteOk}
            title={hasClip && !pasteOk ? why : undefined}
            onSelect={onPaste}
          >
            <ClipboardPaste size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.paste")}</span>
            <span className="ml-auto pl-6 text-dim">{formatShortcut({key:"V",mods:["Mod"]})}</span>
          </ContextMenu.Item>
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          {item("open-tab", "ctx.openNewTab", FolderSymlink)}
          {item("rename", "ctx.rename", PenLine, {
            shortcut: "F2",
            disabled: !can("move"),
          })}
          {item("view", "ctx.view", Eye, {
            shortcut: "F3",
            disabled: !can("preview"),
          })}
          {isMacOS && (
            <ContextMenu.Item
              className={itemCls}
              disabled={inArchive || !can("quickLook")}
              title={can("quickLook") ? undefined : why}
              onSelect={() => runQuickLook(side)}
            >
              <ScanEye size={14} className="shrink-0 text-dim" aria-hidden />
              <span>{t("ctx.quicklook")}</span>
              <span className="ml-auto pl-6 text-dim">{t("key.space")}</span>
            </ContextMenu.Item>
          )}
          {item("edit", "ctx.edit", Pencil, {
            shortcut: "F4",
            disabled: !canOpenWith,
          })}
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger
              className={itemCls}
              disabled={!cursorIsFile || !canOpenWith}
            >
              <AppWindow size={14} className="shrink-0 text-dim" aria-hidden />
              <span>{t("ctx.openWith")}</span>
              <span className="ml-auto pl-6 text-dim">›</span>
            </ContextMenu.SubTrigger>
            <ContextMenu.Portal>
              <ContextMenu.SubContent className="z-50 min-w-[200px] rounded-md border border-edge bg-panel p-1 shadow-xl">
                {programs.map((prog) => (
                  <ContextMenu.Item
                    key={prog.id}
                    className={itemCls}
                    onSelect={() => runOpenWithProgram(side, prog.path)}
                  >
                    <AppWindow
                      size={14}
                      className="shrink-0 text-dim"
                      aria-hidden
                    />
                    <span className="truncate">{prog.name}</span>
                  </ContextMenu.Item>
                ))}
                {defaultEditor && (
                  <ContextMenu.Item
                    className={itemCls}
                    onSelect={() => runOpenWithProgram(side, defaultEditor)}
                  >
                    <Pencil size={14} className="shrink-0 text-dim" aria-hidden />
                    <span>{t("ctx.openWithDefault")}</span>
                  </ContextMenu.Item>
                )}
                {(programs.length > 0 || defaultEditor) && (
                  <ContextMenu.Separator className="my-1 h-px bg-edge" />
                )}
                <ContextMenu.Item
                  className={itemCls}
                  onSelect={() => runOpenWithBrowse(side)}
                >
                  <FileCog size={14} className="shrink-0 text-dim" aria-hidden />
                  <span>{t("ctx.openWithOther")}</span>
                </ContextMenu.Item>
              </ContextMenu.SubContent>
            </ContextMenu.Portal>
          </ContextMenu.Sub>
          {item("mkdir", "ctx.mkdir", FolderPlus, {
            shortcut: "F7",
            disabled: !can("create"),
          })}
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Item
            className={itemCls}
            disabled={inArchive || !canTerminal}
            title={canTerminal ? undefined : why}
            onSelect={onTerminal}
          >
            <Terminal size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.terminal")}</span>
          </ContextMenu.Item>
          <ContextMenu.Item
            className={itemCls}
            onSelect={() => void openOpposite(side, {path: targetDir(), archive: panelOf(usePanes.getState(), side).archive})}
          >
            <FolderSymlink size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.openOpposite")}</span>
          </ContextMenu.Item>
          <ContextMenu.Item
            className={itemCls}
            disabled={inArchive || !can("systemOpen")}
            title={can("systemOpen") ? undefined : why}
            onSelect={onOpenFolder}
          >
            <FolderOpen size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.openFolder")}</span>
          </ContextMenu.Item>
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Item
            className={itemCls}
            disabled={inArchive}
            onSelect={onCopyPath}
          >
            <FileText size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.copyPath")}</span>
          </ContextMenu.Item>
          <ContextMenu.Item
            className={itemCls}
            disabled={inArchive}
            onSelect={onCopyName}
          >
            <Type size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.copyName")}</span>
          </ContextMenu.Item>
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          {item("pack", "ctx.pack", Package, { disabled: !can("archivePackFrom") })}
          {item("extract", "ctx.extract", PackageOpen, {
            disabled: !can("archiveExtractFrom"),
          })}
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Item
            className={itemCls}
            disabled={inArchive || !can("stat")}
            title={can("stat") ? undefined : why}
            onSelect={onProps}
          >
            <ShieldCheck size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.props")}</span>
            <span className="ml-auto pl-6 text-dim">⌥⏎</span>
          </ContextMenu.Item>
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          {item("delete", "ctx.delete", Trash2, {
            shortcut: "F8",
            disabled: !can("deletePermanent"),
          })}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}
