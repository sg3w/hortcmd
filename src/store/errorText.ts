// ============================================================
// Turning a backend error into a sentence (TICKET-036).
//
// The backend never sends a finished message — it sends a stable code
// and a few safe parameters. The wording is chosen here, at the moment
// the error is displayed, in whatever language is set. Three
// consequences the app depends on:
//
//   * **Switching language re-renders.** Nothing display-ready was
//     stored, so an error already on screen follows the setting.
//   * **A newer backend stays readable.** A code this build does not
//     know renders a safe generic sentence with its correlation id, so
//     the user can still report it.
//   * **Detail stays out of sight.** `detail` may carry a server's own
//     words, and its own language; it is diagnostics (ticket 045), not
//     a message.
//
// Deciding *what happened* is `lib/appError.ts` — pure, and usable from
// anywhere. This half needs the language setting, which is why it lives
// beside the store rather than in `lib/`.
// ============================================================

import type { ErrorAction, ItemError } from "@/ipc/bindings";
import type { TransKey } from "@/i18n/dictionaries";
import { DICT } from "@/i18n/dictionaries";
import { asAppError, fillParams as fill, isCancelled } from "@/lib/appError";
import { translate, useSettings } from "./settingsStore";

/** Whether the dictionary has an entry for this code. */
function keyFor(code: string): TransKey | null {
  const key = `error.${code}` as TransKey;
  return key in DICT.de ? key : null;
}

/**
 * The sentence a user reads. A code this build does not know falls back
 * to a generic message naming the code, so a newer backend degrades
 * rather than showing an empty dialog.
 */
export function describeError(e: unknown): string {
  const err = asAppError(e);
  if (!err) return String(e);
  const key = keyFor(err.code);
  return key
    ? fill(translate(key), err.params)
    : fill(translate("error.unknown"), { code: err.code });
}

/**
 * The correlation id, phrased for display. Shown with anything the user
 * might report — an unknown code above all, where it is the only thing
 * that ties the report to a log entry.
 */
export function errorReference(e: unknown): string | null {
  const err = asAppError(e);
  return err ? fill(translate("error.reference"), { id: err.id }) : null;
}

/** Message plus reference, for a dialog that has room for both. */
export function describeErrorFull(e: unknown): string {
  const err = asAppError(e);
  if (!err) return String(e);
  const known = keyFor(err.code) !== null;
  const message = describeError(e);
  // A known code speaks for itself; an unknown one needs the reference
  // to be reportable at all.
  return known ? message : `${message}\n${errorReference(e)}`;
}

const ACTION_KEY: Record<ErrorAction, TransKey> = {
  retry: "error.action.retry",
  reconnect: "error.action.reconnect",
  authenticate: "error.action.authenticate",
  confirmHostKey: "error.action.confirmHostKey",
  providePassword: "error.action.providePassword",
  freeSpace: "error.action.freeSpace",
  checkPermissions: "error.action.checkPermissions",
  unsupported: "error.action.unsupported",
  report: "error.action.report",
};

/** What the user can do about it, as a label — or null. */
export function errorActionLabel(e: unknown): string | null {
  const action = asAppError(e)?.action;
  return action ? translate(ACTION_KEY[action]) : null;
}

/** One item of a failed batch, as `item: message`. */
export function describeItemError(item: ItemError): string {
  return fill(translate("error.item"), {
    item: item.item,
    message: describeError(item.error),
  });
}

/**
 * The lines a batch result shows. Cancellations are dropped — a
 * cancelled item is not a failure — and an item with no name (a whole
 * operation that failed before it began) shows its message alone.
 */
export function describeItemErrors(items: ItemError[]): string[] {
  return items
    .filter((i) => !isCancelled(i.error))
    .map((i) => (i.item ? describeItemError(i) : describeError(i.error)));
}

/**
 * Re-renders when the language changes, for components that show an
 * error. The plain functions above read the current language once,
 * which is right for an imperative call and wrong for a rendered one.
 */
export function useErrorText(e: unknown): string {
  useSettings((s) => s.language);
  return describeError(e);
}
