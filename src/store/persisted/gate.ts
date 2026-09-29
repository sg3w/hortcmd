// ============================================================
// What happens between storage and the store (TICKET-039).
//
// Four persisted domains, one policy:
//
//   1. stored data is migrated to the current schema version,
//   2. then validated — always, not only when the version changed,
//   3. anything unusable is replaced by a safe default,
//   4. the original is *quarantined* rather than overwritten,
//   5. the repaired value is written back once, and
//   6. the user is told once, in their own language, after startup.
//
// Point 4 is the one worth arguing for. When a store rehydrates broken
// data and we repair it, the next write puts the repaired version back —
// and whatever was actually in there is gone, along with any chance of
// working out what happened. A copy under a quarantine key costs a few
// kilobytes once and keeps the evidence.
//
// Point 5 is what TICKET-389 had to add. Zustand's `persist` does not
// write on rehydration, so before it the repaired value lived only in
// memory: the stored text kept whatever needed repairing, the next start
// repaired it again, and the notice came back for ever. The quarantine
// copy is taken first, so writing the repair back loses nothing.
//
// Point 6 is the other one worth arguing for. Silently resetting
// somebody's settings is how an application earns the reputation of "it
// forgets things". One notice, naming the domains, is enough — and it
// must never be a modal that stands between the user and a shell that is
// working fine. It also has to be proportionate: a clamped dialog width
// is not the same event as a blob that could not be parsed at all, so
// the two are reported apart (`Recovery`) and either can be switched off.
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
 * How much a domain lost.
 *
 * `"repaired"` kept the stored value and corrected parts of it — a
 * number clamped into range, an entry without a path dropped. Nothing
 * the user would recognise as a setting has gone.
 *
 * `"reset"` kept nothing: the text could not be parsed, or it came from
 * a schema this version must not read. That is the case the notice was
 * written for, and the only one that deserves to look like an error.
 */
export type Recovery = "repaired" | "reset";

/**
 * Domains that needed recovering in this session, and how badly.
 *
 * Collected rather than reported one by one: rehydration happens while
 * the modules load, long before anything can render a message, and four
 * separate notices for one bad write would be four times the alarm for
 * the same event.
 */
const recovered = new Map<PersistedDomain, Recovery>();

/** Records `level` for `domain`, never downgrading a reset to a repair. */
function mark(domain: PersistedDomain, level: Recovery): void {
  if (level === "reset" || !recovered.has(domain)) recovered.set(domain, level);
}

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

/** The domains recovered at `level`, in the same stable order. */
export function recoveredDomainsAt(level: Recovery): PersistedDomain[] {
  return recoveredDomains().filter((d) => recovered.get(d) === level);
}

/**
 * Test seam: forget what this session recovered.
 *
 * The registered writers are deliberately kept: a store registers once,
 * when its module loads, and clearing them here would leave a test that
 * renders the real application with nothing to write back.
 */
export function resetRecovered(): void {
  recovered.clear();
}

/**
 * How a domain writes its current state back to storage.
 *
 * Registered by each persisted store right after it is created, because
 * only the store knows how to make `persist` write. The gate holds the
 * callbacks so `settleRecovered` can be one call from one place instead
 * of four effects that each have to remember.
 */
const writers = new Map<PersistedDomain, () => void>();

export function onRecovered(domain: PersistedDomain, write: () => void): void {
  writers.set(domain, write);
}

/**
 * Writes every recovered domain back, so the same repair is not made on
 * the next start — and the notice is not raised again for it.
 *
 * Called once after the first render: the stores are hydrated by then,
 * and a write during module load would land in the middle of Zustand's
 * own rehydration. A writer that throws is skipped; storage that refuses
 * writes is already handled by `guardedStorage`, and a domain that
 * cannot be settled simply reports again next time.
 */
export function settleRecovered(): void {
  for (const domain of recoveredDomains()) {
    try {
      writers.get(domain)?.();
    } catch (e) {
      console.warn(`[hortcmd] could not write back repaired "${domain}":`, e);
    }
  }
}

/**
 * Puts the raw stored text aside before anything overwrites it.
 *
 * Deliberately best-effort: the quota may be full — plausibly *because*
 * of whatever went wrong — and failing to keep a diagnostic copy must
 * never stop the application from starting with working defaults.
 */
export function quarantine(
  domain: PersistedDomain,
  raw: string | null,
  level: Recovery = "reset",
): void {
  mark(domain, level);
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
  quarantine(domain, safeRaw(domain), "reset");
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
    quarantine(domain, safeRaw(domain), "repaired");
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
  quarantine(domain, raw, "reset");
  console.warn(`[hortcmd] persisted "${domain}" was unreadable — defaults restored`);
}
