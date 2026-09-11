// ============================================================
// Settings ▸ Open with (TICKET-035).
//
// The program list and the extension mapping behind "Open with", shown
// either by extension or by program.
// ============================================================

import { openFileBrowser } from "@/store/fileBrowserStore";
import { normalizeExt, useSettings, useT } from "@/store/settingsStore";
import type { EditorTrigger } from "@/store/settingsStore";
import { useState } from "react";
import { Field, Segmented, smallBtn, ProgramSelect, ExtensionTags } from "./controls";
import {
  Plus,
  Trash2,
  Type,
  AppWindow,
  Pencil,
  FolderSearch,
} from "lucide-react";

/** Derive the program name from a path (.app/extension removed). */
function programNameFromPath(path: string): string {
  const base = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
  return base.replace(/\.app$/i, "").replace(/\.[^.]+$/, "") || base;
}

export function ProgramsForm() {
  const t = useT();
  const editorTrigger = useSettings((s) => s.editorTrigger);
  const setEditorTrigger = useSettings((s) => s.setEditorTrigger);
  const defaultEditor = useSettings((s) => s.defaultEditor);
  const setDefaultEditor = useSettings((s) => s.setDefaultEditor);
  const [view, setView] = useState<"ext" | "prog">("ext");

  const browseDefault = () =>
    openFileBrowser({
      title: t("settings.editor.default"),
      selectMode: "file",
      onPick: setDefaultEditor,
    });

  return (
    <div className="flex flex-col gap-5">
      <Field icon={Type} label={t("settings.editor.trigger")}>
        <Segmented<EditorTrigger>
          value={editorTrigger}
          onChange={setEditorTrigger}
          options={[
            { value: "shiftF4", label: t("settings.editor.trigger.shiftF4") },
            {
              value: "contextOnly",
              label: t("settings.editor.trigger.contextOnly"),
            },
            { value: "f4", label: t("settings.editor.trigger.f4") },
          ]}
        />
        <span className="text-[11px] text-dim">
          {t("settings.editor.trigger.hint")}
        </span>
      </Field>

      <Field icon={Pencil} label={t("settings.editor.default")}>
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text">
            {defaultEditor || t("settings.editor.none")}
          </span>
          <button onClick={browseDefault} className={smallBtn}>
            <FolderSearch size={13} />
            {t("settings.editor.browse")}
          </button>
          {defaultEditor && (
            <button
              onClick={() => setDefaultEditor("")}
              className="rounded p-1 text-dim hover:bg-red-500/20 hover:text-red-400"
              title={t("settings.editor.remove")}
            >
              <Trash2 size={13} />
            </button>
          )}
        </div>
        <span className="text-[11px] text-dim">
          {t("settings.editor.default.hint")}
        </span>
      </Field>

      <div className="flex flex-col gap-3 border-t border-edge pt-3">
        <Segmented<"ext" | "prog">
          value={view}
          onChange={setView}
          options={[
            { value: "ext", label: t("settings.editor.byExt") },
            { value: "prog", label: t("settings.editor.byProgram") },
          ]}
        />
        {view === "ext" ? <ByExtensionView /> : <ByProgramView />}
      </div>
    </div>
  );
}

/** "By extension" view: mapping extension → program. */
function ByExtensionView() {
  const t = useT();
  const programs = useSettings((s) => s.programs);
  const associations = useSettings((s) => s.associations);
  const setAssociation = useSettings((s) => s.setAssociation);
  const [newExt, setNewExt] = useState("");
  const [newProg, setNewProg] = useState("");

  if (programs.length === 0) {
    return (
      <p className="text-[12px] text-dim">{t("settings.editor.noPrograms")}</p>
    );
  }

  const entries = Object.entries(associations).sort((a, b) =>
    a[0].localeCompare(b[0]),
  );

  const add = () => {
    const ext = normalizeExt(newExt);
    if (!ext || !newProg) return;
    setAssociation(ext, newProg);
    setNewExt("");
  };

  return (
    <div className="flex flex-col gap-2">
      {entries.length === 0 ? (
        <p className="text-[12px] text-dim">{t("settings.editor.noAssoc")}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {entries.map(([ext, pid]) => (
            <li
              key={ext}
              className="flex items-center gap-2 rounded border border-edge bg-panel-inactive px-2 py-1"
            >
              <span className="w-16 shrink-0 font-mono text-[12px] text-accent">
                .{ext}
              </span>
              <ProgramSelect
                value={pid}
                onChange={(v) => setAssociation(ext, v || null)}
              />
              <button
                onClick={() => setAssociation(ext, null)}
                className="shrink-0 rounded p-1 text-dim hover:bg-red-500/20 hover:text-red-400"
                title={t("settings.editor.remove")}
              >
                <Trash2 size={13} />
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="flex items-center gap-2 border-t border-edge pt-2">
        <input
          value={newExt}
          onChange={(e) => setNewExt(e.target.value)}
          placeholder={t("settings.editor.extPlaceholder")}
          spellCheck={false}
          className="w-20 shrink-0 rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
        />
        <ProgramSelect value={newProg} onChange={setNewProg} />
        <button
          onClick={add}
          disabled={!normalizeExt(newExt) || !newProg}
          className="flex shrink-0 items-center gap-1 rounded bg-accent px-2 py-1 text-[12px] text-white hover:brightness-110 disabled:opacity-40"
        >
          <Plus size={13} />
        </button>
      </div>
    </div>
  );
}

/** "By program" view: programs with their extensions. */
function ByProgramView() {
  const t = useT();
  const programs = useSettings((s) => s.programs);
  const associations = useSettings((s) => s.associations);
  const addProgram = useSettings((s) => s.addProgram);
  const updateProgram = useSettings((s) => s.updateProgram);
  const removeProgram = useSettings((s) => s.removeProgram);
  const setAssociation = useSettings((s) => s.setAssociation);

  const addViaBrowser = () =>
    openFileBrowser({
      title: t("settings.editor.addProgram"),
      selectMode: "file",
      onPick: (path) => addProgram(programNameFromPath(path), path),
    });

  const changePath = (id: string) =>
    openFileBrowser({
      selectMode: "file",
      onPick: (path) => updateProgram(id, { path }),
    });

  const extsOf = (id: string) =>
    Object.entries(associations)
      .filter(([, pid]) => pid === id)
      .map(([ext]) => ext)
      .sort();

  return (
    <div className="flex flex-col gap-2">
      {programs.length === 0 ? (
        <p className="text-[12px] text-dim">{t("settings.editor.noPrograms")}</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {programs.map((prog) => (
            <li
              key={prog.id}
              className="flex flex-col gap-2 rounded border border-edge bg-panel-inactive p-2"
            >
              <div className="flex items-center gap-2">
                <AppWindow size={14} className="shrink-0 text-dim" />
                <input
                  value={prog.name}
                  onChange={(e) => updateProgram(prog.id, { name: e.target.value })}
                  spellCheck={false}
                  className="min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-0.5 text-[12px] text-text outline-none focus:border-accent"
                />
                <button
                  onClick={() => removeProgram(prog.id)}
                  className="rounded p-1 text-dim hover:bg-red-500/20 hover:text-red-400"
                  title={t("settings.editor.remove")}
                >
                  <Trash2 size={13} />
                </button>
              </div>
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-dim">
                  {prog.path}
                </span>
                <button onClick={() => changePath(prog.id)} className={smallBtn}>
                  <FolderSearch size={12} />
                  {t("settings.editor.change")}
                </button>
              </div>
              <ExtensionTags
                exts={extsOf(prog.id)}
                onAdd={(ext) => setAssociation(ext, prog.id)}
                onRemove={(ext) => setAssociation(ext, null)}
              />
            </li>
          ))}
        </ul>
      )}
      <button
        onClick={addViaBrowser}
        className="flex items-center justify-center gap-1.5 rounded border border-dashed border-edge px-2 py-1.5 text-[12px] text-dim hover:border-accent hover:text-text"
      >
        <Plus size={13} />
        {t("settings.editor.addProgram")}
      </button>
    </div>
  );
}
