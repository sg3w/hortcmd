// ============================================================
// Reading a connection failure (TICKET-032).
//
// Every attempt ends with a typed `FailureKind`, so the UI can react to
// what went wrong — a timeout invites a retry, a refused sign-in points
// at the credentials, a host-key failure opens the confirmation —
// without matching on message text.
//
// Two shapes carry the same information: the `ConnFailure` on a
// `ConnectionStatus`, and the `AppError` a rejected `net_connect`
// throws. Both resolve to the same localized sentence here.
//
// Since TICKET-036 neither carries a finished message: the failure's
// own `AppError` is translated by `lib/appError.ts`, and the kind adds
// the headline that makes the two languages read alike even when the
// detail behind them came from a server that speaks neither.
// ============================================================

import type { ConnFailure, FailureKind } from "@/ipc/bindings";
import type { TransKey } from "@/i18n/dictionaries";
import { translate } from "@/store/settingsStore";
import { asAppError } from "@/lib/appError";
import { describeError, describeErrorFull } from "@/store/errorText";

const REASON: Record<FailureKind, TransKey> = {
  timeout: "net.fail.timeout",
  cancelled: "net.fail.cancelled",
  auth: "net.fail.auth",
  hostKey: "net.fail.hostKey",
  unreachable: "net.fail.unreachable",
  protocol: "net.fail.protocol",
  keychain: "net.fail.keychain",
};

/**
 * The localized headline for a failure kind, followed by what the
 * underlying error says when that adds something. The kind comes first,
 * so the reason reads the same in both languages.
 */
export function describeFailure(failure: ConnFailure): string {
  const head = translate(REASON[failure.kind]);
  const detail = describeError(failure.error);
  // A generic underlying message would only repeat the headline.
  return detail && detail !== head ? `${head} (${detail})` : head;
}

/**
 * Same for the error a rejected `net_connect` throws. Anything that is
 * not a backend error — a transport failure, say — is passed through
 * rather than dressed up as one.
 */
export function describeConnError(e: unknown): string {
  const err = asAppError(e);
  if (!err) return String(e);
  const kind = failureKindOf(e);
  return kind
    ? describeFailure({ kind, error: err })
    : describeErrorFull(e);
}

/**
 * The lifecycle kind a thrown error corresponds to, or null.
 *
 * Read from the error's code — the markers this used to search for in
 * message text are gone (TICKET-036). The mapping mirrors
 * `net::lifecycle::classify_protocol_error` on the backend.
 */
export function failureKindOf(e: unknown): FailureKind | null {
  const code = asAppError(e)?.code;
  if (!code) return null;
  switch (code) {
    case "net.timeout":
      return "timeout";
    case "cancelled":
      return "cancelled";
    case "net.auth":
    case "cred.wrongPassword":
      return "auth";
    case "security.hostKeyUnknown":
    case "security.hostKeyMismatch":
      return "hostKey";
    case "net.unreachable":
      return "unreachable";
    case "net.protocol":
      return "protocol";
    case "cred.locked":
    case "cred.store":
    case "cred.notFound":
      return "keychain";
    default:
      return null;
  }
}
