// ============================================================
// Passwörter für verschlüsselte Archive: Zwischenspeicher (pro
// Archivpfad) und ein kleiner Passwort-Abfragedialog.
// Genutzt beim Browsen (panesStore) und Entpacken (fileOps).
// ============================================================

import { KeyRound } from "lucide-react";
import { useOps } from "@/store/opsStore";
import { translate } from "@/store/settingsStore";

/** Sentinel-Fehler des Backends (siehe archive.rs). */
export const PW_REQUIRED = "PASSWORD_REQUIRED";
export const PW_WRONG = "PASSWORD_WRONG";

const passwords = new Map<string, string>();

/** Zuletzt für dieses Archiv verwendetes Passwort (oder undefined). */
export function archivePassword(archive: string): string | undefined {
  return passwords.get(archive);
}

/** Enthält eine Fehlermeldung ein Passwort-Sentinel? */
export function isPasswordError(message: string): boolean {
  return message.includes(PW_REQUIRED) || message.includes(PW_WRONG);
}

/**
 * Fragt ein Passwort für `archive` ab und ruft bei Bestätigung `onOk`
 * mit dem (zwischengespeicherten) Passwort auf. `wrong` = vorheriger
 * Versuch war falsch.
 */
export function promptArchivePassword(
  archive: string,
  wrong: boolean,
  onOk: (password: string) => void,
): void {
  useOps.getState().requestPrompt({
    title: translate("archive.password.title"),
    label: wrong
      ? translate("archive.password.retry")
      : translate("archive.password.label"),
    initial: "",
    password: true,
    icon: KeyRound,
    confirmLabel: translate("op.confirm"),
    onSubmit: (value) => {
      if (!value) return;
      passwords.set(archive, value);
      onOk(value);
    },
  });
}
