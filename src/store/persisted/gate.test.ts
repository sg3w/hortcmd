// ============================================================
// The recovery path (TICKET-039).
//
// What happens when stored data is unusable: the original is put aside
// instead of overwritten, the domain is remembered so the user can be
// told once, and the application carries on with defaults. None of that
// is visible from the validators — it only exists where localStorage
// does, which is why it is tested separately.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repairs } from "@/lib/schema";
import {
  acceptVersion,
  clearQuarantine,
  PERSISTED_KEYS,
  quarantinedKeys,
  recoveredDomains,
  rehydrate,
  reportUnreadable,
  resetRecovered,
  wasRecovered,
} from "./gate";
import { guardedStorage } from "./storage";

/** A validator that clamps one number, so a repair is easy to provoke. */
const clampToTen = (value: unknown, r: Repairs) => ({
  n: typeof value === "number" ? Math.min(10, value) : ((r.note("n"), 0) as number),
});

beforeEach(() => {
  localStorage.clear();
  resetRecovered();
  vi.restoreAllMocks();
});

describe("rehydrate", () => {
  it("passes a valid value through without touching anything", () => {
    localStorage.setItem(PERSISTED_KEYS.history, '{"state":{"recent":[]}}');
    const out = rehydrate("history", 5, clampToTen);

    expect(out).toEqual({ n: 5 });
    expect(wasRecovered("history")).toBe(false);
    expect(quarantinedKeys()).toEqual([]);
  });

  it("quarantines the original when it had to repair something", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    localStorage.setItem(PERSISTED_KEYS.settings, '{"state":{"broken":true}}');

    const out = rehydrate("settings", "not a number", clampToTen);

    expect(out).toEqual({ n: 0 });
    expect(wasRecovered("settings")).toBe(true);
    const kept = quarantinedKeys();
    expect(kept.length).toBe(1);
    expect(localStorage.getItem(kept[0])).toBe('{"state":{"broken":true}}');
    // The stored key itself is untouched — the store will overwrite it
    // on the next write, and until then the evidence is in two places.
    expect(localStorage.getItem(PERSISTED_KEYS.settings)).toBe(
      '{"state":{"broken":true}}',
    );
  });

  it("validates even when the version already matched", () => {
    // A value can be out of range without the schema ever changing —
    // which is the whole reason validation is not tied to migration.
    const out = rehydrate("settings", 50, clampToTen);
    expect(out).toEqual({ n: 10 });
  });

  /**
   * A newer schema is not something to read. Migrating forwards is
   * impossible, and reading it with the current validator would drop
   * whatever the newer version added and then write the truncated
   * result back — a silent downgrade of the user's own data.
   */
  it("refuses to read a newer schema and keeps it intact", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    localStorage.setItem(PERSISTED_KEYS.settings, '{"version":9,"state":{"x":1}}');

    expect(acceptVersion("settings", 9, 1)).toBe(false);
    expect(wasRecovered("settings")).toBe(true);
    expect(quarantinedKeys().length).toBe(1);
    // The stored text is still there for the newer installation.
    expect(localStorage.getItem(PERSISTED_KEYS.settings)).toBe(
      '{"version":9,"state":{"x":1}}',
    );
  });

  it("accepts the current version and every older one", () => {
    expect(acceptVersion("settings", 1, 1)).toBe(true);
    expect(acceptVersion("settings", 0, 1)).toBe(true);
    expect(wasRecovered("settings")).toBe(false);
  });

  it("reports every recovered domain once, in a stable order", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    rehydrate("session", "bad", clampToTen);
    rehydrate("settings", "bad", clampToTen);
    rehydrate("settings", "bad", clampToTen);

    expect(recoveredDomains()).toEqual(["settings", "session"]);
  });

  it("keeps only the most recent quarantined copies", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    for (let i = 0; i < 9; i++) {
      localStorage.setItem(PERSISTED_KEYS.history, `copy-${i}`);
      reportUnreadable("history", `copy-${i}`);
      // The key carries a millisecond timestamp; without this two
      // copies in the same tick would collide and the test would be
      // measuring the clock rather than the pruning.
      vi.setSystemTime(Date.now() + 1000);
    }
    expect(quarantinedKeys().length).toBe(5);
    vi.useRealTimers();
  });

  it("survives storage that refuses to be written", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("quota", "QuotaExceededError");
    });

    // No copy can be kept, and that must not stop the recovery.
    expect(() => reportUnreadable("settings", "raw")).not.toThrow();
    expect(wasRecovered("settings")).toBe(true);
  });

  it("clears the quarantine on request", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    reportUnreadable("history", "raw");
    expect(quarantinedKeys().length).toBe(1);
    clearQuarantine();
    expect(quarantinedKeys()).toEqual([]);
  });
});

describe("the storage adapter", () => {
  it("reads a well-formed value", () => {
    const storage = guardedStorage<{ n: number }>("settings");
    localStorage.setItem("k", '{"state":{"n":1},"version":1}');
    expect(storage.getItem("k")).toEqual({ state: { n: 1 }, version: 1 });
    expect(wasRecovered("settings")).toBe(false);
  });

  /**
   * Zustand's own adapter returns `null` here and says nothing, which
   * is the right recovery and the wrong amount of silence: the user's
   * settings are gone and nobody mentions it.
   */
  it("treats unparsable text as a recovery, not as nothing", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const storage = guardedStorage<{ n: number }>("settings");
    localStorage.setItem("k", "{ truncated mid-writ");

    expect(storage.getItem("k")).toBeNull();
    expect(wasRecovered("settings")).toBe(true);
    expect(localStorage.getItem(quarantinedKeys()[0])).toBe("{ truncated mid-writ");
  });

  it("treats a value that is not an object the same way", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const storage = guardedStorage<{ n: number }>("session");
    localStorage.setItem("k", '"just a string"');

    expect(storage.getItem("k")).toBeNull();
    expect(wasRecovered("session")).toBe(true);
  });

  it("says nothing when there was never anything stored", () => {
    const storage = guardedStorage<{ n: number }>("settings");
    expect(storage.getItem("missing")).toBeNull();
    expect(wasRecovered("settings")).toBe(false);
    expect(quarantinedKeys()).toEqual([]);
  });

  /** A private window, or a browser set to block site data. */
  it("survives storage that cannot be read at all", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    const storage = guardedStorage<{ n: number }>("settings");

    expect(storage.getItem("k")).toBeNull();
    // Nothing was lost, because nothing was ever stored — this is not
    // a recovery and must not raise a notice.
    expect(wasRecovered("settings")).toBe(false);
  });

  it("does not throw when a write fails", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("quota", "QuotaExceededError");
    });
    const storage = guardedStorage<{ n: number }>("settings");

    expect(() => storage.setItem("k", { state: { n: 1 }, version: 1 })).not.toThrow();
    expect(() => storage.removeItem("k")).not.toThrow();
  });
});
