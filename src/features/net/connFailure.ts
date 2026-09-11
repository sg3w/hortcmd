// ============================================================
// Reading a connection failure (TICKET-032).
//
// Every attempt ends with a typed `FailureKind`, so the UI can react to
// what went wrong — a timeout invites a retry, a refused sign-in points
// at the credentials, a host-key failure opens the confirmation —
// without matching on message text.
//
// Two shapes carry the same information: the `ConnFailure` on a
// `ConnectionStatus`, and the tagged string a rejected `net_connect`
// throws (the IPC layer still transports errors as strings until
// TICKET-036). Both resolve to the same localized sentence here.
// ============================================================

import type { ConnFailure, FailureKind } from "@/ipc/bindings";
import type { TransKey } from "@/i18n/dictionaries";
import { translate } from "@/store/settingsStore";

const REASON: Record<FailureKind, TransKey> = {
  timeout: "net.fail.timeout",
  cancelled: "net.fail.cancelled",
  auth: "net.fail.auth",
  hostKey: "net.fail.hostKey",
  unreachable: "net.fail.unreachable",
  protocol: "net.fail.protocol",
  keychain: "net.fail.keychain",
};

/** Markers the backend puts in front of a thrown connection error. */
const TAGS: Record<string, FailureKind> = {
  CONN_TIMEOUT: "timeout",
  CONN_CANCELLED: "cancelled",
  CONN_AUTH: "auth",
  CONN_HOSTKEY: "hostKey",
  CONN_UNREACHABLE: "unreachable",
  CONN_PROTOCOL: "protocol",
  CONN_KEYCHAIN: "keychain",
};

/**
 * The localized headline for a failure kind, followed by the server's
 * own words when they add something. The kind is always shown first, so
 * the reason reads the same in both languages even when the detail
 * behind it comes from a server that speaks neither.
 */
export function describeFailure(failure: ConnFailure): string {
  const head = translate(REASON[failure.kind]);
  const detail = failure.message.trim();
  return detail ? `${head} (${detail})` : head;
}

/**
 * Same for the tagged string a rejected `net_connect` throws. An
 * untagged error — anything that did not come from the lifecycle — is
 * passed through unchanged rather than being dressed up as one.
 */
export function describeConnError(e: unknown): string {
  const text = String(e);
  for (const [tag, kind] of Object.entries(TAGS)) {
    const at = text.indexOf(`${tag}: `);
    if (at < 0) continue;
    return describeFailure({
      kind,
      message: text.slice(at + tag.length + 2),
    });
  }
  return text;
}

/** The failure kind inside a thrown error, or null when it carries none. */
export function failureKindOf(e: unknown): FailureKind | null {
  const text = String(e);
  for (const [tag, kind] of Object.entries(TAGS)) {
    if (text.includes(`${tag}: `)) return kind;
  }
  return null;
}
