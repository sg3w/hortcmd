import { registerTranslations } from "../registry";
// ============================================================
// File preview (TICKET-035).
//
// The F3 viewer and the modes it switches between.
// ============================================================

import { ns } from "./ns";

export const preview = ns(
  {
  "preview.truncated": "Nur der Dateianfang wird angezeigt.",
  "preview.mode.rendered": "Vorschau",
  "preview.mode.highlight": "Hervorgehoben",
  "preview.mode.table": "Tabelle",
  "preview.mode.raw": "Quelltext",
  "preview.mode.hex": "Hex",
  "preview.mode.text": "Text",
  "preview.exif": "EXIF",
  "preview.exif.none": "Keine EXIF-Daten vorhanden.",
  },
  {
  "preview.truncated": "Only the beginning of the file is shown.",
  "preview.mode.rendered": "Preview",
  "preview.mode.highlight": "Highlighted",
  "preview.mode.table": "Table",
  "preview.mode.raw": "Source",
  "preview.mode.hex": "Hex",
  "preview.mode.text": "Text",
  "preview.exif": "EXIF",
  "preview.exif.none": "No EXIF data available.",
  },
);

registerTranslations(preview);
