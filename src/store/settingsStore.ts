// ============================================================
// App-Einstellungen (Sprache, Theme, Systemdateien) mit
// localStorage-Persistenz.
// ============================================================

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { DICT, type Lang, type TransKey } from "@/i18n/dictionaries";

export type Theme = "dark" | "light" | "system";
/** Größenstufe für Schrift bzw. Symbole in der Dateiliste. */
export type Scale = "sm" | "md" | "lg";
/** Anzeigeformat der Größenspalte. */
export type SizeFormat = "auto" | "bytes";
/** Anzeigeformat der Datumsspalte. */
export type DateFormat = "medium" | "short" | "iso";

/** Standardbreiten (px) der Spalten mit fester Breite in der Detailansicht. */
export const DEFAULT_COLUMN_WIDTHS: Record<string, number> = {
  ext: 60,
  perms: 100,
  size: 90,
  date: 130,
};

/** Grenzen für per Ziehen einstellbare Spaltenbreiten (px). */
export const MIN_COLUMN_WIDTH = 40;
export const MAX_COLUMN_WIDTH = 600;

export interface Favorite {
  name: string;
  path: string;
}

/** Ein vom Nutzer angelegtes Programm zum Öffnen von Dateien. */
export interface EditorProgram {
  id: string;
  name: string;
  path: string;
}

/** Wie „Im Editor öffnen" (Zuordnung) über die Tastatur ausgelöst wird. */
export type EditorTrigger = "shiftF4" | "contextOnly" | "f4";

interface SettingsStore {
  language: Lang;
  theme: Theme;
  hideSystemFiles: boolean;
  /** Anteil des linken Fensters an der Breite (0.15–0.85). */
  paneSplit: number;
  favorites: Favorite[];
  /** Terminal-Programm für „Im Terminal öffnen" (leer = System-Standard). */
  terminalProgram: string;
  /** Git-Status in der Dateiliste anzeigen (Farben + Branch-Badge). */
  gitEnabled: boolean;
  /** Beim Löschen in den Papierkorb verschieben (Shift = endgültig). */
  useTrash: boolean;

  // ----- Transfers (Kopieren/Verschieben) -----
  /** Kopien nach dem Schreiben per SHA-256 gegen die Quelle prüfen. */
  verifyCopies: boolean;
  /** Geschwindigkeitslimit in KB/s (0 = unbegrenzt). */
  speedLimit: number;
  /** Kopier-/Verschiebevorgänge nacheinander abarbeiten (Warteschlange). */
  queueTransfers: boolean;
  /** Kopier-Puffergröße in KB (Standard 256). */
  bufferSizeKb: number;
  /** Anzahl paralleler Kopier-Threads (1 = sequenziell). */
  copyThreads: number;

  // ----- Dateiansicht -----
  /** Ordner beim Sortieren immer vor Dateien einordnen. */
  foldersFirst: boolean;
  /** Endung als eigene Spalte zeigen (aus = voller Name in der Namensspalte). */
  showExtColumn: boolean;
  /** Rechte-Spalte (rwxr-xr-x) anzeigen. */
  showPermissions: boolean;
  /** Format der Größenspalte. */
  sizeFormat: SizeFormat;
  /** Format der Datumsspalte. */
  dateFormat: DateFormat;
  /** Schriftgröße der Dateiliste. */
  fontScale: Scale;
  /** Symbolgröße der Dateiliste. */
  iconScale: Scale;
  /** Per Ziehen eingestellte Spaltenbreiten (px) je Spalten-id. */
  columnWidths: Record<string, number>;

  // ----- Öffnen mit / Editoren -----
  /** Vom Nutzer angelegte Programme. */
  programs: EditorProgram[];
  /** Zuordnung Endung (klein, ohne Punkt) → Programm-id. */
  associations: Record<string, string>;
  /** Globaler Standard-Editor (Pfad; leer = keiner → Systemstandard). */
  defaultEditor: string;
  /** Auslöse-Verhalten für „Im Editor öffnen". */
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

  addProgram: (name: string, path: string) => string;
  updateProgram: (id: string, patch: Partial<Omit<EditorProgram, "id">>) => void;
  removeProgram: (id: string) => void;
  setAssociation: (ext: string, programId: string | null) => void;
  setProgramExtensions: (id: string, exts: string[]) => void;
  setDefaultEditor: (path: string) => void;
  setEditorTrigger: (v: EditorTrigger) => void;
}

/** Normalisiert eine Endung: klein, ohne führenden Punkt/Leerraum. */
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
          // Zuordnungen auf dieses Programm mitentfernen.
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
          // Bestehende Zuordnungen anderer Programme behalten.
          for (const [ext, pid] of Object.entries(s.associations)) {
            if (pid !== id) associations[ext] = pid;
          }
          // Gewünschte Endungen diesem Programm zuordnen (überschreibt fremde).
          for (const ext of wanted) associations[ext] = id;
          return { associations };
        }),
      setDefaultEditor: (defaultEditor) => set({ defaultEditor }),
      setEditorTrigger: (editorTrigger) => set({ editorTrigger }),
    }),
    { name: "hortcmd-settings" },
  ),
);

/** Das einer Endung zugeordnete Programm (oder undefined). */
export function programForExtension(ext: string): EditorProgram | undefined {
  const s = useSettings.getState();
  const id = s.associations[normalizeExt(ext)];
  return id ? s.programs.find((p) => p.id === id) : undefined;
}

/** Übersetzung außerhalb von React (z. B. in Aktionen). */
export function translate(key: TransKey): string {
  return DICT[useSettings.getState().language][key];
}

/** React-Hook: liefert eine an die aktuelle Sprache gebundene t()-Funktion. */
export function useT(): (key: TransKey) => string {
  const lang = useSettings((s) => s.language);
  return (key: TransKey) => DICT[lang][key];
}
