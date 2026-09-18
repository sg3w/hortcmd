import "@/i18n/namespaces/exporting";
// ============================================================
// "Save/load selection" dialog (TICKET-059).
//
// Saving writes exactly the marked entries — no implicit cursor
// fallback — as versioned JSON; an empty selection is a valid file.
// Loading never touches the pane until the user confirms: the dialog
// first shows what the file says and how much of it matches the folder
// on screen, so a file taken somewhere else cannot silently replace a
// selection. Nothing here connects anywhere or navigates.
//
// Reading, writing and validating the format happen in the backend
// (`save_selection` / `load_selection`); this dialog only ever sees an
// already validated document.
// ============================================================

import { useEffect, useState } from "react";
import { FileWarning, FolderSearch, ListChecks } from "lucide-react";
import type { SelectionFile } from "@/ipc/bindings";
import { loadSelection, saveSelection } from "@/ipc/client";
import { AppDialog } from "@/components/ui/AppDialog";
import { Check } from "@/components/ui/dialogControls";
import { openFileBrowser } from "@/store/fileBrowserStore";
import { useOps } from "@/store/opsStore";
import { locationOf, panelOf, usePanes } from "@/store/panesStore";
import { useSelectionFile } from "@/store/selectionFileStore";
import { useT } from "@/store/settingsStore";
import { notify } from "@/store/noticeStore";
import { baseName, joinPath, parentPath } from "@/lib/path";

const DEFAULT_SIZE = { w: 640, h: 420 };

/** Fills "{x}" placeholders of a translated string. */
function fill(text: string, vars: Record<string, string | number>): string {
  return Object.entries(vars).reduce(
    (acc, [k, v]) => acc.split(`{${k}}`).join(String(v)),
    text,
  );
}

export function SelectionFileDialog() {
  const mode = useSelectionFile((s) => s.mode);
  const side = useSelectionFile((s) => s.side);
  const close = useSelectionFile((s) => s.close);
  const t = useT();

  const [destDir, setDestDir] = useState("");
  const [fileName, setFileName] = useState("Auswahl.json");
  const [sourceFile, setSourceFile] = useState("");
  const [loaded, setLoaded] = useState<SelectionFile | null>(null);
  const [applyHere, setApplyHere] = useState(false);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);

  // Snapshot of the pane taken when the dialog opens, so a watcher
  // refresh behind the dialog cannot change what is on screen here.
  const panel = mode ? panelOf(usePanes.getState(), side) : null;
  const location = panel ? locationOf(panel) : "";
  const selected = panel ? [...panel.selected] : [];

  useEffect(() => {
    if (!mode) return;
    const p = panelOf(usePanes.getState(), side);
    // An archive has no folder to write into; fall back to the folder
    // holding the archive file.
    setDestDir(p.archive ? parentPath(p.archive) : p.path);
    setFileName("Auswahl.json");
    setSourceFile("");
    setLoaded(null);
    setApplyHere(false);
    setStatus(null);
  }, [mode, side]);

  // ---------- save ----------

  const writeTo = async (target: string, overwrite: boolean) => {
    const p = panelOf(usePanes.getState(), side);
    const entries = [...p.selected].map((name) => ({
      name,
      is_dir: p.entries.some((r) => r.name === name && r.is_dir),
    }));
    try {
      const written = await saveSelection(target, locationOf(p), entries, overwrite);
      if (!written) {
        useOps.getState().requestConfirm({
          title: t("op.collision.title"),
          message: t("op.collision.exists").replace("{name}", baseName(target)),
          danger: true,
          confirmLabel: t("op.collision.overwrite"),
          icon: FileWarning,
          onConfirm: () => void writeTo(target, true),
        });
        return;
      }
      setStatus({ ok: true, text: fill(t("selfile.saved"), { path: target }) });
    } catch (e) {
      setStatus({ ok: false, text: String(e) });
    }
  };

  const save = () => {
    const name = fileName.trim();
    if (!name || !destDir) return;
    void writeTo(joinPath(destDir, name), false);
  };

  // ---------- load ----------

  const pickFile = () => {
    openFileBrowser({
      selectMode: "file",
      title: t("selfile.file"),
      initialPath: destDir,
      onPick: (path) => {
        setSourceFile(path);
        setLoaded(null);
        setStatus(null);
        void (async () => {
          try {
            const file = await loadSelection(path);
            setLoaded(file);
            setApplyHere(false);
          } catch (e) {
            setStatus({ ok: false, text: String(e) });
          }
        })();
      },
    });
  };

  // Preview against the folder currently on screen. Only existing,
  // visible entries of the matching kind can be selected.
  const preview = (() => {
    if (!loaded || !panel) return null;
    const byName = new Map(
      panel.entries.filter((r) => !r.parent).map((r) => [r.name, r]),
    );
    const usable = loaded.entries.filter((e) => {
      const row = byName.get(e.name);
      return row !== undefined && row.is_dir === e.is_dir;
    });
    return { usable, missing: loaded.entries.length - usable.length };
  })();

  const sameLocation = loaded?.location === location;
  const canApply = !!preview && (sameLocation || applyHere);

  const apply = () => {
    if (!preview || !canApply) return;
    const { applied, missing } = usePanes
      .getState()
      .applySelection(side, preview.usable.map((e) => e.name));
    notify(
      side,
      missing > 0 || preview.missing > 0 ? "sel.applied.partial" : "sel.applied",
      { n: applied, missing: missing + preview.missing },
      missing + preview.missing > 0 ? "error" : "info",
    );
    close();
  };

  const saving = mode === "save";

  return (
    <AppDialog
      dialogKey="selectionFile"
      open={!!mode}
      onClose={close}
      titleBar={
        <>
          <ListChecks size={15} className="text-accent" />
          {t(saving ? "selfile.save.title" : "selfile.load.title")}
        </>
      }
      defaultSize={DEFAULT_SIZE}
      minSize={{ w: 520, h: 340 }}
      footer={
        <div className="flex items-center gap-3">
          {status && (
            <span
              className={status.ok ? "truncate text-[12px] text-dim" : "truncate text-[12px] text-red-400"}
            >
              {status.text}
            </span>
          )}
          <span className="flex-1" />
          <button
            onClick={close}
            className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
          >
            {t("op.cancel")}
          </button>
          {saving ? (
            <button
              onClick={save}
              disabled={!fileName.trim() || !destDir}
              className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
            >
              {t("selfile.save")}
            </button>
          ) : (
            <button
              onClick={apply}
              disabled={!canApply}
              className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
            >
              {t("selfile.apply")}
            </button>
          )}
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4 text-[13px]">
        {saving ? (
          <>
            <p className="text-dim">
              {fill(t("selfile.count"), { n: selected.length })}
            </p>
            <label className="text-[12px] text-dim">{t("selfile.folder")}</label>
            <div className="flex gap-2">
              <input
                value={destDir}
                onChange={(e) => setDestDir(e.target.value)}
                spellCheck={false}
                className="min-w-0 flex-1 rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
              />
              <button
                onClick={() =>
                  openFileBrowser({
                    selectMode: "folder",
                    title: t("selfile.folder"),
                    initialPath: destDir,
                    onPick: setDestDir,
                  })
                }
                className="flex shrink-0 items-center gap-1 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text hover:border-accent"
              >
                <FolderSearch size={13} />
                {t("selfile.browse")}
              </button>
            </div>
            <label className="text-[12px] text-dim">{t("selfile.name")}</label>
            <input
              value={fileName}
              onChange={(e) => setFileName(e.target.value)}
              spellCheck={false}
              className="rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
            />
          </>
        ) : (
          <>
            <label className="text-[12px] text-dim">{t("selfile.file")}</label>
            <div className="flex gap-2">
              <input
                value={sourceFile}
                readOnly
                placeholder={t("selfile.pick")}
                className="min-w-0 flex-1 rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none"
              />
              <button
                onClick={pickFile}
                className="flex shrink-0 items-center gap-1 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text hover:border-accent"
              >
                <FolderSearch size={13} />
                {t("selfile.browse")}
              </button>
            </div>
            {loaded && preview && (
              <div className="flex flex-col gap-1.5 rounded border border-edge bg-panel-inactive p-3">
                <div className="flex gap-2">
                  <span className="w-32 shrink-0 text-dim">
                    {t("selfile.location")}
                  </span>
                  <span className="min-w-0 break-all font-mono text-[12px]">
                    {loaded.location}
                  </span>
                </div>
                <div className="flex gap-2">
                  <span className="w-32 shrink-0 text-dim">
                    {t("selfile.current")}
                  </span>
                  <span className="min-w-0 break-all font-mono text-[12px]">
                    {location}
                  </span>
                </div>
                <div className="mt-1 flex gap-4">
                  <span>{fill(t("selfile.matches"), { n: preview.usable.length })}</span>
                  {preview.missing > 0 && (
                    <span className="text-red-400">
                      {fill(t("selfile.missing"), { n: preview.missing })}
                    </span>
                  )}
                </div>
                {sameLocation ? (
                  <p className="text-dim">{t("selfile.sameLocation")}</p>
                ) : (
                  <>
                    <p className="text-dim">{t("selfile.otherLocation")}</p>
                    <Check
                      checked={applyHere}
                      onChange={() => setApplyHere((v) => !v)}
                      label={t("selfile.applyHere")}
                    />
                  </>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </AppDialog>
  );
}
