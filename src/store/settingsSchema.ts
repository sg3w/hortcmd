// ============================================================
// What the settings *are*, separate from the store that holds them
// (TICKET-039).
//
// The persisted shape, its enums and its bounds live here so that the
// validator in `store/persisted/settings.ts` can state them without
// importing the store — which would be a cycle, and which the boundary
// check would (rightly) refuse.
//
// `settingsStore.ts` re-exports everything here, so nothing else needs
// to know this file exists.
// ============================================================

import type { Lang } from "@/i18n/dictionaries";
import type {
  CustomColorRule,
  FileColorOverrides,
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

/**
 * Everything that is written to `localStorage`.
 *
 * The store adds its setters on top; this half is the one with a schema
 * version, a migration chain and a validator.
 */
export interface PersistedSettings {
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
  /**
   * Failure-safe transfers (TICKET-029): copy into a hidden staging
   * entry on the destination store and publish it with one rename, so
   * a failure never leaves a partial file under the final name and
   * never destroys the previous destination.
   *
   * On by default, as TICKET-029 specified and TICKET-039 corrected:
   * it costs room for the copy next to the target, and that is the
   * cheaper half of the trade against losing the destination. Turning
   * it off is a decision the File-operations panel keeps visible with
   * a standing warning.
   */
  safeTransfers: boolean;

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
  fileIcons: "system" | "simple";
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

}

/** Normalizes an extension: lowercase, without a leading dot/whitespace. */
export function normalizeExt(ext: string): string {
  return ext.trim().replace(/^\.+/, "").toLowerCase();
}
