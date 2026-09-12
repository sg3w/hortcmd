// ============================================================
// Validating what came back from storage (TICKET-039).
//
// Everything in `localStorage` is input. It was written by an older
// version of this application, or by a newer one, or by a text editor,
// or by a half-finished write during a crash — and the store that reads
// it back has typed fields that say none of that ever happens.
//
// Zustand's `persist` rehydrates straight into the store: a setter that
// clamps `paneSplit` to 0.15…0.85 is bypassed entirely by a stored
// `paneSplit: 400`, and the pane is then invisible with no way to get it
// back. So every persisted domain runs its stored value through a
// validator first, and these are the pieces those validators are made
// of.
//
// Two rules shape the whole file:
//
//   * A validator never throws and never returns `undefined`. It returns
//     the valid value or the default, so a caller cannot forget a case.
//   * Every repair is *counted*. Silently fixing user data and saying
//     nothing is how a setting disappears without anyone knowing why,
//     so the count is what raises the one notification the user gets.
//
// Pure: no store, no React, no IPC. See `store/persisted/` for the
// per-domain schemas built from these.
// ============================================================

/**
 * What a validation run had to change.
 *
 * Carried through a whole domain so the caller can ask one question —
 * "did anything need fixing?" — instead of threading return values.
 */
export class Repairs {
  private readonly entries: string[] = [];

  /** Records one repair, naming the field for the diagnostics. */
  note(field: string): void {
    this.entries.push(field);
  }

  get count(): number {
    return this.entries.length;
  }

  get any(): boolean {
    return this.entries.length > 0;
  }

  /** The repaired fields, for a log line — never for the user's eyes. */
  get fields(): readonly string[] {
    return this.entries;
  }
}

/** A validation run that records nothing — for callers that only want the value. */
export const IGNORE_REPAIRS = new Repairs();

/** `true` for a plain object (not null, not an array). */
export function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** The stored value as an object, or an empty one. */
export function obj(v: unknown): Record<string, unknown> {
  return isObject(v) ? v : {};
}

export function bool(v: unknown, fallback: boolean, field: string, r: Repairs): boolean {
  if (typeof v === "boolean") return v;
  if (v !== undefined) r.note(field);
  return fallback;
}

export interface NumRules {
  min?: number;
  max?: number;
  /** Round to a whole number (sizes, counts, widths). */
  int?: boolean;
}

/**
 * A finite number inside its range.
 *
 * Out of range is *clamped* rather than discarded: a stored `paneSplit`
 * of 400 means the user once dragged the divider, and 0.85 is closer to
 * what they wanted than the default is. A non-number is a different
 * thing — nobody chose `"wide"` — and falls back.
 */
export function num(
  v: unknown,
  fallback: number,
  field: string,
  r: Repairs,
  rules: NumRules = {},
): number {
  if (typeof v !== "number" || !Number.isFinite(v)) {
    if (v !== undefined) r.note(field);
    return fallback;
  }
  let out = rules.int ? Math.round(v) : v;
  if (rules.min !== undefined && out < rules.min) out = rules.min;
  if (rules.max !== undefined && out > rules.max) out = rules.max;
  if (out !== v) r.note(field);
  return out;
}

/** A string, optionally capped in length so one value cannot fill the quota. */
export function str(
  v: unknown,
  fallback: string,
  field: string,
  r: Repairs,
  maxLength = 4096,
): string {
  if (typeof v !== "string") {
    if (v !== undefined) r.note(field);
    return fallback;
  }
  if (v.length > maxLength) {
    r.note(field);
    return v.slice(0, maxLength);
  }
  return v;
}

/** One of a closed set — the shape every enum-valued setting has. */
export function oneOf<T extends string>(
  v: unknown,
  values: readonly T[],
  fallback: T,
  field: string,
  r: Repairs,
): T {
  if (typeof v === "string" && (values as readonly string[]).includes(v)) {
    return v as T;
  }
  if (v !== undefined) r.note(field);
  return fallback;
}

/**
 * A CSS colour this application would actually write: `#rgb`, `#rrggbb`
 * or `#rrggbbaa`.
 *
 * Deliberately not "anything a browser accepts". These end up in inline
 * styles, and a stored value that is not a colour at all is either
 * corruption or an attempt to smuggle something into a style attribute.
 */
export function color(v: unknown, fallback: string, field: string, r: Repairs): string {
  if (typeof v === "string" && /^#(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(v)) {
    return v;
  }
  if (v !== undefined) r.note(field);
  return fallback;
}

/** Like [`color`], but `undefined` stays `undefined` (an absent override). */
export function optionalColor(
  v: unknown,
  field: string,
  r: Repairs,
): string | undefined {
  if (v === undefined) return undefined;
  const c = color(v, "", field, r);
  return c === "" ? undefined : c;
}

export interface ListRules {
  /** Entries beyond this are dropped — an unbounded list is a leak. */
  max?: number;
}

/**
 * An array whose entries each survive `item`.
 *
 * `item` returns `null` to drop an entry: a favourite without a path, a
 * colour rule that is not an object. Dropping beats repairing there,
 * because an entry with no identity cannot be shown or removed.
 */
export function list<T>(
  v: unknown,
  item: (value: unknown, index: number) => T | null,
  field: string,
  r: Repairs,
  rules: ListRules = {},
): T[] {
  if (!Array.isArray(v)) {
    if (v !== undefined) r.note(field);
    return [];
  }
  const max = rules.max ?? 1000;
  const out: T[] = [];
  for (let i = 0; i < v.length && out.length < max; i++) {
    const kept = item(v[i], i);
    if (kept === null) r.note(`${field}[${i}]`);
    else out.push(kept);
  }
  if (v.length > max) r.note(`${field}.length`);
  return out;
}

/**
 * A `Record<string, T>` whose values each survive `value`.
 *
 * `value` returns `null` to drop the key — a column width that is not a
 * number, an association pointing at a program that no longer exists.
 */
export function record<T>(
  v: unknown,
  value: (v: unknown, key: string) => T | null,
  field: string,
  r: Repairs,
  max = 500,
): Record<string, T> {
  if (!isObject(v)) {
    if (v !== undefined) r.note(field);
    return {};
  }
  const out: Record<string, T> = {};
  let kept = 0;
  for (const [key, raw] of Object.entries(v)) {
    if (kept >= max) {
      r.note(`${field}.size`);
      break;
    }
    const ok = value(raw, key);
    if (ok === null) {
      r.note(`${field}.${key}`);
      continue;
    }
    out[key] = ok;
    kept++;
  }
  return out;
}

/**
 * Drops entries that repeat, keeping the first.
 *
 * History and favourites are deduplicated as they are written; a stored
 * list that repeats itself came from somewhere else.
 */
export function unique<T>(items: T[], key: (item: T) => string): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const k = key(item);
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
}

/**
 * A value that must not be there at all.
 *
 * Persisted state is not a place for a secret (TICKET-039), and an
 * import is not a way to put one there. Fields whose *name* suggests one
 * are refused rather than stripped: a preferences file that carries a
 * password is not a preferences file, and quietly removing the field
 * would hide that from whoever is about to import it.
 */
const SECRET_NAME = /(password|passphrase|secret|token|credential|privatekey|private_key|apikey|api_key)/i;

/** The secret-looking field names anywhere in `value`, deepest last. */
export function secretLikeFields(value: unknown, path = ""): string[] {
  const found: string[] = [];
  const walk = (v: unknown, at: string, depth: number) => {
    // A bounded walk: a hostile file should not be able to spend the
    // main thread on nesting alone.
    if (depth > 12) return;
    if (Array.isArray(v)) {
      v.forEach((entry, i) => walk(entry, `${at}[${i}]`, depth + 1));
      return;
    }
    if (!isObject(v)) return;
    for (const [key, entry] of Object.entries(v)) {
      const here = at ? `${at}.${key}` : key;
      if (SECRET_NAME.test(key)) found.push(here);
      walk(entry, here, depth + 1);
    }
  };
  walk(value, path, 0);
  return found;
}
