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
  const setActive = usePanes((s) => s.setActive);
  const hasClip = useClipboard((s) => s.mode !== null && s.names.length > 0);
  const inArchive = usePanes((s) => panelOf(s, side).archive !== null);
  const terminalProgram = useSettings((s) => s.terminalProgram);
  const programs = useSettings((s) => s.programs);
  const defaultEditor = useSettings((s) => s.defaultEditor);
  // Whether the current cursor entry is an (openable) file.
  const cursorIsFile = usePanes((s) => {
    const p = panelOf(s, side);
    const cur = p.entries[p.cursor];
    return !!cur && !cur.parent && !cur.is_dir && p.archive === null;
  });
  const t = useT();

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

  const onTerminal = () => openTerminal(targetDir(), terminalProgram);
  const onOpenFolder = () => openPath(targetDir());

  // Properties/permissions of the right-clicked entry (otherwise the cursor).
  const onProps = () => {
    const p = panelOf(usePanes.getState(), side);
    const idx = targetRef.current;
    const entry =
      idx != null && p.entries[idx] ? p.entries[idx] : p.entries[p.cursor];
    if (!entry || entry.parent) return;
    openProps(joinPath(p.path, entry.name));
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
          {item("clip-copy", "ctx.copy", Copy, { shortcut: "⌘C" })}
          {item("clip-cut", "ctx.cut", Scissors, { shortcut: "⌘X" })}
          <ContextMenu.Item
            className={itemCls}
            disabled={!hasClip}
            onSelect={onPaste}
          >
            <ClipboardPaste size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.paste")}</span>
            <span className="ml-auto pl-6 text-dim">⌘V</span>
          </ContextMenu.Item>
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          {item("open-tab", "ctx.openNewTab", FolderSymlink)}
          {item("rename", "ctx.rename", PenLine, { shortcut: "F2" })}
          {item("view", "ctx.view", Eye, { shortcut: "F3" })}
          {isMacOS && (
            <ContextMenu.Item
              className={itemCls}
              disabled={inArchive}
              onSelect={() => runQuickLook(side)}
            >
              <ScanEye size={14} className="shrink-0 text-dim" aria-hidden />
              <span>{t("ctx.quicklook")}</span>
              <span className="ml-auto pl-6 text-dim">Space</span>
            </ContextMenu.Item>
          )}
          {item("edit", "ctx.edit", Pencil, { shortcut: "F4" })}
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger className={itemCls} disabled={!cursorIsFile}>
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
          {item("mkdir", "ctx.mkdir", FolderPlus, { shortcut: "F7" })}
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Item
            className={itemCls}
            disabled={inArchive}
            onSelect={onTerminal}
          >
            <Terminal size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.terminal")}</span>
          </ContextMenu.Item>
          <ContextMenu.Item
            className={itemCls}
            disabled={inArchive}
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
          {item("pack", "ctx.pack", Package)}
          {item("extract", "ctx.extract", PackageOpen)}
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Item
            className={itemCls}
            disabled={inArchive}
            onSelect={onProps}
          >
            <ShieldCheck size={14} className="shrink-0 text-dim" aria-hidden />
            <span>{t("ctx.props")}</span>
            <span className="ml-auto pl-6 text-dim">⌥⏎</span>
          </ContextMenu.Item>
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          {item("delete", "ctx.delete", Trash2, { shortcut: "F8" })}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}
