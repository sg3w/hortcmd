// ============================================================
// The settings dialog's shell (TICKET-035).
//
// A category list on the left, the selected page on the right, and a
// draggable divider between them. The pages themselves live in
// `settings/`, one module per category, so changing what "File
// operations" offers does not put every other page in the diff.
//
// The dialog is self-contained: its open state and the selected page
// both live in `uiStore`, which is what lets the Konfiguration menu
// open it directly on a page (ticket 058) and what lets it sit behind a
// lazy boundary without props (ticket 034).
// ============================================================

import { useCallback, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import {
  AppWindow,
  Files as FilesIcon,
  GripVertical,
  HardDriveDownload,
  LayoutList,
  Palette,
  Paintbrush,
  SlidersHorizontal,
  Star,
  type LucideIcon,
} from "lucide-react";
import { AppDialog } from "./AppDialog";
import { cn } from "@/lib/cn";
import { useT } from "@/store/settingsStore";
import { useUi, type SettingsCategory as Category } from "@/store/uiStore";
import { ColorsForm } from "./settings/colors";
import { FavoritesForm } from "./settings/favorites";
import { FilesForm } from "./settings/files";
import { GeneralForm } from "./settings/general";
import { OperationsForm } from "./settings/operations";
import { ProgramsForm } from "./settings/programs";
import { ViewForm } from "./settings/view";

const MIN_FRAC = 0.25; // 1:3
const MAX_FRAC = 0.5; // 1:1

export function SettingsDialog() {
  const t = useT();
  // Open state and the selected page both live in `uiStore`, so the
  // Konfiguration menu can open the dialog directly on one of them
  // (TICKET-058) — and so this component is self-contained, like every
  // other dialog, which is what lets it sit behind a lazy boundary
  // without props (TICKET-034).
  const open = useUi((s) => s.settingsOpen);
  const setOpen = useUi((s) => s.setSettingsOpen);
  const close = () => setOpen(false);
  const category = useUi((s) => s.settingsCategory);
  const setCategory = useUi((s) => s.setSettingsCategory);
  const [leftFrac, setLeftFrac] = useState(MIN_FRAC);
  const bodyRef = useRef<HTMLDivElement>(null);

  const startResize = useCallback((e: ReactMouseEvent) => {
    e.preventDefault();
    const onMove = (ev: MouseEvent) => {
      const box = bodyRef.current?.getBoundingClientRect();
      if (!box) return;
      const frac = (ev.clientX - box.left) / box.width;
      setLeftFrac(Math.min(MAX_FRAC, Math.max(MIN_FRAC, frac)));
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }, []);

  const cats: { id: Category; label: string; Icon: LucideIcon }[] = [
    { id: "general", label: t("settings.cat.general"), Icon: SlidersHorizontal },
    { id: "view", label: t("settings.cat.view"), Icon: LayoutList },
    { id: "colors", label: t("settings.cat.colors"), Icon: Paintbrush },
    { id: "files", label: t("settings.cat.files"), Icon: FilesIcon },
    {
      id: "operations",
      label: t("settings.cat.operations"),
      Icon: HardDriveDownload,
    },
    { id: "programs", label: t("settings.cat.programs"), Icon: AppWindow },
    { id: "favorites", label: t("settings.cat.favorites"), Icon: Star },
  ];

  return (
    <AppDialog
      dialogKey="settings"
      open={open}
      onClose={close}
      titleBar={
        <>
          <Palette size={15} className="text-accent" />
          {t("settings.title")}
        </>
      }
      defaultSize={{ w: 720, h: 440 }}
      minSize={{ w: 560, h: 400 }}
    >
      <div ref={bodyRef} className="flex min-h-0 flex-1">
            {/* Category tree */}
            <div
              className="shrink-0 overflow-y-auto border-r border-edge bg-panel-inactive p-2"
              style={{ width: `${leftFrac * 100}%` }}
            >
              {cats.map((c) => (
                <button
                  key={c.id}
                  onClick={() => setCategory(c.id)}
                  className={cn(
                    "mb-0.5 flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[13px]",
                    category === c.id
                      ? "bg-accent-dim text-text"
                      : "text-dim hover:bg-header hover:text-text",
                  )}
                >
                  <c.Icon size={15} className="shrink-0" />
                  <span className="truncate">{c.label}</span>
                </button>
              ))}
            </div>

            {/* Drag handle */}
            <div
              onMouseDown={startResize}
              className="flex w-1.5 shrink-0 cursor-col-resize items-center justify-center bg-edge hover:bg-accent"
            >
              <GripVertical size={12} className="text-dim" />
            </div>

            {/* Form */}
            <div className="min-w-0 flex-1 overflow-y-auto p-4">
              {category === "general" && <GeneralForm />}
              {category === "view" && <ViewForm />}
              {category === "colors" && <ColorsForm />}
              {category === "files" && <FilesForm />}
              {category === "operations" && <OperationsForm />}
              {category === "programs" && <ProgramsForm />}
              {category === "favorites" && <FavoritesForm />}
            </div>
          </div>

      <div className="flex justify-end border-t border-edge bg-header px-4 py-2">
        <button
          onClick={close}
          className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
        >
          {t("settings.close")}
        </button>
      </div>
    </AppDialog>
  );
}
