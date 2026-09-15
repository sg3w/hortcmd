// ============================================================
// App settings (language, theme, system files) with
// localStorage persistence.
//
// The persisted shape, its enums and its bounds are in
// `settingsSchema.ts`; the versioning, migration and validation of what
// comes back out of storage are in `persisted/settings.ts`
// (TICKET-039). This file is the store and its setters.
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { guardedStorage } from "./persisted/storage";
import { acceptVersion, PERSISTED_KEYS, rehydrate } from "./persisted/gate";
import {
  migrateSettings,
  SETTINGS_VERSION,
  validateSettings,
} from "./persisted/settings";
import { DICT, type Lang, type TransKey } from "@/i18n/dictionaries";
import type { CustomColorRule, ThemeMode } from "@/lib/fileColors";
import {
  DEFAULT_COLUMN_WIDTHS,
  MAX_COLUMN_WIDTH,
  MIN_COLUMN_WIDTH,
  normalizeExt,
  type DateFormat,
  type EditorProgram,
  type EditorTrigger,
  type Favorite,
  type PersistedSettings,
  type Scale,
  type SizeFormat,
  type Theme,
} from "./settingsSchema";

export * from "./settingsSchema";

interface SettingsStore extends PersistedSettings {
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
  setCheckTransferSpace: (v: boolean) => void;
  setSafeTransfers: (v: boolean) => void;
  setFoldersFirst: (v: boolean) => void;
  setShowExtColumn: (v: boolean) => void;
  setShowPermissions: (v: boolean) => void;
  setSizeFormat: (v: SizeFormat) => void;
  setDateFormat: (v: DateFormat) => void;
  setFontScale: (v: Scale) => void;
  setFileIcons: (v: "system" | "simple") => void;
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
      safeTransfers: true,
      checkTransferSpace: true,

      foldersFirst: true,
      showExtColumn: true,
      showPermissions: false,
      sizeFormat: "auto",
      dateFormat: "medium",
      fontScale: "md",
      iconScale: "md",
      fileIcons: "system",
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
      setCheckTransferSpace: (checkTransferSpace) => set({ checkTransferSpace }),
      setSafeTransfers: (safeTransfers) => set({ safeTransfers }),
      setFoldersFirst: (foldersFirst) => set({ foldersFirst }),
      setShowExtColumn: (showExtColumn) => set({ showExtColumn }),
      setShowPermissions: (showPermissions) => set({ showPermissions }),
      setSizeFormat: (sizeFormat) => set({ sizeFormat }),
      setDateFormat: (dateFormat) => set({ dateFormat }),
      setFontScale: (fontScale) => set({ fontScale }),
      setFileIcons: (fileIcons) => set({ fileIcons }),
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
    {
      name: PERSISTED_KEYS.settings,
      version: SETTINGS_VERSION,
      storage: guardedStorage("settings"),
      // `migrate` reshapes; `merge` validates. Both run, and `merge`
      // runs even when the version already matched — a stored value can
      // be out of range without the schema ever having changed
      // (TICKET-039).
      migrate: (stored, from) =>
        acceptVersion("settings", from, SETTINGS_VERSION)
          ? migrateSettings(stored, from)
          : {},
      merge: (stored, current) => ({
        ...current,
        ...rehydrate("settings", stored, validateSettings),
      }),
    },
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
