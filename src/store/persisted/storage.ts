// ============================================================
// The storage adapter every persisted store uses (TICKET-039).
//
// Zustand's default adapter is `JSON.parse` with a `try`/`catch` that
// returns `null`. That is the right recovery and the wrong amount of
// silence: a truncated write — a crash mid-save, a full quota, a file
// somebody edited — becomes "your settings are back to defaults" with
// no explanation and no copy of what was there.
//
// So the parse failure is routed through the same gate as a validation
// repair: the raw text is quarantined, the domain is marked recovered,
// and the one notification at startup covers both cases.
//
// It also swallows the *write* side. `localStorage` throws when the
// quota is full and in a browser configured to block site data, and a
// file manager that cannot remember a column width must still open.
// ============================================================

import type { PersistStorage, StorageValue } from "zustand/middleware";
import { reportUnreadable, type PersistedDomain } from "./gate";

export function guardedStorage<T>(domain: PersistedDomain): PersistStorage<T> {
  return {
    getItem: (name) => {
      let raw: string | null = null;
      try {
        raw = localStorage.getItem(name);
      } catch {
        // Storage is unavailable entirely (a private window, blocked
        // site data). Not corruption, and not worth a notice: nothing
        // was lost because nothing was ever stored.
        return null;
      }
      if (raw === null) return null;

      try {
        const parsed: unknown = JSON.parse(raw);
        if (typeof parsed !== "object" || parsed === null) {
          throw new Error("persisted value is not an object");
        }
        return parsed as StorageValue<T>;
      } catch {
        reportUnreadable(domain, raw);
        return null;
      }
    },

    setItem: (name, value) => {
      try {
        localStorage.setItem(name, JSON.stringify(value));
      } catch (e) {
        // Most likely the quota. Reported once per write attempt in the
        // console; raising this to the user on every keystroke in a
        // settings field would be worse than the lost preference.
        console.warn(`[hortcmd] could not persist "${name}":`, e);
      }
    },

    removeItem: (name) => {
      try {
        localStorage.removeItem(name);
      } catch {
        /* nothing to do and nothing to say */
      }
    },
  };
}
