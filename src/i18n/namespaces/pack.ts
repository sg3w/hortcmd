import { registerTranslations } from "../registry";
// ============================================================
// The pack dialog's name options (TICKET-392).
//
// Only the dialog that composes the archive name uses these, and it is
// loaded lazily — so the strings load with it rather than at startup.
// ============================================================

import { ns } from "./ns";

export const pack = ns(
  {
  "archive.base": "Grundname",
  "archive.base.selection": "Aus der Auswahl: {base}",
  "archive.base.generic": "Allgemein: {base}",
  "archive.addDate": "Datum anhängen",
  "archive.addTime": "Uhrzeit anhängen",
  "archive.auto": "Namen aus den Optionen erzeugen",
  "archive.auto.hint":
    "Eingabe erneut überschreiben – der eingetippte Name geht dabei verloren.",
  },
  {
  "archive.base": "Base name",
  "archive.base.selection": "From the selection: {base}",
  "archive.base.generic": "Generic: {base}",
  "archive.addDate": "Add date",
  "archive.addTime": "Add time",
  "archive.auto": "Build the name from the options",
  "archive.auto.hint": "Overrides the input again – the name typed by hand is lost.",
  },
);

registerTranslations(pack);
