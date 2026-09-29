// ============================================================
// Persisted state: the startup recovery notice (TICKET-039, TICKET-389).
//
// The recovery notice is the only place the application admits that it
// lost something the user configured, so it says *what* — naming the
// domains — and how much: a value corrected is not a domain reset, and
// wording both the same way taught the user to ignore the one that
// mattered.
//
// Only these keys are core. The settings page that backs preferences up
// and puts them back is `backup`, which loads with the Settings dialog.
// ============================================================

import { ns } from "./ns";

export const persist = ns(
  {
    "persist.recovered":
      "Gespeicherte Daten waren unlesbar und wurden zurückgesetzt: {domains}",
    "persist.repaired":
      "Einzelne Werte lagen außerhalb des gültigen Bereichs und wurden korrigiert: {domains}",
    "persist.domain.settings": "Einstellungen",
    "persist.domain.session": "Sitzung",
    "persist.domain.history": "Verlauf",
    "persist.domain.dialogSizes": "Dialoggrößen",
    "persist.domain.window": "Fensterposition",
  },
  {
    "persist.recovered":
      "Stored data could not be read and has been reset: {domains}",
    "persist.repaired":
      "Some values were out of range and have been corrected: {domains}",
    "persist.domain.settings": "Settings",
    "persist.domain.session": "Session",
    "persist.domain.history": "History",
    "persist.domain.dialogSizes": "Dialog sizes",
    "persist.domain.window": "Window position",
  },
);
