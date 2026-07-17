// ============================================================
// App settings (language, theme, system files) with
// localStorage persistence.
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { DICT, type Lang, type TransKey } from "@/i18n/dictionaries";
import type {
  CustomColorRule,
  FileColorOverrides,
  ThemeMode,
} from "@/lib/fileColors";

export type Theme = "dark" | "light" | "system";
/** Size step for the font or icons in the file list. */
export type Scale = "sm" | "md" | "lg";
/** Display format of the size column. */
export type SizeFormat = "auto" | "bytes";
/** Display format of the date column. */
export type DateFormat = "medium" | "short" | "iso";

/** Default widths (px) of the fixed-width columns in the detail view. */
export const DEFAULT_COLUMN_WIDTHS: Record<string, number> = {
  ext: 60,
  perms: 100,
  size: 90,
  date: 130,
};

/** Limits for drag-adjustable column widths (px). */
export const MIN_COLUMN_WIDTH = 40;
export const MAX_COLUMN_WIDTH = 600;

export interface Favorite {
  name: string;
  path: string;
}

/** A user-created program for opening files. */
export interface EditorProgram {
  id: string;
  name: string;
  path: string;
}

/** How "Open in editor" (the mapping) is triggered via the keyboard. */
export type EditorTrigger = "shiftF4" | "contextOnly" | "f4";

interface SettingsStore {
  language: Lang;
  theme: Theme;
  hideSystemFiles: boolean;
  /** Share of the width taken by the left pane (0.15–0.85). */
  paneSplit: number;
  favorites: Favorite[];
  /** Terminal program for "Open in terminal" (empty = system default). */
  terminalProgram: string;
  /** Show the Git status in the file list (colors + branch badge). */
  gitEnabled: boolean;
  /** Move to the trash when deleting (Shift = permanent). */
  useTrash: boolean;

  // ----- Transfers (copy/move) -----
  /** Verify copies against the source via SHA-256 after writing. */
  verifyCopies: boolean;
  /** Speed limit in KB/s (0 = unlimited). */
  speedLimit: number;
  /** Process copy/move operations one after another (queue). */
  queueTransfers: boolean;
  /** Copy buffer size in KB (default 256). */
  bufferSizeKb: number;
  /** Number of parallel copy threads (1 = sequential). */
  copyThreads: number;

  // ----- File view -----
  /** Always sort folders before files. */
  foldersFirst: boolean;
  /** Show the extension as its own column (off = full name in the name column). */
  showExtColumn: boolean;
  /** Show the permissions column (rwxr-xr-x). */
  showPermissions: boolean;
  /** Format of the size column. */
  sizeFormat: SizeFormat;
  /** Format of the date column. */
  dateFormat: DateFormat;
  /** Font size of the file list. */
  fontScale: Scale;
  /** Icon size of the file list. */
  iconScale: Scale;
  /** Drag-adjusted column widths (px) per column id. */
  columnWidths: Record<string, number>;

  // ----- File colors (see lib/fileColors.ts) -----
  /** Color overrides per slot id; missing/invalid → registry default. */
  fileColors: FileColorOverrides;
  /** User rules, matched before the built-in rules in list order. */
  customColorRules: CustomColorRule[];

  // ----- Open with / editors -----
  /** Programs created by the user. */
  programs: EditorProgram[];
  /** Mapping extension (lowercase, without the dot) → program id. */
  associations: Record<string, string>;
  /** Global default editor (path; empty = none → system default). */
  defaultEditor: string;
  /** Trigger behavior for "Open in editor". */
  editorTrigger: EditorTrigger;

  setLanguage: (l: Lang) => void;
  setTheme: (t: Theme) => void;
  setHideSystemFiles: (v: boolean) => void;
  setPaneSplit: (v: number) => void;
  addFavorite: (fav: Favorite) => void;
  removeFavorite: (path: string) => void;
  setTerminalProgram: (v: string) => void;
  setGitEnabled: (v: boolean) => void;
  setUseTrash: (v: boolean) => void;
  setVerifyCopies: (v: boolean) => void;
  setSpeedLimit: (v: number) => void;
  setQueueTransfers: (v: boolean) => void;
  setBufferSizeKb: (v: number) => void;
  setCopyThreads: (v: number) => void;
  setFoldersFirst: (v: boolean) => void;
  setShowExtColumn: (v: boolean) => void;
  setShowPermissions: (v: boolean) => void;
  setSizeFormat: (v: SizeFormat) => void;
  setDateFormat: (v: DateFormat) => void;
  setFontScale: (v: Scale) => void;
  setIconScale: (v: Scale) => void;
  setColumnWidth: (id: string, width: number) => void;

  setFileColor: (id: string, theme: ThemeMode, color: string | null) => void;
  /** Drops all overrides → the registry defaults apply again. */
  resetFileColors: () => void;
  addColorRule: () => void;
  updateColorRule: (
    id: string,
    patch: Partial<Omit<CustomColorRule, "id">>,
  ) => void;
  removeColorRule: (id: string) => void;
  /** Moves a rule by `delta` positions (changes its priority). */
  moveColorRule: (id: string, delta: number) => void;

  addProgram: (name: string, path: string) => string;
  updateProgram: (id: string, patch: Partial<Omit<EditorProgram, "id">>) => void;
  removeProgram: (id: string) => void;
  setAssociation: (ext: string, programId: string | null) => void;
  setProgramExtensions: (id: string, exts: string[]) => void;
  setDefaultEditor: (path: string) => void;
  setEditorTrigger: (v: EditorTrigger) => void;
}

/** Normalizes an extension: lowercase, without a leading dot/whitespace. */
export function normalizeExt(ext: string): string {
  return ext.trim().replace(/^\.+/, "").toLowerCase();
}

export const useSettings = create<SettingsStore>()(
  persist(
    (set) => ({
      language: "de",
      theme: "dark",
      hideSystemFiles: true,
      paneSplit: 0.5,
      favorites: [],
      terminalProgram: "",
      gitEnabled: true,
      useTrash: true,
      verifyCopies: false,
      speedLimit: 0,
      queueTransfers: true,
      bufferSizeKb: 256,
      copyThreads: 1,

      foldersFirst: true,
      showExtColumn: true,
      showPermissions: false,
      sizeFormat: "auto",
      dateFormat: "medium",
      fontScale: "md",
      iconScale: "md",
      columnWidths: { ...DEFAULT_COLUMN_WIDTHS },
      fileColors: {},
      customColorRules: [],
      programs: [],
      associations: {},
      defaultEditor: "",
      editorTrigger: "shiftF4",

      setLanguage: (language) => set({ language }),
      setTheme: (theme) => set({ theme }),
      setHideSystemFiles: (hideSystemFiles) => set({ hideSystemFiles }),
      setPaneSplit: (paneSplit) =>
        set({ paneSplit: Math.min(0.85, Math.max(0.15, paneSplit)) }),
      addFavorite: (fav) =>
        set((s) =>
          s.favorites.some((f) => f.path === fav.path)
            ? s
            : { favorites: [...s.favorites, fav] },
        ),
      removeFavorite: (path) =>
        set((s) => ({ favorites: s.favorites.filter((f) => f.path !== path) })),
      setTerminalProgram: (terminalProgram) => set({ terminalProgram }),
      setGitEnabled: (gitEnabled) => set({ gitEnabled }),
      setUseTrash: (useTrash) => set({ useTrash }),
      setVerifyCopies: (verifyCopies) => set({ verifyCopies }),
      setSpeedLimit: (speedLimit) => set({ speedLimit: Math.max(0, speedLimit) }),
      setQueueTransfers: (queueTransfers) => set({ queueTransfers }),
      setBufferSizeKb: (bufferSizeKb) =>
        set({ bufferSizeKb: Math.min(16384, Math.max(0, Math.floor(bufferSizeKb))) }),
      setCopyThreads: (copyThreads) =>
        set({ copyThreads: Math.min(16, Math.max(1, Math.floor(copyThreads))) }),
      setFoldersFirst: (foldersFirst) => set({ foldersFirst }),
      setShowExtColumn: (showExtColumn) => set({ showExtColumn }),
      setShowPermissions: (showPermissions) => set({ showPermissions }),
      setSizeFormat: (sizeFormat) => set({ sizeFormat }),
      setDateFormat: (dateFormat) => set({ dateFormat }),
      setFontScale: (fontScale) => set({ fontScale }),
      setIconScale: (iconScale) => set({ iconScale }),
      setColumnWidth: (id, width) =>
        set((s) => ({
          columnWidths: {
            ...s.columnWidths,
            [id]: Math.max(
              MIN_COLUMN_WIDTH,
              Math.min(MAX_COLUMN_WIDTH, Math.round(width)),
            ),
          },
        })),

      setFileColor: (id, theme, color) =>
        set((s) => {
          const slot = { ...s.fileColors[id] };
          if (color) slot[theme] = color;
          else delete slot[theme];
          const fileColors = { ...s.fileColors };
          // Drop the slot entirely once it holds no override anymore.
          if (Object.keys(slot).length > 0) fileColors[id] = slot;
          else delete fileColors[id];
          return { fileColors };
        }),
      resetFileColors: () => set({ fileColors: {} }),
      addColorRule: () =>
        set((s) => ({
          customColorRules: [
            ...s.customColorRules,
            {
              id: crypto.randomUUID(),
              name: "",
              pattern: "",
              dark: "#f5a43c",
              light: "#c2410c",
            },
          ],
        })),
      updateColorRule: (id, patch) =>
        set((s) => ({
          customColorRules: s.customColorRules.map((r) =>
            r.id === id ? { ...r, ...patch } : r,
          ),
        })),
      removeColorRule: (id) =>
        set((s) => ({
          customColorRules: s.customColorRules.filter((r) => r.id !== id),
        })),
      moveColorRule: (id, delta) =>
        set((s) => {
          const rules = [...s.customColorRules];
          const from = rules.findIndex((r) => r.id === id);
          const to = from + delta;
          if (from < 0 || to < 0 || to >= rules.length) return s;
          [rules[from], rules[to]] = [rules[to], rules[from]];
          return { customColorRules: rules };
        }),

      addProgram: (name, path) => {
        const id = crypto.randomUUID();
        set((s) => ({ programs: [...s.programs, { id, name, path }] }));
        return id;
      },
      updateProgram: (id, patch) =>
        set((s) => ({
          programs: s.programs.map((p) =>
            p.id === id ? { ...p, ...patch } : p,
          ),
        })),
      removeProgram: (id) =>
        set((s) => ({
          programs: s.programs.filter((p) => p.id !== id),
          // Remove mappings to this program as well.
          associations: Object.fromEntries(
            Object.entries(s.associations).filter(([, v]) => v !== id),
          ),
        })),
      setAssociation: (ext, programId) =>
        set((s) => {
          const key = normalizeExt(ext);
          if (!key) return s;
          const associations = { ...s.associations };
          if (programId) associations[key] = programId;
          else delete associations[key];
          return { associations };
        }),
      setProgramExtensions: (id, exts) =>
        set((s) => {
          const wanted = new Set(exts.map(normalizeExt).filter(Boolean));
          const associations: Record<string, string> = {};
          // Keep existing mappings of other programs.
          for (const [ext, pid] of Object.entries(s.associations)) {
            if (pid !== id) associations[ext] = pid;
          }
          // Map the desired extensions to this program (overrides foreign ones).
          for (const ext of wanted) associations[ext] = id;
          return { associations };
        }),
      setDefaultEditor: (defaultEditor) => set({ defaultEditor }),
      setEditorTrigger: (editorTrigger) => set({ editorTrigger }),
    }),
    { name: "hortcmd-settings" },
  ),
);

/** The program mapped to an extension (or undefined). */
export function programForExtension(ext: string): EditorProgram | undefined {
  const s = useSettings.getState();
  const id = s.associations[normalizeExt(ext)];
  return id ? s.programs.find((p) => p.id === id) : undefined;
}

/** Translation outside of React (e.g. in actions). */
export function translate(key: TransKey): string {
  return DICT[useSettings.getState().language][key];
}

/** React hook: returns a t() function bound to the current language. */
export function useT(): (key: TransKey) => string {
  const lang = useSettings((s) => s.language);
  return (key: TransKey) => DICT[lang][key];
}
