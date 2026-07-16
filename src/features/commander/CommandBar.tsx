// ============================================================
// Commandbar: Icon-Leiste für Sonderfunktionen des aktiven
// Fensters plus Umschalter für den Ansichtsmodus.
// ============================================================

import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
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
import { useOps } from "@/store/opsStore";
import { translate, useT } from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";
import { baseName } from "@/lib/path";
import { openEntry } from "./navigate";
import { runAction } from "./actions";
import { cn } from "@/lib/cn";

/** Dateivergleich links↔rechts starten; sonst Hinweis anzeigen. */
function runFileCompare(): void {
  if (openFileCompare()) return;
  useOps.getState().requestConfirm({
    title: translate("fcmp.needTwo.title"),
    message: translate("fcmp.needTwo.msg"),
    onConfirm: () => {},
  });
}

/** „Ein Verzeichnis hoch" im aktiven Fenster (nutzt die ".."-Zeile). */
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

interface Props {
  onOpenSettings: () => void;
}

export function CommandBar({ onOpenSettings }: Props) {
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
    <div className="flex flex-shrink-0 items-center gap-1 border-b border-edge bg-header px-2 py-1">
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
      <ToolButton title={t("tb.up")} Icon={ArrowUp} onClick={goUp} />
      <ToolButton
        title={t("tb.refresh")}
        Icon={RefreshCw}
        onClick={() => refresh(active)}
      />
      <RecentMenu />

      <Divider />

      <ToolButton
        title={t("tb.mkdir")}
        Icon={FolderPlus}
        onClick={() => runAction("mkdir", active)}
      />
      <ToolButton
        title={t("tb.view")}
        Icon={Eye}
        onClick={() => runAction("view", active)}
      />
      <ToolButton
        title={t("tb.pack")}
        Icon={FileArchive}
        onClick={() => runAction("pack", active)}
      />
      <ToolButton
        title={t("tb.rename")}
        Icon={Replace}
        onClick={() => openRename(active)}
      />
      <ToolButton
        title={t("tb.export")}
        Icon={ListTree}
        onClick={() => openExport(active)}
      />
      <ToolButton
        title={t("tb.compare")}
        Icon={FolderSync}
        onClick={() => openCompare()}
      />
      <ToolButton
        title={t("tb.filecompare")}
        Icon={GitCompare}
        onClick={runFileCompare}
      />
      <ToolButton
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

      <ToolButton title={t("tb.settings")} Icon={Settings} onClick={onOpenSettings} />
    </div>
  );
}

/** Dropdown mit den zuletzt besuchten Ordnern (global). */
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
