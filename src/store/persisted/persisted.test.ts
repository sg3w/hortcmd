// ============================================================
// What comes back out of storage (TICKET-039).
//
// Two kinds of test here, and they answer different questions:
//
//   * *Fixtures* — one per released schema version, asserted against
//     the whole current shape. They are the regression net for a
//     migration chain: when version 2 arrives, the v0 and v1 fixtures
//     must still come out right, and only a test that pins the *output*
//     can tell you they do.
//   * *Hostile input* — missing, extra, wrong-typed, out-of-range,
//     oversized and deeply nested values. Not because a user will write
//     them, but because a crash mid-write, a quota error, or a hand-
//     edited file will.
//
// Everything here is pure. The gate that quarantines and notifies is
// tested in `gate.test.ts`, where localStorage is in play.
// ============================================================

import { describe, expect, it } from "vitest";
import { Repairs, secretLikeFields } from "@/lib/schema";
import {
  migrateSettings,
  SETTINGS_VERSION,
  validateSettings,
} from "./settings";
import {
  validateDialogSizes,
  validateHistory,
  validateSession,
  HISTORY_MAX,
  MAX_DIALOG_SIZE,
  MIN_DIALOG_SIZE,
} from "./domains";
import { buildExport, readImport } from "./transfer";

/** Runs the full rehydration path for the settings, as the store does. */
function load(stored: unknown, from = 0) {
  const r = new Repairs();
  const value = validateSettings(migrateSettings(stored, from), r);
  return { value, repairs: r };
}

// ---------- Fixtures per released schema ----------

/**
 * Version 0 — everything shipped up to TICKET-038.
 *
 * Deliberately a *complete* realistic object rather than a fragment:
 * the point of a fixture is to notice when a later migration quietly
 * changes something it was not supposed to touch.
 */
const V0 = {
  language: "en",
  theme: "light",
  hideSystemFiles: false,
  paneSplit: 0.62,
  favorites: [{ name: "Work", path: "/home/u/work" }],
  terminalProgram: "/usr/bin/alacritty",
  gitEnabled: false,
  useTrash: false,
  verifyCopies: true,
  speedLimit: 2048,
  queueTransfers: false,
  bufferSizeKb: 1024,
  copyThreads: 4,
  safeTransfers: false,
  foldersFirst: false,
  showExtColumn: false,
  showPermissions: true,
  sizeFormat: "bytes",
  dateFormat: "iso",
  fontScale: "lg",
  iconScale: "sm",
  columnWidths: { ext: 80, size: 120 },
  fileColors: { archive: { dark: "#ff0000" } },
  customColorRules: [
    { id: "r1", name: "Logs", pattern: "*.log", dark: "#111111", light: "#eeeeee" },
  ],
  programs: [{ id: "p1", name: "Vim", path: "/usr/bin/vim" }],
  associations: { log: "p1" },
  defaultEditor: "/usr/bin/code",
  editorTrigger: "f4",
};

describe("settings fixtures", () => {
  it("keeps every valid v0 preference", () => {
    const { value, repairs } = load(V0, 0);

    expect(repairs.fields).toEqual([]);
    expect(value.language).toBe("en");
    expect(value.theme).toBe("light");
    expect(value.paneSplit).toBe(0.62);
    expect(value.favorites).toEqual([{ name: "Work", path: "/home/u/work" }]);
    expect(value.bufferSizeKb).toBe(1024);
    expect(value.copyThreads).toBe(4);
    expect(value.dateFormat).toBe("iso");
    expect(value.programs).toEqual([{ id: "p1", name: "Vim", path: "/usr/bin/vim" }]);
    expect(value.associations).toEqual({ log: "p1" });
    expect(value.customColorRules?.[0].pattern).toBe("*.log");
    expect(value.fileColors).toEqual({ archive: { dark: "#ff0000" } });
    // Widths that were stored win; the ones that were not keep their default.
    expect(value.columnWidths).toEqual({ ext: 80, perms: 100, size: 120, date: 130 });
  });

  /**
   * The one behavioural migration. TICKET-029 specified failure-safe
   * transfers as the default and shipped them off, so a stored `false`
   * from v0 is the wrong default rather than a decision.
   */
  it("turns failure-safe transfers on when coming from v0", () => {
    expect(load({ ...V0, safeTransfers: false }, 0).value.safeTransfers).toBe(true);
    expect(load({ ...V0, safeTransfers: true }, 0).value.safeTransfers).toBe(true);
    // Absent is the case the ticket names explicitly.
    const withoutIt: Record<string, unknown> = { ...V0 };
    delete withoutIt.safeTransfers;
    expect(load(withoutIt, 0).value.safeTransfers).toBe(true);
  });

  /**
   * The migration covers an *existing* installation. A fresh one never
   * runs it — there is nothing stored to migrate — so the store's own
   * default has to agree with the validator's, or the protection would
   * depend on whether the user had run an older version first.
   */
  it("ships with failure-safe transfers on for a fresh installation", async () => {
    const { useSettings } = await import("@/store/settingsStore");
    expect(useSettings.getState().safeTransfers).toBe(true);
    expect(load({}, SETTINGS_VERSION).value.safeTransfers).toBe(true);
  });

  it("leaves an explicit choice alone once the schema is current", () => {
    expect(load({ ...V0, safeTransfers: false }, 1).value.safeTransfers).toBe(false);
    expect(load({ ...V0, safeTransfers: true }, 1).value.safeTransfers).toBe(true);
  });

  it("is idempotent — migrating twice changes nothing more", () => {
    const once = migrateSettings(V0, 0);
    const twice = migrateSettings(once, SETTINGS_VERSION);
    expect(twice).toEqual(once);
  });

  it("needs no repairs for its own output", () => {
    // The validator's result, fed back in, must be a fixed point.
    const { value } = load(V0, 0);
    const { repairs } = load(value, SETTINGS_VERSION);
    expect(repairs.fields).toEqual([]);
  });
});

// ---------- Hostile input ----------

describe("settings validation", () => {
  it("survives values of entirely the wrong shape", () => {
    for (const stored of [null, undefined, 42, "settings", [], true]) {
      const { value } = load(stored, SETTINGS_VERSION);
      expect(value.language).toBe("de");
      expect(value.paneSplit).toBe(0.5);
      expect(value.favorites).toEqual([]);
    }
  });

  it("clamps the pane split instead of hiding a pane", () => {
    expect(load({ paneSplit: 400 }, 1).value.paneSplit).toBe(0.85);
    expect(load({ paneSplit: -3 }, 1).value.paneSplit).toBe(0.15);
    expect(load({ paneSplit: Number.NaN }, 1).value.paneSplit).toBe(0.5);
    expect(load({ paneSplit: "wide" }, 1).value.paneSplit).toBe(0.5);
  });

  it("clamps the transfer numbers to what the backend accepts", () => {
    const { value } = load(
      { bufferSizeKb: 9_999_999, copyThreads: 512, speedLimit: -5 },
      1,
    );
    expect(value.bufferSizeKb).toBe(16 * 1024);
    expect(value.copyThreads).toBe(16);
    expect(value.speedLimit).toBe(0);
  });

  it("rejects an enum it does not know", () => {
    const { value, repairs } = load(
      { theme: "solarized", sizeFormat: "furlongs", editorTrigger: 7 },
      1,
    );
    expect(value.theme).toBe("dark");
    expect(value.sizeFormat).toBe("auto");
    expect(value.editorTrigger).toBe("shiftF4");
    expect(repairs.count).toBe(3);
  });

  it("rejects a colour that is not one", () => {
    const { value } = load(
      {
        fileColors: { a: { dark: "javascript:alert(1)" }, b: { light: "#abc" } },
        customColorRules: [{ id: "x", name: "n", pattern: "*", dark: "red", light: 5 }],
      },
      1,
    );
    // A slot with nothing valid left is not an override at all.
    expect(value.fileColors).toEqual({ b: { light: "#abc" } });
    expect(value.customColorRules?.[0].dark).toBe("#f5a43c");
    expect(value.customColorRules?.[0].light).toBe("#c2410c");
  });

  it("drops entries with no identity and keeps the rest", () => {
    const { value } = load(
      {
        favorites: [
          { name: "ok", path: "/a" },
          { name: "no path" },
          "not an object",
          { name: "ok2", path: "/b" },
        ],
        programs: [
          { id: "p1", name: "Vim", path: "/usr/bin/vim" },
          { id: "p2", name: "No path" },
          { name: "No id", path: "/x" },
        ],
      },
      1,
    );
    expect(value.favorites?.map((f) => f.path)).toEqual(["/a", "/b"]);
    expect(value.programs?.map((p) => p.id)).toEqual(["p1"]);
  });

  /** A dangling association opens nothing and cannot be seen or removed. */
  it("removes associations pointing at a program that is gone", () => {
    const { value, repairs } = load(
      {
        programs: [{ id: "p1", name: "Vim", path: "/usr/bin/vim" }],
        associations: { log: "p1", txt: "p-deleted", md: 7 },
      },
      1,
    );
    expect(value.associations).toEqual({ log: "p1" });
    expect(repairs.any).toBe(true);
  });

  it("normalizes legacy association keys and records collisions", () => {
    const { value, repairs } = load({ ...V0, associations: { ".LOG": "p1", log: "p1", ".": "p1", constructor: "p1" } }, 1);
    expect(value.associations).toEqual({ log: "p1", constructor: "p1" });
    expect(repairs.fields).toEqual(expect.arrayContaining(["associations..LOG", "associations.log", "associations.."]));
    expect(load(value, 1).repairs.any).toBe(false);
  });

  it("caps lists so a stored loop cannot fill the quota", () => {
    const favorites = Array.from({ length: 5000 }, (_, i) => ({
      name: `f${i}`,
      path: `/p/${i}`,
    }));
    const { value } = load({ favorites }, 1);
    expect(value.favorites?.length).toBe(200);
  });

  it("deduplicates what should have been unique", () => {
    const { value } = load(
      {
        favorites: [
          { name: "a", path: "/same" },
          { name: "b", path: "/same" },
        ],
        programs: [
          { id: "p1", name: "One", path: "/one" },
          { id: "p1", name: "Two", path: "/two" },
        ],
      },
      1,
    );
    expect(value.favorites?.length).toBe(1);
    expect(value.programs).toEqual([{ id: "p1", name: "One", path: "/one" }]);
  });

  it("ignores fields it has never heard of", () => {
    const { value } = load({ ...V0, somethingRemoved: true, nested: { a: [1] } }, 1);
    expect(value).not.toHaveProperty("somethingRemoved");
    expect(value).not.toHaveProperty("nested");
  });

  it("does not count an absent field as a repair", () => {
    // An older installation meeting a newer field is what defaults are
    // for — raising a "your settings were damaged" notice for that
    // would make every upgrade look like data loss.
    const { repairs } = load({ language: "en" }, SETTINGS_VERSION);
    expect(repairs.fields).toEqual([]);
  });

  it("truncates a string long enough to be an attack on the quota", () => {
    const { value, repairs } = load({ terminalProgram: "x".repeat(100_000) }, 1);
    expect(value.terminalProgram?.length).toBe(4096);
    expect(repairs.any).toBe(true);
  });
});

// ---------- Session, history, dialog sizes ----------

describe("session validation", () => {
  it("keeps a well-formed session", () => {
    const r = new Repairs();
    const session = validateSession(
      {
        left: { tabs: [{ path: "/a" }, { path: "/b" }], activeTab: 1, viewMode: "list" },
        right: { tabs: [{ path: "/c" }], activeTab: 0, viewMode: "details" },
        active: "right",
      },
      r,
    );
    expect(session?.left.tabs.length).toBe(2);
    expect(session?.left.activeTab).toBe(1);
    expect(session?.active).toBe("right");
    expect(r.fields).toEqual([]);
  });

  /** An index past the end leaves the pane with no tab selected at all. */
  it("pulls the active tab back into range", () => {
    const r = new Repairs();
    const session = validateSession(
      { left: { tabs: [{ path: "/a" }], activeTab: 7 }, right: { tabs: [] } },
      r,
    );
    expect(session?.left.activeTab).toBe(0);
    expect(r.any).toBe(true);
  });

  it("treats a session with nothing in it as no session", () => {
    const r = new Repairs();
    expect(validateSession({ left: { tabs: [] }, right: { tabs: [] } }, r)).toBeNull();
    expect(validateSession(null, r)).toBeNull();
    expect(validateSession("garbage", r)).toBeNull();
  });

  it("drops tabs that are not paths and keeps the order of the rest", () => {
    const r = new Repairs();
    const session = validateSession(
      {
        left: { tabs: [{ path: "/a" }, {}, { path: "" }, 5, { path: "/b" }] },
        right: { tabs: [] },
      },
      r,
    );
    expect(session?.left.tabs.map((t) => t.path)).toEqual(["/a", "/b"]);
  });
});

describe("history validation", () => {
  it("keeps the cap on the way in as well as the way out", () => {
    const r = new Repairs();
    const recent = validateHistory(
      Array.from({ length: 500 }, (_, i) => `/p/${i}`),
      r,
    );
    expect(recent.length).toBe(HISTORY_MAX);
    expect(recent[0]).toBe("/p/0");
  });

  it("drops what is not a path and deduplicates", () => {
    const r = new Repairs();
    expect(validateHistory(["/a", "", null, "/a", 7, "/b"], r)).toEqual(["/a", "/b"]);
  });

  it("survives a value that is not a list", () => {
    const r = new Repairs();
    expect(validateHistory({ recent: "/a" }, r)).toEqual([]);
  });
});

describe("dialog size validation", () => {
  it("clamps a size to something a window can show", () => {
    const r = new Repairs();
    const sizes = validateDialogSizes(
      { search: { w: 99999, h: 5 }, rename: { w: 700, h: 500 } },
      r,
    );
    expect(sizes.search).toEqual({ w: MAX_DIALOG_SIZE, h: MIN_DIALOG_SIZE });
    expect(sizes.rename).toEqual({ w: 700, h: 500 });
  });

  it("drops an entry that is not a size", () => {
    const r = new Repairs();
    expect(validateDialogSizes({ a: { w: "wide", h: 10 }, b: null }, r)).toEqual({});
  });
});

// ---------- Export and import ----------

describe("preferences export and import", () => {
  it("round-trips a full settings object", () => {
    const exported = { ...V0, safeTransfers: false };
    const result = readImport(buildExport(exported));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.settings.language).toBe("en");
    expect(result.settings.paneSplit).toBe(0.62);
    // An export carries the *current* schema version, so a deliberate
    // choice survives the round trip rather than being migrated again.
    expect(result.settings.safeTransfers).toBe(false);
  });

  it("migrates a file written by an older version", () => {
    const text = JSON.stringify({
      app: "hortcmd",
      kind: "preferences",
      version: 0,
      settings: { ...V0, safeTransfers: false },
    });
    const result = readImport(text);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    // Same path as a v0 localStorage: the file predates the corrected
    // default, so it is not a decision to preserve.
    expect(result.settings.safeTransfers).toBe(true);
  });

  it("refuses a file that is not JSON", () => {
    const result = readImport("{ not json");
    expect(result).toEqual({ ok: false, refusal: { reason: "notJson" } });
  });

  it("refuses a file that is not ours", () => {
    expect(readImport(JSON.stringify({ hello: "world" }))).toEqual({
      ok: false,
      refusal: { reason: "notOurs" },
    });
    expect(
      readImport(JSON.stringify({ app: "hortcmd", kind: "session", settings: {} })),
    ).toEqual({ ok: false, refusal: { reason: "notOurs" } });
  });

  /** A newer schema may mean something different by a field we also know. */
  it("refuses a file from a newer schema instead of misreading it", () => {
    const text = JSON.stringify({
      app: "hortcmd",
      kind: "preferences",
      version: SETTINGS_VERSION + 5,
      settings: { language: "en" },
    });
    expect(readImport(text)).toEqual({
      ok: false,
      refusal: { reason: "tooNew", version: SETTINGS_VERSION + 5 },
    });
  });

  /**
   * Refused, not stripped: quietly removing the field would hide from
   * whoever is importing that somebody put a password in a preferences
   * file — and it is the person exporting who needs to know.
   */
  it("refuses a file carrying anything that looks like a secret", () => {
    const text = JSON.stringify({
      app: "hortcmd",
      kind: "preferences",
      version: SETTINGS_VERSION,
      settings: { language: "en", connections: [{ host: "a", password: "hunter2" }] },
    });
    const result = readImport(text);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.refusal.reason).toBe("hasSecrets");
  });

  it("finds secret-looking fields wherever they are", () => {
    expect(secretLikeFields({ a: { b: { apiKey: 1 } } })).toEqual(["a.b.apiKey"]);
    expect(secretLikeFields([{ privateKey: "x" }])).toEqual(["[0].privateKey"]);
    expect(secretLikeFields({ passphrase: "", token: "" }).length).toBe(2);
    expect(secretLikeFields({ harmless: 1, passenger: 2 })).toEqual([]);
    // Substring matching, so "tokenizer" is a false positive. That is
    // the right way round: refusing a file that was fine costs an
    // explanation, accepting one with a password in it costs more.
    expect(secretLikeFields({ tokenizer: 1 })).toEqual(["tokenizer"]);
  });

  it("does not walk a file deep enough to hang on", () => {
    // A thousand levels of nesting is not a preferences file; the walk
    // stops long before it becomes a way to spend the main thread.
    let deep: Record<string, unknown> = { password: "found?" };
    for (let i = 0; i < 1000; i++) deep = { nested: deep };
    expect(secretLikeFields(deep)).toEqual([]);
  });

  it("applies nothing when it refuses", () => {
    // The shape of the API is the guarantee: a refusal carries no
    // settings at all, so there is nothing a caller could half-apply.
    const result = readImport("nonsense");
    expect(result.ok).toBe(false);
    expect(result).not.toHaveProperty("settings");
  });
});

it("defaults native icons for older settings and validates the persisted mode", () => {
  expect(load({}).value.fileIcons).toBe("system");
  expect(load({ fileIcons: "simple" }).value.fileIcons).toBe("simple");
  expect(load({ fileIcons: "bogus" }).value.fileIcons).toBe("system");
});

it("persists an explicitly disabled quota check and defaults older settings to enabled", () => {
  expect(load({}, SETTINGS_VERSION).value.checkTransferSpace).toBe(true);
  expect(load({ checkTransferSpace: false }, SETTINGS_VERSION).value.checkTransferSpace).toBe(false);
  expect(load({ checkTransferSpace: "false" }, SETTINGS_VERSION).value.checkTransferSpace).toBe(true);
});
