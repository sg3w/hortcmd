// ============================================================
// Settings modal (Radix Dialog).
// Category tree on the left, forms on the right. The split defaults
// to 1:3 and can be dragged up to 1:1.
// ============================================================

import {
  useCallback,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import {
  GripVertical,
  Languages,
  Monitor,
  Moon,
  Palette,
  Plus,
  SlidersHorizontal,
  GitBranch,
  Star,
  Sun,
  Terminal,
  Trash2,
  Type,
  Image as ImageIcon,
  LayoutList,
  FolderTree,
  FileType,
  Lock,
  Ruler,
  Calendar,
  AppWindow,
  Pencil,
  FolderSearch,
  X,
  Files as FilesIcon,
  EyeOff,
  HardDriveDownload,
  ShieldCheck,
  Gauge,
  ListOrdered,
  Cpu,
  type LucideIcon,
} from "lucide-react";
import {
  normalizeExt,
  useSettings,
  useT,
} from "@/store/settingsStore";
import { panelOf, usePanes } from "@/store/panesStore";
import { openFileBrowser } from "@/store/fileBrowserStore";
import type { Lang, TransKey } from "@/i18n/dictionaries";
import type {
  DateFormat,
  EditorTrigger,
  Scale,
  SizeFormat,
  Theme,
} from "@/store/settingsStore";
import { cn } from "@/lib/cn";
import { AppDialog } from "@/components/ui/AppDialog";

type Category =
  | "general"
  | "view"
  | "files"
  | "operations"
  | "programs"
  | "favorites";

/** Options for the size segmentation (small/medium/large). */
const SCALE_OPTIONS: { value: Scale; key: TransKey }[] = [
  { value: "sm", key: "settings.scale.sm" },
  { value: "md", key: "settings.scale.md" },
  { value: "lg", key: "settings.scale.lg" },
];

const MIN_FRAC = 0.25; // 1:3
const MAX_FRAC = 0.5; // 1:1

interface Props {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function SettingsDialog({ open, onOpenChange }: Props) {
  const t = useT();
  const close = () => onOpenChange?.(false);
  const [category, setCategory] = useState<Category>("general");
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
      open={!!open}
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

// ---------- Form: general ----------

function GeneralForm() {
  const t = useT();
  const language = useSettings((s) => s.language);
  const setLanguage = useSettings((s) => s.setLanguage);
  const theme = useSettings((s) => s.theme);
  const setTheme = useSettings((s) => s.setTheme);
  const fontScale = useSettings((s) => s.fontScale);
  const setFontScale = useSettings((s) => s.setFontScale);
  const iconScale = useSettings((s) => s.iconScale);
  const setIconScale = useSettings((s) => s.setIconScale);
  const terminalProgram = useSettings((s) => s.terminalProgram);
  const setTerminalProgram = useSettings((s) => s.setTerminalProgram);

  const scaleOptions = SCALE_OPTIONS.map((o) => ({
    value: o.value,
    label: t(o.key),
  }));

  return (
    <div className="flex flex-col gap-5">
      <Field icon={Languages} label={t("settings.language")}>
        <Segmented<Lang>
          value={language}
          onChange={setLanguage}
          options={[
            { value: "de", label: "Deutsch" },
            { value: "en", label: "English" },
          ]}
        />
      </Field>

      <Field icon={Palette} label={t("settings.theme")}>
        <Segmented<Theme>
          value={theme}
          onChange={setTheme}
          options={[
            { value: "dark", label: t("settings.theme.dark"), Icon: Moon },
            { value: "light", label: t("settings.theme.light"), Icon: Sun },
            {
              value: "system",
              label: t("settings.theme.system"),
              Icon: Monitor,
            },
          ]}
        />
      </Field>

      <Field icon={Type} label={t("settings.fontScale")}>
        <Segmented<Scale>
          value={fontScale}
          onChange={setFontScale}
          options={scaleOptions}
        />
      </Field>

      <Field icon={ImageIcon} label={t("settings.iconScale")}>
        <Segmented<Scale>
          value={iconScale}
          onChange={setIconScale}
          options={scaleOptions}
        />
      </Field>

      <Field icon={Terminal} label={t("settings.terminal")}>
        <input
          value={terminalProgram}
          onChange={(e) => setTerminalProgram(e.target.value)}
          placeholder={t("settings.terminal.placeholder")}
          spellCheck={false}
          className="w-full rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
        />
        <span className="text-[11px] text-dim">{t("settings.terminal.hint")}</span>
      </Field>
    </div>
  );
}

// ---------- Form: file view ----------

function ViewForm() {
  const t = useT();
  const foldersFirst = useSettings((s) => s.foldersFirst);
  const setFoldersFirst = useSettings((s) => s.setFoldersFirst);
  const showExt = useSettings((s) => s.showExtColumn);
  const setShowExt = useSettings((s) => s.setShowExtColumn);
  const showPerms = useSettings((s) => s.showPermissions);
  const setShowPerms = useSettings((s) => s.setShowPermissions);
  const sizeFormat = useSettings((s) => s.sizeFormat);
  const setSizeFormat = useSettings((s) => s.setSizeFormat);
  const dateFormat = useSettings((s) => s.dateFormat);
  const setDateFormat = useSettings((s) => s.setDateFormat);
  const rebuildAll = usePanes((s) => s.rebuildAll);

  return (
    <div className="flex flex-col gap-5">
      <Toggle
        checked={foldersFirst}
        onChange={(v) => {
          setFoldersFirst(v);
          rebuildAll(); // rebuild the sorting immediately.
        }}
        icon={FolderTree}
        label={t("settings.foldersFirst")}
        hint={t("settings.foldersFirst.hint")}
      />
      <Toggle
        checked={showExt}
        onChange={setShowExt}
        icon={FileType}
        label={t("settings.showExt")}
        hint={t("settings.showExt.hint")}
      />
      <Toggle
        checked={showPerms}
        onChange={setShowPerms}
        icon={Lock}
        label={t("settings.showPerms")}
        hint={t("settings.showPerms.hint")}
      />

      <Field icon={Ruler} label={t("settings.sizeFormat")}>
        <Segmented<SizeFormat>
          value={sizeFormat}
          onChange={setSizeFormat}
          options={[
            { value: "auto", label: t("settings.sizeFormat.auto") },
            { value: "bytes", label: t("settings.sizeFormat.bytes") },
          ]}
        />
      </Field>

      <Field icon={Calendar} label={t("settings.dateFormat")}>
        <Segmented<DateFormat>
          value={dateFormat}
          onChange={setDateFormat}
          options={[
            { value: "medium", label: t("settings.dateFormat.medium") },
            { value: "short", label: t("settings.dateFormat.short") },
            { value: "iso", label: t("settings.dateFormat.iso") },
          ]}
        />
      </Field>
    </div>
  );
}

// ---------- Form: files ----------

function FilesForm() {
  const t = useT();
  const hide = useSettings((s) => s.hideSystemFiles);
  const setHide = useSettings((s) => s.setHideSystemFiles);
  const gitEnabled = useSettings((s) => s.gitEnabled);
  const setGitEnabled = useSettings((s) => s.setGitEnabled);
  const rebuildAll = usePanes((s) => s.rebuildAll);
  const reloadGit = usePanes((s) => s.reloadGit);

  return (
    <div className="flex flex-col gap-4">
      <Toggle
        checked={hide}
        onChange={(v) => {
          setHide(v);
          rebuildAll(); // rebuild the displayed lists immediately.
        }}
        icon={EyeOff}
        label={t("settings.hideSystem")}
        hint={t("settings.hideSystem.hint")}
      />
      <Toggle
        checked={gitEnabled}
        onChange={(v) => {
          setGitEnabled(v);
          reloadGit(); // reload/clear the git status in both windows.
        }}
        icon={GitBranch}
        label={t("settings.git")}
        hint={t("settings.git.hint")}
      />
    </div>
  );
}

// ---------- Form: file operations ----------

function OperationsForm() {
  const t = useT();
  const useTrash = useSettings((s) => s.useTrash);
  const setUseTrash = useSettings((s) => s.setUseTrash);
  const queueTransfers = useSettings((s) => s.queueTransfers);
  const setQueueTransfers = useSettings((s) => s.setQueueTransfers);
  const verifyCopies = useSettings((s) => s.verifyCopies);
  const setVerifyCopies = useSettings((s) => s.setVerifyCopies);
  const speedLimit = useSettings((s) => s.speedLimit);
  const setSpeedLimit = useSettings((s) => s.setSpeedLimit);
  const bufferSizeKb = useSettings((s) => s.bufferSizeKb);
  const setBufferSizeKb = useSettings((s) => s.setBufferSizeKb);
  const copyThreads = useSettings((s) => s.copyThreads);
  const setCopyThreads = useSettings((s) => s.setCopyThreads);

  return (
    <div className="flex flex-col gap-4">
      <Toggle
        checked={useTrash}
        onChange={setUseTrash}
        icon={Trash2}
        label={t("settings.trash")}
        hint={t("settings.trash.hint")}
      />
      <Toggle
        checked={queueTransfers}
        onChange={setQueueTransfers}
        icon={ListOrdered}
        label={t("settings.queue")}
        hint={t("settings.queue.hint")}
      />
      <Toggle
        checked={verifyCopies}
        onChange={setVerifyCopies}
        icon={ShieldCheck}
        label={t("settings.verify")}
        hint={t("settings.verify.hint")}
      />
      {/* Speed limit (0 = unlimited). */}
      <label className="flex items-start gap-3">
        <Gauge size={15} className="mt-0.5 shrink-0 text-dim" />
        <span className="flex flex-col gap-1">
          <span className="text-[13px] text-text">{t("settings.speed")}</span>
          <span className="text-[11px] text-dim">{t("settings.speed.hint")}</span>
          <div className="mt-1 flex items-center gap-2">
            <input
              type="number"
              min={0}
              value={speedLimit}
              onChange={(e) =>
                setSpeedLimit(Math.max(0, Math.floor(Number(e.target.value) || 0)))
              }
              className="w-28 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
            />
            <span className="text-[12px] text-dim">KB/s</span>
          </div>
        </span>
      </label>

      {/* Buffer size (KB). */}
      <label className="flex items-start gap-3">
        <HardDriveDownload size={15} className="mt-0.5 shrink-0 text-dim" />
        <span className="flex flex-col gap-1">
          <span className="text-[13px] text-text">{t("settings.buffer")}</span>
          <span className="text-[11px] text-dim">{t("settings.buffer.hint")}</span>
          <div className="mt-1 flex items-center gap-2">
            <input
              type="number"
              min={0}
              value={bufferSizeKb}
              onChange={(e) => setBufferSizeKb(Number(e.target.value) || 0)}
              className="w-28 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
            />
            <span className="text-[12px] text-dim">KB</span>
          </div>
        </span>
      </label>

      {/* Parallel copy threads (1 = sequential). */}
      <label className="flex items-start gap-3">
        <Cpu size={15} className="mt-0.5 shrink-0 text-dim" />
        <span className="flex flex-col gap-1">
          <span className="text-[13px] text-text">{t("settings.threads")}</span>
          <span className="text-[11px] text-dim">
            {t("settings.threads.hint")}
          </span>
          <div className="mt-1 flex items-center gap-2">
            <input
              type="number"
              min={1}
              max={16}
              value={copyThreads}
              onChange={(e) => setCopyThreads(Number(e.target.value) || 1)}
              className="w-28 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
            />
          </div>
        </span>
      </label>
    </div>
  );
}

function Toggle({
  checked,
  onChange,
  icon: Icon,
  label,
  hint,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  icon: LucideIcon;
  label: string;
  hint?: string;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <button
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={cn(
          "mt-0.5 flex h-5 w-9 shrink-0 items-center rounded-full p-0.5 transition",
          checked ? "justify-end bg-accent" : "justify-start bg-cursor-inactive",
        )}
      >
        <span className="h-4 w-4 rounded-full bg-white" />
      </button>
      <span className="flex items-center gap-2">
        <Icon size={15} className="text-dim" />
        <span className="flex flex-col">
          <span className="text-[13px] text-text">{label}</span>
          {hint && <span className="text-[11px] text-dim">{hint}</span>}
        </span>
      </span>
    </label>
  );
}

// ---------- Form: open with / editors ----------

const smallBtn =
  "flex items-center gap-1 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text hover:border-accent";

/** Derive the program name from a path (.app/extension removed). */
function programNameFromPath(path: string): string {
  const base = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
  return base.replace(/\.app$/i, "").replace(/\.[^.]+$/, "") || base;
}

function ProgramsForm() {
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

/** Program selection (native select over the configured programs). */
function ProgramSelect({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const t = useT();
  const programs = useSettings((s) => s.programs);
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="min-w-0 flex-1 rounded border border-edge bg-panel px-1.5 py-1 text-[12px] text-text outline-none focus:border-accent"
    >
      <option value="">{t("settings.editor.none")}</option>
      {programs.map((p) => (
        <option key={p.id} value={p.id}>
          {p.name}
        </option>
      ))}
    </select>
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

/** A program's extension chips with add/remove. */
function ExtensionTags({
  exts,
  onAdd,
  onRemove,
}: {
  exts: string[];
  onAdd: (ext: string) => void;
  onRemove: (ext: string) => void;
}) {
  const t = useT();
  const [val, setVal] = useState("");
  const add = () => {
    const ext = normalizeExt(val);
    if (ext) onAdd(ext);
    setVal("");
  };
  return (
    <div className="flex flex-wrap items-center gap-1">
      {exts.map((ext) => (
        <span
          key={ext}
          className="flex items-center gap-1 rounded bg-header px-1.5 py-0.5 font-mono text-[11px] text-text"
        >
          .{ext}
          <button
            onClick={() => onRemove(ext)}
            className="text-dim hover:text-red-400"
            title={t("settings.editor.remove")}
          >
            <X size={11} />
          </button>
        </span>
      ))}
      <input
        value={val}
        onChange={(e) => setVal(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            add();
          }
        }}
        placeholder={t("settings.editor.extPlaceholder")}
        spellCheck={false}
        className="w-16 rounded border border-edge bg-panel px-1.5 py-0.5 font-mono text-[11px] text-text outline-none focus:border-accent"
      />
    </div>
  );
}

// ---------- Form: favorites ----------

function basename(path: string): string {
  if (!path || path === "/") return path || "/";
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function FavoritesForm() {
  const t = useT();
  const favorites = useSettings((s) => s.favorites);
  const addFavorite = useSettings((s) => s.addFavorite);
  const removeFavorite = useSettings((s) => s.removeFavorite);
  const currentPath = usePanes((s) => panelOf(s, s.active).path);

  const [path, setPath] = useState("");
  const [name, setName] = useState("");

  const effectivePath = (path || currentPath).trim();
  const effectiveName = (name || basename(effectivePath)).trim();

  const add = () => {
    if (!effectivePath) return;
    addFavorite({ name: effectiveName || effectivePath, path: effectivePath });
    setPath("");
    setName("");
  };

  return (
    <div className="flex flex-col gap-4">
      <p className="text-[11px] text-dim">{t("settings.favorites.hint")}</p>

      {/* Existing favorites */}
      {favorites.length === 0 ? (
        <p className="text-[12px] text-dim">{t("settings.favorites.empty")}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {favorites.map((f) => (
            <li
              key={f.path}
              className="flex items-center gap-2 rounded border border-edge bg-panel-inactive px-2 py-1"
            >
              <Star size={14} className="shrink-0 text-amber-400" />
              <div className="flex min-w-0 flex-col leading-tight">
                <span className="truncate text-[12px] text-text">{f.name}</span>
                <span className="truncate font-mono text-[10px] text-dim">
                  {f.path}
                </span>
              </div>
              <button
                onClick={() => removeFavorite(f.path)}
                className="ml-auto rounded p-1 text-dim hover:bg-red-500/20 hover:text-red-400"
                title={t("op.cancel")}
              >
                <Trash2 size={13} />
              </button>
            </li>
          ))}
        </ul>
      )}

      {/* Add a new favorite */}
      <div className="flex flex-col gap-2 border-t border-edge pt-3">
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder={`${t("settings.favorites.path")} (${currentPath})`}
          spellCheck={false}
          className="rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
        />
        <div className="flex gap-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={`${t("settings.favorites.name")} (${basename(effectivePath)})`}
            spellCheck={false}
            className="min-w-0 flex-1 rounded border border-edge bg-panel-inactive px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
          />
          <button
            onClick={add}
            className="flex items-center gap-1.5 rounded bg-accent px-3 py-1 text-[12px] text-white hover:brightness-110"
          >
            <Plus size={13} />
            {t("settings.favorites.add")}
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------- small building blocks ----------

function Field({
  icon: Icon,
  label,
  children,
}: {
  icon: LucideIcon;
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <span className="flex items-center gap-2 text-[13px] text-text">
        <Icon size={15} className="text-dim" />
        {label}
      </span>
      {children}
    </div>
  );
}

function Segmented<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string; Icon?: LucideIcon }[];
}) {
  return (
    <div className="inline-flex overflow-hidden rounded border border-edge">
      {options.map((o) => (
        <button
          key={o.value}
          onClick={() => onChange(o.value)}
          className={cn(
            "flex items-center gap-1.5 px-3 py-1 text-[12px]",
            value === o.value
              ? "bg-accent text-white"
              : "bg-panel text-dim hover:text-text",
          )}
        >
          {o.Icon && <o.Icon size={13} />}
          {o.label}
        </button>
      ))}
    </div>
  );
}
