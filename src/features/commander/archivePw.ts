// ============================================================
// Passwords for encrypted archives: a cache (per
// archive path) and a small password prompt dialog.
// Used when browsing (panesStore) and extracting (fileOps).
// ============================================================

import { KeyRound } from "lucide-react";
import { useOps } from "@/store/opsStore";
import { translate } from "@/store/settingsStore";
import { isError, isOneOf } from "@/lib/appError";

/**
 * The two password outcomes, as error codes (TICKET-036). They used to
 * be sentinel strings matched inside a message, which made a password
 * prompt depend on wording; now they are codes the backend sends and
 * this module branches on.
 */
export const PW_REQUIRED = "archive.passwordRequired";
export const PW_WRONG = "archive.passwordWrong";

const passwords = new Map<string, string>();

/** The password last used for this archive (or undefined). */
export function archivePassword(archive: string): string | undefined {
  return passwords.get(archive);
}

/** Whether a thrown backend error is one of the two password outcomes. */
export function isPasswordError(e: unknown): boolean {
  return isOneOf(e, PW_REQUIRED, PW_WRONG);
}

/** Whether it is specifically the "wrong password" one. */
export function isWrongPassword(e: unknown): boolean {
  return isError(e, PW_WRONG);
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
