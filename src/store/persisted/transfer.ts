// ============================================================
// Carrying preferences to another machine, and back (TICKET-039).
//
// Export writes one file: an envelope naming the application, the kind
// of content and the schema version, wrapping the settings as they are
// stored. Import reads one back through the *same* migration and
// validation the store's own rehydration uses — an imported file is
// exactly as trustworthy as a file on disk, which is to say not at all.
//
// Three things this deliberately does not do:
//
//   * It does not export the session, the history, the dialog sizes or
//     the window position. Those describe one machine's last state, not
//     a preference, and carrying them to another one is noise.
//   * It does not export secrets, because there are none to export —
//     passwords live in the OS keychain and connection profiles in the
//     backend's own config. The check is here anyway: an envelope that
//     carries a secret-looking field is *refused*, not stripped, because
//     stripping it would hide from whoever is importing that somebody
//     put a password in a preferences file.
//   * It never applies half a file. Validation happens first, on a
//     value nothing has been handed yet.
// ============================================================

import { Repairs, secretLikeFields } from "@/lib/schema";
import {
  migrateSettings,
  SETTINGS_VERSION,
  validateSettings,
} from "./settings";
import type { PersistedSettings } from "@/store/settingsSchema";

/** What the file says it is. Checked before anything in it is believed. */
const APP = "hortcmd";
const KIND = "preferences";

export interface PreferencesFile {
  app: typeof APP;
  kind: typeof KIND;
  /** The settings schema version the payload is written in. */
  version: number;
  /** ISO timestamp, for a human reading the file. */
  exportedAt: string;
  settings: Record<string, unknown>;
}

/** The envelope, ready to be written as a file. */
export function buildExport(settings: Record<string, unknown>): string {
  const file: PreferencesFile = {
    app: APP,
    kind: KIND,
    version: SETTINGS_VERSION,
    exportedAt: new Date().toISOString(),
    settings,
  };
  return `${JSON.stringify(file, null, 2)}\n`;
}

export type ImportRefusal =
  | { reason: "notJson" }
  | { reason: "notOurs" }
  | { reason: "tooNew"; version: number }
  | { reason: "hasSecrets"; fields: string[] };

export type ImportResult =
  | { ok: true; settings: Partial<PersistedSettings>; count: number; repaired: number }
  | { ok: false; refusal: ImportRefusal };

/**
 * Reads an exported file, or says why it will not.
 *
 * Nothing is applied here — the caller gets a validated value and
 * decides, which is what makes the confirmation step honest and the
 * application atomic.
 */
export function readImport(text: string): ImportResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return { ok: false, refusal: { reason: "notJson" } };
  }

  if (
    typeof parsed !== "object" ||
    parsed === null ||
    (parsed as PreferencesFile).app !== APP ||
    (parsed as PreferencesFile).kind !== KIND
  ) {
    return { ok: false, refusal: { reason: "notOurs" } };
  }
  const file = parsed as PreferencesFile;

  const version = typeof file.version === "number" ? file.version : 0;
  if (version > SETTINGS_VERSION) {
    // A newer schema may mean something different by a field this
    // version also knows. Refusing is the only answer that cannot
    // quietly misread it.
    return { ok: false, refusal: { reason: "tooNew", version } };
  }

  // The whole file, not just the payload: a secret smuggled into the
  // envelope is still a secret in a file somebody is about to share.
  const secrets = secretLikeFields(file);
  if (secrets.length) {
    return { ok: false, refusal: { reason: "hasSecrets", fields: secrets } };
  }

  const r = new Repairs();
  const settings = validateSettings(migrateSettings(file.settings, version), r);
  return {
    ok: true,
    settings,
    count: Object.keys(settings).length,
    repaired: r.count,
  };
}
