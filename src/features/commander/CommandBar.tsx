// ============================================================
// Command bar: icon bar for special functions of the active
// pane plus a switcher for the view mode.
// ============================================================

import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  PanelsTopLeft,
  FileArchive,
  Folder,
  FolderPlus,
  FolderSync,
  FolderTree,
  GitCompare,
  History,
  Search,
  LayoutGrid,
  List,
  ListTree,
  RefreshCw,
  Replace,
  Rows3,
  Server,
  Settings,
  Trash2,
  Eye,
  type LucideIcon,
} from "lucide-react";
import { panelOf, usePanes, type ViewMode } from "@/store/panesStore";
import { openExport } from "@/store/exportStore";
import { openRename } from "@/store/renameStore";
import { openCompare } from "@/store/compareStore";
import { openFileCompare } from "@/store/fileCompareStore";
import { openSearch } from "@/store/searchStore";
import { useHistory } from "@/store/historyStore";
import { useNet } from "@/features/net/netStore";
import { useOps } from "@/store/opsStore";
import { openSettings } from "@/store/uiStore";
import { translate, useT } from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";
import { baseName } from "@/lib/path";
import { openEntry } from "./navigate";
import { runAction } from "./actions";
import { commandState, runCommand, type CommandId } from "./commands";
import { useCaps } from "@/store/capsStore";
import { fillVars } from "@/store/noticeStore";
import { cn } from "@/lib/cn";

/** Start the left↔right file comparison; otherwise show a hint. */
function runFileCompare(): void {
  if (openFileCompare()) return;
  useOps.getState().requestConfirm({
    title: translate("fcmp.needTwo.title"),
    message: translate("fcmp.needTwo.msg"),
    onConfirm: () => {},
  });
}

/** "Up one directory" in the active pane (uses the ".." row). */
function goUp(): void {
  const s = usePanes.getState();
  const p = panelOf(s, s.active);
  if (p.entries[0]?.parent) openEntry(s.active, 0);
}

const VIEWS: { mode: ViewMode; label: TransKey; Icon: LucideIcon }[] = [
  { mode: "details", label: "view.details", Icon: Rows3 },
  { mode: "list", label: "view.list", Icon: List },
  { mode: "thumbnails", label: "view.thumbnails", Icon: LayoutGrid },
  { mode: "tree", label: "view.tree", Icon: FolderTree },
];

export function CommandBar() {
  const active = usePanes((s) => s.active);
  const viewMode = usePanes((s) => s[s.active].viewMode);
  const canBack = usePanes((s) => panelOf(s, s.active).historyIndex > 0);
  const canForward = usePanes((s) => {
    const t = panelOf(s, s.active);
    return t.historyIndex < t.history.length - 1;
  });
  const setViewMode = usePanes((s) => s.setViewMode);
  const refresh = usePanes((s) => s.refresh);
  const goBack = usePanes((s) => s.goBack);
  const goForward = usePanes((s) => s.goForward);
  const t = useT();

  return (
    <div className="flex min-w-0 flex-shrink-0 items-center gap-1 overflow-x-auto border-b border-edge bg-header px-2 py-1 [&>*]:shrink-0">
      <ToolButton
        title={t("tb.back")}
        Icon={ArrowLeft}
        disabled={!canBack}
        onClick={() => goBack(active)}
      />
      <ToolButton
        title={t("tb.forward")}
        Icon={ArrowRight}
        disabled={!canForward}
        onClick={() => goForward(active)}
      />
      <CommandButton
        id="nav.up"
        title={t("tb.up")}
        Icon={ArrowUp}
        onClick={goUp}
      />
      <CommandButton
        id="view.refresh"
        title={t("tb.refresh")}
        Icon={RefreshCw}
        onClick={() => void refresh(active)}
      />
      <CommandButton
        id="nav.openOpposite"
        title={t("menu.nav.openOpposite")}
        Icon={PanelsTopLeft}
        onClick={() => runCommand("nav.openOpposite")}
      />
      <RecentMenu />

      <Divider />

      {/* Remote connections – global, not per pane (ticket 016). */}
      <ToolButton
        title={t("net.manage")}
        Icon={Server}
        onClick={() => useNet.getState().openManager()}
      />

      <Divider />

      <CommandButton
        id="file.mkdir"
        title={t("tb.mkdir")}
        Icon={FolderPlus}
        onClick={() => runAction("mkdir", active)}
      />
      <CommandButton
        id="file.view"
        title={t("tb.view")}
        Icon={Eye}
        onClick={() => runAction("view", active)}
      />
      <CommandButton
        id="file.pack"
        title={t("tb.pack")}
        Icon={FileArchive}
        onClick={() => runAction("pack", active)}
      />
      <CommandButton
        id="file.batchRename"
        title={t("tb.rename")}
        Icon={Replace}
        onClick={() => openRename(active)}
      />
      <CommandButton
        id="mark.export"
        title={t("tb.export")}
        Icon={ListTree}
        onClick={() => openExport(active)}
      />
      <CommandButton
        id="cmd.compareDirs"
        title={t("tb.compare")}
        Icon={FolderSync}
        onClick={() => openCompare()}
      />
      <CommandButton
        id="file.compareFiles"
        title={t("tb.filecompare")}
        Icon={GitCompare}
        onClick={runFileCompare}
      />
      <CommandButton
        id="cmd.search"
        title={t("tb.search")}
        Icon={Search}
        onClick={() => openSearch(active)}
      />

      <Divider />

      {/* Ansichtsumschalter für das aktive Fenster */}
      <div className="flex items-center gap-0.5">
        {VIEWS.map(({ mode, label, Icon }) => (
          <ToolButton
            key={mode}
            title={t(label)}
            Icon={Icon}
            selected={viewMode === mode}
            onClick={() => setViewMode(active, mode)}
          />
        ))}
      </div>

      <div className="ml-auto" />

      <ToolButton
        title={t("tb.settings")}
        Icon={Settings}
        onClick={() => openSettings()}
      />
    </div>
  );
}

/** Dropdown with the recently visited folders (global). */
function RecentMenu() {
  const recent = useHistory((s) => s.recent);
  const clear = useHistory((s) => s.clear);
  const t = useT();

  const go = (path: string) => {
    const s = usePanes.getState();
    s.setActive(s.active);
    void s.loadDir(s.active, path);
  };

  const itemCls =
    "flex cursor-default items-center gap-2 rounded px-2 py-1 text-[12px] text-text outline-none data-[highlighted]:bg-accent-dim";

  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          title={t("tb.recent")}
          aria-label={t("tb.recent")}
          className="flex items-center justify-center rounded border border-transparent p-1 text-dim hover:border-edge hover:bg-accent-dim hover:text-text"
        >
          <History size={15} />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="start"
          sideOffset={4}
          className="z-50 max-h-[60vh] min-w-[240px] max-w-[440px] overflow-y-auto rounded-md border border-edge bg-panel p-1 shadow-xl"
        >
          {recent.length === 0 ? (
            <div className="px-2 py-1.5 text-[12px] text-dim">
              {t("tb.recentEmpty")}
            </div>
          ) : (
            <>
              {recent.map((p) => (
                <DropdownMenu.Item
                  key={p}
                  className={itemCls}
                  onSelect={() => go(p)}
                >
                  <Folder size={13} className="shrink-0 text-accent" aria-hidden />
                  <span className="flex min-w-0 flex-col leading-tight">
                    <span className="truncate">{baseName(p)}</span>
                    <span className="truncate font-mono text-[10px] text-dim">
                      {p}
                    </span>
                  </span>
                </DropdownMenu.Item>
              ))}
              <DropdownMenu.Separator className="my-1 h-px bg-edge" />
              <DropdownMenu.Item className={itemCls} onSelect={() => clear()}>
                <Trash2 size={13} className="shrink-0 text-dim" aria-hidden />
                <span>{t("tb.recentClear")}</span>
              </DropdownMenu.Item>
            </>
          )}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

function Divider() {
  return <span className="mx-0.5 h-5 w-px bg-edge" aria-hidden />;
}

/**
 * A toolbar button backed by a command from the shared table
 * (TICKET-031). It reads its enabled state and its "why not" from
 * `commandState`, so the toolbar, the menus and the keyboard agree on
 * what a location can do instead of each deciding for itself.
 *
 * The label stays the toolbar's own short wording; only the reason for
 * a disabled button is appended, which is what the tooltip has to say.
 */
function CommandButton({
  id,
  title,
  Icon,
  onClick,
}: {
  id: CommandId;
  title: string;
  Icon: LucideIcon;
  onClick?: () => void;
}) {
  const t = useT();
  // Subscribing to the pane and capability stores keeps the state fresh
  // — a connection dropping re-renders the bar.
  usePanes((s) => s.active);
  useCaps((s) => s.known);
  const st = commandState(id);
  const why =
    st.disabled && st.reason
      ? fillVars(t(st.reason), st.reasonVars)
      : undefined;
  return (
    <ToolButton
      title={why ? `${title} – ${why}` : title}
      Icon={Icon}
      disabled={!!st.disabled || !!st.hidden}
      onClick={onClick ?? (() => runCommand(id))}
    />
  );
}

interface ButtonProps {
  title: string;
  Icon: LucideIcon;
  onClick: () => void;
  selected?: boolean;
  disabled?: boolean;
}

function ToolButton({ title, Icon, onClick, selected, disabled }: ButtonProps) {
  return (
    <button
      title={title}
      aria-label={title}
      aria-pressed={selected}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex items-center justify-center rounded border border-transparent p-1 text-dim hover:border-edge hover:bg-accent-dim hover:text-text",
        selected && "border-accent bg-accent-dim text-accent",
        disabled && "pointer-events-none opacity-30 hover:border-transparent hover:bg-transparent",
      )}
    >
      <Icon size={15} />
    </button>
  );
}
