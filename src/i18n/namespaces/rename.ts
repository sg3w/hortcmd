import { registerTranslations } from "../registry";
// ============================================================
// Batch rename (TICKET-035).
//
// The rule set, the preview and the conflict statuses it reports.
// ============================================================

import { ns } from "./ns";

export const rename = ns(
  {
  "rename.title": "Massenumbenennen",
  "rename.schema": "Namensschema",
  "rename.name": "Name",
  "rename.ext": "Endung",
  "rename.placeholders": "[N] Name · [E] Endung · [C] Zähler",
  "rename.searchReplace": "Suchen & Ersetzen",
  "rename.search": "Suchen",
  "rename.replace": "Ersetzen",
  "rename.regex": "Regex",
  "rename.caseSensitive": "Groß/klein",
  "rename.regexError": "Ungültiger Ausdruck: {err}",
  "rename.counter": "Zähler",
  "rename.counter.start": "Start",
  "rename.counter.step": "Schritt",
  "rename.counter.digits": "Stellen",
  "rename.case": "Groß-/Kleinschreibung",
  "rename.case.keep": "unverändert",
  "rename.case.lower": "klein",
  "rename.case.upper": "GROSS",
  "rename.case.title": "Erster groß",
  "rename.preview": "Vorschau (alt → neu)",
  "rename.preview.empty": "Keine Einträge ausgewählt.",
  "rename.preview.more": "… und {n} weitere",
  "rename.count": "{changed} von {total} werden umbenannt",
  "rename.conflicts": "{n} Konflikt(e)",
  "rename.conflict.invalid": "Ungültiger Zielname",
  "rename.conflict.duplicate": "Zielname mehrfach vergeben",
  "rename.conflict.exists": "Ziel existiert bereits",
  "rename.resize": "Zum Vergrößern ziehen",
  "rename.apply": "Umbenennen",
  },
  {
  "rename.title": "Batch rename",
  "rename.schema": "Name template",
  "rename.name": "Name",
  "rename.ext": "Extension",
  "rename.placeholders": "[N] name · [E] extension · [C] counter",
  "rename.searchReplace": "Search & replace",
  "rename.search": "Search",
  "rename.replace": "Replace",
  "rename.regex": "Regex",
  "rename.caseSensitive": "Case",
  "rename.regexError": "Invalid expression: {err}",
  "rename.counter": "Counter",
  "rename.counter.start": "Start",
  "rename.counter.step": "Step",
  "rename.counter.digits": "Digits",
  "rename.case": "Letter case",
  "rename.case.keep": "unchanged",
  "rename.case.lower": "lower",
  "rename.case.upper": "UPPER",
  "rename.case.title": "Title",
  "rename.preview": "Preview (old → new)",
  "rename.preview.empty": "No entries selected.",
  "rename.preview.more": "… and {n} more",
  "rename.count": "{changed} of {total} will be renamed",
  "rename.conflicts": "{n} conflict(s)",
  "rename.conflict.invalid": "Invalid target name",
  "rename.conflict.duplicate": "Duplicate target name",
  "rename.conflict.exists": "Target already exists",
  "rename.resize": "Drag to resize",
  "rename.apply": "Rename",
  },
);

registerTranslations(rename);
