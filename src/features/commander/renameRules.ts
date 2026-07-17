// ============================================================
// Batch rename: pure name computation from a rule set
// (naming scheme with placeholders, counter, search/replace with
// optional regex, case conversion). The actual
// renaming on disk is done by the backend (`rename_batch`).
//
// Placeholders in the masks:  [N] name without extension · [E] extension · [C] counter
// ============================================================

import type { Row } from "@/store/panesStore";
import { splitName } from "@/lib/format";

export type CaseMode = "keep" | "lower" | "upper" | "title";

export interface RenameRules {
  /** Mask for the name part (default "[N]"). */
  nameMask: string;
  /** Mask for the extension (default "[E]"). */
  extMask: string;
  /** Search text (empty = no replacement). */
  search: string;
  /** Replacement (with regex, $1 … can be used). */
  replace: string;
  /** Interpret the search text as a regular expression. */
  regex: boolean;
  /** Respect case sensitivity in the search. */
  caseSensitive: boolean;
  /** Adjust the case of the name part. */
  caseMode: CaseMode;
  counterStart: number;
  counterStep: number;
  /** Minimum number of counter digits (leading zeros). */
  counterDigits: number;
}

export const DEFAULT_RULES: RenameRules = {
  nameMask: "[N]",
  extMask: "[E]",
  search: "",
  replace: "",
  regex: false,
  caseSensitive: true,
  caseMode: "keep",
  counterStart: 1,
  counterStep: 1,
  counterDigits: 1,
};

/** Replaces the placeholders of a mask in a single pass. */
function expand(mask: string, base: string, ext: string, counter: string): string {
  return mask.replace(/\[([NEC])\]/g, (_, key) =>
    key === "N" ? base : key === "E" ? ext : counter,
  );
}

function padCounter(value: number, digits: number): string {
  const neg = value < 0;
  const body = String(Math.abs(value)).padStart(Math.max(1, digits), "0");
  return neg ? `-${body}` : body;
}

function applyCase(name: string, mode: CaseMode): string {
  switch (mode) {
    case "lower":
      return name.toLowerCase();
    case "upper":
      return name.toUpperCase();
    case "title":
      // Uppercase the first letter of each word (separated by space/separators).
      return name
        .toLowerCase()
        .replace(/(^|[\s._-])(\p{L})/gu, (_, sep, ch) => sep + ch.toUpperCase());
    default:
      return name;
  }
}

/** Applies search/replace to the finished name. */
function applySearch(name: string, rules: RenameRules, re: RegExp | null): string {
  if (!rules.search) return name;
  if (re) return name.replace(re, rules.replace);
  if (rules.caseSensitive) return name.split(rules.search).join(rules.replace);
  // Literal, but case-insensitive.
  const escaped = rules.search.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return name.replace(new RegExp(escaped, "gi"), rules.replace);
}

/**
 * Builds a rename function from the rule set. On an invalid regex, an
 * `error` is returned instead (for display in the dialog).
 */
export function buildRenamer(rules: RenameRules): {
  rename?: (entry: Row, index: number) => string;
  error?: string;
} {
  let re: RegExp | null = null;
  if (rules.search && rules.regex) {
    try {
      re = new RegExp(rules.search, rules.caseSensitive ? "g" : "gi");
    } catch (e) {
      return { error: e instanceof Error ? e.message : String(e) };
    }
  }

  const rename = (entry: Row, index: number): string => {
    const { base, ext } = splitName(entry);
    const counter = padCounter(
      rules.counterStart + index * rules.counterStep,
      rules.counterDigits,
    );
    const namePart = applyCase(
      expand(rules.nameMask, base, ext, counter),
      rules.caseMode,
    );
    const extPart = expand(rules.extMask, base, ext, counter);
    const full = extPart ? `${namePart}.${extPart}` : namePart;
    return applySearch(full, rules, re);
  };

  return { rename };
}

export type RenameStatus =
  | "ok" // wird umbenannt
  | "unchanged" // Name unverändert
  | "invalid" // leer / enthält Pfadtrenner
  | "duplicate" // zwei Einträge → gleicher Zielname
  | "exists"; // Ziel bereits im Ordner (nicht Teil der Umbenennung)

export interface RenamePreviewItem {
  from: string;
  to: string;
  isDir: boolean;
  status: RenameStatus;
}

/**
 * Computes the preview for the target set and detects conflicts.
 * `folderNames` = all names actually present in the folder (incl. hidden ones).
 */
export function buildPreview(
  targets: Row[],
  folderNames: Set<string>,
  rename: (entry: Row, index: number) => string,
): RenamePreviewItem[] {
  const items = targets.map((entry, i) => ({
    from: entry.name,
    to: rename(entry, i),
    isDir: entry.is_dir,
  }));

  // Count the target names (for duplicate detection).
  const toCount = new Map<string, number>();
  for (const it of items) toCount.set(it.to, (toCount.get(it.to) ?? 0) + 1);

  // Names of the involved sources (their old slots become free).
  const sourceNames = new Set(items.map((it) => it.from));

  return items.map((it): RenamePreviewItem => {
    let status: RenameStatus;
    if (it.to === it.from) {
      status = "unchanged";
    } else if (!it.to || it.to.includes("/") || it.to.includes("\\")) {
      status = "invalid";
    } else if ((toCount.get(it.to) ?? 0) > 1) {
      status = "duplicate";
    } else if (folderNames.has(it.to) && !sourceNames.has(it.to)) {
      status = "exists";
    } else {
      status = "ok";
    }
    return { ...it, status };
  });
}
