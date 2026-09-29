import { registerTranslations } from "../registry";
// ============================================================
// Settings ▸ General ▸ Back up settings (TICKET-039, TICKET-389).
//
// Export, import, reset, and the switch for the startup notice. Every
// one of these strings is only ever read inside the Settings dialog, so
// they register with it rather than sitting in the eagerly loaded core
// dictionary — the startup notice itself lives in `persist`.
// ============================================================

import { ns } from "./ns";

export const backup = ns(
  {
    "persist.report": "Beim Start über wiederhergestellte Daten informieren",
    "persist.report.hint":
      "Aus: nur ohne Meldung – Protokoll und Sicherungskopie bleiben.",
    "persist.section": "Einstellungen sichern",
    "persist.section.hint":
      "Alle Einstellungen als Datei sichern oder von einem anderen Rechner übernehmen. Zugangsdaten sind nie enthalten – Passwörter liegen im Schlüsselbund, nicht hier.",
    "persist.export": "Exportieren …",
    "persist.export.done": "Einstellungen nach {path} exportiert",
    "persist.export.failed": "Export fehlgeschlagen: {error}",
    "persist.import": "Importieren …",
    "persist.import.done": "{count} Einstellungen übernommen",
    "persist.import.failed": "Import abgelehnt: {reason}",
    "persist.import.notJson": "Die Datei ist kein gültiges JSON",
    "persist.import.notOurs": "Das ist keine hortcmd-Einstellungsdatei",
    "persist.import.tooNew":
      "Die Datei stammt aus einer neueren Version (Schema {version})",
    "persist.import.hasSecrets":
      "Die Datei enthält vertrauliche Felder ({fields}) und wurde nicht übernommen",
    "persist.import.confirm.title": "Einstellungen übernehmen?",
    "persist.import.confirm.body":
      "{count} Einstellungen aus {path} ersetzen die aktuellen. Sitzung, Verlauf und Fensterposition bleiben unverändert.",
    "persist.reset": "Zurücksetzen …",
    "persist.reset.title": "Einstellungen zurücksetzen?",
    "persist.reset.body":
      "Diese gespeicherten Daten werden gelöscht und auf die Voreinstellung zurückgesetzt:",
    "persist.reset.keeps":
      "Nicht betroffen: Verbindungsprofile, gespeicherte Passwörter und die Dateien selbst.",
    "persist.reset.confirm": "Zurücksetzen",
    "persist.reset.done": "Auf die Voreinstellungen zurückgesetzt",
  },
  {
    "persist.report": "Report recovered data at startup",
    "persist.report.hint":
      "Off: no notice only – the log and the quarantined copy stay.",
    "persist.section": "Back up settings",
    "persist.section.hint":
      "Save every setting to a file, or take one over from another machine. Credentials are never included — passwords live in the keychain, not here.",
    "persist.export": "Export …",
    "persist.export.done": "Settings exported to {path}",
    "persist.export.failed": "Export failed: {error}",
    "persist.import": "Import …",
    "persist.import.done": "{count} settings applied",
    "persist.import.failed": "Import refused: {reason}",
    "persist.import.notJson": "The file is not valid JSON",
    "persist.import.notOurs": "That is not a hortcmd settings file",
    "persist.import.tooNew":
      "The file comes from a newer version (schema {version})",
    "persist.import.hasSecrets":
      "The file contains confidential fields ({fields}) and was not applied",
    "persist.import.confirm.title": "Apply these settings?",
    "persist.import.confirm.body":
      "{count} settings from {path} will replace the current ones. Session, history and window position are left alone.",
    "persist.reset": "Reset …",
    "persist.reset.title": "Reset settings?",
    "persist.reset.body":
      "This stored data will be deleted and returned to its default:",
    "persist.reset.keeps":
      "Not affected: connection profiles, saved passwords, and the files themselves.",
    "persist.reset.confirm": "Reset",
    "persist.reset.done": "Reset to defaults",
  },
);

registerTranslations(backup);
