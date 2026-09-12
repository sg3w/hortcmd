// ============================================================
// The smaller persisted domains (TICKET-039).
//
// Session, history and dialog sizes. Each is small enough that its
// schema, its version and its validator fit together in one place; the
// settings are the exception and live in `settings.ts`.
//
// Same two rules as everywhere in this directory: a validator returns a
// usable value rather than throwing, and everything it had to change is
// counted so the user gets told once.
// ============================================================

import { list, num, obj, oneOf, record, Repairs, str, unique } from "@/lib/schema";
import type { SavedSession, SavedSide } from "@/store/sessionStore";
import type { Side, ViewMode } from "@/store/panes/state";

/** Paths a person might plausibly have open, and then some. */
const MAX_TABS = 100;
const MAX_PATH = 4096;

// ---------- Session ----------

/**
 * 0 — up to TICKET-038: no version recorded.
 * 1 — versioned. No reshaping was needed; the point of the bump is that
 *     the *next* change has a number to migrate from.
 */
export const SESSION_VERSION = 1;

const VIEW_MODES: readonly ViewMode[] = ["details", "list", "thumbnails", "tree"];
const SIDES: readonly Side[] = ["left", "right"];

function savedSide(v: unknown, r: Repairs, where: string): SavedSide {
  const o = obj(v);
  const tabs = unique(
    list(
      o.tabs,
      (t) => {
        const path = str(obj(t).path, "", `${where}.tabs.path`, r, MAX_PATH);
        return path ? { path } : null;
      },
      `${where}.tabs`,
      r,
      { max: MAX_TABS },
    ),
    (t) => t.path,
  );
  // The active index has to point at a tab that exists — a stored 7 with
  // two tabs restored would leave the pane with no selection at all.
  const activeTab = tabs.length
    ? num(o.activeTab, 0, `${where}.activeTab`, r, {
        min: 0,
        max: tabs.length - 1,
        int: true,
      })
    : 0;
  return {
    tabs,
    activeTab,
    viewMode: oneOf(o.viewMode, VIEW_MODES, "details", `${where}.viewMode`, r),
  };
}

/**
 * The saved layout, or `null`.
 *
 * `null` is a legitimate value — a fresh installation has no session —
 * and is not a repair. A stored session with no tabs on either side is
 * treated the same way: restoring it would produce two empty panes.
 */
export function validateSession(stored: unknown, r: Repairs): SavedSession | null {
  if (stored === null || stored === undefined) return null;
  const o = obj(stored);
  const session: SavedSession = {
    left: savedSide(o.left, r, "left"),
    right: savedSide(o.right, r, "right"),
    active: oneOf(o.active, SIDES, "left", "active", r),
  };
  if (!session.left.tabs.length && !session.right.tabs.length) return null;
  return session;
}

// ---------- History ----------

/**
 * 0 — up to TICKET-038.
 * 1 — versioned; the cap is enforced on read as well as on write.
 */
export const HISTORY_VERSION = 1;

/** The same limit the store applies when pushing. */
export const HISTORY_MAX = 20;

/**
 * Recently visited folders: strings, deduplicated, capped.
 *
 * The cap matters on the way *in* too. The push path has always trimmed
 * to 20, but a file edited by hand — or written by a version with a
 * different limit — is not bound by that.
 */
export function validateHistory(stored: unknown, r: Repairs): string[] {
  const paths = list(
    stored,
    (v) => {
      const path = str(v, "", "recent", r, MAX_PATH);
      return path ? path : null;
    },
    "recent",
    r,
    { max: HISTORY_MAX },
  );
  return unique(paths, (p) => p);
}

// ---------- Dialog sizes ----------

/**
 * 0 — up to TICKET-038.
 * 1 — versioned; sizes are clamped to something a window can show.
 */
export const DIALOG_SIZES_VERSION = 1;

/**
 * Bounds for a remembered dialog size.
 *
 * The lower bound is "still has a title bar and a button"; the upper is
 * generous enough for a large display and small enough that a stored
 * 99999 cannot produce a dialog with no reachable edge.
 */
export const MIN_DIALOG_SIZE = 200;
export const MAX_DIALOG_SIZE = 10000;

export interface DialogSize {
  w: number;
  h: number;
}

export function validateDialogSizes(
  stored: unknown,
  r: Repairs,
): Record<string, DialogSize> {
  return record(
    stored,
    (v, key) => {
      const o = obj(v);
      if (typeof o.w !== "number" || typeof o.h !== "number") return null;
      return {
        w: num(o.w, 600, `sizes.${key}.w`, r, {
          min: MIN_DIALOG_SIZE,
          max: MAX_DIALOG_SIZE,
          int: true,
        }),
        h: num(o.h, 400, `sizes.${key}.h`, r, {
          min: MIN_DIALOG_SIZE,
          max: MAX_DIALOG_SIZE,
          int: true,
        }),
      };
    },
    "sizes",
    r,
    100,
  );
}
