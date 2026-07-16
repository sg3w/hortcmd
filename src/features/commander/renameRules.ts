// ============================================================
// Massenumbenennen: reine Namensberechnung aus einem Regelsatz
// (Namensschema mit Platzhaltern, Zähler, Suchen/Ersetzen mit
// optionalem Regex, Groß-/Kleinschreibung). Das eigentliche
// Umbenennen auf der Platte macht das Backend (`rename_batch`).
//
// Platzhalter in den Masken:  [N] Name ohne Endung · [E] Endung · [C] Zähler
// ============================================================

import type { Row } from "@/store/panesStore";
import { splitName } from "@/lib/format";

export type CaseMode = "keep" | "lower" | "upper" | "title";

export interface RenameRules {
  /** Maske für den Namensteil (Standard „[N]"). */
  nameMask: string;
  /** Maske für die Endung (Standard „[E]"). */
  extMask: string;
  /** Suchtext (leer = kein Ersetzen). */
  search: string;
  /** Ersetzung (bei Regex sind $1 … nutzbar). */
  replace: string;
  /** Suchtext als regulärer Ausdruck interpretieren. */
  regex: boolean;
  /** Groß-/Kleinschreibung bei der Suche beachten. */
  caseSensitive: boolean;
  /** Groß-/Kleinschreibung des Namensteils anpassen. */
  caseMode: CaseMode;
  counterStart: number;
  counterStep: number;
  /** Mindestanzahl Stellen des Zählers (führende Nullen). */
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

/** Ersetzt die Platzhalter einer Maske in einem einzigen Durchlauf. */
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
      // Ersten Buchstaben je Wort (getrennt durch Leer-/Trennzeichen) groß.
      return name
        .toLowerCase()
        .replace(/(^|[\s._-])(\p{L})/gu, (_, sep, ch) => sep + ch.toUpperCase());
    default:
      return name;
  }
}

/** Wendet Suchen/Ersetzen auf den fertigen Namen an. */
function applySearch(name: string, rules: RenameRules, re: RegExp | null): string {
  if (!rules.search) return name;
  if (re) return name.replace(re, rules.replace);
  if (rules.caseSensitive) return name.split(rules.search).join(rules.replace);
  // Literal, aber ohne Beachtung der Groß-/Kleinschreibung.
  const escaped = rules.search.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return name.replace(new RegExp(escaped, "gi"), rules.replace);
}

/**
 * Baut aus dem Regelsatz eine Umbenennungsfunktion. Bei ungültigem Regex wird
 * stattdessen ein `error` zurückgegeben (für die Anzeige im Dialog).
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
 * Berechnet die Vorschau für die Zielmenge und erkennt Konflikte.
 * `folderNames` = alle real vorhandenen Namen im Ordner (inkl. versteckte).
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

  // Zielnamen zählen (für Duplikat-Erkennung).
  const toCount = new Map<string, number>();
  for (const it of items) toCount.set(it.to, (toCount.get(it.to) ?? 0) + 1);

  // Namen der beteiligten Quellen (deren alte Plätze werden frei).
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
