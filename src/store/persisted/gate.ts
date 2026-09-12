// ============================================================
// What happens between storage and the store (TICKET-039).
//
// Four persisted domains, one policy:
//
//   1. stored data is migrated to the current schema version,
//   2. then validated — always, not only when the version changed,
//   3. anything unusable is replaced by a safe default,
//   4. the original is *quarantined* rather than overwritten, and
//   5. the user is told once, in their own language, after startup.
//
// Point 4 is the one worth arguing for. When a store rehydrates broken
// data and we repair it, the next write puts the repaired version back —
// and whatever was actually in there is gone, along with any chance of
// working out what happened. A copy under a quarantine key costs a few
// kilobytes once and keeps the evidence.
//
// Point 5 is the other one. Silently resetting somebody's settings is
// how an application earns the reputation of "it forgets things". One
// notice, naming the domains, is enough — and it must never be a modal
// that stands between the user and a shell that is working fine.
// ============================================================

import { Repairs } from "@/lib/schema";

/** The localStorage keys, in one place so the reset path can enumerate them. */
export const PERSISTED_KEYS = {
  settings: "hortcmd-settings",
  session: "hortcmd-session",
  history: "hortcmd-history",
  dialogSizes: "hortcmd-dialog-sizes",
  window: "hortcmd-window",
} as const;

export type PersistedDomain = keyof typeof PERSISTED_KEYS;

/** Prefix for a quarantined copy: `hortcmd-quarantine-<key>-<when>`. */
const QUARANTINE_PREFIX = "hortcmd-quarantine-";

/** Keep the last few; a corrupt file that recurs should not fill the quota. */
const MAX_QUARANTINED = 5;

/**
 * Domains that needed recovering in this session.
 *
 * Collected rather than reported one by one: rehydration happens while
 * the modules load, long before anything can render a message, and four
 * separate notices for one bad write would be four times the alarm for
 * the same event.
 */
const recovered = new Set<PersistedDomain>();

/** True when `domain` had to be repaired or reset during this startup. */
export function wasRecovered(domain: PersistedDomain): boolean {
  return recovered.has(domain);
}

/** Every domain that had to be recovered, in a stable order. */
export function recoveredDomains(): PersistedDomain[] {
  return (Object.keys(PERSISTED_KEYS) as PersistedDomain[]).filter((d) =>
    recovered.has(d),
  );
}

/** Test seam: forget what this session recovered. */
export function resetRecovered(): void {
  recovered.clear();
}

/**
 * Puts the raw stored text aside before anything overwrites it.
 *
 * Deliberately best-effort: the quota may be full — plausibly *because*
 * of whatever went wrong — and failing to keep a diagnostic copy must
 * never stop the application from starting with working defaults.
 */
export function quarantine(domain: PersistedDomain, raw: string | null): void {
  recovered.add(domain);
  if (raw === null) return;
  try {
    const key = `${QUARANTINE_PREFIX}${PERSISTED_KEYS[domain]}-${Date.now()}`;
    localStorage.setItem(key, raw);
    pruneQuarantine();
  } catch {
    // No copy, then. The repair still stands and the notice is still
    // raised — the user learns what happened either way.
  }
}

/** Drops all but the newest [`MAX_QUARANTINED`] copies. */
function pruneQuarantine(): void {
  const keys: string[] = [];
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key?.startsWith(QUARANTINE_PREFIX)) keys.push(key);
  }
  // The timestamp is the tail of the key, so lexical order is time
  // order for as long as the epoch has the same number of digits.
  keys.sort();
  for (const key of keys.slice(0, Math.max(0, keys.length - MAX_QUARANTINED))) {
    localStorage.removeItem(key);
  }
}

/** The quarantined copies, newest first — for diagnostics and for cleanup. */
export function quarantinedKeys(): string[] {
  const keys: string[] = [];
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key?.startsWith(QUARANTINE_PREFIX)) keys.push(key);
  }
  return keys.sort().reverse();
}

export function clearQuarantine(): void {
  for (const key of quarantinedKeys()) localStorage.removeItem(key);
}

/**
 * Whether a stored version may be read at all.
 *
 * Called from each store's `migrate`, which is the only place Zustand
 * hands over the version that was actually on disk.
 *
 * A *newer* schema is not something to migrate — nothing here knows what
 * that version means. Reading it with the current validator would drop
 * whatever the newer version added and then write the truncated result
 * back, silently downgrading data that a newer installation still needs.
 * So it is quarantined and the defaults are used, which leaves the
 * original intact for when that installation runs again.
 */
export function acceptVersion(
  domain: PersistedDomain,
  from: number,
  current: number,
): boolean {
  if (from <= current) return true;
  quarantine(domain, safeRaw(domain));
  console.warn(
    `[hortcmd] persisted "${domain}" is schema ${from}, newer than ${current} — not read`,
  );
  return false;
}

/**
 * Validates a rehydrated value, recording what it cost.
 *
 * Runs on every load, not only when the version changed: a value can be
 * out of range without the schema ever having changed, and that is the
 * common case rather than the exotic one.
 */
export function rehydrate<T>(
  domain: PersistedDomain,
  stored: unknown,
  validate: (value: unknown, r: Repairs) => T,
): T {
  const r = new Repairs();
  const value = validate(stored, r);

  if (r.any) {
    quarantine(domain, safeRaw(domain));
    // The field names go to the console, never to the user: a path in a
    // favourite is the user's business, and a notice that reads like a
    // stack trace is a notice nobody reads.
    console.warn(
      `[hortcmd] repaired persisted "${domain}" (${r.count}): ${r.fields.join(", ")}`,
    );
  }
  return value;
}

/** The raw stored text, or `null` if it cannot be read at all. */
function safeRaw(domain: PersistedDomain): string | null {
  try {
    return localStorage.getItem(PERSISTED_KEYS[domain]);
  } catch {
    return null;
  }
}

/**
 * Reports a domain whose stored text could not even be parsed.
 *
 * Zustand's `persist` swallows a `JSON.parse` failure and starts from
 * the defaults, which is the right behaviour and the wrong amount of
 * silence. This is called from the storage wrapper so the same notice
 * covers corruption and repair alike.
 */
export function reportUnreadable(domain: PersistedDomain, raw: string | null): void {
  quarantine(domain, raw);
  console.warn(`[hortcmd] persisted "${domain}" was unreadable — defaults restored`);
}
