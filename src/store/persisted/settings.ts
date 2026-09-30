import { migrateLegacyLocation } from "@/lib/location";
// ============================================================
// The settings schema, versioned (TICKET-039).
//
// `hortcmd-settings` holds everything the user has ever configured, and
// it is rehydrated straight into the store — past every clamping setter.
// This file is the gate: one declared version, one migration chain, one
// validator that runs on *every* load regardless of version, because a
// value can be wrong without the schema having changed.
//
// Adding a setting: give it a default in `settingsStore.ts` and a line
// here. An absent value is not a repair — that is simply an older
// installation meeting a newer field, which is what the default is for.
// ============================================================

import type { ThemeMode } from "@/lib/fileColors";
import {
  bool,
  color,
  list,
  num,
  obj,
  oneOf,
  optionalColor,
  record,
  Repairs,
  str,
  unique,
} from "@/lib/schema";
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
} from "@/store/settingsSchema";
import type { ArchiveNameOptions } from "@/features/commander/archiveName";
import type { Lang } from "@/i18n/dictionaries";

/**
 * The current schema version.
 *
 * 0 — everything up to TICKET-038: no version recorded, `safeTransfers`
 *     shipped as `false` although TICKET-029 specified it as the
 *     default-on protection.
 * 1 — versioned. `safeTransfers` becomes `true` on the way in.
 */
export const SETTINGS_VERSION = 2;

const THEMES: readonly Theme[] = ["dark", "light", "system"];
const SCALES: readonly Scale[] = ["sm", "md", "lg"];
const SIZE_FORMATS: readonly SizeFormat[] = ["auto", "bytes"];
const DATE_FORMATS: readonly DateFormat[] = ["medium", "short", "iso"];
const EDITOR_TRIGGERS: readonly EditorTrigger[] = ["shiftF4", "contextOnly", "f4"];
const ARCHIVE_BASES: readonly ArchiveNameOptions["base"][] = ["selection", "generic"];
const LANGS: readonly Lang[] = ["de", "en"];
const THEME_MODES: readonly ThemeMode[] = ["dark", "light"];

/** Caps. Generous for a person, small enough that a loop cannot fill the quota. */
const MAX_FAVORITES = 200;
const MAX_COLOR_RULES = 200;
const MAX_PROGRAMS = 100;
const MAX_ASSOCIATIONS = 500;
const MAX_PATH = 4096;

/**
 * Brings an older stored shape up to [`SETTINGS_VERSION`].
 *
 * Migrations are pure, ordered and idempotent — running the chain twice
 * produces the same object — and they only reshape. Anything about
 * *validity* belongs in [`validateSettings`], which runs afterwards in
 * every case, including when no migration was needed.
 */
export function migrateSettings(
  stored: unknown,
  from: number,
): Record<string, unknown> {
  const state = { ...obj(stored) };

  // 0 → 1: failure-safe transfers.
  //
  // TICKET-029 specified this as on by default, for new *and* existing
  // installations, and shipped it as `false`. Which means no stored
  // `false` from before this version was ever a decision: it is the
  // wrong default, recorded. Everyone arriving from v0 therefore gets
  // the protection the ticket asked for, and the setting says plainly
  // what it is doing — the File-operations panel shows an amber warning
  // while it is off, so a user who really wants direct writes turns it
  // back off and sees why.
  if (from < 1) {
    state.safeTransfers = true;
  }

  if (from < 2 && Array.isArray(state.favorites)) {
    state.favorites = state.favorites.map((entry: unknown) => {
      const favorite = obj(entry);
      if (typeof favorite.path !== "string") return entry;
      try { return { ...favorite, path: migrateLegacyLocation(favorite.path) }; }
      catch { return entry; }
    });
  }
  return state;
}

function favorite(v: unknown, r: Repairs): Favorite | null {
  const o = obj(v);
  const path = str(o.path, "", "favorites.path", r, MAX_PATH);
  if (!path) return null;
  return { name: str(o.name, path, "favorites.name", r, 200), path };
}

function program(v: unknown, r: Repairs): EditorProgram | null {
  const o = obj(v);
  const id = str(o.id, "", "programs.id", r, 100);
  const path = str(o.path, "", "programs.path", r, MAX_PATH);
  // Without an id it cannot be referenced, removed or edited; without a
  // path it cannot be run. Either way there is nothing to repair it to.
  if (!id || !path) return null;
  return { id, name: str(o.name, path, "programs.name", r, 200), path };
}

function colorRule(v: unknown, r: Repairs) {
  const o = obj(v);
  const id = str(o.id, "", "customColorRules.id", r, 100);
  if (!id) return null;
  return {
    id,
    name: str(o.name, "", "customColorRules.name", r, 200),
    pattern: str(o.pattern, "", "customColorRules.pattern", r, 500),
    dark: color(o.dark, "#f5a43c", "customColorRules.dark", r),
    light: color(o.light, "#c2410c", "customColorRules.light", r),
  };
}

/**
 * The pack dialog's name options (TICKET-394), each part on its own, so one
 * unknown base does not also cost the two checkboxes.
 *
 * The defaults are stated here rather than imported from `archiveName`: this
 * module is eager and the pack dialog is lazy. A test holds them together with
 * `DEFAULT_NAME_OPTIONS`.
 */
function packOptions(v: unknown, r: Repairs): ArchiveNameOptions {
  const o = obj(v);
  return {
    base: oneOf(
      o.base,
      ARCHIVE_BASES,
      "selection",
      "packNameOptions.base",
      r,
    ),
    date: bool(o.date, true, "packNameOptions.date", r),
    time: bool(o.time, true, "packNameOptions.time", r),
  };
}

/**
 * Every persisted setting, checked and clamped.
 *
 * Returns a complete object: the caller merges it over the store's own
 * defaults, so a field that was never stored simply keeps its default
 * and is not counted as a repair.
 */
export function validateSettings(
  stored: unknown,
  r: Repairs,
): Partial<PersistedSettings> {
  const s = obj(stored);

  const programs = unique(
    list(s.programs, (v) => program(v, r), "programs", r, { max: MAX_PROGRAMS }),
    (p) => p.id,
  );
  const programIds = new Set(programs.map((p) => p.id));

  return {
    language: oneOf(s.language, LANGS, "de", "language", r),
    theme: oneOf(s.theme, THEMES, "dark", "theme", r),
    hideSystemFiles: bool(s.hideSystemFiles, true, "hideSystemFiles", r),
    // The divider: a value outside the range leaves one pane unusable
    // and there is no gesture to get it back.
    paneSplit: num(s.paneSplit, 0.5, "paneSplit", r, { min: 0.15, max: 0.85 }),
    favorites: unique(
      list(s.favorites, (v) => favorite(v, r), "favorites", r, { max: MAX_FAVORITES }),
      (f) => f.path,
    ),
    terminalProgram: str(s.terminalProgram, "", "terminalProgram", r, MAX_PATH),
    gitEnabled: bool(s.gitEnabled, true, "gitEnabled", r),
    useTrash: bool(s.useTrash, true, "useTrash", r),
    // Beta, off by default (TICKET-391). An absent value is an older
    // installation meeting a new field, so it simply gets the default.
    undoEnabled: bool(s.undoEnabled, false, "undoEnabled", r),
    verifyCopies: bool(s.verifyCopies, false, "verifyCopies", r),
    speedLimit: num(s.speedLimit, 0, "speedLimit", r, { min: 0, int: true }),
    queueTransfers: bool(s.queueTransfers, true, "queueTransfers", r),
    // The same bounds the backend enforces (`resolve_buf`): 0 means the
    // default, otherwise 4 KB … 16 MB.
    bufferSizeKb: num(s.bufferSizeKb, 256, "bufferSizeKb", r, {
      min: 0,
      max: 16 * 1024,
      int: true,
    }),
    copyThreads: num(s.copyThreads, 1, "copyThreads", r, { min: 1, max: 16, int: true }),
    safeTransfers: bool(s.safeTransfers, true, "safeTransfers", r),
    checkTransferSpace: bool(s.checkTransferSpace, true, "checkTransferSpace", r),
    // Beta, off by default (TICKET-396), like the file-undo switch above.
    recoveryEnabled: bool(s.recoveryEnabled, false, "recoveryEnabled", r),
    reportPersistRecovery: bool(
      s.reportPersistRecovery,
      true,
      "reportPersistRecovery",
      r,
    ),
    packNameOptions: packOptions(s.packNameOptions, r),

    foldersFirst: bool(s.foldersFirst, true, "foldersFirst", r),
    showExtColumn: bool(s.showExtColumn, true, "showExtColumn", r),
    showPermissions: bool(s.showPermissions, false, "showPermissions", r),
    sizeFormat: oneOf(s.sizeFormat, SIZE_FORMATS, "auto", "sizeFormat", r),
    dateFormat: oneOf(s.dateFormat, DATE_FORMATS, "medium", "dateFormat", r),
    fontScale: oneOf(s.fontScale, SCALES, "md", "fontScale", r),
    fileIcons: oneOf(s.fileIcons, ["system", "simple"] as const, "system", "fileIcons", r),
    iconScale: oneOf(s.iconScale, SCALES, "md", "iconScale", r),
    columnWidths: {
      ...DEFAULT_COLUMN_WIDTHS,
      ...record(
        s.columnWidths,
        (v, key) => {
          if (typeof v !== "number" || !Number.isFinite(v)) return null;
          return num(v, DEFAULT_COLUMN_WIDTHS[key] ?? 100, `columnWidths.${key}`, r, {
            min: MIN_COLUMN_WIDTH,
            max: MAX_COLUMN_WIDTH,
            int: true,
          });
        },
        "columnWidths",
        r,
        50,
      ),
    },
    fileColors: record(
      s.fileColors,
      (v, id) => {
        const slot = obj(v);
        const out: Partial<Record<ThemeMode, string>> = {};
        for (const mode of THEME_MODES) {
          const c = optionalColor(slot[mode], `fileColors.${id}.${mode}`, r);
          if (c) out[mode] = c;
        }
        // A slot with no override left is not an override.
        return Object.keys(out).length > 0 ? out : null;
      },
      "fileColors",
      r,
      200,
    ),
    customColorRules: unique(
      list(s.customColorRules, (v) => colorRule(v, r), "customColorRules", r, {
        max: MAX_COLOR_RULES,
      }),
      (rule) => rule.id,
    ),
    programs,
    // An association pointing at a program that is gone opens nothing
    // and cannot be seen in the settings — it is a dangling reference,
    // and dropping it is the only outcome the user could have wanted.
    associations: normalizeAssociations(record(
      s.associations,
      (v) => {
        if (typeof v !== "string" || !programIds.has(v)) return null;
        return v;
      },
      "associations",
      r,
      MAX_ASSOCIATIONS,
    ), r),
    defaultEditor: str(s.defaultEditor, "", "defaultEditor", r, MAX_PATH),
    editorTrigger: oneOf(s.editorTrigger, EDITOR_TRIGGERS, "shiftF4", "editorTrigger", r),
  };
}

/**
 * Association keys are extensions, and the setter normalizes them —
 * lowercase, no leading dot. A stored `".LOG"` came from before that, or
 * from an import.
 */
export function normalizeAssociations(
  associations: Record<string, string>,
  r: Repairs,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [ext, id] of Object.entries(associations)) {
    const key = normalizeExt(ext);
    if (!key) {
      r.note(`associations.${ext}`);
      continue;
    }
    if (key !== ext) r.note(`associations.${ext}`);
    // First wins, so a normalization collision is deterministic.
    if (Object.prototype.hasOwnProperty.call(out, key)) {
      r.note(`associations.${ext}`);
    } else {
      Object.defineProperty(out, key, { value: id, enumerable: true, writable: true, configurable: true });
    }
  }
  return out;
}
