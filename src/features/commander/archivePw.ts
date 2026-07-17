// ============================================================
// Passwords for encrypted archives: a cache (per
// archive path) and a small password prompt dialog.
// Used when browsing (panesStore) and extracting (fileOps).
// ============================================================

import { KeyRound } from "lucide-react";
import { useOps } from "@/store/opsStore";
import { translate } from "@/store/settingsStore";

/** Sentinel errors of the backend (see archive.rs). */
export const PW_REQUIRED = "PASSWORD_REQUIRED";
export const PW_WRONG = "PASSWORD_WRONG";

const passwords = new Map<string, string>();

/** The password last used for this archive (or undefined). */
export function archivePassword(archive: string): string | undefined {
  return passwords.get(archive);
}

/** Does an error message contain a password sentinel? */
export function isPasswordError(message: string): boolean {
  return message.includes(PW_REQUIRED) || message.includes(PW_WRONG);
}

/**
 * Prompts for a password for `archive` and, on confirmation, calls `onOk`
 * with the (cached) password. `wrong` = the previous
 * attempt was incorrect.
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
