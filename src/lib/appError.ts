// ============================================================
// Recognizing a backend error (TICKET-036).
//
// The backend sends a stable code and a few safe parameters; the
// sentence is produced here, in the user's language, at the moment the
// error is shown. Three consequences the app depends on:
//
//   * **Control flow uses codes.** `isError(e, "archive.passwordWrong")`
//     replaces the substring matching that used to decide whether to
//     prompt for a password or park an action behind an unlock dialog.
//   * **Switching language re-renders.** Nothing display-ready was
//     stored, so an error already on screen follows the setting.
//   * **A newer backend stays readable.** A code this build does not
//     know renders a safe generic sentence with its correlation id, so
//     the user can still report it.
//
// This half is pure: what an error *is* and what code it carries, with
// no knowledge of the user's language. Turning one into a sentence is
// `store/errorText.ts`, which needs the language setting and therefore
// cannot live in `lib/`.
// ============================================================

import type { AppError } from "@/ipc/bindings";

/** Whether a value is a backend error rather than some other throwable. */
export function isAppError(e: unknown): e is AppError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as AppError).code === "string" &&
    typeof (e as AppError).id === "string" &&
    typeof (e as AppError).version === "number"
  );
}

/** The backend error inside a thrown value, or null. */
export function asAppError(e: unknown): AppError | null {
  return isAppError(e) ? e : null;
}

/**
 * Whether a thrown value carries this code. The way every branch is
 * written now — never a substring of a message.
 */
export function isError(e: unknown, code: string): boolean {
  return asAppError(e)?.code === code;
}

/** Whether a thrown value carries any of these codes. */
export function isOneOf(e: unknown, ...codes: string[]): boolean {
  const code = asAppError(e)?.code;
  return code !== undefined && codes.includes(code);
}

/** A cancelled operation is not a failure and is never shown as one. */
export function isCancelled(e: unknown): boolean {
  return isError(e, "cancelled");
}


/**
 * Fills `{name}` placeholders from a parameter map. A placeholder whose
 * parameter is missing is left visible rather than silently emptied —
 * a gap in a sentence is a bug report; a quietly lost value is not.
 */
export function fillParams(
  text: string,
  params: Record<string, string | undefined>,
): string {
  return Object.entries(params).reduce(
    (acc, [k, v]) => (v === undefined ? acc : acc.split(`{${k}}`).join(v)),
    text,
  );
}

/** A transport failure can precede a structured backend response. */
export function ipcError(error: unknown): AppError {
  return asAppError(error) ?? {
    version: 1, code: "fs.io", params: {}, op: "ipc", provider: null,
    retryable: true, action: "retry", detail: null,
    id: typeof crypto.randomUUID === "function" ? crypto.randomUUID() : `ipc-${Date.now()}`,
  };
}
